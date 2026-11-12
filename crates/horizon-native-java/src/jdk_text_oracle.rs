//! Replays what Java 25 answered (`jdk_text_oracle.json`, written by
//! `tools/jdk_tables/TextOracle.java`) through the runtime's own models, one
//! section each ([`SECTIONS`]): `Double.toString`/`Float.toString`
//! (`jdk_float_text.rs`), the `Character` statics (`jdk_character.rs`) and
//! `StringBuilder`/`StringBuffer` calls (`jdk_string_builder.rs`).
//!
//! Every expected value in the file is the JDK's output for a generated
//! input; nothing is derived from the code under test. The committed file is
//! a 300-case sample; the ignored test replays a larger run of the same
//! generator from `HORIZON_JDK_TEXT_ORACLE`.
//!
//! Java 25 is the spec: Minecraft 26.3's launcher manifest pins it
//! (`java-runtime-epsilon`, major version 25), and the sample's `jdk_build`
//! names the exact build that wrote it.

use serde_json::Value as Json;

use crate::jdk_string_builder::{Answer, Arg, Outcome, call};
use crate::{MethodRef, NativeError, Value};

const SAMPLE: &str = include_str!("jdk_text_oracle.json");

/// A JSON text: a string, or an array of UTF-16 code units (one that holds a
/// lone surrogate, which a JSON string cannot carry).
fn units(json: &Json) -> Option<Vec<u16>> {
    match json {
        Json::String(text) => Some(text.encode_utf16().collect()),
        Json::Array(units) => Some(
            units
                .iter()
                .map(|unit| unit.as_u64().unwrap() as u16)
                .collect(),
        ),
        _ => None,
    }
}

fn text(json: &Json) -> Option<String> {
    units(json).map(|units| String::from_utf16(&units).expect("a text without lone surrogates"))
}

fn argument(json: &Json) -> Arg {
    let (kind, value) = json.as_object().unwrap().iter().next().unwrap();
    match kind.as_str() {
        "int" => Arg::Int(value.as_i64().unwrap() as i32),
        "long" => Arg::Long(value.as_i64().unwrap()),
        "float" => Arg::Float(f64::from(f32::from_bits(value.as_u64().unwrap() as u32))),
        "double" => Arg::Double(f64::from_bits(value.as_str().unwrap().parse().unwrap())),
        "chars" => Arg::Chars(units(value)),
        // A text argument never holds a lone surrogate the runtime would
        // have to carry: the generator keeps those to `char` arguments.
        "text" => Arg::Text(units(value).map(|units| String::from_utf16_lossy(&units))),
        other => panic!("unknown argument kind {other}"),
    }
}

/// A capacity or length request the builder model leaves counted
/// (`MAX_UNITS` in `jdk_string_builder.rs`).
fn heap_sized(name: &str, args: &[Arg]) -> bool {
    let big = |arg: Option<&Arg>| matches!(arg, Some(Arg::Int(size)) if *size > 1 << 24);
    match name {
        "<init>" | "ensureCapacity" | "setLength" => big(args.first()),
        // Java 25 (and 27) overflow the length here (`repeat("z", Integer.MAX_VALUE)`
        // leaves the builder one unit shorter instead of throwing).
        "repeat" => big(args.get(1)),
        _ => false,
    }
}

/// Check every builder case of `oracle`; returns the failures.
fn check_builders(oracle: &Json) -> Vec<String> {
    let mut failures = Vec::new();
    for case in oracle["builders"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let descriptor = case["descriptor"].as_str().unwrap();
        let initial = text(&case["initial"]).unwrap();
        let args: Vec<Arg> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(argument)
            .collect();
        // A text argument with a lone surrogate cannot reach the model.
        if case["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|arg| arg.get("text").is_some_and(Json::is_array))
        {
            continue;
        }
        let outcome = call(name, descriptor, &initial, &args);
        let expect = &case["expect"];
        let matched = match (&outcome, expect) {
            (Outcome::Unmodelled, Json::String(word)) => word == "unmodelled",
            // A capacity or length past the model's bound is counted on
            // purpose: whether the JDK allocates it depends on its heap.
            (Outcome::Unmodelled, _) if heap_sized(name, &args) => true,
            (Outcome::Throw { class, message }, _) if expect.get("throws").is_some() => {
                let thrown = &expect["throws"];
                thrown["class"] == *class && text(&thrown["message"]).as_deref() == Some(message)
            }
            (
                Outcome::Done {
                    text: after,
                    answer,
                },
                _,
            ) if expect.get("after").is_some() => {
                let after = after.clone().unwrap_or_else(|| initial.clone());
                let expected = &expect["answer"];
                let answered = match answer {
                    Answer::This => expected == "this",
                    Answer::Void => expected == "void",
                    Answer::Int(value) => expected["int"] == *value,
                    Answer::Text(value) => text(&expected["text"]).as_deref() == Some(value),
                    Answer::WriteChars { at, units: written } => {
                        expected["write"]["at"] == *at
                            && units(&expected["write"]["units"]).as_ref() == Some(written)
                    }
                };
                answered && text(&expect["after"]).as_deref() == Some(after.as_str())
            }
            _ => false,
        };
        if !matched {
            failures.push(format!(
                "{} {name}{descriptor} on {initial:?} with {args:?}: got {outcome:?}, JDK {expect}",
                case["owner"]
            ));
        }
    }
    failures
}

fn check_float_text(oracle: &Json) -> Vec<String> {
    let mut failures = Vec::new();
    for pair in oracle["doubles"].as_array().unwrap() {
        let bits: u64 = pair[0].as_str().unwrap().parse().unwrap();
        let got = crate::jdk_float_text::double_to_string(f64::from_bits(bits));
        if got != pair[1] {
            failures.push(format!(
                "Double.toString({bits:#x}): got {got}, JDK {}",
                pair[1]
            ));
        }
    }
    for pair in oracle["floats"].as_array().unwrap() {
        let bits = pair[0].as_u64().unwrap() as u32;
        let got = crate::jdk_float_text::float_to_string(f64::from(f32::from_bits(bits)));
        if got != pair[1] {
            failures.push(format!(
                "Float.toString({bits:#x}): got {got}, JDK {}",
                pair[1]
            ));
        }
    }
    failures
}

/// What `jdk_character::static_call` gave, in the oracle's notation.
fn answered(result: Option<Result<Value, NativeError>>) -> Json {
    match result {
        None => Json::from("unmodelled"),
        Some(Ok(Value::Int(value))) => Json::from(value),
        Some(Ok(Value::Ref(None))) => Json::Null,
        Some(Ok(Value::Ref(Some(marker)))) => match marker.strip_prefix("unicode-block/") {
            Some(block) => serde_json::json!({ "block": block }),
            None => serde_json::json!({ "text": marker }),
        },
        Some(Err(NativeError::UncaughtException { class, message, .. })) => {
            serde_json::json!({ "throws": { "class": class, "message": message } })
        }
        Some(other) => Json::from(format!("{other:?}")),
    }
}

fn check_characters(oracle: &Json) -> Vec<String> {
    let mut failures = Vec::new();
    for case in oracle["characters"].as_array().unwrap() {
        let cp = case["cp"].as_i64().unwrap() as i32;
        for (key, expected) in case["answers"].as_object().unwrap() {
            // `name(descriptor)` with an optional `/second-argument`.
            let (call, second) = match key.rsplit_once('/') {
                Some((call, second)) if second.parse::<i32>().is_ok() => {
                    (call, second.parse::<i32>().ok())
                }
                _ => (key.as_str(), None),
            };
            let (owner, name, descriptor) = match call.split_once('(') {
                Some(("UnicodeBlock.of", params)) => (
                    "java/lang/Character$UnicodeBlock",
                    "of",
                    format!("({params}Ljava/lang/Character$UnicodeBlock;"),
                ),
                Some((name, rest)) => ("java/lang/Character", name, format!("({rest}")),
                None => panic!("bad key {key}"),
            };
            let r = MethodRef {
                owner: owner.into(),
                name: name.into(),
                descriptor: descriptor.clone(),
            };
            let mut args = vec![Value::Int(cp)];
            args.extend(second.map(Value::Int));
            let got = answered(crate::jdk_character::static_call(&r, &args));
            // The oracle writes texts and messages as strings (or code-unit
            // arrays); normalise both sides to strings.
            let normalised = |json: &Json| -> Json {
                let mut json = json.clone();
                if let Some(message) = json.pointer_mut("/throws/message")
                    && let Some(text) = text(message)
                {
                    *message = Json::from(text);
                }
                if let Some(value) = json.pointer_mut("/text")
                    && let Some(text) = text(value)
                {
                    *value = Json::from(text);
                }
                json
            };
            if normalised(expected) != normalised(&got) {
                failures.push(format!("{key} of {cp:#x}: got {got}, JDK {expected}"));
            }
        }
    }
    failures
}

/// The oracle's sections and the check that replays each.
type Check = fn(&Json) -> Vec<String>;
const SECTIONS: &[(&str, Check)] = &[
    ("doubles and floats", check_float_text),
    ("characters", check_characters),
    ("builders", check_builders),
];

/// Every section's failures, labelled by section, and each section's size.
fn check_all(oracle: &Json) -> (Vec<String>, Vec<String>) {
    let mut failures = Vec::new();
    let mut sizes = Vec::new();
    for (section, check) in SECTIONS {
        let found = check(oracle);
        sizes.push(format!("{section}: {} differ", found.len()));
        failures.extend(found);
    }
    for section in ["doubles", "floats", "characters", "builders"] {
        if let Some(cases) = oracle[section].as_array() {
            sizes.push(format!("{} {section}", cases.len()));
        }
    }
    (failures, sizes)
}

#[test]
fn the_committed_sample_answers_as_java_25() {
    let oracle: Json = serde_json::from_str(SAMPLE).unwrap();
    assert!(
        oracle["jdk"]
            .as_str()
            .is_some_and(|jdk| jdk.starts_with("25.")),
        "the sample must come from Java 25, the game's runtime: {}",
        oracle["jdk_build"]
    );
    let (failures, sizes) = check_all(&oracle);
    assert!(failures.is_empty(), "{sizes:?}:\n{}", failures.join("\n"));
}

/// The full differential: a larger run of the generator, from a file.
#[test]
#[ignore = "needs HORIZON_JDK_TEXT_ORACLE=<a TextOracle.java run>"]
fn a_generated_run_answers_as_the_jdk_that_wrote_it() {
    let path = std::env::var("HORIZON_JDK_TEXT_ORACLE").expect("HORIZON_JDK_TEXT_ORACLE");
    let oracle: Json = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let (failures, sizes) = check_all(&oracle);
    assert!(
        failures.is_empty(),
        "{sizes:?}:\n{}",
        failures
            .iter()
            .take(50)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
