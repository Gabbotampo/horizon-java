//! NightConfig's `getOrElse` on the empty config the runtime holds: the
//! caller's default is the answer, which is what the library does for a file
//! that declares no value.
//!
//! TerraBlender's `Config.addNumber` and BiomesOPlenty's generation config
//! both read through it; the stub interfaces are compiled for `javac` only,
//! so the probe reaches the host model as it reaches the real library.

#[path = "support/scratch.rs"]
mod scratch;

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, process::Command};
use zip::write::SimpleFileOptions;

const STUBS: &[(&str, &str)] = &[(
    "com/electronwill/nightconfig/core/CommentedConfig.java",
    r#"package com.electronwill.nightconfig.core;
import java.util.List;
public interface CommentedConfig {
    <T> T getOrElse(String key, T defaultValue);
    <T> T getOrElse(List<String> key, T defaultValue);
}
"#,
)];

const PROBE: &str = r#"package demo;

import com.electronwill.nightconfig.core.CommentedConfig;
import java.util.List;

public final class ConfigProbe {
    public static String run(CommentedConfig config) {
        int size = (Integer) config.getOrElse("general.overworld_region_size", Integer.valueOf(3));
        String name = (String) config.getOrElse(List.of("general", "name"), "fallback");
        return size + "|" + name;
    }
}
"#;

#[test]
fn source_built_probe_reads_nightconfig_defaults() {
    let root = scratch::Scratch::new("night-config");
    let src = root.join("src");
    let classes = root.join("classes");
    let mut sources = Vec::new();
    for (relative, body) in STUBS {
        let path = src.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
        sources.push(path);
    }
    fs::create_dir_all(src.join("demo")).unwrap();
    let probe = src.join("demo/ConfigProbe.java");
    fs::write(&probe, PROBE).unwrap();
    sources.push(probe);

    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| std::path::PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    fs::create_dir_all(&classes).unwrap();
    let status = Command::new(javac)
        .arg("--release")
        .arg("17")
        .arg("-d")
        .arg(&classes)
        .args(&sources)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");

    let jar_path = root.join("config-probe.jar");
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
    // The config object a loader hands the mod (the probe used to call
    // `getOrElse` on a local `null`, which the JVM answers with a
    // `NullPointerException`).
    let config = Value::Ref(Some(
        "com/electronwill/nightconfig/core/CommentedConfig#0".into(),
    ));
    let value = jar
        .invoke_method_with_args(
            "demo.ConfigProbe",
            "run",
            "(Lcom/electronwill/nightconfig/core/CommentedConfig;)Ljava/lang/String;",
            None,
            &[config],
            &mut host,
        )
        .unwrap();
    assert_eq!(value, Value::Ref(Some("3|fallback".into())));
    assert_eq!(
        host.unknown_host_calls(),
        Vec::new(),
        "the defaults are modelled, not neutral host answers"
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
