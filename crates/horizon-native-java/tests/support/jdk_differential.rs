#![allow(dead_code)]

//! A differential check of a javac-built probe against JDK 25's output.
//!
//! Each probe method is `static String m()`: it exercises one behaviour,
//! logs every call the JDK body makes into the probe's own overrides, and
//! returns `"<log> => <result>"` (or the exception's class and message). The
//! expected strings are the lines the same probe printed on OpenJDK 25.0.5
//! (`javac --release 21`, `java -cp out demo.Probe`), pasted verbatim, so the
//! runtime must make the same calls in the same order and give the same
//! answers. A case must also count no unknown host call: an exact body has
//! nothing to count.
//!
//! Include it beside `javac_fixture.rs`:
//! `#[path = "support/jdk_differential.rs"] mod jdk_differential;`.

use horizon_native_java::{StdHost, Value};

use super::javac_fixture::Fixture;

/// Build `source` as `demo/Probe.java` once and run every `(method,
/// expected)` case on a fresh host, reporting every mismatch together.
pub fn check(test: &str, source: &str, cases: &[(&str, &str)]) {
    let fixture = Fixture::build(test, &[("demo/Probe.java", source)]);
    let mut failures = Vec::new();
    for (method, expected) in cases {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let answer = fixture.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        let unknown = host.unknown_host_calls();
        match answer {
            Ok(Value::Ref(Some(text))) if text == *expected && unknown.is_empty() => {}
            Ok(Value::Ref(Some(text))) => failures.push(format!(
                "{method}\n  jdk 25: {expected}\n  native: {text}\n  unknown: {unknown:?}"
            )),
            other => failures.push(format!(
                "{method}\n  jdk 25: {expected}\n  native: {other:?}\n  unknown: {unknown:?}"
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from JDK 25:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
