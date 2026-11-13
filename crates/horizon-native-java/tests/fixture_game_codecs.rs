//! `StringRepresentable.fromEnum` builds the non-null codec a class keeps.
//!
//! fzzy_config's `ContextInput` stores the result in a static field and a
//! Kotlin `checkNotNullExpressionValue` rejects a null, so a class
//! initializer that is valid on the JVM failed on the neutral answer. The
//! stub game classes are compiled for `javac` only.

#[path = "support/scratch.rs"]
mod scratch;

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, process::Command};
use zip::write::SimpleFileOptions;

const STUBS: &[(&str, &str)] = &[(
    "net/minecraft/util/StringRepresentable.java",
    r#"package net.minecraft.util;
import java.util.function.Supplier;
public interface StringRepresentable {
    String getSerializedName();
    final class EnumCodec {
        private EnumCodec() {}
    }
    static <T extends Enum<T> & StringRepresentable> EnumCodec fromEnum(Supplier<T[]> values) {
        return new EnumCodec();
    }
}
"#,
)];

const PROBE: &str = r#"package demo;

import java.util.function.Supplier;
import net.minecraft.util.StringRepresentable;

public final class CodecProbe {
    static final StringRepresentable.EnumCodec CODEC = build();

    private static StringRepresentable.EnumCodec build() {
        Supplier<Sample[]> values = () -> Sample.values();
        return StringRepresentable.fromEnum(values);
    }

    public static String run() {
        return (CODEC != null) + "|" + (StringRepresentable.fromEnum(Sample::values) != null);
    }

    enum Sample implements StringRepresentable {
        A;
        public String getSerializedName() { return "a"; }
    }
}
"#;

#[test]
fn source_built_probe_sees_a_non_null_enum_codec() {
    let root = scratch::Scratch::new("game-codecs");
    let src = root.join("src/demo");
    let stub_src = root.join("src/stubs/net/minecraft/util");
    let classes = root.join("classes");
    let stubs = root.join("stubs");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&stub_src).unwrap();
    fs::create_dir_all(&stubs).unwrap();
    fs::create_dir_all(&classes).unwrap();

    let probe = src.join("CodecProbe.java");
    fs::write(&probe, PROBE).unwrap();
    let mut stub_sources = Vec::new();
    for (relative, body) in STUBS {
        let path = root.join("src/stubs").join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
        stub_sources.push(path);
    }

    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| std::path::PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let status = Command::new(&javac)
        .arg("--release")
        .arg("17")
        .arg("-d")
        .arg(&stubs)
        .args(&stub_sources)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed for the stubs");
    let status = Command::new(javac)
        .arg("--release")
        .arg("17")
        .arg("-cp")
        .arg(&stubs)
        .arg("-d")
        .arg(&classes)
        .arg(&probe)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");

    let jar_path = root.join("codec-probe.jar");
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
            "demo.CodecProbe",
            "run",
            "()Ljava/lang/String;",
            None,
            &mut host,
        )
        .unwrap();
    assert_eq!(value, Value::Ref(Some("true|true".into())));
    assert_eq!(
        host.unknown_host_calls(),
        Vec::new(),
        "the factory is modelled, not a neutral answer"
    );

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
