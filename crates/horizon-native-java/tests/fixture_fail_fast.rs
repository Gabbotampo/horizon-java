//! Fail-fast iteration (`iteration_guard.rs`), `ArrayList.removeIf`, and
//! `ArrayDeque`'s lookups by `equals` (`list_equality.rs`), as JDK 25 runs
//! them.
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `K` logs each `equals` as `e<id>:<other id>`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r##"package demo;

import java.util.*;

public final class Probe {
    static final StringBuilder log = new StringBuilder();
    /// Logs `e<id>:<other id>` per `equals`; equal by id.
    static final class K {
        final int id; K(int id) { this.id = id; }
        @Override public boolean equals(Object o) {
            log.append('e').append(id).append(':').append(o instanceof K k ? String.valueOf(k.id) : String.valueOf(o)).append(' ');
            return o instanceof K k && k.id == id;
        }
        @Override public int hashCode() { return id; }
        @Override public String toString() { return "K" + id; }
    }
    static String it(Iterable<?> c) { StringBuilder sb = new StringBuilder("["); for (Object o : c) sb.append(o).append(','); return sb.append(']').toString(); }

    /// The reviewer's `ArrayDeque` cases on #105.
    public static String dequeLookups() {
        log.setLength(0);
        ArrayDeque<K> dq = new ArrayDeque<>();
        for (int i = 0; i < 4; i++) dq.add(new K(i));
        String a = dq.contains(new K(2)) + "#" + log; log.setLength(0);
        String b = dq.remove(new K(1)) + it(dq) + "#" + log; log.setLength(0);
        dq.add(new K(0));
        String c = dq.removeLastOccurrence(new K(0)) + it(dq) + "#" + log; log.setLength(0);
        String d = dq.removeFirstOccurrence(new K(9)) + "|" + dq.contains(null) + "#" + log;
        return a + "|" + b + "|" + c + "|" + d;
    }

    /// The reviewer's `removeIf` that adds inside the predicate.
    public static String removeIfAdds() {
        List<Integer> l = new ArrayList<>(List.of(1, 2, 3));
        String r;
        try { r = String.valueOf(l.removeIf(x -> { if (x == 1) l.add(7); return x == 2; })); }
        catch (ConcurrentModificationException e) { r = "CME"; }
        return r + l;
    }

    public static String removeIfPlain() {
        List<Integer> l = new ArrayList<>(List.of(1, 2, 3, 4, 5, 6));
        boolean a = l.removeIf(x -> x % 2 == 0);
        boolean b = l.removeIf(x -> x > 10);
        return a + "|" + b + "|" + l;
    }

    /// The predicate throws on its third test: nothing was removed yet.
    public static String removeIfThrows() {
        List<Integer> l = new ArrayList<>(List.of(1, 2, 3, 4));
        int[] n = {0};
        try { l.removeIf(x -> { if (++n[0] == 3) throw new IllegalStateException("p"); return x == 1; }); }
        catch (IllegalStateException e) { return "threw" + l; }
        return "no" + l;
    }

    /// The reviewer's `forEach` that adds.
    public static String forEachAdds() {
        List<Integer> l = new ArrayList<>(List.of(1, 2, 3, 4));
        StringBuilder sb = new StringBuilder();
        try { l.forEach(x -> { sb.append(x); if (x == 2) l.add(9); }); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + l.size();
    }

    /// The reviewer's iterator after an `add`.
    public static String iteratorAfterAdd() {
        List<Integer> l = new ArrayList<>(List.of(1, 2, 3));
        Iterator<Integer> i = l.iterator();
        i.next();
        l.add(4);
        boolean more = i.hasNext();
        try { i.next(); return more + "no"; } catch (ConcurrentModificationException e) { return more + "CME"; }
    }

    /// Removing the second-to-last element ends the loop: no exception.
    public static String removeSecondToLast() {
        List<String> l = new ArrayList<>(List.of("a", "b", "c"));
        StringBuilder sb = new StringBuilder();
        try { for (String s : l) { sb.append(s); if (s.equals("b")) l.remove("a"); } }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + l;
    }

    public static String forEachRemove() {
        List<String> l = new ArrayList<>(List.of("a", "b", "c", "d"));
        StringBuilder sb = new StringBuilder();
        try { for (String s : l) { sb.append(s); if (s.equals("a")) l.remove("c"); } }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + l;
    }

    public static String linkedListLoops() {
        LinkedList<String> l = new LinkedList<>(List.of("a", "b", "c"));
        StringBuilder sb = new StringBuilder();
        try { for (String s : l) { sb.append(s); if (s.equals("a")) l.add("z"); } }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        try { l.forEach(s -> { sb.append(s); if (s.equals("b")) l.removeLast(); }); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + l;
    }

    public static String vectorForEach() {
        Vector<Integer> v = new Vector<>(List.of(1, 2, 3));
        StringBuilder sb = new StringBuilder();
        try { v.forEach(x -> { sb.append(x); if (x == 1) v.add(5); }); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + v;
    }

    /// `set` while iterating is not structural: no exception.
    public static String setWhileIterating() {
        List<String> l = new ArrayList<>(List.of("a", "b", "c"));
        StringBuilder sb = new StringBuilder();
        for (String s : l) { sb.append(s); if (s.equals("a")) l.set(2, "z"); }
        return sb + "" + l;
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

/// The reviewer's case on #105: `ArrayDeque.contains`/`remove(Object)`/
/// `removeFirstOccurrence` run `o.equals(e)` head to tail and
/// `removeLastOccurrence` tail to head (the host compared by marker).
#[test]
fn array_deque_looks_up_by_equals() {
    assert_exact(
        "array-deque-looks-up-by-equals",
        &[(
            "dequeLookups",
            "true#e2:0 e2:1 e2:2 |true[K0,K2,K3,]#e1:0 e1:1 |true[K0,K2,K3,]#e0:0 |false|false#e9:0 e9:2 e9:3 ",
        )],
    );
}

/// `ArrayList.removeIf` tests every element first and throws
/// `ConcurrentModificationException` before removing anything if the
/// predicate changed the list, which keeps what it added (the reviewer's
/// case: `[1, 2, 3, 7]`; the host lost the 7).
#[test]
fn array_list_remove_if_is_jdk_25s() {
    assert_exact(
        "array-list-remove-if-is-jdk-25s",
        &[
            ("removeIfAdds", "CME[1, 2, 3, 7]"),
            ("removeIfPlain", "true|false|[1, 3, 5]"),
            ("removeIfThrows", "threw[1, 2, 3, 4]"),
        ],
    );
}

/// `forEach` and iterators throw on the next step after a structural
/// change (the reviewer's programs), and removing the second-to-last
/// element ends an `ArrayList` loop quietly, as `cursor != size` does.
#[test]
fn fail_fast_iteration_throws_where_jdk_25_does() {
    assert_exact(
        "fail-fast-iteration-throws-where-jdk-25-",
        &[
            ("forEachAdds", "12CME5"),
            ("iteratorAfterAdd", "trueCME"),
            ("removeSecondToLast", "ab[b, c]"),
            ("forEachRemove", "aCME[a, b, d]"),
            ("linkedListLoops", "aCMEabCME[a, b, c]"),
            ("vectorForEach", "1CME[1, 2, 3, 5]"),
        ],
    );
}

/// A list of the same size with other elements: a `set` (no `modCount`)
/// or a remove and an add (a JVM throws). Counted.
#[test]
fn a_set_or_a_remove_and_add_while_iterating_is_counted() {
    let fixture = fixture("a-set-or-a-remove-and-add-while-iteratin");
    let cases: &[(&str, &[(&str, usize)])] = &[(
        "setWhileIterating",
        &[("java/util/Iterator#next()Ljava/lang/Object;", 2)],
    )];
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
