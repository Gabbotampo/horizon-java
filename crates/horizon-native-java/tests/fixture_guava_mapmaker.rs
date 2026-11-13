//! Guava's `MapMaker`, compiled from real Java at test time.
//!
//! The stub declares the builder surface a mod compiles against; the runtime
//! answers the calls from `guava_model.rs`. The expected values below are the
//! same program run on a real JVM with guava-33.6.0-jre on the class path
//! (javac 21, `--release 17`):
//!
//! ```text
//! empty=[MapMaker{}]
//! capacity=[MapMaker{initialCapacity=16}]
//! all=[MapMaker{initialCapacity=16, concurrencyLevel=4, keyStrength=weak, valueStrength=weak}]
//! neg=[java.lang.IllegalArgumentException|null]
//! zero=[java.lang.IllegalArgumentException|null]
//! twice=[java.lang.IllegalStateException|initial capacity was already set to 1]
//! weak-twice=[java.lang.IllegalStateException|Key strength was already set to WEAK]
//! map=[null|2|2|true|1|1]            (the real map's class is ConcurrentHashMap)
//! weak=[9|1]
//! putIfAbsent=[null|1|1]
//! ```
//!
//! Named departure: `makeMap()` answers the runtime's map model, not a
//! `ConcurrentHashMap`/`MapMakerInternalMap` instance (`getClass()` on it is
//! the model's marker, so the fixture does not print it), and `weakKeys()`/
//! `weakValues()` are accepted while the map holds its entries strongly — the
//! runtime has one thread and no garbage collector, so the identity is not
//! observable.

use horizon_native_java::{NativeError, NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const MAP_MAKER: &str = r#"package com.google.common.collect;

import java.util.concurrent.ConcurrentMap;

public final class MapMaker {
    public MapMaker() {}
    public MapMaker initialCapacity(int capacity) { return this; }
    public MapMaker concurrencyLevel(int level) { return this; }
    public MapMaker weakKeys() { return this; }
    public MapMaker weakValues() { return this; }
    public <K, V> ConcurrentMap<K, V> makeMap() { return null; }
    public String toString() { return null; }
}
"#;

const PROBE: &str = r#"package demo;

import com.google.common.collect.MapMaker;
import java.util.concurrent.ConcurrentMap;

public final class Probe {
    public static String empty() {
        return new MapMaker().toString();
    }

    public static String capacity() {
        return new MapMaker().initialCapacity(16).toString();
    }

    public static String all() {
        return new MapMaker().initialCapacity(16).concurrencyLevel(4).weakKeys().weakValues().toString();
    }

    public static String negative() {
        try {
            new MapMaker().initialCapacity(-1);
            return "no-iae";
        } catch (RuntimeException error) {
            return error.getClass().getName() + "|" + error.getMessage();
        }
    }

    public static String zero() {
        try {
            new MapMaker().concurrencyLevel(0);
            return "no-iae";
        } catch (RuntimeException error) {
            return error.getClass().getName() + "|" + error.getMessage();
        }
    }

    public static String twice() {
        try {
            new MapMaker().initialCapacity(1).initialCapacity(2);
            return "no-ise";
        } catch (RuntimeException error) {
            return error.getClass().getName() + "|" + error.getMessage();
        }
    }

    public static String weakTwice() {
        try {
            new MapMaker().weakKeys().weakKeys();
            return "no-ise";
        } catch (RuntimeException error) {
            return error.getClass().getName() + "|" + error.getMessage();
        }
    }

    public static String map() {
        ConcurrentMap<String, Integer> map = new MapMaker().initialCapacity(2).makeMap();
        Integer old = map.put("a", 1);
        map.put("b", 2);
        return old + "|" + map.size() + "|" + map.get("b") + "|" + map.containsKey("a")
            + "|" + map.remove("a") + "|" + map.size();
    }

    public static String weak() {
        ConcurrentMap<String, Integer> map = new MapMaker().weakKeys().makeMap();
        map.put("k", 9);
        return map.get("k") + "|" + map.size();
    }

    public static String putIfAbsent() {
        ConcurrentMap<String, Integer> map = new MapMaker().makeMap();
        return map.putIfAbsent("x", 1) + "|" + map.putIfAbsent("x", 2) + "|" + map.get("x");
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
            "horizon-native-java-guava-mapmaker-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };
        let classes = fixture.root.join("classes");
        let sources = [
            ("com/google/common/collect/MapMaker.java", MAP_MAKER),
            ("demo/Probe.java", PROBE),
        ];
        let mut javac = Command::new(javac_path());
        javac.args(["--release", "17", "-d"]).arg(&classes);
        for (path, source) in sources {
            let file = fixture.root.join("src").join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, source).unwrap();
            javac.arg(file);
        }
        let status = javac.status().unwrap();
        assert!(
            status.success(),
            "javac failed for the guava mapmaker fixture {test}"
        );

        let jar_path = fixture.root.join("mapmaker.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"mapmaker-fixture","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
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
fn the_builder_prints_the_options_it_has_set() {
    let fixture = Fixture::build("toString");
    // java: MapMaker{}
    assert_eq!(fixture.text("empty"), "MapMaker{}");
    // java: MapMaker{initialCapacity=16}
    assert_eq!(fixture.text("capacity"), "MapMaker{initialCapacity=16}");
    // java: MapMaker{initialCapacity=16, concurrencyLevel=4, keyStrength=weak, valueStrength=weak}
    assert_eq!(
        fixture.text("all"),
        "MapMaker{initialCapacity=16, concurrencyLevel=4, keyStrength=weak, valueStrength=weak}"
    );
}

#[test]
fn invalid_and_repeated_options_raise_what_guava_raises() {
    let fixture = Fixture::build("options");
    // java: java.lang.IllegalArgumentException|null
    assert_eq!(
        fixture.text("negative"),
        "java.lang.IllegalArgumentException|null"
    );
    // java: java.lang.IllegalArgumentException|null
    assert_eq!(
        fixture.text("zero"),
        "java.lang.IllegalArgumentException|null"
    );
    // java: java.lang.IllegalStateException|initial capacity was already set to 1
    assert_eq!(
        fixture.text("twice"),
        "java.lang.IllegalStateException|initial capacity was already set to 1"
    );
    // java: java.lang.IllegalStateException|Key strength was already set to WEAK
    assert_eq!(
        fixture.text("weakTwice"),
        "java.lang.IllegalStateException|Key strength was already set to WEAK"
    );
}

#[test]
fn make_map_answers_a_working_concurrent_map() {
    let fixture = Fixture::build("map");
    // java: null|2|2|true|1|1
    assert_eq!(fixture.text("map"), "null|2|2|true|1|1");
    // java: 9|1
    assert_eq!(fixture.text("weak"), "9|1");
    // java: null|1|1
    assert_eq!(fixture.text("putIfAbsent"), "null|1|1");
}
