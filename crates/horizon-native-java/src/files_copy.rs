//! `Files.copy(InputStream, Path, CopyOption...)`, as JDK 25 runs it, into the
//! file model `Files`' writers already use (`invoke_io`, `open_options.rs`).
//!
//! night-config's `FileNotFoundAction.copyData` writes a config's bundled
//! defaults to disk with it before parsing the file. Forge Config API Port's
//! `ForgeConfigApiPortConfig.loadFrom` uses that action, so on a Fabric run
//! that runs the port's own classes (#124) the call was counted, no file was
//! made, and the parser's open threw `NoSuchFileException` (cropsloverain,
//! scholar, ArmorPoser).
//!
//! JDK 25's body (`Files.java`, Temurin 25.0.5+8 `src.zip`):
//!
//! 1. `Objects.requireNonNull(in)`: a null stream is an NPE with no message;
//! 2. each option: `REPLACE_EXISTING` sets the flag, a null throws
//!    `NullPointerException("options contains 'null'")`, any other throws
//!    `UnsupportedOperationException(opt + " not supported")`;
//! 3. `REPLACE_EXISTING` deletes the target if it exists;
//! 4. `newOutputStream(target, CREATE_NEW, WRITE)`: an existing target throws
//!    `FileAlreadyExistsException` with the path as its message;
//! 5. `in.transferTo(out)`: the stream's remaining bytes, whose count is the
//!    `long` answer, and the stream is left at its end.
//!
//! The source is a stream the host holds bytes for (a jar resource opened by
//! `getResourceAsStream`). Any other stream, and an option this model has no
//! constant for (`LinkOption.NOFOLLOW_LINKS`), is counted. As the model's other
//! writers do, the target's parent directory is not checked (the JDK throws
//! `NoSuchFileException` when it is missing), and the bytes are kept as text.

use super::*;

const COPY_OPTION: &str = "java/nio/file/StandardCopyOption";

/// `StandardCopyOption`'s constants, in JDK 25's declaration order.
pub(super) const STANDARD_COPY_OPTIONS: &[&str] =
    &["REPLACE_EXISTING", "COPY_ATTRIBUTES", "ATOMIC_MOVE"];

fn thrown(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message,
        frame: None,
    }
}

impl<H: Host> Runtime<'_, H> {
    /// `Files.copy(InputStream, Path, CopyOption...)`; `Ok(None)` for any other
    /// call.
    pub(super) fn invoke_files_copy(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str())
            != (
                "java/nio/file/Files",
                "copy",
                "(Ljava/io/InputStream;Ljava/nio/file/Path;[Ljava/nio/file/CopyOption;)J",
            )
        {
            return Ok(None);
        }
        let Some(Value::Ref(Some(stream))) = args.first().cloned() else {
            return Err(thrown("java/lang/NullPointerException", None));
        };
        let Some(target) = args.get(1).and_then(|value| self.io_path_of(value)) else {
            return Ok(None);
        };
        let options = match args.get(2) {
            Some(value @ Value::Ref(Some(_))) => self.array(value)?.clone(),
            _ => Vec::new(),
        };
        let mut replace_existing = false;
        for option in &options {
            let name = match option {
                Value::Ref(None) => {
                    return Err(thrown(
                        "java/lang/NullPointerException",
                        Some("options contains 'null'".into()),
                    ));
                }
                Value::Ref(Some(marker)) => marker
                    .strip_prefix("enum/")
                    .and_then(|rest| rest.strip_prefix(COPY_OPTION))
                    .and_then(|rest| rest.strip_prefix('/'))
                    .filter(|name| STANDARD_COPY_OPTIONS.contains(name)),
                _ => None,
            };
            match name {
                Some("REPLACE_EXISTING") => replace_existing = true,
                Some(other) => {
                    return Err(thrown(
                        "java/lang/UnsupportedOperationException",
                        Some(format!("{other} not supported")),
                    ));
                }
                None => return self.host.unknown_operand_call(r).map(Some),
            }
        }
        // Only a stream the host holds bytes for can be copied; a zero-length
        // read asks without moving the stream.
        if self.host.resource_stream_bytes(&stream, Some(0)).is_none() {
            return self.host.unknown_operand_call(r).map(Some);
        }
        if replace_existing {
            // `deleteIfExists(target)` first (`watch_service.rs`).
            self.watch_deleted(&target);
            self.io_files.remove(&target);
        }
        if self.io_files.contains_key(&target) {
            return Err(thrown(
                "java/nio/file/FileAlreadyExistsException",
                Some(target),
            ));
        }
        let Some(bytes) = self.host.resource_stream_bytes(&stream, None) else {
            return self.host.unknown_operand_call(r).map(Some);
        };
        let count = bytes.len() as i64;
        // `newOutputStream(target, CREATE_NEW, WRITE)`, then the bytes.
        self.watch_change(&target, watch_service::Change::Create);
        if count > 0 {
            self.watch_change(&target, watch_service::Change::Modify);
        }
        self.io_files
            .insert(target, String::from_utf8_lossy(&bytes).into_owned());
        Ok(Some(Value::Long(count)))
    }
}
