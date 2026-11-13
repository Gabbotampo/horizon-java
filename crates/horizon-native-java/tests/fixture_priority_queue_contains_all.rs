//! `PriorityQueue.containsAll` with jar `equals` (`priority_queue.rs`), and
//! `List.sort` with a comparator that changes the list (`jdk_stream.rs`).
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `K` logs `e<id>:<other id>` per `equals`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r##"package demo;

import java.util.*;

public final class Probe {
    static final StringBuilder log = new StringBuilder();
    static final class K implements Comparable<K> {
        final int id; K(int id) { this.id = id; }
        public int compareTo(K o) { return Integer.compare(id, o.id); }
        @Override public boolean equals(Object o) {
            log.append('e').append(id).append(':').append(o instanceof K k ? String.valueOf(k.id) : String.valueOf(o)).append(' ');
            return o instanceof K k && k.id == id;
        }
        @Override public int hashCode() { return id; }
    }
    static PriorityQueue<K> queue() { PriorityQueue<K> pq = new PriorityQueue<>(); for (int i = 1; i <= 4; i++) pq.add(new K(i)); log.setLength(0); return pq; }

    // The reviewer's program on #110.
    public static String containsAllList() { PriorityQueue<K> pq = queue(); boolean r = pq.containsAll(List.of(new K(2), new K(3))); return r + "#" + log; }
    public static String containsAllMissing() { PriorityQueue<K> pq = queue(); boolean r = pq.containsAll(new ArrayList<>(List.of(new K(9), new K(1)))); return r + "#" + log; }
    public static String containsAllEmpty() { PriorityQueue<K> pq = queue(); return pq.containsAll(List.of()) + "#" + log; }

    // A comparator that changes the list it sorts.
    static String sortWith(java.util.function.Consumer<List<Integer>> sideEffect) {
        List<Integer> l = new ArrayList<>(List.of(1, 2, 3, 4)); boolean[] once = {false};
        try { l.sort((a, b) -> { if (!once[0]) { once[0] = true; sideEffect.accept(l); } return Integer.compare(b, a); }); return "no" + l; }
        catch (ConcurrentModificationException e) { return "CME" + l; }
        catch (NullPointerException e) { return "NPE" + l; }
    }
    public static String sortComparatorAdds() { return sortWith(l -> l.add(0)); }
    public static String sortComparatorRemoves() { return sortWith(l -> l.remove(0)); }
    public static String sortComparatorSets() { return sortWith(l -> l.set(0, 7)); }
    public static String sortPlain() { return sortWith(l -> {}); }
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

/// The reviewer's program on #110: `containsAll` is `contains(e)` per
/// element of `c`, each `e.equals` over the heap array in order.
#[test]
fn priority_queue_contains_all_calls_equals() {
    assert_exact(
        "priority-queue-contains-all-calls-equals",
        &[
            ("containsAllList", "true#e2:1 e2:2 e3:1 e3:2 e3:3 "),
            ("containsAllMissing", "false#e9:1 e9:2 e9:3 e9:4 "),
            ("containsAllEmpty", "true#"),
            ("sortPlain", "no[4, 3, 2, 1]"),
        ],
    );
}

/// The JDK sorts around such a change (`[4, 3, 2, 7]`) and then checks
/// `modCount`: counted, ending in the exception a structural change gives.
#[test]
fn a_comparator_that_changes_the_list_is_counted() {
    let fixture = fixture("a-comparator-that-changes-the-list-is-co");
    let cases: &[(&str, &[(&str, usize)])] = &[
        (
            "sortComparatorAdds",
            &[("java/util/List#sort(Ljava/util/Comparator;)V", 1)],
        ),
        (
            "sortComparatorRemoves",
            &[("java/util/List#sort(Ljava/util/Comparator;)V", 1)],
        ),
        (
            "sortComparatorSets",
            &[("java/util/List#sort(Ljava/util/Comparator;)V", 1)],
        ),
    ];
    for (method, expected) in cases {
        let mut host = fixture.jar.seed_host(StdHost::new());
        fixture.text_on(&mut host, method);
        let counted = host.unknown_host_calls();
        for (call, count) in *expected {
            assert!(
                counted.iter().any(|(c, n)| c == call && n == count),
                "{method}: {counted:?}"
            );
        }
    }
}
