//! The P8 host objects against Java 25 (`tests/fixtures/host_events`, #156).
//!
//! `demo/HostEvents.java` is compiled against compile-only stubs and only `demo/*` ships, so every
//! platform call reaches the host. Each probe takes the objects as `Object`: this test hands it the
//! runtime's handles; `oracle/HostEventsOracle.java` handed the real objects to the same probes on
//! OpenJDK 25.0.5+8 (two runs alike), and every expected string below is what it printed. A case
//! is one of:
//!
//! * `Exact`: Java's answer, nothing counted;
//! * `CountedAs`: Java's answer, and the run counts a call it cannot answer exactly yet;
//! * `Counted`: Java's answer has no stable value (an identity hash) or needs what the key does not
//!   say, and the run counts it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use super::{deferred_work, game_bus, imc_message, imc_supplier, mod_bus, posted_event};
use crate::{NativeJar, StdHost, Value};

enum Outcome {
    Exact(&'static str),
    CountedAs(&'static str),
    Counted,
}
use Outcome::*;

fn fixture_dir() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    [
        manifest.join("tests/fixtures/host_events"),
        manifest.join("../crates/horizon-native-java/tests/fixtures/host_events"),
    ]
    .into_iter()
    .find(|dir| dir.join("demo/HostEvents.java").is_file())
    .expect("the fixture is in crates/horizon-native-java/tests/fixtures/host_events")
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

/// The probe jar, compiled once per test process: only `demo/*`, never the stubs.
fn jar() -> &'static NativeJar {
    static JAR: OnceLock<NativeJar> = OnceLock::new();
    JAR.get_or_init(|| {
        use std::io::Write;
        let fixtures = fixture_dir();
        let scratch = std::env::temp_dir().join(format!(
            "horizon-native-java-host-events-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&scratch);
        let classes = scratch.join("classes");
        std::fs::create_dir_all(&classes).unwrap();
        let mut sources = Vec::new();
        for part in ["stubs", "demo"] {
            java_files(&fixtures.join(part), &mut sources);
        }
        let javac = std::env::var_os("JAVA_HOME")
            .map(|home| PathBuf::from(home).join("bin/javac"))
            .unwrap_or_else(|| "javac".into());
        let status = Command::new(javac)
            .args(["--release", "21", "-nowarn", "-d"])
            .arg(&classes)
            .args(&sources)
            .status()
            .expect("javac runs");
        assert!(status.success(), "javac failed on the host events fixture");
        let mut bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            let options = zip::write::SimpleFileOptions::default();
            let mut class_files: Vec<PathBuf> = std::fs::read_dir(classes.join("demo"))
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            class_files.sort();
            for path in class_files {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                zip.start_file(format!("demo/{name}"), options).unwrap();
                zip.write_all(&std::fs::read(&path).unwrap()).unwrap();
            }
            zip.finish().unwrap();
        }
        let _ = std::fs::remove_dir_all(&scratch);
        NativeJar::from_bytes(&bytes).unwrap()
    })
}

/// Run one probe on a fresh host: its answer and the calls it counted.
fn run(method: &str, args: &[Value]) -> (Result<Value, String>, Vec<(String, usize)>) {
    let descriptor = format!(
        "({})Ljava/lang/String;",
        "Ljava/lang/Object;".repeat(args.len())
    );
    let mut host = StdHost::new();
    let answer = jar()
        .invoke_method_with_args(
            "demo/HostEvents",
            method,
            &descriptor,
            None,
            args,
            &mut host,
        )
        .map_err(|error| error.to_string());
    (answer, host.unknown_host_calls())
}

#[test]
fn p8_host_objects_answer_as_java_25_or_count() {
    let message = imc_message(0);
    let future = deferred_work(1);
    let event = posted_event("demo", true);
    let bus = game_bus();
    let cases: Vec<(&str, Vec<Value>, Outcome)> = vec![
        // A string spelled like a handle's text form is a String, whole: class, hash, equality,
        // text, a `HashMap` key; and none of the host objects' types.
        (
            "encoded",
            vec![],
            Exact(
                "truejava.lang.String1086760140falsefalsefalsefalsefalsefalsefalsehnfbus:game,truejava.lang.String-1956321946falsefalsefalsefalsefalsefalsefalsehfgbus:demo/Tick,truejava.lang.String828179995falsefalsefalsefalsefalsefalsefalsehfevt:0,truejava.lang.String831637673falsefalsefalsefalsefalsefalsefalsehfinv:0,truejava.lang.String920882860falsefalsefalsefalsefalsefalsefalsehimcm:0,truejava.lang.String920888626falsefalsefalsefalsefalsefalsefalsehimcs:0,truejava.lang.String1057603370falsefalsefalsefalsefalsefalsefalsehnfdw:1,truejava.lang.String-1656167998falsefalsefalsefalsefalsefalsefalsehpev:cancelled/demo,s118",
            ),
        ),
        // A string spelled like an old marker equals only itself and prints as itself.
        (
            "spelled",
            vec![],
            Exact(
                "falsefalsetrueneoforge-bus/game17,falsefalsetrueneoforge-bus/mod/demo21,falsefalsetrueforge-bus/demo/Tick19,falsefalsetruefabric-event/014,falsefalsetruefabric-event/0/invoker22,falsefalsetrueneoforge-imc-message/022,falsefalsetrueneoforge-imc-supplier/023,falsefalsetrueneoforge-deferred-work/124,falsefalsetrueevent/cancelled/demo20,falsefalsetruecancelled/demo14,3trueevent/cancelled/demo",
            ),
        ),
        // ... and is none of their types: `"neoforge-bus/game" instanceof IEventBus` was `true`.
        // `word/rest` text is still not told from a marker by `is_plain_string` until #156 P17,
        // so each check is counted, with Java's answer.
        (
            "spelledTypes",
            vec![],
            CountedAs(
                "falsefalsefalsefalsefalse,falsefalsefalsefalsefalse,falsefalsefalsefalsefalse,falsefalsefalsefalsefalse,falsefalsefalsefalsefalse,falsefalsefalsefalsefalse,falsefalsefalsefalsefalse,falsefalsefalsefalsefalse,falsefalsefalsefalsefalse,falsefalsefalsefalsefalse,",
            ),
        ),
        // The game bus read twice (separately made handles, and the same handle) and a mod bus.
        (
            "bus",
            vec![game_bus(), game_bus(), mod_bus(Some("demo"))],
            Exact("truetruefalsenet.neoforged.bus.EventBus|truefalsetruetrue|truefalsefalse|2"),
        ),
        (
            "bus",
            vec![bus.clone(), bus.clone(), mod_bus(Some("demo"))],
            Exact("truetruefalsenet.neoforged.bus.EventBus|truefalsetruetrue|truefalsefalse|2"),
        ),
        // `EventBus` prints and hashes by identity: no stable value.
        ("busText", vec![game_bus()], Counted),
        // Two `createArrayBacked` events, their invokers, and an API field read twice.
        (
            "fabric",
            vec![],
            Exact(
                "truenet.fabricmc.fabric.impl.base.event.ArrayBackedEventtrue|falsefalsetruefalse|truefalsefalse|true|3",
            ),
        ),
        (
            "imc",
            vec![
                imc_message(0),
                imc_message(0),
                imc_message(1),
                imc_supplier(0),
            ],
            Exact(
                "truetruefalsenet.neoforged.fml.InterModComms$IMCMessagetrue|truefalsetruefalse|3",
            ),
        ),
        (
            "imc",
            vec![message.clone(), message, imc_message(1), imc_supplier(0)],
            Exact(
                "truetruefalsenet.neoforged.fml.InterModComms$IMCMessagetrue|truefalsetruefalse|3",
            ),
        ),
        // Two messages may hold equal components; the keys do not say (Java: `false`).
        ("imcEquals", vec![imc_message(0), imc_message(1)], Counted),
        (
            "future",
            vec![deferred_work(1), deferred_work(1), deferred_work(2)],
            Exact("truetruetruefalsejava.util.concurrent.CompletableFuture|truefalsetruefalse|2"),
        ),
        (
            "future",
            vec![future.clone(), future, deferred_work(2)],
            Exact("truetruetruefalsejava.util.concurrent.CompletableFuture|truefalsetruefalse|2"),
        ),
        (
            "posted",
            vec![posted_event("demo", true), posted_event("demo", false)],
            Exact("truetruetruefalse"),
        ),
        // A posted event is a fresh value: the same handle is one object; two made alike may be
        // two (Java's two events: `false`), so that `==` is counted.
        ("postedIdentity", vec![event.clone(), event], Exact("true")),
        (
            "postedIdentity",
            vec![posted_event("demo", true), posted_event("demo", true)],
            Counted,
        ),
    ];
    let mut failures = Vec::new();
    for (method, args, outcome) in &cases {
        let (answer, unknown) = run(method, args);
        let ok = match outcome {
            Exact(expected) => {
                answer.as_ref() == Ok(&Value::Ref(Some((*expected).into()))) && unknown.is_empty()
            }
            CountedAs(expected) => {
                answer.as_ref() == Ok(&Value::Ref(Some((*expected).into()))) && !unknown.is_empty()
            }
            Counted => !unknown.is_empty(),
        };
        if !ok {
            failures.push(format!(
                "{method}({args:?})\n  native:  {answer:?}\n  unknown: {unknown:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
