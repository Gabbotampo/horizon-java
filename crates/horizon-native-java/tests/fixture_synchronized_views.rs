//! `Collections.synchronizedMap`/`List`/`Set` (`synchronized_views.rs`).
//!
//! `fixtures/synchronized_views/Probe.java` printed
//! `2|1|2|true|{a=1, b=2}|false|true|true|{b=2}|false|xy|1|[x, y]|[p, q]|true|npe:null`
//! on JDK 25 (`jdk-25.0.5+8`): writes through the view reach the wrapped
//! collection and the other way round, the view is not the same object, the
//! two are `equals` both ways, and a null throws an NPE with no message.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = include_str!("fixtures/synchronized_views/Probe.java");

#[test]
fn synchronized_views_run_every_call_on_what_they_wrap() {
    let fixture = Fixture::build("synchronized-views", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "run"),
        "2|1|2|true|{a=1, b=2}|false|true|true|{b=2}|false|xy|1|[x, y]|[p, q]|true|npe:null"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
