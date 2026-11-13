//! `Files.move(Path, Path, CopyOption...)` and `Files.copy(Path, Path,
//! CopyOption...)` in the file model, as JDK 25's Unix provider runs them.
//!
//! The host used to answer both with the target path and do nothing, uncounted.
//! night-config writes a config to a temporary sibling and then moves it into
//! place (`ConfigWriter.write(.., WritingMode.REPLACE_ATOMIC)`), so the move
//! silently lost the file: Forge Config API Port's `ConfigTracker.loadConfig`
//! then read nothing back and threw `Failed to create default config file`
//! (scholar, particular, ArmorPoser on the #124 stack).
//!
//! JDK 25 (`sun.nio.fs.UnixFileSystem`, Temurin 25.0.5+8 `src.zip`):
//!
//! * `move` options: `ATOMIC_MOVE`, `REPLACE_EXISTING`, `NOFOLLOW_LINKS`
//!   (ignored); null throws a bare `NullPointerException`, anything else
//!   `UnsupportedOperationException("Unsupported option: <opt>")`. `copy`
//!   options: `REPLACE_EXISTING`, `NOFOLLOW_LINKS`, `COPY_ATTRIBUTES`; anything
//!   else `UnsupportedOperationException("Unsupported copy option: <opt>")`.
//! * An atomic move is a `rename`: it replaces an existing target, and a
//!   missing source throws `NoSuchFileException("<source> -> <target>")`.
//! * Otherwise a missing source throws `NoSuchFileException("<source>")`; a
//!   target that is the source is left alone; an existing target throws
//!   `FileAlreadyExistsException("<target>")` without `REPLACE_EXISTING` and is
//!   replaced with it. `move` removes the source; both answer the target.
//!
//! The model keeps files (and directories, as empty entries) by path, so a path
//! is "the same file" when it is the same path. A source the model does not
//! hold but a jar resource answers, and a `LinkOption`, are counted.

use super::*;

fn thrown(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message,
        frame: None,
    }
}

impl<H: Host> Runtime<'_, H> {
    /// `Files.move`/`Files.copy` between two paths; `Ok(None)` for any other
    /// call.
    pub(super) fn invoke_files_move(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let moving = match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            (
                "java/nio/file/Files",
                "move",
                "(Ljava/nio/file/Path;Ljava/nio/file/Path;[Ljava/nio/file/CopyOption;)Ljava/nio/file/Path;",
            ) => true,
            (
                "java/nio/file/Files",
                "copy",
                "(Ljava/nio/file/Path;Ljava/nio/file/Path;[Ljava/nio/file/CopyOption;)Ljava/nio/file/Path;",
            ) => false,
            _ => return Ok(None),
        };
        let (Some(source), Some(target)) = (
            args.first().and_then(|value| self.io_path_of(value)),
            args.get(1).and_then(|value| self.io_path_of(value)),
        ) else {
            return Ok(None);
        };
        let options = match args.get(2) {
            Some(value @ Value::Ref(Some(_))) => self.array(value)?.clone(),
            _ => Vec::new(),
        };
        let (mut atomic, mut replace, mut attributes) = (false, false, false);
        for option in &options {
            let name = match option {
                Value::Ref(None) => return Err(thrown("java/lang/NullPointerException", None)),
                Value::Ref(Some(marker)) => marker
                    .strip_prefix("enum/java/nio/file/StandardCopyOption/")
                    .filter(|name| crate::runtime::files_copy::STANDARD_COPY_OPTIONS.contains(name)),
                _ => None,
            };
            match (name, moving) {
                (Some("REPLACE_EXISTING"), _) => replace = true,
                (Some("ATOMIC_MOVE"), true) => atomic = true,
                (Some("COPY_ATTRIBUTES"), false) => attributes = true,
                (Some(other), true) => {
                    return Err(thrown(
                        "java/lang/UnsupportedOperationException",
                        Some(format!("Unsupported option: {other}")),
                    ));
                }
                (Some(other), false) => {
                    return Err(thrown(
                        "java/lang/UnsupportedOperationException",
                        Some(format!("Unsupported copy option: {other}")),
                    ));
                }
                // A `LinkOption` or anything else the model has no constant for.
                (None, _) => return self.host.unknown_operand_call(r).map(Some),
            }
        }
        let Some(text) = self.io_files.get(&source).cloned() else {
            if library_dispatch::resource_text(&mut *self.host, &source).is_some() {
                return self.host.unknown_operand_call(r).map(Some);
            }
            let message = if atomic {
                format!("{source} -> {target}")
            } else {
                source
            };
            return Err(thrown("java/nio/file/NoSuchFileException", Some(message)));
        };
        let answer = args.get(1).cloned().unwrap_or(Value::Ref(None));
        if !atomic && self.io_files.contains_key(&target) {
            if source == target {
                return Ok(Some(answer));
            }
            if !replace {
                return Err(thrown(
                    "java/nio/file/FileAlreadyExistsException",
                    Some(target),
                ));
            }
        }
        if source == target {
            return Ok(Some(answer));
        }
        // A directory stays one (`file_attributes.rs`): a copy makes an empty
        // directory, a move renames it.
        let directory = self.io_dirs.contains(&source);
        self.watch_move_or_copy(
            &source,
            &target,
            moving,
            atomic,
            directory,
            attributes,
            text.is_empty(),
        );
        if moving {
            self.io_dirs.remove(&source);
            self.io_files.remove(&source);
        }
        if directory {
            self.io_dirs.insert(target.clone());
        } else {
            self.io_dirs.remove(&target);
        }
        self.io_files.insert(target, text);
        Ok(Some(answer))
    }
}
