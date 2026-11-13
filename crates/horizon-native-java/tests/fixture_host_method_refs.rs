//! Method references to host methods, compiled from real Java at test time.
//!
//! A method reference whose target is a JDK method (`list::clear` as a
//! `Runnable`, `map::size` as a `Supplier`, `String::length` as a `Function`)
//! is a callback marker, and calling the interface's abstract method on it must
//! run the referenced method. The expected values below are the same program on
//! a real JVM (javac 21, `--release 17`):
//!
//! ```text
//! clear=[true]
//! size=[2]
//! add=[true|1]
//! length=[5]
//! nonnull=[true|false]
//! put=[1|9]
//! consumer=[1|q]
//! ```

use horizon_native_java::{NativeError, NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const PROBE: &str = r#"package demo;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.function.BiFunction;
import java.util.function.Consumer;
import java.util.function.Function;
import java.util.function.Predicate;
import java.util.function.Supplier;

public final class Probe {
    public static String clear() {
        List<String> list = new ArrayList<>(List.of("a", "b"));
        Runnable clear = list::clear;
        clear.run();
        return String.valueOf(list.isEmpty());
    }

    public static String size() {
        Map<String, Integer> map = new HashMap<>();
        map.put("x", 1);
        map.put("y", 2);
        Supplier<Integer> size = map::size;
        return String.valueOf(size.get());
    }

    public static String add() {
        List<String> list = new ArrayList<>();
        Function<String, Boolean> add = list::add;
        return add.apply("z") + "|" + list.size();
    }

    public static String length() {
        Function<String, Integer> length = String::length;
        return String.valueOf(length.apply("hello"));
    }

    public static String nonNull() {
        Predicate<Object> nonNull = Objects::nonNull;
        return nonNull.test("x") + "|" + nonNull.test(null);
    }

    public static String put() {
        Map<String, Integer> map = new HashMap<>();
        map.put("x", 1);
        BiFunction<String, Integer, Integer> put = map::put;
        return put.apply("x", 9) + "|" + map.get("x");
    }

    public static String consumer() {
        List<String> list = new ArrayList<>();
        Consumer<String> consumer = list::add;
        consumer.accept("q");
        return list.size() + "|" + list.get(list.size() - 1);
    }
}
"#;

fn javac_path() -> PathBuf {
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
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "horizon-native-java-host-method-refs-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };
        let classes = fixture.root.join("classes");
        let file = fixture.root.join("src").join("demo/Probe.java");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, PROBE).unwrap();
        let status = Command::new(javac_path())
            .args(["--release", "17", "-d"])
            .arg(&classes)
            .arg(&file)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "javac failed for the host method refs fixture {test}"
        );

        let jar_path = fixture.root.join("host-method-refs.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"host-method-refs-fixture","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
        )
        .unwrap();
        for class in files_under(&classes) {
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

    fn call(&self, method: &str) -> Result<Value, NativeError> {
        self.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut StdHost::new(),
        )
    }

    fn text(&self, method: &str) -> String {
        match self.call(method).unwrap() {
            Value::Ref(Some(text)) => text,
            other => panic!("{method} did not return a string: {other:?}"),
        }
    }
}

#[test]
fn a_bound_method_reference_runs_the_host_method() {
    let fixture = Fixture::build("bound");
    // java: true
    assert_eq!(fixture.text("clear"), "true");
    // java: 2
    assert_eq!(fixture.text("size"), "2");
    // java: true|1
    assert_eq!(fixture.text("add"), "true|1");
    // java: 1|9
    assert_eq!(fixture.text("put"), "1|9");
    // java: 1|q
    assert_eq!(fixture.text("consumer"), "1|q");
}

#[test]
fn an_unbound_and_a_static_method_reference_run_too() {
    let fixture = Fixture::build("unbound");
    // java: 5
    assert_eq!(fixture.text("length"), "5");
    // java: true|false
    assert_eq!(fixture.text("nonNull"), "true|false");
}
