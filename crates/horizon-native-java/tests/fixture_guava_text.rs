//! Guava's `Splitter`, `Joiner` and `Lists.newArrayList(Iterable)` on a probe
//! compiled with `javac` (`guava_text.rs`).
//!
//! The expected strings are what the same `demo.Probe` printed on JDK 25
//! (`jdk-25.0.5+8`) against the real Guava 33.6.0 the 26.3 game ships. The
//! stubs here only let `javac` compile the probe and are never packed.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const SPLITTER_STUB: &str = r#"package com.google.common.base;

public class Splitter {
    public static Splitter on(String separator) { throw new AssertionError(); }
    public static Splitter on(char separator) { throw new AssertionError(); }
    public Iterable<String> split(CharSequence sequence) { throw new AssertionError(); }
}
"#;

const JOINER_STUB: &str = r#"package com.google.common.base;

public class Joiner {
    public static Joiner on(String separator) { throw new AssertionError(); }
    public static Joiner on(char separator) { throw new AssertionError(); }
    public final String join(Iterable<?> parts) { throw new AssertionError(); }
}
"#;

const LISTS_STUB: &str = r#"package com.google.common.collect;

public final class Lists {
    public static <E> java.util.ArrayList<E> newArrayList(Iterable<? extends E> elements) {
        throw new AssertionError();
    }
}
"#;

const PROBE: &str = include_str!("fixtures/guava_text/Probe.java");

#[test]
fn splitter_joiner_and_new_array_list_answer_like_guava() {
    let fixture = Fixture::build(
        "guava-text",
        &[
            ("com/google/common/base/Splitter.java", SPLITTER_STUB),
            ("com/google/common/base/Joiner.java", JOINER_STUB),
            ("com/google/common/collect/Lists.java", LISTS_STUB),
            ("demo/Probe.java", PROBE),
        ],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    assert_eq!(
        fixture.text_on(&mut host, "split"),
        "[a, b, , c, ]|5|[x, y]|[p, q:r, ]|[]|\
         IllegalArgumentException:The separator may not be the empty string.|\
         NullPointerException:null"
    );
    assert_eq!(
        fixture.text_on(&mut host, "join"),
        "general.value||a,b,|NullPointerException:null|k.l.m|NullPointerException:null"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}
