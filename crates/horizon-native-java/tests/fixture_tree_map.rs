//! `TreeMap`/`TreeSet` as JDK 25 runs them (`tree_map.rs`): the red-black
//! tree's `compareTo`/comparator calls, its order, and its navigation.
//!
//! Every expected value is OpenJDK 25.0.5's output for the same sources
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `RandomTree` is generated (`gen_tree.py 1 150`): random
//! `TreeMap`/`TreeSet` operations, natural order or a comparator, over
//! records that log every comparison as `a<b` (`a>b` for the comparator),
//! printing the iteration order after every operation.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"package demo;

import java.util.*;

public final class Probe {
    /// Orders by `n` only.
    record Ver(int n, String tag) implements Comparable<Ver> {
        public int compareTo(Ver o) { return Integer.compare(n, o.n); }
    }
    enum Letter { A, B, C }

    /// The reviewer's programs on #98: a key equal by `compareTo` replaces the
    /// value (keeping the first key); iteration is sorted, for records,
    /// enums and `Integer`s.
    public static String reviewerCases() {
        TreeMap<Ver, String> t = new TreeMap<>();
        t.put(new Ver(1, "a"), "x");
        String old = t.put(new Ver(1, "b"), "y");
        TreeSet<Ver> s = new TreeSet<>();
        boolean first = s.add(new Ver(2, "p"));
        boolean second = s.add(new Ver(2, "q"));
        TreeSet<Ver> order = new TreeSet<>();
        order.add(new Ver(3, "c")); order.add(new Ver(1, "a")); order.add(new Ver(2, "b"));
        StringBuilder sb = new StringBuilder();
        for (Ver v : order) sb.append(v.tag());
        TreeSet<Letter> letters = new TreeSet<>(List.of(Letter.C, Letter.A, Letter.B));
        for (Letter l : letters) sb.append(l.name());
        TreeMap<Integer, String> ints = new TreeMap<>();
        ints.put(5, "a"); ints.put(1, "b"); ints.put(3, "c");
        return t.size() + "|" + old + "|" + t.get(new Ver(1, "zz")) + "|" + t.firstKey().tag()
            + "|" + first + "|" + second + "|" + s.size() + "|" + sb + "|" + ints.keySet();
    }

    /// A sorted set finds by `compareTo`, not `equals`.
    public static String containsByCompareTo() {
        TreeSet<Ver> s = new TreeSet<>(List.of(new Ver(15, "a"), new Ver(3, "b")));
        return s.contains(new Ver(15, "c")) + "|" + s.remove(new Ver(3, "z")) + "|" + s.first().tag() + s.size();
    }

    /// Navigation with a comparator, and copies of sorted sources.
    public static String navigation() {
        TreeMap<Integer, String> m = new TreeMap<>(Comparator.reverseOrder());
        for (int i : new int[] {10, 40, 20, 30}) m.put(i, "v" + i);
        TreeMap<Integer, String> copy = new TreeMap<>(m);
        return m.firstKey() + "|" + m.floorKey(25) + "|" + m.ceilingKey(25) + "|" + m.higherKey(40)
            + "|" + m.lowerKey(10) + "|" + m.pollLastEntry().getKey() + "|" + m + "|" + copy.firstKey()
            + "|" + new TreeSet<>(m.keySet()).first();
    }

    /// A navigable view is not modelled: counted.
    public static String headMap() {
        TreeMap<Integer, String> m = new TreeMap<>();
        m.put(1, "a"); m.put(2, "b");
        return String.valueOf(m.headMap(2).size());
    }

    public static String random_m0() { return RandomTree.m0(); }
    public static String random_m1() { return RandomTree.m1(); }
    public static String random_m2() { return RandomTree.m2(); }
    public static String random_m3() { return RandomTree.m3(); }
    public static String random_m4() { return RandomTree.m4(); }
    public static String random_m5() { return RandomTree.m5(); }
    public static String random_m6() { return RandomTree.m6(); }
    public static String random_m7() { return RandomTree.m7(); }
    public static String random_m8() { return RandomTree.m8(); }
    public static String random_m9() { return RandomTree.m9(); }
    public static String random_m10() { return RandomTree.m10(); }
    public static String random_m11() { return RandomTree.m11(); }
    public static String random_m12() { return RandomTree.m12(); }
    public static String random_m13() { return RandomTree.m13(); }
}
"#;

const RANDOM_TREE: &str = r#"package demo;

import java.util.*;

/// Seeded random tree map and set programs (`gen_tree.py 1 150`, the first 14).
final class RandomTree {
    static final StringBuilder log = new StringBuilder();
    /** Orders by n only (tags differ for equal-ordering keys); logs n<n. */
    record C(int n, String tag) implements Comparable<C> {
        public int compareTo(C o) { log.append(n).append('<').append(o.n).append(' '); return Integer.compare(n, o.n); }
        @Override public String toString() { return n + tag; }
    }
    static final Comparator<C> DESC = (a, b) -> { log.append(a.n()).append('>').append(b.n()).append(' '); return Integer.compare(b.n(), a.n()); };
    static String it(Iterable<?> c) {
        StringBuilder sb = new StringBuilder("[");
        for (Object o : c) sb.append(o).append(',');
        return sb.append(']').toString();
    }
    static String es(Map.Entry<C, Integer> e) { return e == null ? "null" : e.getKey() + "=" + e.getValue(); }
    static String run(java.util.function.Supplier<Object> op) {
        try {
            return String.valueOf(op.get());
        }
        catch (RuntimeException e) { return "!" + e.getClass().getSimpleName(); }
    }

    public static String m0() {
        log.setLength(0);
        TreeMap<C, Integer> a = new TreeMap<>();
        a.put(new C(3, "b"), 3);
        a.put(new C(15, "c"), 3);
        a.put(new C(6, "a"), 3);
        a.put(new C(0, "b"), 3);
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> new TreeMap<>(a).size())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.put(new C(3, "b"), 0))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.replace(new C(20, "c"), 7))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.containsValue(4))).append(';');
        log.append(it(a.keySet())).append('|');
        return log.toString();
    }
    public static String m1() {
        log.setLength(0);
        TreeSet<C> a = new TreeSet<>();
        a.add(new C(7, "b"));
        a.add(new C(7, "c"));
        a.add(new C(7, "b"));
        a.add(new C(9, "a"));
        a.add(new C(13, "c"));
        a.add(new C(20, "a"));
        a.add(new C(5, "c"));
        a.add(new C(23, "b"));
        log.append(it(a)).append('|');
        log.append(run(() -> a.pollFirst())).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> { TreeSet<C> b = new TreeSet<>(); b.addAll(List.of(new C(18, "b"), new C(16, "b"), new C(18, "a"))); return it(b); })).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.lower(new C(7, "c")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.lower(new C(2, "b")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a)).append('|');
        return log.toString();
    }
    public static String m2() {
        log.setLength(0);
        TreeSet<C> a = new TreeSet<>();
        a.add(new C(7, "a"));
        a.add(new C(24, "a"));
        a.add(new C(17, "c"));
        a.add(new C(7, "b"));
        a.add(new C(16, "b"));
        a.add(new C(18, "b"));
        a.add(new C(14, "b"));
        a.add(new C(21, "c"));
        log.append(it(a)).append('|');
        log.append(run(() -> a.add(new C(12, "c")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.lower(new C(15, "b")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> { TreeSet<C> b = new TreeSet<>(); b.addAll(a); return it(b); })).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.contains(new C(19, "a")))).append(';');
        log.append(it(a)).append('|');
        return log.toString();
    }
    public static String m3() {
        log.setLength(0);
        TreeSet<C> a = new TreeSet<>();
        log.append(it(a)).append('|');
        log.append(run(() -> a.remove(new C(2, "a")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.last())).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> { TreeSet<C> b = new TreeSet<>(); b.addAll(List.of(new C(20, "c"), new C(9, "b"), new C(22, "b"))); return it(b); })).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.floor(new C(15, "a")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.pollLast())).append(';');
        log.append(it(a)).append('|');
        return log.toString();
    }
    public static String m4() {
        log.setLength(0);
        TreeMap<C, Integer> a = new TreeMap<>();
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.containsValue(3))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> new TreeMap<>(a).size())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.lastKey())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.containsKey(new C(4, "a")))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.ceilingKey(new C(2, "b")))).append(';');
        log.append(it(a.keySet())).append('|');
        return log.toString();
    }
    public static String m5() {
        log.setLength(0);
        TreeSet<C> a = new TreeSet<>();
        log.append(it(a)).append('|');
        log.append(run(() -> a.add(new C(18, "a")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.pollFirst())).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.higher(new C(15, "a")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.first())).append(';');
        log.append(it(a)).append('|');
        return log.toString();
    }
    public static String m6() {
        log.setLength(0);
        TreeMap<C, Integer> a = new TreeMap<>();
        a.put(new C(21, "a"), 3);
        a.put(new C(17, "b"), 4);
        a.put(new C(15, "c"), 1);
        a.put(new C(2, "c"), 0);
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.replace(new C(4, "a"), 7))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.firstKey())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.lastKey())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> new TreeMap<>(a).size())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.ceilingKey(new C(17, "a")))).append(';');
        log.append(it(a.keySet())).append('|');
        return log.toString();
    }
    public static String m7() {
        log.setLength(0);
        TreeMap<C, Integer> a = new TreeMap<>();
        a.put(new C(4, "b"), 0);
        a.put(new C(19, "c"), 3);
        a.put(new C(2, "c"), 4);
        a.put(new C(7, "c"), 0);
        a.put(new C(8, "b"), 2);
        a.put(new C(18, "c"), 0);
        a.put(new C(14, "b"), 0);
        a.put(new C(1, "b"), 0);
        a.put(new C(19, "c"), 0);
        a.put(new C(2, "b"), 0);
        a.put(new C(1, "a"), 1);
        a.put(new C(18, "b"), 1);
        a.put(new C(3, "b"), 1);
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.floorKey(new C(5, "c")))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.lowerKey(new C(17, "b")))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.get(new C(10, "a")))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> es(a.pollFirstEntry()))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.containsKey(new C(14, "b")))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.remove(new C(2, "b")))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.lowerKey(new C(8, "a")))).append(';');
        log.append(it(a.keySet())).append('|');
        return log.toString();
    }
    public static String m8() {
        log.setLength(0);
        TreeSet<C> a = new TreeSet<>();
        a.add(new C(17, "a"));
        a.add(new C(9, "a"));
        log.append(it(a)).append('|');
        log.append(run(() -> a.higher(new C(11, "a")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.ceiling(new C(7, "b")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.pollLast())).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.contains(new C(7, "a")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.first())).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.last())).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.ceiling(new C(24, "a")))).append(';');
        log.append(it(a)).append('|');
        return log.toString();
    }
    public static String m9() {
        log.setLength(0);
        TreeMap<C, Integer> a = new TreeMap<>();
        a.put(new C(17, "c"), 0);
        a.put(new C(24, "b"), 4);
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.getOrDefault(new C(5, "b"), -1))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.containsKey(new C(1, "c")))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.firstKey())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.put(new C(17, "b"), 4))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.lowerKey(new C(12, "b")))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.put(new C(0, "c"), 3))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.putIfAbsent(new C(1, "c"), 9))).append(';');
        log.append(it(a.keySet())).append('|');
        return log.toString();
    }
    public static String m10() {
        log.setLength(0);
        TreeSet<C> a = new TreeSet<>();
        a.add(new C(8, "b"));
        a.add(new C(18, "b"));
        a.add(new C(5, "c"));
        a.add(new C(2, "a"));
        log.append(it(a)).append('|');
        log.append(run(() -> a.pollLast())).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.higher(new C(7, "b")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.contains(new C(17, "c")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.ceiling(new C(24, "c")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> { TreeSet<C> b = new TreeSet<>(); b.addAll(a); return it(b); })).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.first())).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.add(new C(22, "c")))).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.first())).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a)).append('|');
        log.append(run(() -> a.pollFirst())).append(';');
        log.append(it(a)).append('|');
        return log.toString();
    }
    public static String m11() {
        log.setLength(0);
        TreeMap<C, Integer> a = new TreeMap<>(DESC);
        a.put(new C(13, "a"), 2);
        a.put(new C(8, "a"), 3);
        a.put(new C(23, "c"), 0);
        a.put(new C(6, "c"), 3);
        a.put(new C(18, "a"), 0);
        a.put(new C(20, "c"), 1);
        a.put(new C(8, "a"), 1);
        a.put(new C(9, "a"), 4);
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.higherKey(new C(8, "b")))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.ceilingKey(new C(5, "c")))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.replace(new C(3, "a"), 7))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> new TreeMap<>(a).size())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.putIfAbsent(new C(23, "a"), 9))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.lastKey())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> es(a.pollFirstEntry()))).append(';');
        log.append(it(a.keySet())).append('|');
        return log.toString();
    }
    public static String m12() {
        log.setLength(0);
        TreeMap<C, Integer> a = new TreeMap<>();
        a.put(new C(14, "b"), 2);
        a.put(new C(17, "b"), 2);
        a.put(new C(23, "c"), 4);
        a.put(new C(15, "a"), 3);
        a.put(new C(12, "a"), 4);
        a.put(new C(0, "b"), 4);
        a.put(new C(23, "c"), 4);
        a.put(new C(6, "b"), 4);
        a.put(new C(16, "b"), 2);
        a.put(new C(22, "a"), 3);
        a.put(new C(19, "c"), 4);
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> new TreeMap<>(a).size())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> new TreeMap<>(a).size())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.containsValue(3))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.ceilingKey(new C(20, "c")))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.firstKey())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> es(a.pollLastEntry()))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.lastKey())).append(';');
        log.append(it(a.keySet())).append('|');
        return log.toString();
    }
    public static String m13() {
        log.setLength(0);
        TreeMap<C, Integer> a = new TreeMap<>();
        a.put(new C(5, "b"), 4);
        a.put(new C(1, "b"), 4);
        a.put(new C(3, "c"), 4);
        a.put(new C(13, "a"), 2);
        a.put(new C(2, "c"), 3);
        a.put(new C(0, "a"), 4);
        a.put(new C(22, "a"), 0);
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.lastKey())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> es(a.pollFirstEntry()))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> es(a.pollLastEntry()))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.getOrDefault(new C(14, "c"), -1))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> es(a.pollLastEntry()))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> { TreeMap<C, Integer> b = new TreeMap<>(); b.putAll(a); return it(b.keySet()); })).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> { TreeMap<C, Integer> b = new TreeMap<>(); b.putAll(a); return it(b.keySet()); })).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.floorKey(new C(22, "a")))).append(';');
        log.append(it(a.keySet())).append('|');
        log.append(run(() -> a.replace(new C(10, "b"), 7))).append(';');
        log.append(it(a.keySet())).append('|');
        return log.toString();
    }
}
"#;

fn fixture(test: &str) -> Fixture {
    Fixture::build(
        test,
        &[
            ("demo/Probe.java", PROBE),
            ("demo/RandomTree.java", RANDOM_TREE),
        ],
    )
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

/// The reviewer's programs on #98: a key equal by `compareTo` replaces the
/// value and keeps the first key, a `TreeSet` refuses it, and records,
/// enums and `Integer`s iterate sorted. A sorted set finds by `compareTo`.
/// Navigation with a comparator, and copies of a sorted map.
#[test]
fn tree_maps_compare_and_order_like_jdk_25() {
    assert_exact(
        "tree-maps-compare-and-order-like-jdk-25",
        &[
            ("reviewerCases", "1|x|y|a|true|false|1|abcABC|[1, 3, 5]"),
            ("containsByCompareTo", "true|true|a1"),
            (
                "navigation",
                "40|30|20|30|20|10|{40=v40, 30=v30, 20=v20}|40|20",
            ),
        ],
    );
}

/// Generated programs: every `compareTo`/`compare` call (both operands,
/// so the tree's shape), every answer and every iteration order matches
/// JDK 25.
#[test]
fn random_tree_programs_match_jdk_25() {
    assert_exact(
        "random-tree-programs-match-jdk-25",
        &[
            (
                "random_m0",
                "3<3 15<3 6<3 6<15 0<6 0<3 [0b,3b,6a,15c,]|4;[0b,3b,6a,15c,]|3<6 3<3 3;[0b,3b,6a,15c,]|20<6 20<15 null;[0b,3b,6a,15c,]|false;[0b,3b,6a,15c,]|",
            ),
            (
                "random_m1",
                "7<7 7<7 7<7 9<7 13<7 13<9 20<9 20<13 5<9 5<7 23<9 23<13 23<20 [5c,7b,9a,13c,20a,23b,]|5c;[7b,9a,13c,20a,23b,]|18<18 16<18 18<18 [16b,18b,];[7b,9a,13c,20a,23b,]|7<9 7<7 null;[7b,9a,13c,20a,23b,]|2<9 2<7 null;[7b,9a,13c,20a,23b,]|5;[7b,9a,13c,20a,23b,]|",
            ),
            (
                "random_m2",
                "7<7 24<7 17<7 17<24 7<17 7<7 16<17 16<7 18<17 18<24 14<17 14<7 14<16 21<17 21<24 21<18 [7a,14b,16b,17c,18b,21c,24a,]|12<17 12<14 12<7 true;[7a,12c,14b,16b,17c,18b,21c,24a,]|15<17 15<14 15<16 14b;[7a,12c,14b,16b,17c,18b,21c,24a,]|[7a,12c,14b,16b,17c,18b,21c,24a,];[7a,12c,14b,16b,17c,18b,21c,24a,]|19<17 19<21 19<18 false;[7a,12c,14b,16b,17c,18b,21c,24a,]|",
            ),
            (
                "random_m3",
                "[]|false;[]|!NoSuchElementException;[]|20<20 9<20 22<20 [9b,20c,22b,];[]|null;[]|null;[]|",
            ),
            (
                "random_m4",
                "[]|0;[]|false;[]|0;[]|!NoSuchElementException;[]|false;[]|null;[]|",
            ),
            (
                "random_m5",
                "[]|18<18 true;[18a,]|18a;[]|null;[]|!NoSuchElementException;[]|",
            ),
            (
                "random_m6",
                "21<21 17<21 15<21 15<17 2<17 2<15 [2c,15c,17b,21a,]|4<17 4<15 4<2 null;[2c,15c,17b,21a,]|2c;[2c,15c,17b,21a,]|21a;[2c,15c,17b,21a,]|4;[2c,15c,17b,21a,]|17<17 17b;[2c,15c,17b,21a,]|",
            ),
            (
                "random_m7",
                "4<4 19<4 2<4 7<4 7<19 8<4 8<19 8<7 18<4 18<8 18<19 14<4 14<8 14<19 14<18 1<4 1<2 19<4 19<8 19<18 19<19 2<4 2<2 1<4 1<2 1<1 18<4 18<8 18<18 3<4 3<2 [1b,2c,3b,4b,7c,8b,14b,18c,19c,]|5<4 5<8 5<7 4b;[1b,2c,3b,4b,7c,8b,14b,18c,19c,]|17<4 17<8 17<18 17<14 14b;[1b,2c,3b,4b,7c,8b,14b,18c,19c,]|10<4 10<8 10<18 10<14 null;[1b,2c,3b,4b,7c,8b,14b,18c,19c,]|1b=1;[2c,3b,4b,7c,8b,14b,18c,19c,]|14<4 14<8 14<18 14<14 true;[2c,3b,4b,7c,8b,14b,18c,19c,]|2<4 2<2 0;[3b,4b,7c,8b,14b,18c,19c,]|8<4 8<8 8<7 7c;[3b,4b,7c,8b,14b,18c,19c,]|",
            ),
            (
                "random_m8",
                "17<17 9<17 [9a,17a,]|11<17 11<9 17a;[9a,17a,]|7<17 7<9 9a;[9a,17a,]|17a;[9a,]|7<9 false;[9a,]|9a;[9a,]|9a;[9a,]|24<9 null;[9a,]|",
            ),
            (
                "random_m9",
                "17<17 24<17 [17c,24b,]|5<17 5<17 -1;[17c,24b,]|1<17 false;[17c,24b,]|17c;[17c,24b,]|17<17 0;[17c,24b,]|12<17 null;[17c,24b,]|0<17 null;[0c,17c,24b,]|1<17 1<0 null;[0c,1c,17c,24b,]|",
            ),
            (
                "random_m10",
                "8<8 18<8 5<8 2<8 2<5 [2a,5c,8b,18b,]|18b;[2a,5c,8b,]|7<5 7<8 8b;[2a,5c,8b,]|17<5 17<8 false;[2a,5c,8b,]|24<5 24<8 null;[2a,5c,8b,]|[2a,5c,8b,];[2a,5c,8b,]|2a;[2a,5c,8b,]|3;[2a,5c,8b,]|22<5 22<8 true;[2a,5c,8b,22c,]|2a;[2a,5c,8b,22c,]|4;[2a,5c,8b,22c,]|2a;[5c,8b,22c,]|",
            ),
            (
                "random_m11",
                "13>13 8>13 23>13 6>13 6>8 18>13 18>23 20>13 20>23 20>18 8>13 8>8 9>13 9>8 [23c,20c,18a,13a,9a,8a,6c,]|8>13 8>8 8>6 6c;[23c,20c,18a,13a,9a,8a,6c,]|5>13 5>8 5>6 null;[23c,20c,18a,13a,9a,8a,6c,]|3>13 3>8 3>6 null;[23c,20c,18a,13a,9a,8a,6c,]|7;[23c,20c,18a,13a,9a,8a,6c,]|23>13 23>20 23>23 0;[23c,20c,18a,13a,9a,8a,6c,]|6c;[23c,20c,18a,13a,9a,8a,6c,]|23c=0;[20c,18a,13a,9a,8a,6c,]|",
            ),
            (
                "random_m12",
                "14<14 17<14 23<14 23<17 15<17 15<14 12<17 12<14 0<17 0<14 0<12 23<17 23<23 6<17 6<14 6<12 6<0 16<17 16<14 16<15 22<17 22<23 19<17 19<23 19<22 [0b,6b,12a,14b,15a,16b,17b,19c,22a,23c,]|10;[0b,6b,12a,14b,15a,16b,17b,19c,22a,23c,]|10;[0b,6b,12a,14b,15a,16b,17b,19c,22a,23c,]|true;[0b,6b,12a,14b,15a,16b,17b,19c,22a,23c,]|20<17 20<22 20<19 22a;[0b,6b,12a,14b,15a,16b,17b,19c,22a,23c,]|0b;[0b,6b,12a,14b,15a,16b,17b,19c,22a,23c,]|23c=4;[0b,6b,12a,14b,15a,16b,17b,19c,22a,]|22a;[0b,6b,12a,14b,15a,16b,17b,19c,22a,]|",
            ),
            (
                "random_m13",
                "5<5 1<5 3<5 3<1 13<3 13<5 2<3 2<1 0<3 0<1 22<3 22<5 22<13 [0a,1b,2c,3c,5b,13a,22a,]|22a;[0a,1b,2c,3c,5b,13a,22a,]|0a=4;[1b,2c,3c,5b,13a,22a,]|22a=0;[1b,2c,3c,5b,13a,]|14<3 14<13 14<3 14<13 -1;[1b,2c,3c,5b,13a,]|13a=2;[1b,2c,3c,5b,]|4;[1b,2c,3c,5b,]|[1b,2c,3c,5b,];[1b,2c,3c,5b,]|[1b,2c,3c,5b,];[1b,2c,3c,5b,]|22<3 22<5 5b;[1b,2c,3c,5b,]|10<3 10<5 null;[1b,2c,3c,5b,]|",
            ),
        ],
    );
}

/// `headMap` and the other navigable views are not modelled: counted.
#[test]
fn navigable_views_are_counted() {
    let fixture = fixture("navigable-views-are-counted");
    let cases: &[(&str, &[(&str, usize)])] = &[(
        "headMap",
        &[(
            "java/util/TreeMap#headMap(Ljava/lang/Object;)Ljava/util/SortedMap;",
            2,
        )],
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
