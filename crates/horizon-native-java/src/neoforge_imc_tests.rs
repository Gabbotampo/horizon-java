//! The IMC fixture (`tests/fixtures/neoforge_imc`): a sender jar and a receiver jar on one
//! [`ImcBus`], loaded the way the game's content pass loads them: both jars' mods in the bus's
//! `ModList`, each host joined before its constructor, and every host but the running one's lent
//! to the bus while the loading stages run.
//!
//! The expected lines are pasted from `oracle.jsonl`: the same two mods on the real loader-12.0.12
//! `InterModComms`, `ModList` and `ModLoadingContext` on OpenJDK 25.0.5+8 (README.md there).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::ImcBus;
use crate::neoforge_lifecycle::LoadingStage;
use crate::{ExecutionBudget, NativeJar, StdHost};

/// `oracle.jsonl`, case `two-jars`: everything both mods printed, `== <step>` before each step.
const ORACLE: &[&str] = &[
    "== Mod Construction",
    "S constructor early true",
    "== Common setup",
    "== Sided setup",
    "== Enqueue IMC",
    "S enqueue greet true",
    "S enqueue count true",
    "S enqueue unread true",
    "S enqueue object true",
    "S enqueue missing false",
    "S enqueue null-target false",
    "S enqueue named true",
    "R enqueue back true",
    "S work deferred true",
    "== Process IMC",
    "S got receiver sender back from-receiver",
    "R process self true",
    "R got sender receiver greet",
    "S supplier greet runs",
    "R value hello",
    "R got sender receiver count",
    "S supplier count runs 1",
    "R value 10",
    "S supplier count runs 2",
    "R again 20",
    "R got custom-sender receiver named",
    "R value named-value",
    "R then sender early from-constructor",
    "R then sender deferred from-work",
    "R then receiver self me",
    "R nobody 0",
    "== Complete loading",
    "== Network registry lock",
    "S payloads late true",
    "R left sender unread",
    "R left sender object",
    "R object-is-string false",
    "R left minecraft late",
    "R late after-loading",
];

fn fixtures() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    [
        manifest.join("tests/fixtures"),
        manifest.join("../crates/horizon-native-java/tests/fixtures"),
    ]
    .into_iter()
    .find(|dir| dir.join("neoforge_imc/oracle.jsonl").is_file())
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

/// The two jars' bytes (`imcsend/` as mod `sender`, `imcrecv/` as `receiver`), compiled once.
pub(crate) fn jar_bytes() -> &'static (Vec<u8>, Vec<u8>) {
    static JARS: OnceLock<(Vec<u8>, Vec<u8>)> = OnceLock::new();
    JARS.get_or_init(|| {
        let fixtures = fixtures();
        let scratch =
            std::env::temp_dir().join(format!("horizon-native-java-imc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let classes = scratch.join("classes");
        std::fs::create_dir_all(&classes).unwrap();
        let mut sources = Vec::new();
        java_files(&fixtures.join("neoforge_bus/bus-stubs"), &mut sources);
        java_files(&fixtures.join("neoforge_lifecycle/stubs"), &mut sources);
        java_files(&fixtures.join("neoforge_imc/stubs"), &mut sources);
        java_files(&fixtures.join("neoforge_imc/mod"), &mut sources);
        let javac = std::env::var_os("JAVA_HOME")
            .map(|home| PathBuf::from(home).join("bin/javac"))
            .unwrap_or_else(|| "javac".into());
        let status = Command::new(javac)
            .args(["--release", "17", "-nowarn", "-d"])
            .arg(&classes)
            .args(&sources)
            .status()
            .expect("javac runs");
        assert!(status.success(), "javac failed on the IMC fixture");
        let jar = |package: &str, id: &str| {
            use std::io::Write;
            let mut bytes = Vec::new();
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("META-INF/neoforge.mods.toml", options)
                .unwrap();
            let toml =
                format!("modLoader = \"javafml\"\n[[mods]]\nmodId = \"{id}\"\nversion = \"1.0\"\n");
            zip.write_all(toml.as_bytes()).unwrap();
            let mut files: Vec<PathBuf> = std::fs::read_dir(classes.join(package))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            files.sort();
            for path in files {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                zip.start_file(format!("{package}/{name}"), options)
                    .unwrap();
                zip.write_all(&std::fs::read(&path).unwrap()).unwrap();
            }
            zip.finish().unwrap();
            bytes
        };
        let jars = (jar("imcsend", "sender"), jar("imcrecv", "receiver"));
        let _ = std::fs::remove_dir_all(&scratch);
        jars
    })
}

/// Both jars through every step, as `mod/src/content.rs` runs them. Answers each step's printed
/// lines (the sender's, then the receiver's: a host keeps its own output), the failures, and the
/// two hosts.
fn load() -> (Vec<String>, Vec<String>, StdHost, StdHost) {
    let (send, receive) = jar_bytes();
    let budget = ExecutionBudget::within(Duration::from_secs(20));
    let bus = ImcBus::new();
    let jars: Vec<(&str, Arc<NativeJar>)> = vec![
        ("send.jar", Arc::new(NativeJar::from_bytes(send).unwrap())),
        (
            "receive.jar",
            Arc::new(NativeJar::from_bytes(receive).unwrap()),
        ),
    ];
    for (_, jar) in &jars {
        bus.add_mods(jar);
    }
    let mut out = vec![Vec::new(); 2];
    for (index, (key, jar)) in jars.iter().enumerate() {
        let mut host = jar.seed_host(StdHost::new().with_environment_type("SERVER"));
        bus.join(key, jar, &mut host);
        for (kind, entry) in jar.role_entrypoints(false) {
            jar.execute_with_budget(&entry.class, None, kind, &mut host, &budget)
                .unwrap_or_else(|error| panic!("{}: {error}", entry.class));
        }
        out[index].push(format!("== {}", LoadingStage::Construct.name()));
        out[index].extend(host.take_stdout().lines().map(str::to_owned));
        bus.lend(key, Arc::clone(jar), host);
    }
    let mut failures = Vec::new();
    for stage in LoadingStage::ALL {
        for run_work in [false, true] {
            for (key, jar) in &jars {
                let mut host = bus.reclaim(key).unwrap();
                failures.extend(if run_work {
                    jar.run_deferred_work(&mut host, &budget)
                } else {
                    jar.run_loading_stage(stage, &mut host, &budget)
                });
                bus.lend(key, Arc::clone(jar), host);
            }
        }
        for (index, (key, jar)) in jars.iter().enumerate() {
            let mut host = bus.reclaim(key).unwrap();
            if stage != LoadingStage::Construct {
                out[index].push(format!("== {}", stage.name()));
            }
            out[index].extend(host.take_stdout().lines().map(str::to_owned));
            bus.lend(key, Arc::clone(jar), host);
        }
    }
    let sender = bus.reclaim("send.jar").unwrap();
    let receiver = bus.reclaim("receive.jar").unwrap();
    let mut lines = out.remove(0);
    lines.extend(out.remove(0));
    (lines, failures, sender, receiver)
}

/// The oracle's lines of one jar (`S ` or `R `), each under its step's `== <step>` line.
fn oracle_of(prefix: &str) -> Vec<String> {
    ORACLE
        .iter()
        .filter(|line| line.starts_with("== ") || line.starts_with(prefix))
        .map(|line| line.to_string())
        .collect()
}

#[test]
fn messages_cross_jars_in_fmls_order_and_suppliers_run_when_the_receiver_calls_them() {
    let (lines, failures, sender, receiver) = load();
    assert!(failures.is_empty(), "{failures:?}");
    // Each host's own output, step by step, is the oracle's lines of that jar in that step: the
    // sender's suppliers print in "Process IMC", where the receiver calls them (twice for
    // `count`, whose sender-side counter keeps its value), and never for the unread message.
    let split = lines
        .iter()
        .skip(1)
        .position(|line| line == "== Mod Construction")
        .unwrap()
        + 1;
    assert_eq!(lines[..split], oracle_of("S ")[..]);
    assert_eq!(lines[split..], oracle_of("R ")[..]);
    assert_eq!(sender.unknown_host_calls(), []);
    // Departure, not FML's behaviour: the `object` message's value is a `StringBuilder` of the
    // sender's runtime, which does not cross jars here. Its `get()` is counted and answers `null`
    // (the JVM answers the builder; the fixture only asks `instanceof String`, false either way).
    assert_eq!(
        receiver.unknown_host_calls(),
        [(
            "java/util/function/Supplier#get()Ljava/lang/Object; (an IMC value that only exists \
             in the sending jar)"
                .to_owned(),
            1
        )]
    );
}
