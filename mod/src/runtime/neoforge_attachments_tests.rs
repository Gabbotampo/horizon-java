//! Tests for NeoForge attachments on players (`neoforge_attachments.rs`, `attachment_codec.rs`,
//! `java_nbt.rs`).
//!
//! The fixture mod (`tests/fixtures/attachments/mod`) is compiled with `javac` against
//! compile-only stubs, and only its own classes go in the jar. Expected values come from two
//! independent sources, never from this code:
//!
//! * `tests/fixtures/attachments/oracle.jsonl`, the real 26.3 classes run on a JVM (codec encode and
//!   decode, the `CompoundTag` methods, the builder checks, the unbound-holder key);
//! * the bytecode of NeoForge's `AttachmentHolder`/`IAttachmentHolder`/`AttachmentInternals`, for the
//!   holder calls the oracle could not run (`AttachmentHolder`'s initialiser needs FML). Those tests
//!   say "bytecode-derived, not run" and name the offsets.
//!
//! The game's store is a small fake here that applies the runtime's journal; the mod's tests run
//! the same flows against the game's real `Attachments`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use super::*;
use crate::runtime::java_nbt::Nbt;

const DEMO: &str = "demo/Demo";
const PLAYER: &str = "Lnet/minecraft/world/entity/player/Player;";
const COMPOUND: &str = "Lnet/minecraft/nbt/CompoundTag;";
const P: u128 = 0x0123_4567_89ab_cdef_0011_2233_4455_6677;

// Which type `Demo.pick` answers.
const DATA: i32 = 0;
const KEPT: i32 = 1;
const TRANSIENT: i32 = 2;
const OPAQUE: i32 = 3;
const FAILS: i32 = 4;
const TOUCHES: i32 = 5;
const UNKNOWN_DEFAULT: i32 = 6;
const SKIPPED: i32 = 7;
const SPINS: i32 = 8;

/// The fixture directory, from this crate or from the mod that vendors it.
pub(crate) fn fixture_dir() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    [
        manifest.join("tests/fixtures/attachments"),
        manifest.join("../crates/horizon-native-java/tests/fixtures/attachments"),
    ]
    .into_iter()
    .find(|dir| dir.join("oracle.jsonl").is_file())
    .expect("the attachment fixtures are in crates/horizon-native-java/tests/fixtures/attachments")
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

/// The fixture mod's jar, compiled once per test process: its own classes and a NeoForge
/// `mods.toml`, never the stubs.
pub(crate) fn fixture_jar_bytes() -> &'static [u8] {
    static JAR: OnceLock<Vec<u8>> = OnceLock::new();
    JAR.get_or_init(|| {
        use std::io::Write;
        let fixtures = fixture_dir();
        let scratch = std::env::temp_dir().join(format!(
            "horizon-native-java-attachments-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&scratch);
        let classes = scratch.join("classes");
        std::fs::create_dir_all(&classes).unwrap();
        let mut sources = Vec::new();
        java_files(&fixtures.join("stubs"), &mut sources);
        java_files(&fixtures.join("mod"), &mut sources);
        let javac = std::env::var_os("JAVA_HOME")
            .map(|home| PathBuf::from(home).join("bin/javac"))
            .unwrap_or_else(|| "javac".into());
        let status = Command::new(javac)
            .args(["--release", "17", "-nowarn", "-d"])
            .arg(&classes)
            .args(&sources)
            .status()
            .expect("javac runs");
        assert!(status.success(), "javac failed on the attachment fixture");
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

/// One oracle line by its case name.
pub(crate) fn oracle(case: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(fixture_dir().join("oracle.jsonl")).unwrap();
    text.lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|line| line["case"] == case)
        .unwrap_or_else(|| panic!("oracle case {case}"))
}

/// An oracle tag (`{"t":..,"v":..}`) as a tree.
pub(crate) fn tag(json: &serde_json::Value) -> Nbt {
    let v = &json["v"];
    let ints = |v: &serde_json::Value| {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_i64().unwrap())
            .collect::<Vec<_>>()
    };
    match json["t"].as_str().unwrap() {
        "byte" => Nbt::Byte(v.as_i64().unwrap() as i8),
        "short" => Nbt::Short(v.as_i64().unwrap() as i16),
        "int" => Nbt::Int(v.as_i64().unwrap() as i32),
        "long" => Nbt::Long(v.as_i64().unwrap()),
        "float" => Nbt::Float(json["bits"].as_i64().unwrap() as u32),
        "double" => Nbt::Double(json["bits"].as_i64().unwrap() as u64),
        "string" => Nbt::String(v.as_str().unwrap().to_owned()),
        "byte[]" => Nbt::ByteArray(ints(v).into_iter().map(|b| b as i8 as u8).collect()),
        "int[]" => Nbt::IntArray(ints(v).into_iter().map(|i| i as i32).collect()),
        "long[]" => Nbt::LongArray(ints(v)),
        "list" => Nbt::List(v.as_array().unwrap().iter().map(tag).collect()),
        "compound" => Nbt::Compound(compound(json)),
        other => panic!("oracle tag type {other}"),
    }
}

pub(crate) fn compound(json: &serde_json::Value) -> NbtCompound {
    json["v"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), tag(value)))
        .collect()
}

fn text(value: &str) -> Value {
    Value::Ref(Some(value.to_owned()))
}

fn player(uuid: u128) -> Value {
    text(&format!("game/player/{uuid:032x}"))
}

/// One stored value in the fake store.
#[derive(Clone, Debug, PartialEq)]
enum Fake {
    Ours {
        token: u64,
        saved: Option<NbtCompound>,
        copy_on_death: bool,
    },
    Raw(NbtCompound),
}

/// The fixture jar, its host, and a fake of the game's store that applies the journal.
struct Fixture {
    jar: NativeJar,
    host: StdHost,
    store: BTreeMap<(u128, String), Fake>,
    budget: ExecutionBudget,
}

impl Fixture {
    /// The jar loaded and its register's bus registered, before the registration point.
    fn loaded() -> Self {
        let jar = NativeJar::from_bytes(fixture_jar_bytes()).unwrap();
        let host = jar.seed_host(StdHost::new().with_environment_type("SERVER"));
        let mut fixture = Self {
            jar,
            host,
            store: BTreeMap::new(),
            budget: ExecutionBudget::within(Duration::from_secs(20)),
        };
        fixture
            .call(
                "init",
                "(Lnet/neoforged/bus/api/IEventBus;)V",
                &[text("net/neoforged/bus/api/IEventBus")],
            )
            .unwrap();
        fixture
    }

    fn registered() -> Self {
        let mut fixture = Self::loaded();
        let problems = fixture
            .jar
            .register_attachment_types(&mut fixture.host, &fixture.budget);
        assert_eq!(problems, Vec::<String>::new());
        fixture
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

    fn on(&mut self, name: &str, which: i32) -> Value {
        let descriptor = match name {
            "has" => format!("({PLAYER}I)Z"),
            "sync" => format!("({PLAYER}I)V"),
            _ => format!("({PLAYER}I)Ljava/lang/Object;"),
        };
        self.call(name, &descriptor, &[player(P), Value::Int(which)])
            .unwrap()
    }

    fn set(&mut self, which: i32, value: Value) -> Value {
        self.call(
            "set",
            &format!("({PLAYER}ILjava/lang/Object;)Ljava/lang/Object;"),
            &[player(P), Value::Int(which), value],
        )
        .unwrap()
    }

    fn fresh(&mut self, selected: i32) -> Value {
        self.call("fresh", "(I)Ljava/lang/Object;", &[Value::Int(selected)])
            .unwrap()
    }

    fn select(&mut self, data: &Value, selected: i32) {
        self.call(
            "select",
            "(Ljava/lang/Object;I)V",
            &[data.clone(), Value::Int(selected)],
        )
        .unwrap();
    }

    fn selected(&mut self, data: &Value) -> i32 {
        match self
            .call(
                "selected",
                "(Ljava/lang/Object;)I",
                std::slice::from_ref(data),
            )
            .unwrap()
        {
            Value::Int(v) => v,
            other => panic!("selected answered {other:?}"),
        }
    }

    /// The compound a `Data` value holds now.
    fn nbt_of(&mut self, data: &Value) -> NbtCompound {
        let nbt = self
            .call(
                "nbtOf",
                &format!("(Ljava/lang/Object;){COMPOUND}"),
                std::slice::from_ref(data),
            )
            .unwrap();
        let marker = expect_ref(&nbt).unwrap().clone();
        let tree = self
            .jar
            .with_resident_runtime(&mut self.host, &self.budget, |rt| rt.nbt_read(&marker))
            .unwrap()
            .unwrap();
        match tree {
            Nbt::Compound(c) => c,
            other => panic!("{other:?}"),
        }
    }

    fn alloc(&mut self, tree: &Nbt) -> Value {
        self.jar
            .with_resident_runtime(&mut self.host, &self.budget, |rt| rt.nbt_alloc(tree))
            .unwrap()
    }

    /// Opens a batch with the fake store's entries for `P`.
    fn open(&mut self) {
        let ids = self.host.attachment_store_ids();
        let entries = ids
            .iter()
            .map(|id| {
                let entry = match self.store.get(&(P, id.clone())) {
                    None => StoreEntry::Absent,
                    Some(Fake::Ours { token, .. }) => StoreEntry::Ours(*token),
                    Some(Fake::Raw(saved)) => StoreEntry::Raw(saved.clone()),
                };
                (id.clone(), entry)
            })
            .collect();
        self.host
            .open_attachment_batch(BTreeMap::from([(P, Ok(entries))]));
    }

    /// Ends the batch (R1 with `unknown` neutral calls) and applies the journal.
    fn close(&mut self, unknown: usize) -> Vec<StoreWrite> {
        self.jar
            .attachment_batch_end(&mut self.host, &self.budget, unknown);
        let writes = self.host.close_attachment_batch();
        for write in &writes {
            match write {
                StoreWrite::Store {
                    player,
                    id,
                    token,
                    encoded,
                    copy_on_death,
                    ..
                } => {
                    self.store.insert(
                        (*player, id.clone()),
                        Fake::Ours {
                            token: *token,
                            saved: encoded.clone(),
                            copy_on_death: *copy_on_death,
                        },
                    );
                }
                StoreWrite::Discard { player, id } => {
                    self.store.remove(&(*player, id.clone()));
                }
            }
        }
        writes
    }

    /// What the game's store would save for `id`: the codec's bytes, or nothing.
    fn saved(&self, id: &str) -> Option<NbtCompound> {
        match self.store.get(&(P, id.to_owned()))? {
            Fake::Ours { saved, .. } => saved.clone(),
            Fake::Raw(saved) => Some(saved.clone()),
        }
    }

    /// A restart: every stored value is saved and comes back raw; the runtime keeps nothing.
    fn restart(&mut self) {
        self.store = std::mem::take(&mut self.store)
            .into_iter()
            .filter_map(|(key, fake)| match fake {
                Fake::Ours {
                    saved: Some(saved), ..
                }
                | Fake::Raw(saved) => Some((key, Fake::Raw(saved))),
                Fake::Ours { saved: None, .. } => None,
            })
            .collect();
        self.host.reset_attachment_values();
    }

    /// The game's death copy (`Attachments::copy_on_death` + `replace_contents`): only entries
    /// stored with `copy_on_death` and bytes survive, as raw saved values.
    fn die(&mut self) {
        self.store = std::mem::take(&mut self.store)
            .into_iter()
            .filter_map(|(key, fake)| match fake {
                Fake::Ours {
                    saved: Some(saved),
                    copy_on_death: true,
                    ..
                } => Some((key, Fake::Raw(saved))),
                _ => None,
            })
            .collect();
    }

    fn unknown(&self, key_part: &str) -> usize {
        self.host
            .unknown_host_calls()
            .iter()
            .filter(|(key, _)| key.contains(key_part))
            .map(|(_, count)| count)
            .sum()
    }

    fn supplier_runs(&mut self) -> i32 {
        self.host
            .persistent_class_state()
            .0
            .iter()
            .find(|(key, _)| key.starts_with("demo/Demo#supplierRuns"))
            .map_or(0, |(_, value)| match value {
                Value::Int(v) => *v,
                _ => 0,
            })
    }
}

fn carry(fields: &[(&str, Nbt)]) -> NbtCompound {
    NbtCompound::from([(
        "CarryOnData".to_owned(),
        Nbt::Compound(
            fields
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
        ),
    )])
}

/// The type's child as the oracle's `serializeAttachments` left it.
fn oracle_child(case: &str) -> NbtCompound {
    let saved = compound(&oracle(case)["saved"]);
    match saved.get("carryon:carry_on_data") {
        Some(Nbt::Compound(child)) => child.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_registry_is_neoforges_plural_key_and_registration_waits_for_the_register_event() {
    let mut fixture = Fixture::loaded();
    // The oracle's `NeoForgeRegistries.Keys.ATTACHMENT_TYPES`.
    let key = oracle("registry/key")["key"].as_str().unwrap().to_owned();
    assert_eq!(
        key,
        format!("ResourceKey[minecraft:root / {ATTACHMENT_TYPES_REGISTRY}]")
    );
    assert!(fixture.host.registrations().iter().any(|row| row.registry
        == format!("deferred-register/registry/{ATTACHMENT_TYPES_REGISTRY}/demo")
        && row.id == "data"));
    // `DeferredHolder.value` @0–31 before the RegisterEvent: an NPE naming the key, whose text is
    // the oracle's `ResourceKey.toString`.
    let unbound = fixture
        .call("unbound", "()Ljava/lang/Object;", &[])
        .unwrap_err();
    // The oracle printed carryon's key; the fixture's id takes its place in the same text.
    let key = oracle("deferred/key")["key"]
        .as_str()
        .unwrap()
        .replace("carryon:carry_on_data", "demo:data");
    assert_eq!(key, "ResourceKey[neoforge:attachment_types / demo:data]");
    match unbound {
        NativeError::UncaughtException { class, message, .. } => {
            assert_eq!(class, "java/lang/NullPointerException");
            assert_eq!(
                message.as_deref(),
                Some(format!("Trying to access unbound value: {key}").as_str())
            );
        }
        other => panic!("{other:?}"),
    }
    // The supplier runs at the registration point, not before.
    assert_eq!(fixture.supplier_runs(), 0);
    assert!(fixture.host.attachment_store_ids().is_empty());
    fixture
        .jar
        .register_attachment_types(&mut fixture.host, &fixture.budget);
    let ids = fixture.host.attachment_store_ids();
    assert!(
        ids.contains(&"demo:data".to_owned()) && !ids.contains(&"demo:opaque".to_owned()),
        "{ids:?}"
    );
    let bound = fixture
        .call("unbound", "()Ljava/lang/Object;", &[])
        .unwrap();
    assert!(expect_ref(&bound).unwrap().starts_with("nf-attachment/"));
}

#[test]
fn the_builder_checks_throw_neoforges_exceptions() {
    let mut fixture = Fixture::loaded();
    let checks = oracle("builder/checks");
    for (which, case) in [
        (0, "copyOnDeathFirst"),
        (1, "serializeTwice"),
        (2, "copyHandlerFirst"),
        (3, "serializeThenCopyOnDeath"),
    ] {
        let answer = fixture
            .call(
                "builderCheck",
                "(I)Ljava/lang/String;",
                &[Value::Int(which)],
            )
            .unwrap();
        let expected = &checks[case]["message"];
        match expected.as_str() {
            Some(message) => assert_eq!(answer, text(message), "{case}"),
            None => assert_eq!(answer, Value::Ref(None), "{case}"),
        }
    }
}

#[test]
fn compound_reads_narrow_every_tag_type_as_the_server_jar_does() {
    let mut fixture = Fixture::registered();
    // The oracle's compound for `compound/reads`.
    let tree = Nbt::Compound(NbtCompound::from([
        ("b0".into(), Nbt::Byte(0)),
        ("b1".into(), Nbt::Byte(1)),
        ("b2".into(), Nbt::Byte(2)),
        ("s".into(), Nbt::String("x".into())),
        ("i256".into(), Nbt::Int(256)),
        ("i1".into(), Nbt::Int(1)),
        ("l".into(), Nbt::Long((1 << 33) + 5)),
        ("d".into(), Nbt::Double((-2.5f64).to_bits())),
        ("nan".into(), Nbt::Float(f32::NAN.to_bits())),
        ("sh".into(), Nbt::Short(-1)),
        ("big".into(), Nbt::Double(1e20f64.to_bits())),
    ]));
    let tag = fixture.alloc(&tree);
    let reads = oracle("compound/reads");
    for key in [
        "b0", "b1", "b2", "s", "i256", "i1", "l", "d", "nan", "sh", "big", "absent",
    ] {
        let args = [tag.clone(), text(key)];
        let boolean = fixture
            .call(
                "booleanOr",
                &format!("({COMPOUND}Ljava/lang/String;)Z"),
                &args,
            )
            .unwrap();
        assert_eq!(
            boolean,
            Value::Int(i32::from(
                reads[format!("getBooleanOr/{key}")].as_bool().unwrap()
            )),
            "getBooleanOr {key}"
        );
        let int = fixture
            .call("intOr", &format!("({COMPOUND}Ljava/lang/String;)I"), &args)
            .unwrap();
        assert_eq!(
            int,
            Value::Int(reads[format!("getIntOr/{key}")].as_i64().unwrap() as i32),
            "getIntOr {key}"
        );
        let string = fixture
            .call(
                "stringOr",
                &format!("({COMPOUND}Ljava/lang/String;)Ljava/lang/String;"),
                &args,
            )
            .unwrap();
        assert_eq!(
            string,
            text(reads[format!("getStringOr/{key}")].as_str().unwrap()),
            "getStringOr {key}"
        );
        let contains = fixture
            .call(
                "contains",
                &format!("({COMPOUND}Ljava/lang/String;)Z"),
                &args,
            )
            .unwrap();
        assert_eq!(
            contains,
            Value::Int(i32::from(
                reads[format!("contains/{key}")].as_bool().unwrap()
            )),
            "contains {key}"
        );
    }
}

#[test]
fn compound_copy_is_deep_and_get_compound_or_empty_answers_the_live_child() {
    let mut fixture = Fixture::registered();
    let case = oracle("compound/copy");
    let original = tag(&case["original"]);
    let marker = fixture.alloc(&original);
    let answer = fixture
        .call(
            "copyCase",
            &format!("({COMPOUND})[Ljava/lang/Object;"),
            std::slice::from_ref(&marker),
        )
        .unwrap();
    let elements = fixture
        .host
        .resident_heap()
        .arrays
        .get(expect_ref(&answer).unwrap())
        .unwrap()
        .clone();
    let read = |fixture: &mut Fixture, value: &Value| {
        let marker = expect_ref(value).unwrap().clone();
        fixture
            .jar
            .with_resident_runtime(&mut fixture.host, &fixture.budget, |rt| {
                rt.nbt_read(&marker)
            })
            .unwrap()
            .unwrap()
    };
    assert_eq!(read(&mut fixture, &elements[0]), tag(&case["copy"]));
    assert_eq!(read(&mut fixture, &elements[1]), tag(&case["orEmptyOnInt"]));
    // A `boolean` boxed into an `Object[]` is the runtime's `Boolean` marker.
    assert_eq!(
        elements[2],
        text(&format!(
            "boolean:{}",
            case["childIdentity"].as_bool().unwrap()
        ))
    );
    // The original is untouched, and the missing compound was not put into it.
    assert_eq!(read(&mut fixture, &marker), original);
}

#[test]
fn compound_put_answers_the_previous_tag_and_refuses_null_as_the_server_jar_does() {
    let mut fixture = Fixture::registered();
    let case = oracle("compound/put");
    let answer = fixture
        .call("putCase", "()[Ljava/lang/Object;", &[])
        .unwrap();
    let elements = fixture
        .host
        .resident_heap()
        .arrays
        .get(expect_ref(&answer).unwrap())
        .unwrap()
        .clone();
    // The oracle's first put answered null and the second the tag it replaced.
    assert_eq!(case["first"], serde_json::Value::Null);
    assert_eq!(elements[0], Value::Ref(None));
    assert_eq!(elements[1], text("boolean:true"));
    assert_eq!(
        elements[2],
        text(case["nullPut"]["message"].as_str().unwrap())
    );
    assert_eq!(
        case["nullString"]["threw"],
        "java.lang.NullPointerException"
    );
    assert_eq!(elements[3], text("npe"));
}

#[test]
fn encode_writes_the_bytes_neoforges_serializer_writes() {
    let mut fixture = Fixture::registered();
    let spec = fixture
        .host
        .neoforge_attachments
        .types
        .values()
        .find(|spec| {
            matches!(
                &spec.serializer,
                Serializer::MapCodec {
                    predicate: None,
                    ..
                }
            ) && spec.sync.is_some()
        })
        .unwrap()
        .clone();
    for case in ["encode/empty", "encode/carry-state", "encode/every-type"] {
        let input = tag(&oracle(case)["input"]);
        let nbt = fixture.alloc(&input);
        let value = fixture
            .call("wrap", &format!("({COMPOUND})Ljava/lang/Object;"), &[nbt])
            .unwrap();
        let coded = fixture
            .jar
            .with_resident_runtime(&mut fixture.host, &fixture.budget, |rt| {
                rt.attachment_encode(&spec, &value)
            })
            .unwrap();
        assert_eq!(coded, Coded::Ok(Some(oracle_child(case))), "{case}");
        assert!(oracle(case)["wrote"].as_bool().unwrap());
    }
}

#[test]
fn decode_reads_saved_bytes_and_a_bad_entry_is_dropped_for_the_default() {
    for case in ["decode/empty", "decode/carry-state", "decode/every-type"] {
        let mut fixture = Fixture::registered();
        let child = compound(&oracle(case)["input"]);
        fixture
            .store
            .insert((P, "demo:data".into()), Fake::Raw(child));
        fixture.open();
        let value = fixture.on("get", DATA);
        let nbt = fixture.nbt_of(&value);
        assert_eq!(Nbt::Compound(nbt), tag(&oracle(case)["value"]), "{case}");
        // A decoded value is not a default: the supplier never ran.
        assert_eq!(fixture.supplier_runs(), 0);
    }
    for case in ["decode/missing-field", "decode/int-field"] {
        let mut fixture = Fixture::registered();
        // NeoForge's read throws `READ_FAILED` for both; `deserializeAttachments` logs and skips.
        assert_eq!(oracle(case)["message"], READ_FAILED);
        fixture.store.insert(
            (P, "demo:data".into()),
            Fake::Raw(compound(&oracle(case)["input"])),
        );
        fixture.open();
        assert_eq!(fixture.on("has", DATA), Value::Int(0), "{case}");
        let value = fixture.on("get", DATA);
        assert_eq!(fixture.nbt_of(&value), NbtCompound::new());
        assert_eq!(fixture.supplier_runs(), 1);
        let writes = fixture.close(0);
        assert!(
            matches!(&writes[0], StoreWrite::Discard { id, .. } if id == "demo:data"),
            "{writes:?}"
        );
        let log = fixture.host.take_attachment_log();
        assert!(
            log[0].starts_with("Failed to deserialize data attachment demo:data. Skipping."),
            "{log:?}"
        );
    }
}

/// Bytecode-derived, not run: `AttachmentHolder#getData` @0–52 runs the default once, puts it and
/// answers the live object; `syncData` @48 follows the put.
#[test]
fn get_data_makes_the_default_once_and_answers_the_same_object() {
    let mut fixture = Fixture::registered();
    fixture.open();
    let first = fixture.on("get", DATA);
    let second = fixture.on("get", DATA);
    assert_eq!(first, second);
    assert_eq!(fixture.supplier_runs(), 1);
    let by_type = fixture
        .call(
            "getByType",
            &format!("({PLAYER})Ljava/lang/Object;"),
            &[player(P)],
        )
        .unwrap();
    assert_eq!(by_type, first);
    let writes = fixture.close(0);
    assert_eq!(writes.len(), 1, "{writes:?}");
    assert!(matches!(
        &writes[0],
        StoreWrite::Store {
            write: StoreWriteKind::Default,
            copy_on_death: false,
            ..
        }
    ));
    // The bytes the game saves are the oracle's empty-compound child.
    assert_eq!(
        fixture.saved("demo:data"),
        Some(oracle_child("encode/empty"))
    );
    assert_eq!(fixture.unknown(SYNC_NOT_SENT), 1);
}

#[test]
fn a_change_in_place_reaches_the_store_after_the_batch_and_no_change_stores_nothing() {
    let mut fixture = Fixture::registered();
    fixture.open();
    let value = fixture.on("get", DATA);
    fixture.close(0);
    fixture.open();
    fixture.select(&value, 3);
    let writes = fixture.close(0);
    assert!(
        matches!(
            &writes[..],
            [StoreWrite::Store {
                write: StoreWriteKind::Set,
                ..
            }]
        ),
        "{writes:?}"
    );
    assert_eq!(
        fixture.saved("demo:data"),
        Some(carry(&[("selected", Nbt::Int(3))]))
    );
    fixture.open();
    assert_eq!(fixture.close(0), Vec::new());
    // A restart reads the saved bytes back into a new object with the same state.
    fixture.restart();
    fixture.open();
    let reloaded = fixture.on("get", DATA);
    assert_ne!(reloaded, value);
    assert_eq!(fixture.selected(&reloaded), 3);
}

/// Bytecode-derived, not run: `AttachmentHolder#setData` @5 `requireNonNull(data)`, @16 `put`,
/// @24 `syncData`, @28 the previous value.
#[test]
fn set_data_answers_the_previous_value_and_rejects_null() {
    let mut fixture = Fixture::registered();
    fixture.open();
    let first_value = fixture.fresh(1);
    let first = fixture.set(DATA, first_value.clone());
    assert_eq!(first, Value::Ref(None));
    let replacement = fixture.fresh(7);
    let previous = fixture.set(DATA, replacement.clone());
    assert_eq!(previous, first_value);
    assert_eq!(fixture.on("get", DATA), replacement);
    let null = fixture
        .call(
            "nullSet",
            &format!("({PLAYER})Ljava/lang/String;"),
            &[player(P)],
        )
        .unwrap();
    assert_eq!(null, text("npe"));
    fixture.close(0);
    assert_eq!(
        fixture.saved("demo:data"),
        Some(carry(&[("selected", Nbt::Int(7))]))
    );
    assert_eq!(fixture.unknown(SYNC_NOT_SENT), 2);
    assert_eq!(fixture.supplier_runs(), 0);
}

/// Bytecode-derived, not run: `hasData` and `getExistingDataOrNull` read without inserting
/// (@5–30, @5–24); `removeData` answers null with no sync when the map was never made (@9–13), and
/// otherwise removes, syncs even when nothing was there (@25) and answers the previous value.
#[test]
fn reads_never_insert_and_remove_syncs_only_once_the_map_exists() {
    let mut fixture = Fixture::registered();
    fixture.open();
    assert_eq!(fixture.on("has", DATA), Value::Int(0));
    assert_eq!(fixture.on("existingOrNull", DATA), Value::Ref(None));
    assert_eq!(fixture.on("existing", DATA), Value::Ref(None));
    assert_eq!(fixture.on("remove", DATA), Value::Ref(None));
    assert_eq!(fixture.unknown(SYNC_NOT_SENT), 0);
    assert_eq!(fixture.supplier_runs(), 0);
    let value = fixture.on("get", DATA);
    assert_eq!(fixture.on("has", DATA), Value::Int(1));
    assert_eq!(fixture.on("existingOrNull", DATA), value);
    assert_eq!(fixture.on("existing", DATA), value);
    assert_eq!(fixture.on("remove", DATA), value);
    assert_eq!(fixture.on("has", DATA), Value::Int(0));
    // The map exists now: a removal of nothing still syncs.
    assert_eq!(fixture.on("remove", DATA), Value::Ref(None));
    // Default creation, then the two removals.
    assert_eq!(fixture.unknown(SYNC_NOT_SENT), 3);
    fixture.close(0);
    assert_eq!(fixture.saved("demo:data"), None);
}

/// Bytecode-derived, not run: `Entity#syncData` → `AttachmentSync.syncEntityUpdate`, which does
/// nothing for a type without a sync handler (@1–4).
#[test]
fn sync_data_is_counted_not_sent_and_only_for_a_synced_type() {
    let mut fixture = Fixture::registered();
    fixture.open();
    fixture.on("sync", DATA);
    fixture.on("sync", KEPT);
    assert_eq!(fixture.unknown(SYNC_NOT_SENT), 1);
    assert_eq!(fixture.close(0), Vec::new());
}

#[test]
fn an_unmodelled_serializer_is_a_counted_refusal_that_stores_nothing() {
    let mut fixture = Fixture::registered();
    fixture.open();
    assert_eq!(fixture.on("get", OPAQUE), Value::Ref(None));
    let value = fixture.fresh(1);
    assert_eq!(fixture.set(OPAQUE, value), Value::Ref(None));
    assert_eq!(
        fixture.unknown("(attachment demo:opaque: serializer not modelled"),
        2
    );
    assert_eq!(fixture.close(0), Vec::new());
    assert_eq!(fixture.host.attachment_stats().refused_types, 1);
}

#[test]
fn an_encode_or_default_that_makes_an_unmodelled_call_never_reaches_the_store() {
    let mut fixture = Fixture::registered();
    fixture.open();
    // The value exists, but its bytes rest on a neutral answer: stored without bytes.
    fixture.on("get", TOUCHES);
    // The default's supplier makes the unmodelled call: refused, nothing inserted.
    let refused = fixture
        .call(
            "get",
            &format!("({PLAYER}I)Ljava/lang/Object;"),
            &[player(P), Value::Int(UNKNOWN_DEFAULT)],
        )
        .unwrap_err();
    assert!(
        matches!(refused, NativeError::Metadata(ref why) if why.contains("unmodelled host call")),
        "{refused:?}"
    );
    assert_eq!(fixture.on("has", UNKNOWN_DEFAULT), Value::Int(0));
    fixture.close(0);
    assert_eq!(fixture.saved("demo:touches"), None);
    assert!(fixture.store.contains_key(&(P, "demo:touches".into())));
    assert!(
        !fixture
            .store
            .contains_key(&(P, "demo:unknown_default".into()))
    );
    assert!(fixture.host.attachment_stats().encodes_withheld >= 2);
}

#[test]
fn a_failing_encoder_saves_neoforges_empty_child_and_a_false_predicate_saves_nothing() {
    let mut fixture = Fixture::registered();
    fixture.open();
    let failing = fixture.on("get", FAILS);
    let skipped = fixture.on("get", SKIPPED);
    fixture.close(0);
    // `TagValueOutput.store` reports the error and merges nothing into the child the holder made
    // (oracle `encode/error`: the child is `{}`); the value stays readable.
    assert_eq!(
        fixture.saved("demo:fails"),
        Some(oracle_child("encode/error"))
    );
    assert!(matches!(
        fixture.store.get(&(P, "demo:fails".into())),
        Some(Fake::Ours {
            copy_on_death: false,
            ..
        })
    ));
    assert!(
        fixture.host.take_attachment_log()[0]
            .starts_with("Failed to serialize data attachment demo:fails")
    );
    fixture.open();
    assert_eq!(fixture.on("get", FAILS), failing);
    // `write` false (oracle `encode/predicate-false`) discards the child: nothing saved.
    assert_eq!(oracle("encode/predicate-false")["wrote"], false);
    assert_eq!(fixture.saved("demo:skipped"), None);
    fixture
        .call("keep", "(Ljava/lang/Object;)V", &[skipped])
        .unwrap();
    fixture.close(0);
    assert_eq!(
        fixture.saved("demo:skipped"),
        Some(NbtCompound::from([(
            "S".into(),
            Nbt::Compound(NbtCompound::from([("keep".into(), Nbt::Byte(1))]))
        )]))
    );
}

#[test]
fn a_non_terminating_encoder_is_ended_by_the_budget_and_stores_no_bytes() {
    let mut fixture = Fixture::registered();
    fixture.budget = ExecutionBudget::within(Duration::from_millis(300));
    fixture.open();
    let _ = fixture.call(
        "get",
        &format!("({PLAYER}I)Ljava/lang/Object;"),
        &[player(P), Value::Int(SPINS)],
    );
    fixture.close(0);
    assert_eq!(fixture.saved("demo:spins"), None);
}

#[test]
fn a_batch_with_a_neutral_answer_taints_live_values_and_keeps_their_last_exact_bytes() {
    let mut fixture = Fixture::registered();
    fixture.open();
    let value = fixture.on("get", DATA);
    fixture.close(0);
    fixture.open();
    fixture.select(&value, 9);
    let before = fixture.host.unknown_host_call_count();
    fixture.call("touch", "()V", &[]).unwrap();
    let unknown = fixture.host.unknown_host_call_count() - before;
    assert_eq!(fixture.close(unknown), Vec::new());
    assert_eq!(
        fixture.saved("demo:data"),
        Some(oracle_child("encode/empty"))
    );
    // The sync counts of a clean batch are accounting, not neutral answers: no taint.
    let mut clean = Fixture::registered();
    clean.open();
    let value = clean.on("get", DATA);
    clean.select(&value, 4);
    let synced = clean.unknown(SYNC_NOT_SENT);
    clean.close(synced);
    assert_eq!(
        clean.saved("demo:data"),
        Some(carry(&[("selected", Nbt::Int(4))]))
    );
}

/// Bytecode-derived, not run: `AttachmentInternals.copyEntityAttachments(from, to, isDeath = true)`
/// copies only `copyOnDeath` types (@0–26), each through the default copy handler into a new
/// object (`AttachmentType.defaultCopyHandler`: write @…, read @83).
#[test]
fn death_keeps_only_copy_on_death_types_as_new_objects() {
    let mut fixture = Fixture::registered();
    fixture.open();
    let carried = fixture.on("get", DATA);
    fixture.select(&carried, 2);
    let kept = fixture.on("get", KEPT);
    fixture.select(&kept, 5);
    fixture.close(0);
    fixture.die();
    fixture.open();
    fixture
        .jar
        .attachment_player_cloned(&mut fixture.host, &fixture.budget, P, true);
    let kept_after = fixture.on("get", KEPT);
    assert_ne!(kept_after, kept);
    assert_eq!(fixture.selected(&kept_after), 5);
    // carryon's type has no copyOnDeath: gone, so the next getData makes a new default.
    let carried_after = fixture.on("get", DATA);
    assert_eq!(fixture.selected(&carried_after), -1);
    assert_eq!(fixture.supplier_runs(), 2);
    fixture.close(0);
}

/// `PlayerEvent.Clone#getOriginal` is the old `ServerPlayer`, still whole when
/// `ServerPlayer.restoreFrom` posts the event: its attachment map holds the very objects the jar
/// last saw, the non-`copyOnDeath` ones included, while the new player starts from the death copy.
/// A default made on the old player, or a value set on it, stays on the old player: nothing is
/// journaled to the store, and the view ends with the batch.
#[test]
fn a_clones_original_keeps_the_old_players_objects() {
    let original = text(&format!("game/original-player/{P:032x}"));
    let on_original = |fixture: &mut Fixture, name: &str, which: i32| {
        let descriptor = match name {
            "has" => format!("({PLAYER}I)Z"),
            _ => format!("({PLAYER}I)Ljava/lang/Object;"),
        };
        fixture
            .call(name, &descriptor, &[original.clone(), Value::Int(which)])
            .unwrap()
    };
    let mut fixture = Fixture::registered();
    fixture.open();
    let carried = fixture.on("get", DATA);
    fixture.select(&carried, 2);
    let kept = fixture.on("get", KEPT);
    fixture.select(&kept, 5);
    fixture.close(0);
    // The game's view of the old player's store, taken before its death copy.
    let ids = fixture.host.attachment_store_ids();
    let before_death = ids
        .iter()
        .map(|id| {
            let entry = match fixture.store.get(&(P, id.clone())) {
                Some(Fake::Ours { token, .. }) => StoreEntry::Ours(*token),
                Some(Fake::Raw(saved)) => StoreEntry::Raw(saved.clone()),
                None => StoreEntry::Absent,
            };
            (id.clone(), entry)
        })
        .collect();
    fixture.die();
    fixture.open();
    fixture.host.open_original_attachments(P, Ok(before_death));
    fixture
        .jar
        .attachment_player_cloned(&mut fixture.host, &fixture.budget, P, true);
    let runs = fixture.supplier_runs();

    // The old player answers its own objects; carryon's type (no copyOnDeath) included.
    assert_eq!(on_original(&mut fixture, "get", DATA), carried);
    assert_eq!(fixture.selected(&carried), 2);
    assert_eq!(on_original(&mut fixture, "get", KEPT), kept);
    assert_eq!(on_original(&mut fixture, "has", TRANSIENT), Value::Int(0));
    // The new player has the death copy: a new KEPT object, and DATA gone (a new default).
    let kept_after = fixture.on("get", KEPT);
    assert_ne!(kept_after, kept);
    assert_eq!(fixture.selected(&kept_after), 5);
    let carried_after = fixture.on("get", DATA);
    assert_ne!(carried_after, carried);
    assert_eq!(fixture.selected(&carried_after), -1);
    assert_eq!(fixture.supplier_runs(), runs + 1);

    // A value set on the old player stays there.
    let nine = fixture.fresh(9);
    let previous = fixture
        .call(
            "set",
            &format!("({PLAYER}ILjava/lang/Object;)Ljava/lang/Object;"),
            &[original.clone(), Value::Int(DATA), nine.clone()],
        )
        .unwrap();
    assert_eq!(previous, carried);
    assert_eq!(on_original(&mut fixture, "get", DATA), nine);
    assert_eq!(fixture.on("get", DATA), carried_after);
    // Only the new player's default reaches the store; the 9 set on the old player never does.
    fixture.close(0);
    assert_eq!(fixture.saved("demo:data"), Some(carry(&[])));

    // After the batch the old player is gone: a read is refused, never answered.
    fixture.open();
    assert_eq!(on_original(&mut fixture, "get", DATA), Value::Ref(None));
    assert_eq!(fixture.unknown("expired with its clone event"), 1);
    fixture.close(0);
}

/// Bytecode-derived, not run: an End return (`restoreAll = true`, so `isDeath = false`) copies
/// every serializable type through the copy handler, a new object, and never a type without a
/// serializer (`copyAttachments` @20).
#[test]
fn an_end_return_copies_serializable_values_into_new_objects_and_drops_transient_ones() {
    let mut fixture = Fixture::registered();
    fixture.open();
    let carried = fixture.on("get", DATA);
    fixture.select(&carried, 6);
    fixture.on("get", TRANSIENT);
    fixture.close(0);
    fixture.open();
    fixture
        .jar
        .attachment_player_cloned(&mut fixture.host, &fixture.budget, P, false);
    let copied = fixture.on("get", DATA);
    assert_ne!(copied, carried);
    assert_eq!(fixture.selected(&copied), 6);
    assert_eq!(fixture.on("has", TRANSIENT), Value::Int(0));
    fixture.close(0);
    assert_eq!(
        fixture.saved("demo:data"),
        Some(carry(&[("selected", Nbt::Int(6))]))
    );
    assert!(!fixture.store.contains_key(&(P, "demo:transient".into())));
}

#[test]
fn logout_forgets_the_objects_and_a_rejoin_decodes_the_saved_bytes_eagerly() {
    let mut fixture = Fixture::registered();
    fixture.open();
    let value = fixture.on("get", DATA);
    fixture.select(&value, 8);
    fixture.close(0);
    fixture.host.attachment_player_left(P);
    // The runtime no longer has the object: a store entry still naming it is refused, never
    // answered from the old heap object.
    fixture.open();
    assert_eq!(fixture.on("get", DATA), Value::Ref(None));
    assert_eq!(fixture.unknown("no longer has"), 1);
    fixture.close(0);
    fixture.restart();
    fixture.open();
    fixture
        .jar
        .attachment_player_joined(&mut fixture.host, &fixture.budget, P);
    assert_eq!(fixture.host.attachment_stats().decoded, 1);
    // The join's initial self-sync of the synced type, not sent.
    assert_eq!(fixture.unknown(SYNC_NOT_SENT), 2);
    let rejoined = fixture.on("get", DATA);
    assert_eq!(fixture.selected(&rejoined), 8);
    fixture.close(0);
}

#[test]
fn holder_calls_outside_a_server_host_or_on_another_holder_stay_counted() {
    let mut fixture = Fixture::registered();
    // No batch open: no ServerHost in this hook.
    assert_eq!(fixture.on("get", DATA), Value::Ref(None));
    assert_eq!(
        fixture.unknown("(attachment demo:data: no ServerHost in this hook)"),
        1
    );
    // A receiver that is not a player stays the host's, as before: a façade entity refuses the
    // unmodelled `getData` by name.
    fixture.open();
    let mob = text("game/entity/7");
    let answer = fixture.call(
        "get",
        &format!("({PLAYER}I)Ljava/lang/Object;"),
        &[mob, Value::Int(DATA)],
    );
    assert!(
        matches!(&answer, Err(NativeError::UnknownHostCall(r)) if r.name == "getData"),
        "{answer:?}"
    );
    assert_eq!(fixture.close(0), Vec::new());
}
