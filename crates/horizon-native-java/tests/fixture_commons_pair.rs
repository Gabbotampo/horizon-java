//! commons-lang3's `Pair`/`ImmutablePair` (`commons_pair.rs`).
//!
//! `fixtures/commons_pair/Probe.java` printed
//! `a|1|a|1|(a,1)|x|null|(x,null)|true|null|null|uoe:null` on JDK 25
//! (`jdk-25.0.5+8`) against the real `commons-lang3-3.20.0.jar` the 26.3 game
//! ships. The stubs below only let `javac` compile the probe and are never
//! packed.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PAIR_STUB: &str = r#"package org.apache.commons.lang3.tuple;

public abstract class Pair<L, R> implements java.util.Map.Entry<L, R> {
    public static <L, R> Pair<L, R> of(L left, R right) { throw new AssertionError(); }
    public abstract L getLeft();
    public abstract R getRight();
    public final L getKey() { throw new AssertionError(); }
    public R getValue() { throw new AssertionError(); }
}
"#;

const IMMUTABLE_PAIR_STUB: &str = r#"package org.apache.commons.lang3.tuple;

public class ImmutablePair<L, R> extends Pair<L, R> {
    public final L left = null;
    public final R right = null;
    public ImmutablePair(L left, R right) { throw new AssertionError(); }
    public static <L, R> Pair<L, R> left(L left) { throw new AssertionError(); }
    public static <L, R> Pair<L, R> right(R right) { throw new AssertionError(); }
    public L getLeft() { throw new AssertionError(); }
    public R getRight() { throw new AssertionError(); }
    public R setValue(R value) { throw new AssertionError(); }
}
"#;

const PROBE: &str = include_str!("fixtures/commons_pair/Probe.java");

#[test]
fn pairs_answer_like_commons_lang3() {
    let fixture = Fixture::build(
        "commons-pair",
        &[
            ("org/apache/commons/lang3/tuple/Pair.java", PAIR_STUB),
            (
                "org/apache/commons/lang3/tuple/ImmutablePair.java",
                IMMUTABLE_PAIR_STUB,
            ),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "run"),
        "a|1|a|1|(a,1)|x|null|(x,null)|true|null|null|uoe:null"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
