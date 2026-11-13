//! The threads a jar starts are named in the report (`parked_threads.rs`).
//!
//! `Thread.start` stays a counted unknown call and the thread's body never
//! runs. `fixtures/parked_threads/Probe.java` starts three threads and makes a
//! fourth it never starts. The expected names come from the fixture's source:
//! the `Runnable` each thread was given, and the name passed to its
//! constructor.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{ParkedThread, StdHost};
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/parked_threads/Probe.java");

#[test]
fn started_threads_are_counted_and_name_what_never_ran() {
    let fixture = Fixture::build("parked-threads", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    // No body ran, so the flag every one of them would set is still false.
    assert_eq!(fixture.text_on(&mut host, "run"), "false");
    let parked = |name: Option<&str>, runnable: &str| ParkedThread {
        name: name.map(str::to_owned),
        runnable: runnable.to_owned(),
    };
    assert_eq!(
        host.parked_threads(),
        [
            parked(Some("watcher-1"), "demo/Probe$Worker"),
            parked(None, "demo/Probe#lambda$run$0"),
            parked(Some("named-only"), "null"),
        ]
    );
    assert_eq!(
        host.unknown_host_calls(),
        vec![("java/lang/Thread#start()V".to_owned(), 3)]
    );
    // The game's mod logs each parked thread once.
    assert_eq!(host.take_new_parked_threads().len(), 3);
    assert!(host.take_new_parked_threads().is_empty());
}
