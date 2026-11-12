//! `java.util.Locale` construction and queries, and `Thread` identity.
//!
//! `localeFields` expected value is what `java` prints for the same calls on a
//! JVM (JDK 27, `--release 21`): language, country, `toString`, language tag
//! and `hashCode` for a constructed locale and the three constants.
//!
//! `threadIds` is this runtime's contract, not a JVM value: `Thread.getId` is
//! opaque and implementation-defined (JDK 27's main thread is 3, older
//! HotSpot's was 1). This runtime runs one thread, so its main thread is 1;
//! a `Thread` object a jar constructs never runs and gets its own stable id,
//! because two distinct objects must not compare equal.

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

import java.util.Locale;

public class Probe {
    static String describe(Locale locale) {
        return locale.getLanguage() + '|' + locale.getCountry() + '|' + locale.toString()
                + '|' + locale.toLanguageTag() + '|' + locale.hashCode()
                + '|' + locale.equals(Locale.US);
    }

    static String localeFields() {
        StringBuilder out = new StringBuilder();
        out.append(describe(new Locale("en", "US")));
        out.append(';').append(describe(new Locale("EN", "us")));
        out.append(';').append(describe(new Locale("en")));
        out.append(';').append(describe(new Locale("", "US")));
        out.append(';').append(describe(Locale.ROOT));
        out.append(';').append(describe(Locale.ENGLISH));
        return out.toString();
    }

    static String threadIds() {
        long main = Thread.currentThread().getId();
        Thread first = new Thread();
        Thread second = new Thread();
        boolean distinct = first.getId() != main && second.getId() != main
                && first.getId() != second.getId();
        boolean stable = first.getId() == first.getId() && first.threadId() == first.getId();
        return main + ":" + distinct + ":" + stable;
    }
}
"##;

const ENTRY: &str = r##"package demo;

public class Entry {
    public void onInitialize() {
        Probe.localeFields();
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
            "horizon-native-java-locale-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };
        let classes = fixture.root.join("classes");
        let mut javac = Command::new(javac());
        javac.args(["--release", "21", "-d"]).arg(&classes);
        for (path, source) in [("demo/Probe.java", PROBE), ("demo/Entry.java", ENTRY)] {
            let file = fixture.root.join("src").join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, source).unwrap();
            javac.arg(file);
        }
        let status = javac.status().unwrap();
        assert!(status.success(), "javac failed for fixture {test}");

        let jar_path = fixture.root.join("locale.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"locale-fixture","version":"1.0","entrypoints":{"main":"demo.Entry"}}"#,
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

    fn text(&self, method: &str) -> String {
        let mut host = self.jar.seed_host(StdHost::new());
        match self
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
        }
    }
}

#[test]
fn a_constructed_locale_answers_the_jdks_fields_and_hash() {
    let fixture = Fixture::build("fields");
    assert_eq!(
        fixture.text("localeFields"),
        "en|US|en_US|en-US|3118343|true;\
         en|US|en_US|en-US|3118343|true;\
         en||en|en|3241|false;\
         |US|_US|und-US|3742|false;\
         |||und|0|false;\
         en||en|en|3241|false"
    );
}

#[test]
fn the_runtimes_one_thread_has_a_stable_distinct_id() {
    let fixture = Fixture::build("threads");
    assert_eq!(fixture.text("threadIds"), "1:true:true");
}
