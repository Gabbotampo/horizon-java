//! Synchronized views run on the collection they wrap (`lib.rs`'s dispatch,
//! `hash_collections.rs`).
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

public final class Probe {
    static String text(Collection<?> c) { StringBuilder sb = new StringBuilder("["); for (Object o : c) sb.append(o).append(','); return sb.append(']').toString(); }

    // The reviewer's R2 program on a `HashSet`, and other synchronized views.
    public static String syncHashSetToArray() {
        Set<Integer> c = Collections.synchronizedSet(new HashSet<>(List.of(9, 7, 0)));
        c.addAll(Arrays.asList(4, 6, 9));
        c.addAll(new LinkedHashSet<>(List.of(9)));
        StringBuilder sb = new StringBuilder(); for (Object o : c.toArray()) sb.append(o).append(','); return sb.toString();
    }
    public static String syncHashSetRemoveIf() {
        Set<Integer> s = new HashSet<>(List.of(1, 2, 3, 4, 5)); Set<Integer> v = Collections.synchronizedSet(s);
        boolean r = v.removeIf(x -> x % 2 == 0); return r + text(s);
    }
    public static String syncListLoopAdd() {
        List<Integer> l = Collections.synchronizedList(new ArrayList<>(List.of(1, 2, 3))); StringBuilder sb = new StringBuilder();
        try { for (Integer x : l) { sb.append(x.intValue()); if (x == 2) l.add(9); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb.toString();
    }
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

/// The reviewer's R2 program on a `HashSet` (`[0, 4, 6, 7, 9]`): a
/// synchronized view's calls run on the wrapped set's table, its `removeIf`
/// too, and a loop over a synchronized list checks the list's `modCount`.
#[test]
fn synchronized_views_run_the_wrapped_model() {
    assert_exact(
        "synchronized-views-run-the-wrapped-model",
        &[
            ("syncHashSetToArray", "0,4,6,7,9,"),
            ("syncHashSetRemoveIf", "true[1,3,5,]"),
            ("syncListLoopAdd", "12CME"),
        ],
    );
}
