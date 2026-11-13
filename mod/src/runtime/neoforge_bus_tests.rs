//! The fixture (`tests/fixtures/neoforge_bus`) run through the runtime and fired through the game's
//! dispatch ([`crate::runtime::events::dispatch_order`], [`NativeJar::run_game_listener`]).
//!
//! Every expected value is pasted from `oracle.jsonl`: the same fixture on the real bus-8.0.5 with
//! NeoForge 26.3.0.101's own `NeoForge.EVENT_BUS`, on OpenJDK 25.0.5+8 (README.md there says how to
//! rerun it). The runtime's own answers are never the expectation.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::runtime::events::{EventContext, EventFacade, EventFiring, GameEvent, dispatch_order};
use crate::runtime::{ExecutionBudget, MethodRef, NativeError, NativeJar, StdHost, Value};

const DEMO: &str = "demo/BusDemo";
const BUS: &str = "Lnet/neoforged/bus/api/IEventBus;";

fn fixture_dir() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    [
        manifest.join("tests/fixtures/neoforge_bus"),
        manifest.join("../crates/horizon-native-java/tests/fixtures/neoforge_bus"),
    ]
    .into_iter()
    .find(|dir| dir.join("oracle.jsonl").is_file())
    .expect("the bus fixtures are in crates/horizon-native-java/tests/fixtures/neoforge_bus")
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

/// The fixture mod's jar, compiled once per test process: only `demo/*`, never the stubs.
pub(crate) fn fixture_jar_bytes() -> &'static [u8] {
    static JAR: OnceLock<Vec<u8>> = OnceLock::new();
    JAR.get_or_init(|| {
        use std::io::Write;
        let fixtures = fixture_dir();
        let scratch = std::env::temp_dir().join(format!(
            "horizon-native-java-neoforge-bus-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&scratch);
        let classes = scratch.join("classes");
        std::fs::create_dir_all(&classes).unwrap();
        let mut sources = Vec::new();
        for part in ["bus-stubs", "events", "mod"] {
            java_files(&fixtures.join(part), &mut sources);
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
        assert!(status.success(), "javac failed on the bus fixture");
        let mut bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("META-INF/neoforge.mods.toml", options)
                .unwrap();
            zip.write_all(
                b"modLoader = \"javafml\"\n[[mods]]\nmodId = \"demo\"\nversion = \"1.0\"\n",
            )
            .unwrap();
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
        bytes
    })
}

fn text(value: &str) -> Value {
    Value::Ref(Some(value.to_owned()))
}

fn game_bus() -> Value {
    super::game_bus()
}

fn mod_bus() -> Value {
    super::mod_bus(Some("demo"))
}

struct Fixture {
    jar: NativeJar,
    host: StdHost,
    budget: ExecutionBudget,
}

impl Fixture {
    fn new() -> Self {
        let jar = NativeJar::from_bytes(fixture_jar_bytes()).unwrap();
        let host = jar.seed_host(StdHost::new().with_environment_type("SERVER"));
        Self {
            jar,
            host,
            budget: ExecutionBudget::within(Duration::from_secs(20)),
        }
    }

    fn call(&mut self, name: &str, descriptor: &str, args: &[Value]) -> Result<Value, NativeError> {
        let target = MethodRef {
            owner: DEMO.into(),
            name: name.into(),
            descriptor: descriptor.into(),
        };
        self.jar
            .invoke_persistent(&target, None, args, &mut self.host, &self.budget)
    }

    /// Fire `event` the way the game does: the jar's listeners for it in dispatch order, one shared
    /// firing. Answers the lines they printed and whether the firing ends cancelled.
    fn fire(&mut self, event: GameEvent) -> (Vec<String>, bool) {
        let _ = self.host.take_stdout();
        let listeners = self.host.game_listeners(&self.jar, true).listeners;
        let entries: Vec<_> = listeners
            .iter()
            .enumerate()
            .filter(|(_, listener)| listener.event == event)
            .map(|(slot, listener)| (0, slot, listener))
            .collect();
        let mut firing = EventFiring::new(
            event,
            Arc::new(EventFacade::default()),
            EventContext::default(),
        );
        for (_, slot) in dispatch_order(&entries, &[]) {
            let run = self
                .jar
                .run_game_listener(&listeners[slot], &mut firing, &mut self.host, &self.budget)
                .unwrap_or_else(|error| panic!("{}: {error}", listeners[slot].source));
            if run.stop {
                break;
            }
        }
        let out = self.host.take_stdout().lines().map(str::to_owned).collect();
        (out, firing.cancelled)
    }
}

#[test]
fn listeners_run_in_the_real_buses_order() {
    // Every overload, each priority: HIGHEST to LOWEST, registration order within one.
    let mut fixture = Fixture::new();
    fixture
        .call("order", &format!("({BUS})V"), &[game_bus()])
        .unwrap();
    assert_eq!(fixture.host.unknown_host_call_count(), 0);
    let (out, _) = fixture.fire(GameEvent::ServerTickEnd);
    // oracle.jsonl, case "order".
    assert_eq!(
        out,
        [
            "highest",
            "captured",
            "high-typed",
            "static",
            "typed",
            "object",
            "receives",
            "plain-typed",
            "bound",
            "lowest"
        ]
    );
}

#[test]
fn a_cancelled_event_skips_listeners_that_do_not_receive_it() {
    let mut fixture = Fixture::new();
    fixture
        .call("cancel", &format!("({BUS})V"), &[game_bus()])
        .unwrap();
    let (out, cancelled) = fixture.fire(GameEvent::LivingAllowDeath);
    // oracle.jsonl, case "cancel": `skipped` and `after-last` are reached while cancelled.
    assert_eq!(out, ["cancels", "uncancels", "runs-again", "cancels-last"]);
    assert!(cancelled);
}

#[test]
fn unregister_removes_that_consumer_only() {
    let mut fixture = Fixture::new();
    fixture
        .call("unregister", &format!("({BUS})V"), &[game_bus()])
        .unwrap();
    // oracle.jsonl, case "unregister".
    assert_eq!(fixture.fire(GameEvent::ServerTickStart).0, ["kept"]);
}

#[test]
fn listeners_for_events_nothing_fires_are_counted() {
    // NeoForge accepts both (oracle.jsonl, case "not_fired": a real mod bus runs `setup` when FML
    // posts `FMLCommonSetupEvent`). The common-setup stage posts it here too
    // (`neoforge_lifecycle.rs`), so that listener is held; nothing posts the jar's own event, so
    // that registration is an unknown host call named after its event.
    let mut fixture = Fixture::new();
    fixture
        .call(
            "notFired",
            &format!("({BUS}{BUS})V"),
            &[mod_bus(), game_bus()],
        )
        .unwrap();
    let listeners = fixture.host.game_listeners(&fixture.jar, true);
    assert!(listeners.listeners.is_empty(), "{:?}", listeners.listeners);
    assert_eq!(
        fixture.host.unknown_host_calls(),
        [(
            "net/neoforged/bus/api/IEventBus#addListener(Ljava/util/function/Consumer;)V (no game event fires demo/DemoEvent)".to_owned(),
            1
        )]
    );
    let _ = fixture.host.take_stdout();
    let failures = fixture.jar.run_loading_stage(
        crate::runtime::neoforge_lifecycle::LoadingStage::CommonSetup,
        &mut fixture.host,
        &fixture.budget,
    );
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(
        fixture.host.take_stdout().lines().collect::<Vec<_>>(),
        ["setup"]
    );
}

#[test]
fn refused_registrations_throw_neoforges_exceptions() {
    // oracle.jsonl, cases "refused_0" to "refused_3", each on the bus the oracle used.
    let cases = [
        (
            game_bus(),
            "java.lang.IllegalArgumentException: Listener for event class net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent takes an argument that is not valid for this bus",
        ),
        (
            mod_bus(),
            "java.lang.IllegalArgumentException: Listener for event class net.neoforged.neoforge.event.tick.ServerTickEvent$Post takes an argument that is not valid for this bus",
        ),
        (
            game_bus(),
            "java.lang.IllegalArgumentException: Cannot register listeners for abstract class net.neoforged.neoforge.event.tick.ServerTickEvent. Register a listener to one of its subclasses instead!",
        ),
        (
            game_bus(),
            "java.lang.IllegalArgumentException: Abstract event class demo.BadAbstract has a non-abstract superclass class demo.BadConcrete. The superclass must be made abstract.",
        ),
    ];
    for (which, (bus, expected)) in cases.into_iter().enumerate() {
        let mut fixture = Fixture::new();
        let answer = fixture
            .call(
                "refused",
                &format!("({BUS}I)Ljava/lang/String;"),
                &[bus, Value::Int(which as i32)],
            )
            .unwrap();
        assert_eq!(answer, text(expected), "case {which}");
        assert!(
            fixture
                .host
                .game_listeners(&fixture.jar, true)
                .listeners
                .is_empty(),
            "case {which} registered nothing"
        );
    }
}

#[test]
fn register_takes_an_objects_instance_and_a_classs_static_subscribers() {
    // `subscribe`: a lambda, `register(object)`, `register(StaticSubscribers.class)`, the same
    // object again (ignored), two more lambdas; each event fired as the game fires it.
    let mut fixture = Fixture::new();
    fixture
        .call("subscribe", &format!("({BUS})V"), &[game_bus()])
        .unwrap();
    // oracle.jsonl, cases "subscribe_tick", "subscribe_death", "subscribe_pre".
    assert_eq!(
        fixture.fire(GameEvent::ServerTickEnd).0,
        [
            "object-high",
            "lambda-first",
            "object-normal",
            "lambda-last",
            "static-low"
        ]
    );
    assert_eq!(
        fixture.fire(GameEvent::LivingAllowDeath),
        (vec!["static-cancels".into(), "object-death".into()], true)
    );
    assert_eq!(fixture.fire(GameEvent::ServerTickStart).0, ["static-pre"]);
    // `Subscribers.demo(DemoEvent)` is accepted (registered once: the second `register` of the
    // object is ignored), and nothing fires the jar's own event.
    assert_eq!(
        fixture.host.unknown_host_calls(),
        [(
            "net/neoforged/bus/api/IEventBus#register(Ljava/lang/Object;)V (no game event fires demo/DemoEvent)".to_owned(),
            1
        )]
    );
}

#[test]
fn unregister_takes_every_method_of_a_registered_object() {
    let mut fixture = Fixture::new();
    fixture
        .call("unsubscribe", &format!("({BUS})V"), &[game_bus()])
        .unwrap();
    // oracle.jsonl, case "unsubscribe".
    assert_eq!(
        fixture.fire(GameEvent::ServerTickEnd).0,
        ["kept-high", "kept-normal"]
    );
}

#[test]
fn refused_registers_throw_neoforges_exceptions() {
    // oracle.jsonl, cases "refused_register_0" to "refused_register_7", each on a fresh game bus.
    let expected = [
        "java.lang.IllegalArgumentException: Expected @SubscribeEvent method public void demo.Refused$OneInstance.on(net.neoforged.neoforge.event.tick.ServerTickEvent$Post) to be static\nbecause register() was called with a class type.\nEither make the method static, or call register() with an instance of class demo.Refused$OneInstance.\n",
        "java.lang.IllegalArgumentException: Expected @SubscribeEvent method public static void demo.Refused$OneStatic.on(net.neoforged.neoforge.event.tick.ServerTickEvent$Post) to NOT be static\nbecause register() was called with an instance type.\nEither make the method non-static, or call register(OneStatic.class).\n",
        "java.lang.IllegalArgumentException: class demo.Refused$Nothing has no @SubscribeEvent methods, but register was called anyway.\nThe event bus only recognizes listener methods that have the @SubscribeEvent annotation.\n",
        "java.lang.IllegalArgumentException: Attempting to register a listener object of type class demo.Refused$Child,\nhowever its supertype class demo.Refused$Parent has a @SubscribeEvent method: public void demo.Refused$Parent.inherited(net.neoforged.neoforge.event.tick.ServerTickEvent$Post).\nThis is not allowed! Only the listener object can have @SubscribeEvent methods.\n",
        "java.lang.IllegalArgumentException: Method public static void demo.Refused$TwoArguments.on(net.neoforged.neoforge.event.tick.ServerTickEvent$Post,int) has @SubscribeEvent annotation. It has 2 arguments, but event handler methods require a single argument only.",
        "java.lang.IllegalArgumentException: Method public static void demo.Refused$NotAnEvent.on(java.lang.String) has @SubscribeEvent annotation, but takes an argument that is not an Event subtype : class java.lang.String",
        "java.lang.IllegalArgumentException: Method protected static synchronized void demo.Refused$ModBusOnly.on(net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent) has @SubscribeEvent annotation, but takes an argument that is not valid for this busclass net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent",
        "java.lang.IllegalArgumentException: Cannot register listeners for abstract class net.neoforged.neoforge.event.tick.ServerTickEvent. Register a listener to one of its subclasses instead!",
    ];
    for (which, expected) in expected.into_iter().enumerate() {
        let mut fixture = Fixture::new();
        let answer = fixture
            .call(
                "refusedRegister",
                &format!("({BUS}I)Ljava/lang/String;"),
                &[game_bus(), Value::Int(which as i32)],
            )
            .unwrap();
        assert_eq!(answer, text(expected), "case {which}");
        assert!(
            fixture
                .host
                .game_listeners(&fixture.jar, true)
                .listeners
                .is_empty(),
            "case {which} registered nothing"
        );
    }
}

#[test]
fn a_mod_constructor_registers_its_own_instance() {
    // `ModEntry`'s `@Mod` constructor runs `NeoForge.EVENT_BUS.register(this)`; FML builds it with
    // its mod bus. oracle.jsonl, case "mod_entry".
    let mut fixture = Fixture::new();
    fixture
        .jar
        .execute("demo.ModEntry", None, "neoforge", &mut fixture.host)
        .unwrap();
    assert_eq!(fixture.host.unknown_host_call_count(), 0);
    assert_eq!(
        fixture.fire(GameEvent::RegisterCommands).0,
        ["entry-commands"]
    );
}

#[test]
fn injected_subscribers_join_the_constructors_listeners_in_priority_order() {
    // The `@Mod` constructor registers first, then FML injects the container's
    // `@EventBusSubscriber`s: `AutoSubscribers` (initialised, its static methods registered),
    // never the client-only class or another mod's.
    let mut fixture = Fixture::new();
    fixture
        .call("beforeInject", &format!("({BUS})V"), &[game_bus()])
        .unwrap();
    let _ = fixture.host.take_stdout();
    let failures = fixture
        .jar
        .inject_event_subscribers(&mut fixture.host, &fixture.budget, &[]);
    assert!(failures.is_empty(), "{failures:?}");
    // oracle.jsonl, case "inject_init": only `AutoSubscribers` is initialised.
    assert_eq!(
        fixture.host.take_stdout().lines().collect::<Vec<_>>(),
        ["auto-clinit"]
    );
    // oracle.jsonl, case "inject_tick".
    assert_eq!(
        fixture.fire(GameEvent::ServerTickEnd).0,
        ["ctor-highest", "ctor-high", "auto-high"]
    );
    // oracle.jsonl, case "inject_setup": the mod bus runs `auto-setup` when the common-setup stage
    // posts `FMLCommonSetupEvent`.
    assert_eq!(fixture.host.unknown_host_call_count(), 0);
    let failures = fixture.jar.run_loading_stage(
        crate::runtime::neoforge_lifecycle::LoadingStage::CommonSetup,
        &mut fixture.host,
        &fixture.budget,
    );
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(
        fixture.host.take_stdout().lines().collect::<Vec<_>>(),
        ["auto-setup"]
    );
}

#[test]
fn a_container_whose_constructor_failed_injects_nothing() {
    // `FMLModContainer#constructMod` @313–364: a throwing `@Mod` constructor ends the task before
    // `AutomaticEventSubscriber.inject` @388–395.
    let mut fixture = Fixture::new();
    let failures = fixture.jar.inject_event_subscribers(
        &mut fixture.host,
        &fixture.budget,
        &["demo.ModEntry"],
    );
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(fixture.host.take_stdout(), "");
    assert!(fixture.fire(GameEvent::ServerTickEnd).0.is_empty());
    assert_eq!(fixture.host.unknown_host_call_count(), 0);
}

#[test]
fn refused_subscribers_fail_their_container_with_fmls_exception() {
    // oracle.jsonl, cases "inject_refused_0" to "inject_refused_3": each class is its own mod.
    let expected = [
        "java.lang.IllegalArgumentException: Method public void demo.InjectRefused$NotStatic.on(net.neoforged.neoforge.event.tick.ServerTickEvent$Post) annotated with @SubscribeEvent is not static",
        "java.lang.IllegalArgumentException: Method public static void demo.InjectRefused$TwoArguments.on(net.neoforged.neoforge.event.tick.ServerTickEvent$Post,int) annotated with @SubscribeEvent must have only one parameter that is an Event subtype",
        "java.lang.IllegalArgumentException: Method public static void demo.InjectRefused$NotAnEvent.on(java.lang.String) annotated with @SubscribeEvent must have only one parameter that is an Event subtype",
        "java.lang.IllegalArgumentException: Cannot register listeners for abstract class net.neoforged.neoforge.event.tick.ServerTickEvent. Register a listener to one of its subclasses instead!",
    ];
    for (which, expected) in expected.into_iter().enumerate() {
        let mut fixture = Fixture::new();
        let container = format!("refuse{which}");
        let jar = &fixture.jar;
        let result = jar
            .with_resident_runtime(&mut fixture.host, &fixture.budget, |runtime| {
                runtime.inject_container(jar, &container)
            })
            .unwrap();
        match result {
            Err(NativeError::UncaughtException { class, message, .. }) => {
                assert_eq!(
                    (class.as_str(), message.as_deref()),
                    ("java/lang/RuntimeException", Some(expected)),
                    "case {which}"
                );
            }
            other => panic!("case {which}: {other:?}"),
        }
        assert!(
            fixture
                .host
                .game_listeners(&fixture.jar, true)
                .listeners
                .is_empty(),
            "case {which} registered nothing"
        );
    }
}
