//! The loading-stage fixture (`tests/fixtures/neoforge_lifecycle`) run the way the probe and the game
//! load a NeoForge jar: the `@Mod` constructors, `@EventBusSubscriber` injection, then every
//! [`LoadingStage`] followed by its deferred work.
//!
//! The expected lines are pasted from `oracle.jsonl`: the same mods on the real loader-12.0.12 and
//! NeoForge 26.3.0.101 classes on OpenJDK 25.0.5+8 (README.md there says how to rerun it).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use super::LoadingStage;
use crate::runtime::{ExecutionBudget, NativeJar, StdHost};

fn fixture_dir() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    [
        manifest.join("tests/fixtures"),
        manifest.join("../crates/horizon-native-java/tests/fixtures"),
    ]
    .into_iter()
    .find(|dir| dir.join("neoforge_lifecycle/oracle.jsonl").is_file())
    .expect("the fixtures are in crates/horizon-native-java/tests/fixtures")
}

fn java_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            java_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "java") {
            out.push(path);
        }
    }
}

/// The fixture's jar, compiled once per test process: only `life/*`, never the stubs.
pub(crate) fn fixture_jar_bytes() -> &'static [u8] {
    static JAR: OnceLock<Vec<u8>> = OnceLock::new();
    JAR.get_or_init(|| {
        use std::io::Write;
        let fixtures = fixture_dir();
        let scratch = std::env::temp_dir().join(format!(
            "horizon-native-java-neoforge-lifecycle-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&scratch);
        let classes = scratch.join("classes");
        std::fs::create_dir_all(&classes).unwrap();
        let mut sources = vec![
            fixtures.join("neoforge_bus/events/net/neoforged/neoforge/event/RegisterCommandsEvent.java"),
        ];
        java_files(&fixtures.join("neoforge_bus/bus-stubs"), &mut sources);
        for part in ["stubs", "mod"] {
            java_files(&fixtures.join("neoforge_lifecycle").join(part), &mut sources);
        }
        let javac = std::env::var_os("JAVA_HOME")
            .map(|home| PathBuf::from(home).join("bin/javac"))
            .unwrap_or_else(|| "javac".into());
        let status = Command::new(javac)
            .args(["--release", "17", "-nowarn", "-d"])
            .arg(&classes)
            .args(&sources)
            .status()
            .expect("javac runs");
        assert!(status.success(), "javac failed on the lifecycle fixture");
        let mut bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("META-INF/neoforge.mods.toml", options)
                .unwrap();
            zip.write_all(
                b"modLoader = \"javafml\"\n[[mods]]\nmodId = \"life\"\nversion = \"1.0\"\n[[mods]]\nmodId = \"second\"\nversion = \"1.0\"\n",
            )
            .unwrap();
            let mut class_files: Vec<PathBuf> = std::fs::read_dir(classes.join("life"))
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            class_files.sort();
            for path in class_files {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                zip.start_file(format!("life/{name}"), options).unwrap();
                zip.write_all(&std::fs::read(&path).unwrap()).unwrap();
            }
            zip.finish().unwrap();
        }
        let _ = std::fs::remove_dir_all(&scratch);
        bytes
    })
}

/// A load of the fixture on `side`: the printed lines with the oracle's failure marks, the
/// failures, and the host.
fn load(side: &str) -> (Vec<String>, Vec<String>, NativeJar, StdHost) {
    let jar = NativeJar::from_bytes(fixture_jar_bytes()).unwrap();
    let mut host = jar.seed_host(StdHost::new().with_environment_type(side));
    let budget = ExecutionBudget::within(Duration::from_secs(20));
    let mut out = Vec::new();
    let mut failures = Vec::new();
    let take = |host: &mut StdHost, out: &mut Vec<String>| {
        out.extend(host.take_stdout().lines().map(str::to_owned));
    };
    for (kind, entry) in jar.role_entrypoints(side == "CLIENT") {
        jar.execute_with_budget(&entry.class, None, kind, &mut host, &budget)
            .unwrap_or_else(|error| panic!("{}: {error}", entry.class));
    }
    take(&mut host, &mut out);
    assert!(
        jar.inject_event_subscribers(&mut host, &budget, &[])
            .is_empty()
    );
    for stage in LoadingStage::ALL {
        let posts = jar.run_loading_stage(stage, &mut host, &budget);
        take(&mut host, &mut out);
        for failure in &posts {
            // "<stage>: <mod>: <event> listener ...".
            let id = failure.split(": ").nth(1).unwrap();
            out.push(format!("!post-failed {id}"));
        }
        let work = jar.run_deferred_work(&mut host, &budget);
        take(&mut host, &mut out);
        if !work.is_empty() {
            out.push("!work-failed".into());
        }
        failures.extend(posts);
        failures.extend(work);
    }
    (out, failures, jar, host)
}

#[test]
fn every_stage_posts_in_neoforges_order_and_runs_its_deferred_work_after() {
    let (out, failures, jar, host) = load("SERVER");
    // oracle.jsonl, case "server".
    assert_eq!(
        out,
        [
            "constructor life",
            "second-constructor",
            "construct life",
            "construct-work",
            "common-highest",
            "subscriber-common-high",
            "common",
            "common-after-enqueue",
            "common-low",
            "second-common",
            "common-work-run",
            "common-work-supply",
            "second-work",
            "server-setup",
            "imc-enqueue",
            "second-enqueue-throws",
            "!post-failed second",
            "imc-process",
            "work-after-failure",
            "!work-failed",
            "load-complete",
            "subscriber-complete-lowest",
            "load-complete-work",
            "payloads"
        ]
    );
    // The two failures are the jar's own exceptions, named.
    assert_eq!(failures.len(), 2, "{failures:?}");
    assert!(
        failures[0].starts_with("Enqueue IMC: second: InterModEnqueueEvent listener ")
            && failures[0].contains("java/lang/IllegalStateException")
            && failures[0].contains("listener fails"),
        "{}",
        failures[0]
    );
    assert!(
        failures[1].starts_with("deferred work of life: ") && failures[1].contains("work fails"),
        "{}",
        failures[1]
    );
    // Every call the handlers made is modelled: the client setup listener is one NeoForge never
    // posts on a dedicated server, and `registrar` is the host's payload registrar.
    assert_eq!(host.unknown_host_calls(), []);
    // The game-bus listener `common` added during setup is held like any other.
    let listeners = host.game_listeners(&jar, true);
    assert_eq!(
        listeners
            .listeners
            .iter()
            .map(|listener| listener.event.name())
            .collect::<Vec<_>>(),
        [crate::runtime::events::GameEvent::RegisterCommands.name()]
    );
}

#[test]
fn a_client_posts_the_client_setup_instead() {
    // `CommonModLoader.load` @21–48: "Sided setup" builds `FMLClientSetupEvent` when
    // `FMLEnvironment.getDist().isClient()`, else `FMLDedicatedServerSetupEvent` (bytecode, not
    // the oracle, which runs a dedicated server).
    let (out, _, _, host) = load("CLIENT");
    assert!(out.iter().any(|line| line == "client-setup"), "{out:?}");
    assert!(!out.iter().any(|line| line == "server-setup"), "{out:?}");
    assert_eq!(host.unknown_host_calls(), []);
}
