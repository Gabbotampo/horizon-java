//! The two Commons IO file-text calls a config bootstrap writes its defaults
//! through.
//!
//! BiomesOPlenty checks for its `biome_toggles.json`, writes the defaults
//! when it is absent, then reads the file back and parses it. The runtime's
//! virtual file table is the only file system a jar may touch, so these two
//! calls answer from it: `write` stores the Java text, `readFileToString`
//! returns it. A read of a file the host does not hold raises the
//! `IOException` the real call raises instead of answering an empty string.

use crate::runtime::{Host, MethodRef, NativeError, Runtime, Value, expect_ref};

const FILE_UTILS: &str = "org/apache/commons/io/FileUtils";

impl<H: Host> Runtime<'_, H> {
    pub(super) fn invoke_commons_io(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.owner != FILE_UTILS {
            return Ok(None);
        }
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("readFileToString", "(Ljava/io/File;)Ljava/lang/String;")
            | (
                "readFileToString",
                "(Ljava/io/File;Ljava/nio/charset/Charset;)Ljava/lang/String;",
            ) => {
                let Some(path) = args.first().and_then(|value| self.io_path_of(value)) else {
                    return Ok(None);
                };
                match self.io_text(&path) {
                    Some(text) => Ok(Some(Value::Ref(Some(text)))),
                    None => Err(NativeError::UncaughtException {
                        class: "java/io/FileNotFoundException".into(),
                        message: Some(path),
                        frame: None,
                    }),
                }
            }
            ("write", "(Ljava/io/File;Ljava/lang/CharSequence;)V")
            | ("writeStringToFile", "(Ljava/io/File;Ljava/lang/String;)V")
            | (
                "writeStringToFile",
                "(Ljava/io/File;Ljava/lang/String;Ljava/nio/charset/Charset;)V",
            ) => {
                let Some(path) = args.first().and_then(|value| self.io_path_of(value)) else {
                    return Ok(None);
                };
                let Some(text) = args.get(1).and_then(expect_ref) else {
                    return Ok(None);
                };
                self.io_files.insert(path, text.clone());
                Ok(Some(Value::Void))
            }
            _ => Ok(None),
        }
    }

    /// The text of a virtual file, if the host holds one at `path`.
    fn io_text(&self, path: &str) -> Option<String> {
        self.io_files.get(path).cloned()
    }
}
