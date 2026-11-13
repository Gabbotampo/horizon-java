//! Watch services over the file model (`watch_service.rs`).
//!
//! `fixtures/watch_service/Probe.java`'s `run` changes files in a watched
//! directory one way at a time and drains the service after each. Its driver
//! on the oracle set `Probe.settle` to a 200 ms sleep so the JVM's poller
//! thread caught up, and ran it in an empty directory on JDK 25
//! (`jdk-25.0.5+8`, Linux, elysium) five times with identical output:
//! `fixtures/watch_service/jdk25-output.txt`. In the runtime `settle` stays a
//! no-op, because its events are settled when the change returns.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/watch_service/Probe.java");
const JDK_25: &str = include_str!("fixtures/watch_service/jdk25-output.txt");

#[test]
fn the_runs_own_file_changes_reach_its_watch_keys_like_jdk_25() {
    let fixture = Fixture::build("watch-service", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = fixture.text_on(&mut host, "run");
    for (line, (got, want)) in text.lines().zip(JDK_25.lines()).enumerate() {
        assert_eq!(got, want, "line {}", line + 1);
    }
    assert_eq!(text, JDK_25.trim_end_matches('\n'));
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}

/// What the JVM's timing decides is counted, not answered: an
/// `ENTRY_MODIFY`'s count (the kernel merges identical unread events), a
/// `take()` with nothing pending (it would block for ever), and a poll while
/// a writer holds unflushed text in the watched directory.
#[test]
fn timing_dependent_answers_are_counted() {
    let fixture = Fixture::build("watch-service-counted", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    // The count reads as the neutral 0 and `take` as null, both counted; the
    // poll still answers the key the writer's open signalled.
    assert_eq!(
        fixture.text_on(&mut host, "counted"),
        "ENTRY_CREATE1ENTRY_MODIFY0,null,true"
    );
    assert_eq!(
        host.unknown_host_calls(),
        vec![
            ("java/nio/file/WatchEvent#count()I".to_owned(), 1),
            (
                "java/nio/file/WatchService#poll()Ljava/nio/file/WatchKey;".to_owned(),
                1
            ),
            (
                "java/nio/file/WatchService#take()Ljava/nio/file/WatchKey;".to_owned(),
                1
            ),
        ]
    );
}
