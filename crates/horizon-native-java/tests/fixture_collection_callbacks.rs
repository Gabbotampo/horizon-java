//! `Collection.forEach`/`removeIf` run the jar's own callback, and a list's
//! out-of-range access throws the JDK's `IndexOutOfBoundsException`.
//!
//! The expected strings come from `java` running the same `Probe` methods on a
//! JVM (JDK 27, `--release 21`), not from this interpreter: `forEach` order,
//! `removeIf`'s kept order and answer, and the four exception messages appear
//! verbatim in the JDK's `Objects.checkIndex` and `ArrayList.rangeCheckForAdd`.

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

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;

public class Probe {
    static String collectForEach() {
        List<String> values = new ArrayList<>();
        values.add("a");
        values.add("b");
        values.add("c");
        StringBuilder out = new StringBuilder();
        values.forEach(value -> out.append(value));
        // The map's order is its iteration order.
        Map<String, Integer> map = new LinkedHashMap<>();
        map.put("x", 1);
        map.put("y", 2);
        map.forEach((key, value) -> out.append(key).append(value));
        return out.toString();
    }

    static String removeIf() {
        List<Integer> values = new ArrayList<>();
        for (int i = 1; i <= 5; i++) {
            values.add(i);
        }
        boolean removed = values.removeIf(value -> value % 2 == 0);
        StringBuilder out = new StringBuilder();
        out.append(removed).append(':');
        values.forEach(value -> out.append(value));
        boolean none = values.removeIf(value -> value > 100);
        return out.append(':').append(none).toString();
    }

    static String anonymousConsumer() {
        List<String> values = new ArrayList<>();
        values.add("p");
        values.add("q");
        StringBuilder out = new StringBuilder();
        // An object of a jar class implementing the interface, not an indy lambda.
        values.forEach(new Consumer<String>() {
            @Override
            public void accept(String value) {
                out.append(value.toUpperCase());
            }
        });
        return out.toString();
    }

    static String bounds() {
        List<String> values = new ArrayList<>();
        values.add("only");
        StringBuilder out = new StringBuilder();
        out.append(getMessage(values, 1)).append('|');
        out.append(setMessage(values, 1)).append('|');
        out.append(removeMessage(values, 1)).append('|');
        out.append(addMessage(values, 2)).append('|');
        try {
            values.get(-1);
            out.append("no throw");
        } catch (RuntimeException thrown) {
            out.append(thrown.getClass().getName()).append(' ').append(thrown.getMessage());
        }
        return out.toString();
    }

    static String getMessage(List<String> values, int index) {
        try {
            values.get(index);
            return "no throw";
        } catch (RuntimeException thrown) {
            return thrown.getClass().getName() + ' ' + thrown.getMessage();
        }
    }

    static String setMessage(List<String> values, int index) {
        try {
            values.set(index, "x");
            return "no throw";
        } catch (RuntimeException thrown) {
            return thrown.getClass().getName() + ' ' + thrown.getMessage();
        }
    }

    static String removeMessage(List<String> values, int index) {
        try {
            values.remove(index);
            return "no throw";
        } catch (RuntimeException thrown) {
            return thrown.getClass().getName() + ' ' + thrown.getMessage();
        }
    }

    static String addMessage(List<String> values, int index) {
        try {
            values.add(index, "x");
            return "no throw";
        } catch (RuntimeException thrown) {
            return thrown.getClass().getName() + ' ' + thrown.getMessage();
        }
    }
}
"##;

const ENTRY: &str = r##"package demo;

public class Entry {
    public void onInitialize() {
        Probe.collectForEach();
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
            "horizon-native-java-collections-{test}-{}-{unique}",
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

        let jar_path = fixture.root.join("collections.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"collections-fixture","version":"1.0","entrypoints":{"main":"demo.Entry"}}"#,
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
fn for_each_runs_the_lambda_in_iteration_order_for_collections_and_maps() {
    let fixture = Fixture::build("for-each");
    assert_eq!(fixture.text("collectForEach"), "abcx1y2");
}

#[test]
fn remove_if_tests_every_element_and_keeps_the_rest_in_order() {
    let fixture = Fixture::build("remove-if");
    assert_eq!(fixture.text("removeIf"), "true:135:false");
}

#[test]
fn for_each_runs_an_anonymous_class_that_implements_the_interface() {
    let fixture = Fixture::build("anonymous");
    assert_eq!(fixture.text("anonymousConsumer"), "PQ");
}

#[test]
fn list_bounds_raise_the_jdks_index_exception() {
    let fixture = Fixture::build("bounds");
    assert_eq!(
        fixture.text("bounds"),
        "java.lang.IndexOutOfBoundsException Index 1 out of bounds for length 1|\
         java.lang.IndexOutOfBoundsException Index 1 out of bounds for length 1|\
         java.lang.IndexOutOfBoundsException Index 1 out of bounds for length 1|\
         java.lang.IndexOutOfBoundsException Index: 2, Size: 1|\
         java.lang.IndexOutOfBoundsException Index -1 out of bounds for length 1"
    );
}
