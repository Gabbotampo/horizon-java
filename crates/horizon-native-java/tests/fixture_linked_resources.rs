//! A linked jar's resources reach `Class.getResourceAsStream`/`getResource`
//! (`linked_resources.rs`).
//!
//! `fixtures/linked_resources/Probe.java` printed `lib|own|absent|true` on JDK 25
//! (`jdk-25.0.5+8`) with `java -cp own.jar:lib.jar`: `own.jar` holds the probe
//! and `shared.properties` (`from=own`), `lib.jar` holds
//! `lib-defaults.properties` and its own `shared.properties` (both
//! `from=lib`). The class path's first jar answers a name both hold.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{NativeJar, StdHost, Value};
use javac_fixture::Fixture;
use std::io::Write;
use zip::write::SimpleFileOptions;

const PROBE: &str = include_str!("fixtures/linked_resources/Probe.java");

fn jar(entries: &[(&str, &[u8])]) -> NativeJar {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    NativeJar::from_bytes(&zip.finish().unwrap().into_inner()).unwrap()
}

#[test]
fn a_linked_jars_resources_follow_the_running_jars() {
    let fixture = Fixture::build_with_lib(
        "linked-resources",
        &[("demo/Probe.java", PROBE)],
        &[("shared.properties", b"from=own\n")],
    );
    let lib = jar(&[
        (
            "fabric.mod.json",
            br#"{"id":"resource-lib","version":"1.0"}"#,
        ),
        ("lib-defaults.properties", b"from=lib\n"),
        ("shared.properties", b"from=lib\n"),
    ]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    host.link_jar("lib", &lib).unwrap();
    let answer = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            "run",
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap();
    assert_eq!(answer, Value::Ref(Some("lib|own|absent|true".into())));
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
