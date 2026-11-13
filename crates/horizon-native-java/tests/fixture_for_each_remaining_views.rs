//! `forEachRemaining` over a live `keySet()` (#108) and a `PriorityQueue`
//! (heap order, #110) checks `modCount` (`iteration_guard.rs`).
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r##"package demo;

import java.util.*;
import java.util.function.*;

public final class Probe {
    static String show(Object x) { return x == null ? "n" : String.valueOf(x); }
    static <T> String remaining(Collection<T> c, Iterator<T> it, BiConsumer<T, StringBuilder> act) {
        StringBuilder sb = new StringBuilder();
        try { it.forEachRemaining(x -> { sb.append(show(x)); act.accept(x, sb); }); sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + c;
    }
    static List<Integer> list() { return new ArrayList<>(List.of(1, 2, 3, 4)); }

    public static String hashSetPutAtA() {
        Map<String, Integer> m = new HashMap<>(); m.put("a", 0); m.put("b", 1); m.put("c", 2); m.put("d", 3);
        Set<String> ks = m.keySet(); return remaining(ks, ks.iterator(), (x, sb) -> { if (x.equals("a")) m.put("z", 9); });
    }
    public static String queueAddAt1() { PriorityQueue<Integer> q = new PriorityQueue<>(list()); return remaining(q, q.iterator(), (x, sb) -> { if (x == 1) q.add(0); }); }
}
"##;

fn fixture(test: &str) -> Fixture {
    Fixture::build(test, &[("demo/Probe.java", PROBE)])
}

/// Runs each `(method, JDK 25 answer)` and checks that nothing was counted.
fn assert_exact(test: &str, cases: &[(&str, &str)]) {
    let fixture = fixture(test);
    for (method, expected) in cases {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let text = fixture.text_on(&mut host, method);
        assert_eq!(&text, expected, "{method}");
        assert_eq!(
            host.unknown_host_call_count(),
            0,
            "{method}: {:?}",
            host.unknown_host_calls()
        );
    }
}

/// A `put` on the map under a `keySet()` iterator, and an `add` to a
/// `PriorityQueue`, make the default `forEachRemaining`'s next `next()`
/// throw (`a` then CME, `1` then CME).
#[test]
fn for_each_remaining_over_a_view_and_a_queue() {
    assert_exact(
        "for-each-remaining-over-a-view-and-a-que",
        &[
            ("hashSetPutAtA", "aCME[a, b, c, d, z]"),
            ("queueAddAt1", "1CME[0, 1, 3, 4, 2]"),
        ],
    );
}
