//! The JDK statics a mod's bootstrap reads before the game exists: the file
//! separator constants, the standard streams, the runtime object, compiled
//! patterns and the default filesystem's watch service.
//!
//! Every expected string is what `java -cp` printed for the same `demo.Probe`
//! methods on a JVM, not what the interpreter returns: `/|:|/|:` (the host
//! reports `os.name=Linux`, so the platform constants are that platform's),
//! `true|true|true|false` for the stream identities, `true|true` for the
//! runtime, `a+b|a+b` for the pattern, and `true|true` for an empty watch
//! poll and a key reset.
//!
//! The probe jar is compiled at test time from the source below; no JDK class
//! ships in it, so every call reaches the host's model.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use zip::write::SimpleFileOptions;

#[path = "support/unique_dir.rs"]
mod unique_dir;
use unique_dir::unique_suffix;

const PROBE: &str = r##"package demo;

import java.io.File;
import java.nio.file.FileSystems;
import java.nio.file.Path;
import java.nio.file.StandardWatchEventKinds;
import java.nio.file.WatchKey;
import java.nio.file.WatchService;
import java.util.regex.Pattern;

public class Probe {
    public static String separators() {
        return File.separator + "|" + File.pathSeparator + "|" + File.separatorChar + "|" + File.pathSeparatorChar;
    }

    public static String streams() {
        System.out.println("probe-out");
        System.err.println("probe-err");
        return (System.out != null) + "|" + (System.err != null) + "|" + (System.out == System.out) + "|" + System.out.checkError();
    }

    public static String runtime() {
        return (Runtime.getRuntime() != null) + "|" + (Runtime.getRuntime().availableProcessors() >= 1);
    }

    public static String pattern() {
        Pattern compiled = Pattern.compile("a+b");
        return compiled.pattern() + "|" + compiled;
    }

    public static String watch() throws Exception {
        Path directory = Path.of(".");
        WatchService service = FileSystems.getDefault().newWatchService();
        WatchKey key = directory.register(service, StandardWatchEventKinds.ENTRY_MODIFY);
        WatchKey polled = service.poll();
        boolean reset = key.reset();
        service.close();
        return (polled == null) + "|" + reset;
    }
}
"##;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
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

struct Fixture {
    root: PathBuf,
    jar: NativeJar,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn build(test: &str) -> Self {
        let unique = unique_suffix();
        let root = std::env::temp_dir().join(format!(
            "horizon-native-java-jdk-statics-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };
        let classes = fixture.root.join("classes");
        let source = fixture.root.join("src/demo/Probe.java");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, PROBE).unwrap();
        let status = Command::new(javac())
            .args(["--release", "21", "-d"])
            .arg(&classes)
            .arg(&source)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "javac failed for jdk statics fixture {test}"
        );

        let jar_path = fixture.root.join("statics.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"jdk-statics-fixture","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
        )
        .unwrap();
        for class in files_under(&classes.join("demo")) {
            let entry = class
                .strip_prefix(&classes)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(entry, SimpleFileOptions::default()).unwrap();
            zip.write_all(&fs::read(class).unwrap()).unwrap();
        }
        zip.finish().unwrap();

        fixture.jar = NativeJar::from_path(&jar_path).unwrap();
        fixture
    }

    /// Run one probe method and require that nothing crossed the host
    /// boundary unmodelled: a neutral answer here is the failure the test
    /// exists to catch, even when the returned text looks right.
    fn text(&self, method: &str) -> String {
        let mut host = self.jar.seed_host(StdHost::new());
        let text = match self
            .jar
            .invoke_method_with_args(
                "demo/Probe",
                method,
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            )
            .unwrap_or_else(|error| panic!("{method} failed: {error}"))
        {
            Value::Ref(Some(text)) => text,
            other => panic!("{method} did not return a string: {other:?}"),
        };
        assert_eq!(
            host.unknown_host_calls(),
            Vec::<(String, usize)>::new(),
            "{method} left unmodelled host calls"
        );
        text
    }
}

#[test]
fn file_separator_constants_are_the_linux_platforms() {
    let fixture = Fixture::build("separators");
    assert_eq!(fixture.text("separators"), "/|:|/|:");
}

#[test]
fn the_standard_streams_exist_and_absorb_writes() {
    // The JVM's `System.out.checkError()` is false after a `println`, and the
    // stream is the same object on every read.
    let fixture = Fixture::build("streams");
    assert_eq!(fixture.text("streams"), "true|true|true|false");
}

#[test]
fn the_runtime_object_exists_and_reports_its_processors() {
    let fixture = Fixture::build("runtime");
    assert_eq!(fixture.text("runtime"), "true|true");
}

#[test]
fn compiled_patterns_keep_their_text() {
    let fixture = Fixture::build("pattern");
    assert_eq!(fixture.text("pattern"), "a+b|a+b");
}

#[test]
fn a_watch_service_polls_empty_and_its_key_resets() {
    let fixture = Fixture::build("watch");
    assert_eq!(fixture.text("watch"), "true|true");
}
