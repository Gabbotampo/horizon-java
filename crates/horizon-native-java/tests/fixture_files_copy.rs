//! `Files.copy(InputStream, Path, CopyOption...)` (`files_copy.rs`).
//!
//! `fixtures/files_copy/Probe.java`, run from a jar holding `defaults.toml`
//! (`enabled = true\n`) in an empty directory, printed
//! `15|enabled = true|FileAlreadyExistsException:cfg.toml|15|0|0|UnsupportedOperationException:COPY_ATTRIBUTES not supported|NullPointerException:options contains 'null'|NullPointerException:null`
//! on JDK 25 (`jdk-25.0.5+8`): the copy's byte count, the file it wrote, the
//! existing target refused without `REPLACE_EXISTING`, a replaced copy, an
//! exhausted stream copying nothing, and the three argument checks.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/files_copy/Probe.java");

#[test]
fn files_copy_writes_the_stream_into_the_file_model() {
    let fixture = Fixture::build_with_lib(
        "files-copy",
        &[("demo/Probe.java", PROBE)],
        &[("defaults.toml", b"enabled = true\n")],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "run"),
        "15|enabled = true|FileAlreadyExistsException:cfg.toml|15|0|0|\
         UnsupportedOperationException:COPY_ATTRIBUTES not supported|\
         NullPointerException:options contains 'null'|NullPointerException:null"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
