//! The probe runs every server stage it is given, in order, the way Fabric
//! Loader's `Hooks.startServer` runs `main` and then `server`, and labels the
//! run with the game's own verdict (`verdict.rs`): a stage that fails while
//! another ran is `ran-partial`, never `ran-clean`.

#[path = "support/scratch.rs"]
mod scratch;

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use zip::write::SimpleFileOptions;

/// A Fabric jar whose `main` entrypoint records that it ran and whose
/// `server` entrypoint either returns or throws.
fn two_stage_jar(tag: &str, server_throws: bool) -> scratch::Scratch {
    let root = scratch::Scratch::new(&format!("stages-{tag}"));
    let src = root.join("src/demo");
    let classes = root.join("classes");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&classes).unwrap();
    fs::write(
        src.join("Common.java"),
        "package demo;\npublic final class Common { public static int ran; public void onInitialize() { ran++; } }\n",
    )
    .unwrap();
    let body = if server_throws {
        // One call the runtime does not model, counted before the throw.
        "java.util.concurrent.locks.LockSupport.parkNanos(1L); throw new IllegalStateException(\"server stage\");"
    } else {
        "if (Common.ran != 1) throw new IllegalStateException(\"main did not run first\");"
    };
    fs::write(
        src.join("Server.java"),
        format!("package demo;\npublic final class Server {{ public void onInitializeServer() {{ {body} }} }}\n"),
    )
    .unwrap();
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let status = Command::new(javac)
        .args(["--release", "17", "-d"])
        .arg(&classes)
        .arg(src.join("Common.java"))
        .arg(src.join("Server.java"))
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");
    let jar = root.join("stages.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar).unwrap());
    let options = SimpleFileOptions::default();
    zip.start_file("fabric.mod.json", options).unwrap();
    zip.write_all(
        br#"{"id":"stages","version":"1.0","entrypoints":{"main":["demo.Common"],"server":["demo.Server"]}}"#,
    )
    .unwrap();
    for class in ["Common", "Server"] {
        zip.start_file(format!("demo/{class}.class"), options)
            .unwrap();
        zip.write_all(&fs::read(classes.join(format!("demo/{class}.class"))).unwrap())
            .unwrap();
    }
    zip.finish().unwrap();
    root.into_path(jar)
}

fn probe(jar: &Path, kinds: &str) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_horizon-native-java"))
        .arg("--json")
        .arg(jar)
        .arg(kinds)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .rev()
        .find(|line| line.starts_with('{'))
        .unwrap();
    serde_json::from_str(line).unwrap()
}

#[test]
fn both_server_stages_run_in_loader_order() {
    let report = probe(&two_stage_jar("ok", false), "main,server");
    assert_eq!(report["status"], "ok", "{report}");
    assert_eq!(report["verdict"], "ran-clean");
    assert_eq!(
        report["entrypoints"],
        serde_json::json!(["demo.Common", "demo.Server"])
    );
}

#[test]
fn a_failing_stage_beside_a_running_one_is_partial() {
    let report = probe(&two_stage_jar("partial", true), "main,server");
    assert_eq!(report["status"], "ok", "{report}");
    assert_eq!(report["verdict"], "ran-partial");
    let failed = report["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 1);
    assert!(
        failed[0]
            .as_str()
            .unwrap()
            .starts_with("server demo.Server"),
        "{failed:?}"
    );

    // The single `server` stage alone is what the census probed before: it
    // missed `main` entirely, and here it fails outright.
    let report = probe(&two_stage_jar("server-only", true), "server");
    assert_eq!(report["status"], "error");
    // What the stage counted before it threw stays in the report: a refused
    // run's unknown calls are what explain it.
    assert_eq!(report["unknown_host_call_count"], 1, "{report}");
    assert_eq!(
        report["unknown_host_calls"],
        serde_json::json!([["java/util/concurrent/locks/LockSupport#parkNanos(J)V", 1]])
    );
}
