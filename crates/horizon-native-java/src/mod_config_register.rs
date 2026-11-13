//! NeoForge's `ModContainer.registerConfig`, both overloads, over a spec the
//! `ModConfigSpec.Builder` model (`mod_config_spec.rs`) built.
//!
//! Ported from the bytecode of FancyModLoader 12.0.12 (`ModContainer`,
//! `config/ConfigTracker`, `config/ModConfig`, `config/ModConfig$Type`) and
//! `neoforge-26.3.0.101-universal.jar` (`ModConfigSpec.isEmpty`/`validateSpec`,
//! `$RestartType`, `$Range.test`, the `define*` validator lambdas), the build
//! the NeoForge 26.3 corpus pins:
//!
//! 1. `spec.isEmpty()`: the builder's live spec storage has no value (a value
//!    defined after `build()` counts, the storage is shared). An empty spec
//!    is logged at debug and dropped, whatever its type: nothing is tracked.
//! 2. `ConfigTracker.registerConfig(type, spec, container, fileName)`, where
//!    the two-argument form names the file `<modid>-<type>.toml` (the type's
//!    name in lower case):
//!    * `spec.validateSpec(modConfig)` walks the spec's value config (the
//!      values defined before `build()`, one per path, the last definition of
//!      a path winning at the path's first position) depth first in insertion
//!      order. For each value: a `GAME` restart in a `SYNCED` config throws
//!      `IllegalArgumentException("Configuration value <a.b> defined in config
//!      <file> has restart of type GAME which cannot be used for configs of
//!      type SYNCED")`; then `ValueSpec.test(getDefault())` (which runs a
//!      default supplier and the mod's predicates) failing throws
//!      `IllegalArgumentException("Configuration value <a.b> defined in config
//!      <file> has a validator that does not accept its own default value of
//!      <String.valueOf(default)>")`.
//!    * `trackConfig`: a file name already tracked throws
//!      `RuntimeException("Detected config file conflict on <file> from
//!      <modid> (already registered by <modid>)")`.
//!    * Only a `STARTUP` config is opened here, synchronously.
//!
//! # When a config loads, and what this answers
//!
//! The runtime's world is a fresh install inside the mod's constructor. In
//! 26.3, `ModConfig.Type` is `LOCAL`, `CLIENT`, `SYNCED` and `STARTUP` (there is
//! no `COMMON` or `SERVER`). `LOCAL` and, on a client, `CLIENT` configs load in
//! the loader's "Config loading" step after every mod is constructed
//! (`CommonModLoader.begin`); `SYNCED` loads when a server is about to start
//! (`ServerLifecycleHooks.handleServerAboutToStart`). So a `LOCAL`, `CLIENT` or
//! `SYNCED` registration is answered here, and the spec stays unloaded for the
//! rest of the constructor: `ConfigValue.get()` keeps throwing NeoForge's
//! `Cannot get config value before config is loaded.` and `isLoaded()` answers
//! `false`, exactly as on NeoForge, and no `ModConfigEvent` fires.
//!
//! A `STARTUP` config is different: `registerConfig` opens it on the spot. A
//! fresh install writes the spec's defaults to a new TOML file, reads it back
//! (`isCorrect`/`correct` can then change a value's boxed class), hands it to
//! the spec and posts `ModConfigEvent.Loading` to the mod's bus, which is live
//! during construction and runs the listeners registered so far. The runtime
//! models neither the TOML round trip nor the mod-bus listeners, so after
//! validation and tracking (both exact) the call is left to the host and
//! counted, and the spec is treated as possibly loaded (its `get()` and
//! `isLoaded()` are counted too).
//!
//! A validator that refuses its default is NeoForge's `IllegalArgumentException`
//! only while the run has counted no unknown host call: once one was answered
//! with a neutral value, the default may be built from it (carryon's came from
//! `Field.getInt` before that was modelled, and read `0` against a range that
//! starts at 1), so the registration is counted instead of thrown.
//!
//! Also counted, never answered: a container, type, spec or file name the
//! model cannot read (a null, `COMMON`/`SERVER`, a spec this model did not
//! build or whose builder had a counted call), and a spec holding a
//! `defineInList` value (`Collection.contains` on a mod's collection is not
//! answered here). A predicate, supplier or list the runtime cannot run or
//! read partway through validation stops the model there and the call is
//! counted.
//!
//! The tracker is the runtime's: it holds the configs this run registered.
//! Another mod's configs (a different jar, or another `@Mod` class run as a
//! separate entrypoint) are not visible, so a file name clash with one of
//! them is not detected.

use super::mod_config_spec::{
    B_DEFINED, B_TAINTED, B_VALUES, Check, S_BUILDER, S_COUNT, S_ESCAPED, V_CHECK, V_MAX, V_MIN,
    V_PATH, V_RESTART, V_VALIDATOR,
};
use super::*;

const MOD_CONTAINER: &str = "net/neoforged/fml/ModContainer";
const REGISTER_TWO: &str =
    "(Lnet/neoforged/fml/config/ModConfig$Type;Lnet/neoforged/fml/config/IConfigSpec;)V";
const REGISTER_THREE: &str = "(Lnet/neoforged/fml/config/ModConfig$Type;Lnet/neoforged/fml/config/IConfigSpec;Ljava/lang/String;)V";
/// Where this run's `ConfigTracker.fileMap` lives in the field table: a JSON
/// object from file name to the registering mod's id.
const TRACKER_FILES: &str = "fml-config-tracker|mcs#files";

/// One value of a spec, as `validateSpec` reaches it.
struct SpecValue {
    marker: String,
    path: Vec<String>,
}

/// A node of the spec's value config: night-config's insertion-ordered map.
enum Node {
    Value(usize),
    Level(Vec<(String, Node)>),
}

/// `Config.set(path, value)` on an insertion-ordered config: a new key goes
/// last, an existing key keeps its place and takes the new value. `None` when
/// the path runs through a value (night-config refuses that, and so does the
/// builder's own storage at the `define`, which this model does not check).
fn tree_set(level: &mut Vec<(String, Node)>, path: &[String], index: usize) -> Option<()> {
    let (head, rest) = path.split_first()?;
    let position = level.iter().position(|(key, _)| key == head);
    if rest.is_empty() {
        match position {
            Some(at) => match &mut level[at].1 {
                Node::Value(slot) => *slot = index,
                Node::Level(_) => return None,
            },
            None => level.push((head.clone(), Node::Value(index))),
        }
        return Some(());
    }
    let at = match position {
        Some(at) => at,
        None => {
            level.push((head.clone(), Node::Level(Vec::new())));
            level.len() - 1
        }
    };
    match &mut level[at].1 {
        Node::Level(inner) => tree_set(inner, rest, index),
        Node::Value(_) => None,
    }
}

/// `forEachValue` over the value config: depth first, in insertion order.
fn tree_walk(level: &[(String, Node)], out: &mut Vec<usize>) {
    for (_, node) in level {
        match node {
            Node::Value(index) => out.push(*index),
            Node::Level(inner) => tree_walk(inner, out),
        }
    }
}

/// `String.equalsIgnoreCase` with a constant: equal lengths, and each pair of
/// characters equal, or equal after `Character.toUpperCase`, or after
/// `toLowerCase` of that. The targets here are ASCII, so every character
/// compared is a single UTF-16 unit when the lengths agree.
fn java_equals_ignore_case(text: &str, target: &str) -> bool {
    fn upper(c: char) -> char {
        let mut it = c.to_uppercase();
        match (it.next(), it.next()) {
            (Some(single), None) => single,
            _ => c,
        }
    }
    fn lower(c: char) -> char {
        let mut it = c.to_lowercase();
        match (it.next(), it.next()) {
            (Some(single), None) => single,
            _ => c,
        }
    }
    if text.encode_utf16().count() != target.encode_utf16().count() {
        return false;
    }
    text.chars()
        .zip(target.chars())
        .all(|(a, b)| a == b || upper(a) == upper(b) || lower(upper(a)) == lower(upper(b)))
}

fn thrown(class: &str, message: String) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message: Some(message),
        frame: None,
    }
}

/// What `ValueSpec.test(getDefault())` came to, or that the model cannot say.
enum Tested {
    Pass,
    /// The default, for the message.
    Fail(Value),
    Unknown,
}

impl<H: Host> Runtime<'_, H> {
    /// `ModContainer.registerConfig(type, spec[, fileName])`, or `None` to
    /// leave the call to the host, which counts it.
    pub(super) fn mcs_register_config(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.owner != MOD_CONTAINER
            || r.name != "registerConfig"
            || !matches!(r.descriptor.as_str(), REGISTER_TWO | REGISTER_THREE)
        {
            return Ok(None);
        }
        let Some(mod_id) = receiver
            .and_then(expect_ref)
            .and_then(|marker| marker.strip_prefix("neoforge-mod-container/"))
            .and_then(|rest| rest.split_once('/'))
            .map(|(id, _)| id.to_owned())
        else {
            return Ok(None);
        };
        let Some(kind) = args
            .first()
            .and_then(expect_ref)
            .and_then(|marker| marker.strip_prefix("fml-config-type/"))
            .filter(|kind| matches!(*kind, "LOCAL" | "CLIENT" | "SYNCED" | "STARTUP"))
            .map(str::to_owned)
        else {
            return Ok(None);
        };
        let Some(spec) = args
            .get(1)
            .and_then(expect_ref)
            .filter(|spec| self.mcs_get(spec, S_BUILDER).is_some())
            .cloned()
        else {
            return Ok(None);
        };
        let builder = self
            .mcs_get(&spec, S_BUILDER)
            .and_then(expect_ref)
            .cloned()
            .unwrap_or_default();
        if self.mcs_int(&builder, B_TAINTED) != 0 {
            return Ok(None);
        }
        let file = if r.descriptor == REGISTER_THREE {
            match args.get(2).and_then(expect_ref) {
                Some(file) => file.clone(),
                None => return Ok(None),
            }
        } else {
            // `String.format(Locale.ROOT, "%s-%s.toml", modId, type.extension())`.
            format!("{mod_id}-{}.toml", kind.to_lowercase())
        };
        // `IConfigSpec.isEmpty()`: logged at debug, and nothing else happens.
        if self.mcs_int(&builder, B_DEFINED) == 0 {
            return Ok(Some(Value::Void));
        }
        let Some(values) = self.mcs_spec_values(&spec, &builder) else {
            return Ok(None);
        };
        // Every check the walk will make must be one the model answers before
        // the first predicate runs, so a counted call never follows a
        // half-run validation it could have refused up front.
        if values.iter().any(|value| {
            !matches!(
                self.mcs_get(&value.marker, V_CHECK)
                    .and_then(expect_ref)
                    .and_then(|name| Check::parse(name)),
                Some(check) if check != Check::InList
            )
        }) {
            return Ok(None);
        }
        for value in &values {
            let restart = self
                .mcs_get(&value.marker, V_RESTART)
                .and_then(expect_ref)
                .cloned();
            if kind == "SYNCED" && restart.as_deref() == Some("GAME") {
                return Err(thrown(
                    "java/lang/IllegalArgumentException",
                    format!(
                        "Configuration value {} defined in config {file} has restart of type GAME which cannot be used for configs of type {kind}",
                        value.path.join(".")
                    ),
                ));
            }
            match self.mcs_test_default(&value.marker)? {
                Tested::Pass => {}
                // A default the run may have built from a neutral answer
                // (an unknown host call was already counted) cannot
                // convict the mod's validator: counted, not thrown.
                Tested::Fail(_) if self.host.counted_unknown_calls() > 0 => {
                    self.mcs_set(&spec, S_ESCAPED, Value::Int(1));
                    return Ok(None);
                }
                Tested::Fail(default) => {
                    let text = self.value_of_text(
                        &default,
                        "Ljava/lang/Object;",
                        "net/neoforged/neoforge/common/ModConfigSpec",
                        "validateSpec",
                    )?;
                    return Err(thrown(
                        "java/lang/IllegalArgumentException",
                        format!(
                            "Configuration value {} defined in config {file} has a validator that does not accept its own default value of {text}",
                            value.path.join(".")
                        ),
                    ));
                }
                Tested::Unknown => {
                    self.mcs_set(&spec, S_ESCAPED, Value::Int(1));
                    return Ok(None);
                }
            }
        }
        // `trackConfig`: one config per file name.
        let mut files: BTreeMap<String, String> = self
            .fields
            .get(TRACKER_FILES)
            .and_then(expect_ref)
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_default();
        if let Some(owner) = files.get(&file) {
            return Err(thrown(
                "java/lang/RuntimeException",
                format!(
                    "Detected config file conflict on {file} from {mod_id} (already registered by {owner})"
                ),
            ));
        }
        files.insert(file, mod_id);
        let text = serde_json::to_string(&files).unwrap_or_else(|_| "{}".into());
        self.fields
            .insert(TRACKER_FILES.into(), Value::Ref(Some(text)));
        if kind == "STARTUP" {
            // `openConfig` now: a TOML round trip and a `ModConfigEvent.Loading`
            // post to the mod's live bus, neither modelled. Counted, and the
            // spec may be loaded from here on.
            self.mcs_set(&spec, S_ESCAPED, Value::Int(1));
            return Ok(None);
        }
        Ok(Some(Value::Void))
    }

    /// The spec's value config in `forEachValue` order: the values defined
    /// before its `build()`, the last definition of a path in that path's
    /// first place. `None` when a path runs through a value.
    fn mcs_spec_values(&self, spec: &str, builder: &str) -> Option<Vec<SpecValue>> {
        let count = usize::try_from(self.mcs_int(spec, S_COUNT)).ok()?;
        let defined: Vec<String> = self
            .mcs_list(builder, B_VALUES)
            .into_iter()
            .take(count)
            .collect();
        let paths: Vec<Vec<String>> = defined
            .iter()
            .map(|marker| self.mcs_list(marker, V_PATH))
            .collect();
        let mut root = Vec::new();
        for (index, path) in paths.iter().enumerate() {
            tree_set(&mut root, path, index)?;
        }
        let mut order = Vec::new();
        tree_walk(&root, &mut order);
        Some(
            order
                .into_iter()
                .map(|index| SpecValue {
                    marker: defined[index].clone(),
                    path: paths[index].clone(),
                })
                .collect(),
        )
    }

    /// `ValueSpec.test(configValue.getDefault())` for one value.
    fn mcs_test_default(&mut self, value: &str) -> Result<Tested, NativeError> {
        let Some(check) = self
            .mcs_get(value, V_CHECK)
            .and_then(expect_ref)
            .and_then(|name| Check::parse(name))
        else {
            return Ok(Tested::Unknown);
        };
        let Some(default) = self.mcs_default(value)? else {
            return Ok(Tested::Unknown);
        };
        let passed = match check {
            // A non-null default is an instance of its own class, and every
            // constant of an enum is in `getEnumConstants()`.
            Check::Class | Check::Enum => true,
            Check::Predicate => {
                let predicate = self
                    .mcs_get(value, V_VALIDATOR)
                    .cloned()
                    .unwrap_or(Value::Ref(None));
                match self.invoke_callback(&predicate, std::slice::from_ref(&default))? {
                    Some(answer) => truthy(&answer),
                    None => return Ok(Tested::Unknown),
                }
            }
            Check::Boolean => {
                if self.value_instance_of(&default, "java/lang/String") {
                    let text = expect_ref(&default).cloned().unwrap_or_default();
                    java_equals_ignore_case(&text, "true")
                        || java_equals_ignore_case(&text, "false")
                } else {
                    self.value_instance_of(&default, "java/lang/Boolean")
                }
            }
            Check::Range => {
                // `isNumber(o)` holds for the boxed `Integer`/`Long`/`Double`
                // ranges these overloads make, so the test compares
                // `doubleValue()`s with `dcmpg` (a NaN fails). Anything else
                // fails `clazz.isInstance(o)`.
                if !self.value_instance_of(&default, "java/lang/Number") {
                    false
                } else {
                    let bounds = (
                        self.mcs_get(value, V_MIN).and_then(boxed_number),
                        boxed_number(&default),
                        self.mcs_get(value, V_MAX).and_then(boxed_number),
                    );
                    match bounds {
                        (Some(min), Some(number), Some(max)) => min <= number && number <= max,
                        _ => return Ok(Tested::Unknown),
                    }
                }
            }
            Check::List => {
                if !self.value_instance_of(&default, "java/util/List") {
                    false
                } else {
                    let Ok(elements) = runtime_collection_values(&*self.host, &default) else {
                        return Ok(Tested::Unknown);
                    };
                    let element_validator = self
                        .mcs_get(value, V_VALIDATOR)
                        .cloned()
                        .unwrap_or(Value::Ref(None));
                    // `stream().allMatch`: stops at the first element refused.
                    let mut all = true;
                    for element in elements {
                        let Ok(element) = parse_value_marker(&element) else {
                            return Ok(Tested::Unknown);
                        };
                        match self.invoke_callback(&element_validator, &[element])? {
                            Some(answer) if truthy(&answer) => {}
                            Some(_) => {
                                all = false;
                                break;
                            }
                            None => return Ok(Tested::Unknown),
                        }
                    }
                    all
                }
            }
            Check::InList => return Ok(Tested::Unknown),
        };
        Ok(if passed {
            Tested::Pass
        } else {
            Tested::Fail(default)
        })
    }
}

/// The `boolean` a predicate returned.
fn truthy(value: &Value) -> bool {
    boxed_boolean(value).unwrap_or(false)
}
