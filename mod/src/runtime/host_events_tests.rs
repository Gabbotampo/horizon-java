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

use super::{
    deferred_work, fabric_array_event, fabric_event_name, fabric_field_event, fabric_field_of,
    fabric_registry_event, forge_bus, forge_bus_event, game_bus, imc_message, imc_supplier,
    is_cancelled_event, is_game_bus, mod_bus, posted_event,
};
use crate::runtime::{NativeJar, StdHost, Value};

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
    // `Class.method` runs another probe class than `demo/HostEvents`.
    let (class, method) = method
        .split_once('.')
        .unwrap_or(("demo/HostEvents", method));
    let mut host = StdHost::new();
    let answer = jar()
        .invoke_method_with_args(class, method, &descriptor, None, args, &mut host)
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
        // `EventBus` implements only the NeoForge bus interfaces.
        ("busForge", vec![game_bus()], Exact("false")),
        // `EventBus` prints and hashes by identity: no stable value.
        ("busText", vec![game_bus()], Counted),
        // Two `createArrayBacked` events, their invokers, and an API field read twice.
        (
            "fabric",
            vec![],
            Exact(
                "truenet.fabricmc.fabric.impl.base.event.ArrayBackedEventtrue|falsefalsetruefalse|falsefalse|true|2",
            ),
        ),
        // A Fabric event is no jar class (a Mixin cannot add a superclass) nor an `Integer`
        // (final JDK): exact. An interface no loaded Mixin adds may still come from another jar's
        // Mixin: counted (Java, no Mixin applied: `false`).
        ("fabricNotAClass", vec![], Exact("falsefalse")),
        ("fabricOtherInterface", vec![], Counted),
        // A Mixin accessor on `ArrayBackedEvent` (c2me's `IArrayBackedEvent`) is part of every
        // Fabric event: `instanceof` and the cast hold. Expected from Mixin's contract (the
        // interface joins the target's), not a JVM run: no Mixin applies on a plain JVM. One named
        // by text only, which the runtime's Mixin scan does not read, is counted.
        ("demo/HostEventsMixin.accessor", vec![], Exact("truetrue")),
        ("demo/HostEventsMixin.namedAccessor", vec![], Counted),
        // `register` builds a new invoker (`ArrayBackedEvent.update`): two reads may be two
        // objects (Java: `true|falsefalse`), so `==` and `equals` on them are counted ...
        ("fabricInvoker", vec![], Counted),
        // ... while one read held twice is one object.
        ("fabricInvokerSame", vec![], Exact("truetrue1")),
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
        // A supplier another jar sent is the sender's lambda, a class the runtime does not know
        // (`ClassRule::Unknown`): `getClass()` is counted (Java: a lambda class), never answered
        // with the declared `Supplier`; `instanceof Supplier` is exact, and so is `false` for
        // `String` and `Integer` (final, not suppliers); `instanceof Runnable` needs the class.
        ("supplierClass", vec![imc_supplier(0)], Counted),
        (
            "supplierTypes",
            vec![imc_supplier(0)],
            Exact("truefalsefalse"),
        ),
        ("supplierRunnable", vec![imc_supplier(0)], Counted),
        // The declared `Supplier` is not its class: `isAssignableFrom(getClass())` counts the
        // `getClass()` (Java: `true`), and a type switch counts `case Runnable` and picks Java's
        // `case Supplier`.
        ("supplierAssignable", vec![imc_supplier(0)], Counted),
        ("supplierSwitch", vec![imc_supplier(0)], CountedAs("S")),
        // A final class is ruled out only when its whole hierarchy is known: one that is a
        // `Supplier` through an absent jar's interface, or a jar class over a JDK superclass (another
        // jar's Mixin may add an interface), is counted (Java: `false` for this lambda).
        ("supplierFinalViaMissing", vec![imc_supplier(0)], Counted),
        ("supplierFinalList", vec![imc_supplier(0)], Counted),
        // `hierarchy_verdict` (checkcast.rs), audited: a jar class's absent-jar interface may extend
        // a platform interface (counted; Java: `false`), while a JDK superclass never implements one.
        ("jarViaMissing", vec![], Counted),
        ("jarViaJdk", vec![], Exact("false")),
        (
            "posted",
            vec![posted_event("demo", true)],
            Exact("truetruetrue"),
        ),
        // A stand-in that is not cancelled is surely an `Event`, but not surely cancellable: the
        // type switch counts `case ICancellableEvent` and picks Java's `case Event`.
        (
            "postedOpen",
            vec![posted_event("demo", false)],
            CountedAs("trueE"),
        ),
        // A posted event is a fresh value: the same handle is one object; two made alike may be
        // two objects, which the key does not say, so that `==` is counted.
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

/// Keys hold jar text (a mod id, a field name, an event name) only through the escaping encoder:
/// names with every separator read back as themselves and never alias another object.
#[test]
fn keys_never_alias_jar_text() {
    let names = [
        "",
        "x",
        "game",
        "mod",
        "cancelled/x",
        "a,b",
        "a\\",
        "\\,",
        ",",
        "i1,rx",
        "rx",
        "a.b",
        "hpev:x",
    ];
    let mut seen: Vec<Value> = Vec::new();
    let mut fresh = |value: Value| {
        assert!(!seen.contains(&value), "{value:?} aliases another key");
        seen.push(value);
    };
    fresh(game_bus());
    fresh(mod_bus(None));
    for name in names {
        for cancelled in [false, true] {
            let event = posted_event(name, cancelled);
            assert_eq!(is_cancelled_event(&event), cancelled, "{name:?}");
            fresh(event);
        }
        let bus = mod_bus(Some(name));
        assert_eq!(is_game_bus(&bus), Some(false), "{name:?}");
        fresh(bus);
        let bus = forge_bus(name);
        assert_eq!(forge_bus_event(&bus).as_deref(), Some(name));
        fresh(bus);
        for owner in names {
            let event = fabric_field_event(owner, name);
            assert_eq!(
                fabric_field_of(&event),
                Some((owner.to_owned(), name.to_owned()))
            );
            fresh(event);
        }
        let event = fabric_registry_event(name);
        assert_eq!(fabric_field_of(&event), None);
        fresh(event);
    }
    assert_eq!(is_game_bus(&game_bus()), Some(true));
    assert_eq!(is_game_bus(&mod_bus(None)), Some(false));
    let array = fabric_array_event(7);
    assert_eq!(fabric_event_name(&array).as_deref(), Some("7"));
    fresh(array);
}
