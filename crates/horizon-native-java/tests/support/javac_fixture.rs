#![allow(dead_code)]

//! A probe jar compiled at test time with `javac`, for the JDK and game model
//! fixtures. Only the `demo/` classes ship, so every call to a stubbed
//! platform class reaches the host's model, as it does for a real mod.
//!
//! Include it with `#[path = "support/javac_fixture.rs"] mod javac_fixture;`.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

pub struct Fixture {
    root: PathBuf,
    pub jar: NativeJar,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

impl Fixture {
    /// Compile `sources` (`(path, text)`, e.g. `("demo/Probe.java", ..)`) with
    /// `javac --release 21` and pack the `demo/` classes as a Fabric jar with
    /// `demo.Probe` as its `main` entrypoint.
    pub fn build(test: &str, sources: &[(&str, &str)]) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "horizon-native-java-{test}-{}-{unique}",
            std::process::id()
        ));
        let classes = root.join("classes");
        let javac = std::env::var_os("JAVA_HOME")
            .map(|home| PathBuf::from(home).join("bin/javac"))
            .unwrap_or_else(|| "javac".into());
        let mut javac = Command::new(javac);
        javac.args(["--release", "21", "-d"]).arg(&classes);
        for (path, source) in sources {
            let file = root.join("src").join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, source).unwrap();
            javac.arg(file);
        }
        assert!(javac.status().unwrap().success(), "javac failed for {test}");

        let jar_path = root.join("probe.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"probe-fixture","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
        )
        .unwrap();
        for class in files_under(&classes.join("demo")) {
            let entry = class.strip_prefix(&classes).unwrap().to_string_lossy();
            zip.start_file(entry.replace('\\', "/"), SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&fs::read(class).unwrap()).unwrap();
        }
        zip.finish().unwrap();
        let jar = NativeJar::from_path(&jar_path).unwrap();
        Self { root, jar }
    }

    /// Run the static `String demo.Probe.<method>()` on a seeded host.
    pub fn text(&self, method: &str) -> String {
        let mut host = self.jar.seed_host(StdHost::new());
        self.text_on(&mut host, method)
    }

    /// [`Self::text`] on a host the caller prepared.
    pub fn text_on(&self, host: &mut StdHost, method: &str) -> String {
        match self
            .jar
            .invoke_method_with_args(
                "demo/Probe",
                method,
                "()Ljava/lang/String;",
                None,
                &[],
                host,
            )
            .unwrap_or_else(|error| panic!("{method} failed: {error}"))
        {
            Value::Ref(Some(text)) => text,
            other => panic!("{method} did not return a string: {other:?}"),
        }
    }
}
