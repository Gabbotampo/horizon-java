//! A run that makes more than 256 host collections keeps each one its own.
//!
//! `fixtures/host_collections/ManyLists.java` printed
//! `1200|3[a, b, c]|2[a, b]|c|[a, b]` on JDK 25 (`jdk-25.0.5+8`) with the real
//! Guava 33.6.0 the game ships. It makes 400 `Lists.newArrayList` copies (each
//! a collection the host builds), then two more lists and a `subList`. Past
//! the old limit of 256 the host handed every new one the same shared marker:
//! the later lists overwrote `three`'s row, so `three.size()` answered the
//! last write and `three.get(2)` threw `IndexOutOfBoundsException` (the shape
//! night-config's `AbstractConfig.set` hit in particular).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const LISTS_STUB: &str = r#"package com.google.common.collect;

public final class Lists {
    public static <E> java.util.ArrayList<E> newArrayList(Iterable<? extends E> elements) {
        throw new AssertionError();
    }
}
"#;

const PROBE: &str = include_str!("fixtures/host_collections/ManyLists.java");

#[test]
fn collections_past_the_old_limit_stay_distinct() {
    let fixture = Fixture::build(
        "host-collection-count",
        &[
            ("com/google/common/collect/Lists.java", LISTS_STUB),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "run"),
        "1200|3[a, b, c]|2[a, b]|c|[a, b]"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
