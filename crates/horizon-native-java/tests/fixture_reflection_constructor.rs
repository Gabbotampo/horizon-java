//! `Class.getConstructor` answers exactly the constructors the class declares.
//!
//! Balm's `ModBusEventRegisters.getRegistrations` asks for a
//! `Registrations(String)` constructor, catches `NoSuchMethodException`, and
//! builds the no-argument one instead. The old lookup invented the requested
//! signature, so the fallback constructor ran with a `String` and was refused
//! as a missing method. The expected words below are what `java` prints for
//! the same probe.

#[path = "support/scratch.rs"]
mod scratch;

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, process::Command};
use zip::write::SimpleFileOptions;

const PROBE: &str = r#"package demo;

import java.lang.reflect.Constructor;

public final class ConstructorProbe {
    public static final class OnlyDefault {
        public OnlyDefault() {}
    }

    public static String run() {
        String absent;
        try {
            OnlyDefault.class.getConstructor(String.class).newInstance("balm");
            absent = "wrong";
        } catch (NoSuchMethodException error) {
            absent = "missing";
        } catch (ReflectiveOperationException error) {
            absent = "other";
        }
        String created;
        try {
            Constructor<OnlyDefault> constructor = OnlyDefault.class.getConstructor();
            created = constructor.newInstance() == null ? "null" : "created";
        } catch (ReflectiveOperationException error) {
            created = "failed";
        }
        return absent + "|" + created;
    }
}
"#;

#[test]
fn source_built_probe_refuses_an_undeclared_constructor() {
    let root = scratch::Scratch::new("reflection-constructor");
    let src = root.join("src/demo");
    let classes = root.join("classes");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&classes).unwrap();
    let probe = src.join("ConstructorProbe.java");
    fs::write(&probe, PROBE).unwrap();

    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| std::path::PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let status = Command::new(javac)
        .arg("--release")
        .arg("17")
        .arg("-d")
        .arg(&classes)
        .arg(&probe)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");

    let jar_path = root.join("constructor-probe.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    for class in files_under(&classes.join("demo")) {
        let name = class
            .strip_prefix(&classes)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(class).unwrap()).unwrap();
    }
    zip.finish().unwrap();

    let jar = NativeJar::from_path(&jar_path).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    let value = jar
        .invoke_method(
            "demo.ConstructorProbe",
            "run",
            "()Ljava/lang/String;",
            None,
            &mut host,
        )
        .unwrap();
    assert_eq!(value, Value::Ref(Some("missing|created".into())));

    let _ = fs::remove_dir_all(root);
}

fn files_under(root: &std::path::Path) -> Vec<std::path::PathBuf> {
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
