//! Commons IO's file-text pair over the runtime's virtual file table.
//!
//! BiomesOPlenty writes its default `biome_toggles.json` with
//! `FileUtils.write`, then reads the same file back with
//! `readFileToString` and parses it. `java` with the real library round-trips
//! the text; the expectations below are that contract, and a read of a file
//! nobody wrote is the `IOException` the library raises.

#[path = "support/scratch.rs"]
mod scratch;

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, process::Command};
use zip::write::SimpleFileOptions;

const FILE_UTILS_STUB: &str = r#"package org.apache.commons.io;
import java.io.File;
import java.io.IOException;
public class FileUtils {
    public static void write(File file, CharSequence data) throws IOException {}
    public static String readFileToString(File file) throws IOException { return null; }
}
"#;

const PROBE: &str = r#"package demo;

import java.io.File;
import java.io.IOException;
import org.apache.commons.io.FileUtils;

public final class IoProbe {
    public static String run() throws IOException {
        File config = new File("/horizon/config/demo/toggles.json");
        FileUtils.write(config, "{\"minecraft:dirt\":true}");
        String readBack = FileUtils.readFileToString(config);
        String missing;
        try {
            FileUtils.readFileToString(new File("/horizon/config/demo/absent.json"));
            missing = "no-throw";
        } catch (IOException error) {
            missing = "missing";
        }
        return readBack + "|" + missing;
    }
}
"#;

#[test]
fn source_built_probe_round_trips_commons_io_text() {
    let root = scratch::Scratch::new("commons-io");
    let src = root.join("src/demo");
    let stub_src = root.join("src/stubs/org/apache/commons/io");
    let classes = root.join("classes");
    // Compiled for `javac` only: the probe must reach the host's model, not
    // a library class in its own jar.
    let stubs = root.join("stubs");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&stub_src).unwrap();
    let probe = src.join("IoProbe.java");
    fs::write(&probe, PROBE).unwrap();
    let stub = stub_src.join("FileUtils.java");
    fs::write(&stub, FILE_UTILS_STUB).unwrap();

    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| std::path::PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    fs::create_dir_all(&classes).unwrap();
    fs::create_dir_all(&stubs).unwrap();
    let status = Command::new(&javac)
        .arg("--release")
        .arg("17")
        .arg("-d")
        .arg(&stubs)
        .arg(&stub)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed for the stub");
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

    let jar_path = root.join("io-probe.jar");
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
            "demo.IoProbe",
            "run",
            "()Ljava/lang/String;",
            None,
            &mut host,
        )
        .unwrap();
    assert_eq!(
        value,
        Value::Ref(Some("{\"minecraft:dirt\":true}|missing".into()))
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
