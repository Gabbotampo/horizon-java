//! `Files.move`/`Files.copy` between two paths (`files_move.rs`).
//!
//! `fixtures/files_move/Probe.java`, run in an empty directory, printed
//! `b.toml,false,one|FileAlreadyExistsException:b.toml|two|three,false|a.toml,three,true|FileAlreadyExistsException:a.toml|NoSuchFileException:missing.toml|NoSuchFileException:missing.toml -> b.toml|UnsupportedOperationException:Unsupported option: COPY_ATTRIBUTES|UnsupportedOperationException:Unsupported copy option: ATOMIC_MOVE|b.toml,three|NullPointerException:null`
//! on JDK 25 (`jdk-25.0.5+8`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/files_move/Probe.java");

#[test]
fn move_and_copy_between_paths_answer_like_jdk_25() {
    let fixture = Fixture::build("files-move", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "run"),
        "b.toml,false,one|FileAlreadyExistsException:b.toml|two|three,false|\
         a.toml,three,true|FileAlreadyExistsException:a.toml|\
         NoSuchFileException:missing.toml|NoSuchFileException:missing.toml -> b.toml|\
         UnsupportedOperationException:Unsupported option: COPY_ATTRIBUTES|\
         UnsupportedOperationException:Unsupported copy option: ATOMIC_MOVE|\
         b.toml,three|NullPointerException:null"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
