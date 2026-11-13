//! `Files.readAttributes(path, BasicFileAttributes.class)`, `Files.isDirectory`
//! / `isRegularFile` and `File.isDirectory` / `isFile`, answered from the file
//! model as JDK 25 answers them on Linux.
//!
//! night-config's `FileWatcher.addOrPutWatch` (Forge Config API Port's
//! `ConfigTracker.openConfig`) asks `Files.readAttributes(path,
//! BasicFileAttributes.class).isDirectory()` of the config it just wrote. The
//! call was counted and answered null, so the very next `isDirectory()` threw
//! an NPE (scholar, particular and ArmorPoser on the #124 stack). `Files
//! .isDirectory`/`isRegularFile` answered false for every path, uncounted, and
//! `File.isDirectory` false and `File.isFile` true for a directory the run
//! made: silent wrong answers for paths the model holds.
//!
//! **What the model knows.** It keeps the files a run wrote (`io_files`) and,
//! since this module, the directories it made (`io_dirs`: `createDirectory`,
//! `createDirectories`, `File.mkdir`, `File.mkdirs`). A path is
//!
//! * a directory when the run made it, when it is the working directory
//!   (`""`, `"."`), when it is the parent (at any depth) of a file or
//!   directory the model holds, or when it is one of the folders the
//!   loader hands out (`/horizon/game`, `/horizon/config`, `/horizon/mods`,
//!   `/horizon/defaultconfigs`) or one of their parents: a game always has
//!   those;
//! * a regular file when the run wrote it, or when it is a resource the host
//!   was seeded with (what `Files.exists` already answers);
//! * missing otherwise. The model's filesystem holds nothing else
//!   (`Files.exists` answers false for every other path), so JDK 25 throws
//!   `NoSuchFileException(<path>)`.
//!
//! **`Path.equals`/`hashCode`** compare and hash the path text as
//! `UnixPath` keeps it (repeated and trailing slashes dropped), byte by byte.
//! A non-ASCII path's hash is counted: its bytes depend on the machine's
//! `sun.jnu.encoding`.
//!
//! **`Path.toRealPath()`** answers an absolute path in normal form that
//! exists with itself (the model has no links), and throws as JDK 25 does
//! for a missing one; see [`Runtime::real_path`].
//!
//! **`readAttributes(path, type, options...)`**, checked against JDK 25.0.5
//! (Temurin, `UnixFileSystemProvider.readAttributes` and a program run on
//! elysium):
//!
//! * a null path throws the helpful NPE `Cannot invoke
//!   "java.nio.file.Path.getFileSystem()" because "path" is null`;
//! * a null `type` throws a bare NPE; any type but `BasicFileAttributes` or
//!   `PosixFileAttributes` throws a bare `UnsupportedOperationException`;
//!   `PosixFileAttributes` (owner, group, permissions) is counted;
//! * a `LinkOption` is counted (the model has no constant for one);
//! * a missing path throws `NoSuchFileException(<path>)`;
//! * otherwise a new attributes object (`readAttributes` twice gives two
//!   objects), a snapshot taken at the call.
//!
//! On that object, `isDirectory`, `isRegularFile`, `isSymbolicLink` (false:
//! the model has no links) and `isOther` (false) are exact. `size`,
//! `lastModifiedTime`, `lastAccessTime`, `creationTime` and `fileKey` are
//! counted. The model keeps a file's text, not its bytes or charset (a
//! UTF-16 writer makes `"ab"` six bytes long). A directory's size is the
//! filesystem's (8192 on the oracle's). Times and the inode belong to the
//! machine.

use super::*;

const FILES: &str = "java/nio/file/Files";
const BASIC: &str = "java/nio/file/attribute/BasicFileAttributes";
const POSIX: &str = "java/nio/file/attribute/PosixFileAttributes";
/// The class JDK 25's `readAttributes(path, BasicFileAttributes.class)`
/// answers on Linux.
const ATTRIBUTES_CLASS: &str = "sun/nio/fs/UnixFileAttributes$UnixAsBasicFileAttributes";
/// The synthetic field holding a snapshot's kind (`1` directory, `0` file).
const DIRECTORY_FIELD: &str = "#isDirectory";

/// The folders the loader answers `getGameDir`/`getConfigDir`/`getModsDir`
/// (and NeoForge's `FMLPaths`) with.
const LOADER_DIRECTORIES: &[&str] = &[
    "/horizon/game",
    "/horizon/config",
    "/horizon/mods",
    "/horizon/defaultconfigs",
];

/// What the file model knows a path to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileKind {
    Directory,
    RegularFile,
    Missing,
}

fn thrown(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message,
        frame: None,
    }
}

/// The NPE `Files.provider(path)` throws for a null path.
fn null_path() -> NativeError {
    thrown(
        "java/lang/NullPointerException",
        Some(
            "Cannot invoke \"java.nio.file.Path.getFileSystem()\" because \"path\" is null".into(),
        ),
    )
}

/// A path as `UnixPath` keeps it: repeated slashes collapsed and a trailing
/// slash dropped (`normalizeAndCheck`).
fn unix_normal(path: &str) -> String {
    let mut normal = String::with_capacity(path.len());
    for c in path.chars() {
        if !(c == '/' && normal.ends_with('/')) {
            normal.push(c);
        }
    }
    if normal.len() > 1 && normal.ends_with('/') {
        normal.pop();
    }
    normal
}

/// Whether `ancestor` is a proper parent of `path`, by path text.
fn is_parent_of(ancestor: &str, path: &str) -> bool {
    let ancestor = ancestor.trim_end_matches('/');
    path.strip_prefix(ancestor)
        .is_some_and(|rest| rest.starts_with('/') && rest.len() > 1)
        || (ancestor.is_empty() && path.starts_with('/') && path.len() > 1)
}

impl<H: Host> Runtime<'_, H> {
    /// What `path` is in the file model (see the module docs).
    pub(crate) fn file_kind(&mut self, path: &str) -> FileKind {
        let path = path.trim_end_matches('/');
        // The working directory (`Path.of("")`, `Path.of(".")`) always exists.
        if path.is_empty()
            || path == "."
            || self.io_dirs.contains(path)
            || LOADER_DIRECTORIES
                .iter()
                .any(|root| *root == path || is_parent_of(path, root))
            || self.io_dirs.iter().any(|known| is_parent_of(path, known))
            || self.io_files.keys().any(|known| is_parent_of(path, known))
        {
            return FileKind::Directory;
        }
        if self.io_files.contains_key(path)
            || library_dispatch::resource_text(&mut *self.host, path).is_some()
        {
            return FileKind::RegularFile;
        }
        FileKind::Missing
    }

    /// Answer `Files.readAttributes`/`isDirectory`/`isRegularFile` or a call on
    /// an attributes snapshot; `Ok(None)` for any other call.
    pub(super) fn invoke_file_attributes(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if let Some(marker) = receiver.and_then(expect_ref).filter(|marker| {
            marker
                .rsplit_once('#')
                .is_some_and(|(class, _)| class == ATTRIBUTES_CLASS)
        }) {
            return self.attributes_call(r, &marker.clone());
        }
        if r.owner == "java/nio/file/Path"
            && let Some(receiver @ Value::Ref(Some(marker))) = receiver
            && marker.starts_with("path:")
        {
            return match (r.name.as_str(), r.descriptor.as_str()) {
                ("toRealPath", "([Ljava/nio/file/LinkOption;)Ljava/nio/file/Path;") => {
                    self.real_path(r, Some(receiver), args.first())
                }
                // `UnixPath.equals`/`hashCode` over the normalized path bytes.
                ("equals", "(Ljava/lang/Object;)Z") => {
                    let other = match args.first() {
                        Some(Value::Ref(Some(other))) => other.strip_prefix("path:"),
                        _ => None,
                    };
                    Ok(Some(Value::Int(i32::from(other.is_some_and(|other| {
                        unix_normal(other) == unix_normal(path_text(receiver))
                    })))))
                }
                // A path's bytes are its text in `sun.jnu.encoding`, which is
                // the machine's locale: only ASCII is the same everywhere.
                ("hashCode", "()I") if !path_text(receiver).is_ascii() => {
                    self.host.unknown_operand_call(r).map(Some)
                }
                ("hashCode", "()I") => Ok(Some(Value::Int(
                    unix_normal(path_text(receiver))
                        .bytes()
                        .fold(0i32, |hash, byte| {
                            hash.wrapping_mul(31).wrapping_add(i32::from(byte))
                        }),
                ))),
                _ => Ok(None),
            };
        }
        if r.owner != FILES {
            return Ok(None);
        }
        // A path inside a mod's own root is the jar's content, which
        // `jar_root_files` answers.
        if args
            .first()
            .and_then(expect_ref)
            .is_some_and(|_| path_text(&args[0]).starts_with(library_dispatch::MOD_ROOT_PREFIX))
        {
            return Ok(None);
        }
        match (r.name.as_str(), r.descriptor.as_str()) {
            (
                "readAttributes",
                "(Ljava/nio/file/Path;Ljava/lang/Class;[Ljava/nio/file/LinkOption;)Ljava/nio/file/attribute/BasicFileAttributes;",
            ) => self.read_attributes(r, args).map(Some),
            (
                "isDirectory" | "isRegularFile",
                "(Ljava/nio/file/Path;[Ljava/nio/file/LinkOption;)Z",
            ) => {
                let path = match StdHost::argument(r, args, 0)? {
                    Value::Ref(None) => return Err(null_path()),
                    value => path_text(value).to_owned(),
                };
                if !self.no_link_options(args.get(1)) {
                    return self.host.unknown_operand_call(r).map(Some);
                }
                let wanted = if r.name == "isDirectory" {
                    FileKind::Directory
                } else {
                    FileKind::RegularFile
                };
                Ok(Some(Value::Int(i32::from(self.file_kind(&path) == wanted))))
            }
            _ => Ok(None),
        }
    }

    /// `Path.toRealPath()`: the model has no links, so an absolute path in
    /// normal form that exists is its own real path. JDK 25 (`realpath(3)`)
    /// throws `NoSuchFileException(<path>)` for a missing path and
    /// `FileSystemException("<path>: Not a directory")` for one below a regular
    /// file. A relative path (the model has no working directory to resolve
    /// it against), a `.`/`..` segment and a `LinkOption` are counted.
    fn real_path(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        options: Option<&Value>,
    ) -> Result<Option<Value>, NativeError> {
        let Some(receiver @ Value::Ref(Some(_))) = receiver else {
            return Ok(None);
        };
        let path = path_text(receiver).to_owned();
        if path.starts_with(library_dispatch::MOD_ROOT_PREFIX) {
            return Ok(None);
        }
        let normal = path.starts_with('/')
            && (path == "/"
                || path[1..]
                    .split('/')
                    .all(|segment| !matches!(segment, "" | "." | "..")));
        if !normal || !self.no_link_options(options) {
            return self.host.unknown_operand_call(r).map(Some);
        }
        if self.file_kind(&path) != FileKind::Missing {
            return Ok(Some(path_value(&path)));
        }
        let mut ancestor = path.as_str();
        while let Some((parent, _)) = ancestor.rsplit_once('/') {
            if parent.is_empty() {
                break;
            }
            if self.file_kind(parent) == FileKind::RegularFile {
                return Err(thrown(
                    "java/nio/file/FileSystemException",
                    Some(format!("{path}: Not a directory")),
                ));
            }
            ancestor = parent;
        }
        Err(thrown("java/nio/file/NoSuchFileException", Some(path)))
    }

    /// Whether `options`, a `LinkOption[]`, is empty. A `LinkOption` (the
    /// model has no constant for one, so the jar's value never reaches here)
    /// or a null array is counted by the caller.
    fn no_link_options(&mut self, options: Option<&Value>) -> bool {
        match options {
            None => true,
            Some(Value::Ref(None)) => false,
            Some(value) => self.array(value).is_ok_and(|items| items.is_empty()),
        }
    }

    fn read_attributes(&mut self, r: &MethodRef, args: &[Value]) -> Result<Value, NativeError> {
        let path = match StdHost::argument(r, args, 0)? {
            Value::Ref(None) => return Err(null_path()),
            value => path_text(value).to_owned(),
        };
        match StdHost::argument(r, args, 1)? {
            Value::Ref(None) => return Err(thrown("java/lang/NullPointerException", None)),
            Value::Ref(Some(class)) if class == BASIC => {}
            Value::Ref(Some(class)) if class == POSIX => return self.host.unknown_operand_call(r),
            _ => return Err(thrown("java/lang/UnsupportedOperationException", None)),
        }
        if !self.no_link_options(args.get(2)) {
            return self.host.unknown_operand_call(r);
        }
        let directory = match self.file_kind(&path) {
            FileKind::Missing => {
                return Err(thrown("java/nio/file/NoSuchFileException", Some(path)));
            }
            kind => kind == FileKind::Directory,
        };
        let snapshot = self.new_object(ATTRIBUTES_CLASS);
        if let Some(marker) = expect_ref(&snapshot) {
            self.fields.insert(
                format!("{marker}|{DIRECTORY_FIELD}"),
                Value::Int(i32::from(directory)),
            );
        }
        Ok(snapshot)
    }

    /// A call on a snapshot `read_attributes` made.
    fn attributes_call(
        &mut self,
        r: &MethodRef,
        marker: &str,
    ) -> Result<Option<Value>, NativeError> {
        let directory = matches!(
            self.fields.get(&format!("{marker}|{DIRECTORY_FIELD}")),
            Some(Value::Int(1))
        );
        Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("isDirectory", "()Z") => Value::Int(i32::from(directory)),
            ("isRegularFile", "()Z") => Value::Int(i32::from(!directory)),
            ("isSymbolicLink" | "isOther", "()Z") => Value::Int(0),
            // Bytes, times and the inode are not the model's to know.
            ("size", "()J")
            | (
                "lastModifiedTime" | "lastAccessTime" | "creationTime",
                "()Ljava/nio/file/attribute/FileTime;",
            )
            | ("fileKey", "()Ljava/lang/Object;") => {
                return self.host.unknown_operand_call(r).map(Some);
            }
            _ => return Ok(None),
        }))
    }
}
