//! Replays what OpenJDK 27 answered (`jdk_text_oracle.json`, written by
//! `tools/jdk_tables/TextOracle.java`) through the runtime's own models, one
//! section each ([`SECTIONS`]): `Double.toString`/`Float.toString`
//! (`jdk_float_text.rs`). The file also records the `Character` and
//! `StringBuilder` answers the models that follow are replayed against.
//!
//! Every expected value in the file is the JDK's output for a generated
//! input; nothing is derived from the code under test. The committed file is
//! a 300-case sample; the ignored test replays a larger run of the same
//! generator from `HORIZON_JDK_TEXT_ORACLE`.

use serde_json::Value as Json;

const SAMPLE: &str = include_str!("jdk_text_oracle.json");

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

/// The oracle's sections and the check that replays each.
type Check = fn(&Json) -> Vec<String>;
const SECTIONS: &[(&str, Check)] = &[("doubles and floats", check_float_text)];

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
fn the_committed_sample_answers_as_jdk_27() {
    let oracle: Json = serde_json::from_str(SAMPLE).unwrap();
    assert_eq!(oracle["jdk"], "27");
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
