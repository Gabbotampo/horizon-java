//! `Iterator.forEachRemaining` on fail-fast iterators checks `modCount` as
//! JDK 25's classes do (`iteration_guard.rs`, with `collection_callback.rs`).
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `n` prints a `null` the action was handed.

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

    public static String arrayListPlain() { List<Integer> l = list(); return remaining(l, l.iterator(), (x, sb) -> {}); }
    public static String arrayListAddAt2() { List<Integer> l = list(); return remaining(l, l.iterator(), (x, sb) -> { if (x == 2) l.add(9); }); }
    public static String arrayListAddAtLast() { List<Integer> l = list(); return remaining(l, l.iterator(), (x, sb) -> { if (x == 4) l.add(9); }); }
    public static String arrayListSetMidway() { List<Integer> l = list(); Iterator<Integer> it = l.iterator(); it.next(); return remaining(l, it, (x, sb) -> { if (x == 2) l.set(3, 8); }); }
    public static String linkedAddAtLast() { List<Integer> l = new LinkedList<>(list()); return remaining(l, l.iterator(), (x, sb) -> { if (x == 4) l.add(9); }); }
    public static String linkedRemoveAt2() { List<Integer> l = new LinkedList<>(list()); return remaining(l, l.iterator(), (x, sb) -> { if (x == 2) l.remove(Integer.valueOf(4)); }); }
    public static String hashSetPutAtA() {
        Map<String, Integer> m = new HashMap<>(); m.put("a", 0); m.put("b", 1); m.put("c", 2); m.put("d", 3);
        Set<String> ks = m.keySet(); return remaining(ks, ks.iterator(), (x, sb) -> { if (x.equals("a")) m.put("z", 9); });
    }
    public static String hashSetPlain() { Set<String> s = new HashSet<>(List.of("a", "b", "c")); return remaining(s, s.iterator(), (x, sb) -> {}); }
    public static String treeSetAddAtLast() { TreeSet<Integer> s = new TreeSet<>(list()); return remaining(s, s.iterator(), (x, sb) -> { if (x == 4) s.add(9); }); }
    public static String treeSetAddAt2() { TreeSet<Integer> s = new TreeSet<>(list()); return remaining(s, s.iterator(), (x, sb) -> { if (x == 2) s.add(9); }); }
    public static String queueAddAt1() { PriorityQueue<Integer> q = new PriorityQueue<>(list()); return remaining(q, q.iterator(), (x, sb) -> { if (x == 1) q.add(0); }); }
    public static String dequePollLast() { ArrayDeque<Integer> d = new ArrayDeque<>(list()); return remaining(d, d.iterator(), (x, sb) -> { if (x != null && x == 2) d.pollLast(); }); }
    public static String dequeMidway() { ArrayDeque<Integer> d = new ArrayDeque<>(list()); Iterator<Integer> it = d.iterator(); it.next(); return remaining(d, it, (x, sb) -> {}); }
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

/// `ArrayList.Itr`/`LinkedList.ListItr` check `modCount` in the loop and
/// after it (an `add` at the last element throws); `TreeSet`'s iterator runs
/// the default (`hasNext`/`next`), which does not; `ArrayDeque`'s hands an
/// emptied slot as `null` and throws if `tail` moved.
#[test]
fn for_each_remaining_checks_mod_count_like_its_class() {
    assert_exact(
        "for-each-remaining-checks-mod-count-like",
        &[
            ("arrayListPlain", "1234no[1, 2, 3, 4]"),
            ("arrayListAddAt2", "12CME[1, 2, 3, 4, 9]"),
            ("arrayListAddAtLast", "1234CME[1, 2, 3, 4, 9]"),
            ("arrayListSetMidway", "238no[1, 2, 3, 8]"),
            ("linkedAddAtLast", "1234CME[1, 2, 3, 4, 9]"),
            ("linkedRemoveAt2", "12CME[1, 2, 3]"),
            ("hashSetPlain", "abcno[a, b, c]"),
            ("treeSetAddAtLast", "1234no[1, 2, 3, 4, 9]"),
            ("treeSetAddAt2", "12CME[1, 2, 3, 4, 9]"),
            ("dequePollLast", "123nCME[1, 2, 3]"),
            ("dequeMidway", "234no[1, 2, 3, 4]"),
        ],
    );
}
