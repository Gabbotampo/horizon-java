//! `TreeMap`/`TreeSet`: the first/last lookups by exact name, the
//! `compute` family and `merge`, and fail-fast `forEach` and loops
//! (`tree_map.rs`, `iteration_guard.rs`).
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `C` logs `c<a><b>` per `compareTo`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r##"package demo;

import java.util.*;

public final class Probe {
    static final StringBuilder log = new StringBuilder();
    /// Ordered by `n` only, logging `c<a><b>` per `compareTo`; `tag` tells
    /// compare-equal keys apart.
    record C(int n, String tag) implements Comparable<C> {
        public int compareTo(C o) { log.append('c').append(n).append(o.n).append(' '); return Integer.compare(n, o.n); }
    }

    /// The reviewer's `firstEntry` programs on #111.
    public static String firstEntryKeys() {
        TreeMap<Integer, String> t = new TreeMap<>();
        t.put(2, "b"); t.put(1, "a"); t.put(3, "c");
        return t.firstEntry().getKey() + "" + t.lastEntry().getKey() + t.firstKey() + t.pollFirstEntry().getKey() + t.firstEntry().getKey();
    }

    public static String firstEntryValues() {
        TreeMap<Integer, String> t = new TreeMap<>();
        for (int i = 0; i < 6; i++) t.put(i, "v" + i);
        t.put(1, "w1");
        StringBuilder sb = new StringBuilder(t.firstEntry().getValue());
        sb.append(',').append(t.pollFirstEntry().getValue());
        sb.append(',').append(t.firstEntry().getValue()).append('/').append(t.firstEntry().getKey());
        sb.append(',').append(t.lastEntry().getValue()).append(',').append(t.pollLastEntry().getKey()).append(',').append(t.lastEntry().getKey());
        return sb.toString();
    }

    public static String firstEntryRecords() {
        log.setLength(0);
        TreeMap<C, String> t = new TreeMap<>();
        t.put(new C(1, "a"), "x"); t.put(new C(2, "b"), "y"); t.put(new C(1, "c"), "z");
        return t.firstEntry().getValue() + t.firstEntry().getKey().tag() + "#" + log;
    }

    public static String treeSetEnds() {
        TreeSet<String> s = new TreeSet<>(List.of("m", "c", "x", "a"));
        return s.first() + s.last() + s.pollFirst() + s.pollLast() + s.first() + s.last() + s.ceiling("d") + s.floor("d") + s.higher("c") + s.lower("m");
    }

    /// The reviewer's `merge` program on #111.
    public static String mergeStrings() {
        TreeMap<Integer, String> t = new TreeMap<>();
        t.put(5, "a"); t.put(3, "d");
        String a = t.merge(3, "e", String::concat);
        String b = t.get(3);
        String c = t.merge(7, "n", String::concat);
        String d = t.merge(5, "x", (p, q) -> null);
        return a + " " + b + " " + c + " " + d + " " + t.size() + " " + t;
    }

    public static String computeStrings() {
        TreeMap<String, Integer> t = new TreeMap<>();
        t.put("b", 1); t.put("d", 2);
        Integer a = t.compute("b", (k, v) -> v + 10);
        Integer b = t.compute("a", (k, v) -> v == null ? 7 : v);
        Integer c = t.compute("d", (k, v) -> null);
        Integer d = t.computeIfPresent("b", (k, v) -> v * 2);
        Integer e = t.computeIfPresent("z", (k, v) -> 1);
        Integer f = t.computeIfAbsent("c", k -> 3);
        Integer g = t.computeIfAbsent("b", k -> 99);
        Integer h = t.computeIfAbsent("q", k -> null);
        Integer i = t.computeIfPresent("a", (k, v) -> null);
        return a + " " + b + " " + c + " " + d + " " + e + " " + f + " " + g + " " + h + " " + i + " " + t;
    }

    /// `compute*`/`merge` over logging keys: the walk's comparisons.
    public static String computeRecords() {
        log.setLength(0);
        TreeMap<C, Integer> t = new TreeMap<>();
        for (int i : new int[] {5, 2, 8, 1, 9}) t.put(new C(i, "k" + i), i);
        log.setLength(0);
        t.merge(new C(2, "m"), 100, Integer::sum);
        log.append("| ");
        t.compute(new C(7, "n"), (k, v) -> 70);
        log.append("| ");
        t.computeIfAbsent(new C(3, "o"), k -> 30);
        log.append("| ");
        t.computeIfPresent(new C(8, "p"), (k, v) -> null);
        log.append("| ");
        t.merge(new C(4, "q"), 40, Integer::sum);
        return t.values() + "#" + log;
    }

    /// `merge`/`computeIfAbsent` on an empty map compare the key with itself
    /// (`addEntryToEmptyMap`) after the function.
    public static String computeEmpty() {
        log.setLength(0);
        TreeMap<C, Integer> t = new TreeMap<>();
        t.computeIfAbsent(new C(4, "a"), k -> { log.append("f "); return 1; });
        TreeMap<C, Integer> u = new TreeMap<>();
        u.merge(new C(6, "b"), 2, Integer::sum);
        return t.firstKey().tag() + t.get(new C(4, "z")) + u.firstKey().tag() + u.size() + "#" + log;
    }

    /// A function that changes the map: CME after it.
    public static String computeChangesMap() {
        TreeMap<Integer, Integer> t = new TreeMap<>(Map.of(1, 1, 2, 2));
        try { t.computeIfAbsent(3, k -> { t.put(9, 9); return 3; }); return "no" + t; }
        catch (ConcurrentModificationException e) { return "CME" + t; }
    }

    public static String mergeChangesMap() {
        TreeMap<Integer, Integer> t = new TreeMap<>(Map.of(1, 1, 2, 2));
        try { t.merge(2, 5, (a, b) -> { t.remove(1); return a + b; }); return "no" + t; }
        catch (ConcurrentModificationException e) { return "CME" + t; }
    }

    /// A value replaced inside the function is not structural.
    public static String computeReplacesValue() {
        TreeMap<Integer, Integer> t = new TreeMap<>(Map.of(1, 1, 2, 2));
        try { t.compute(2, (k, v) -> { t.put(1, 10); return v + 1; }); return "no" + t; }
        catch (ConcurrentModificationException e) { return "CME" + t; }
    }

    /// The reviewer's CME programs on #111.
    public static String keySetLoopPut() {
        TreeMap<Integer, String> t = new TreeMap<>();
        for (int i = 0; i < 10; i += 2) t.put(i, "v");
        StringBuilder sb = new StringBuilder();
        try { for (Integer k : t.keySet()) { sb.append(k); if (k == 2) t.put(3, "n"); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb.toString();
    }

    public static String treeSetLoopRemove() {
        TreeSet<String> t = new TreeSet<>(List.of("a", "c", "e"));
        StringBuilder sb = new StringBuilder();
        try { for (String k : t) { sb.append(k); if (k.equals("a")) t.remove("e"); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + t;
    }

    /// `TreeMap.forEach` throws right after the action that changed it.
    public static String mapForEachPut() {
        TreeMap<Integer, String> t = new TreeMap<>();
        for (int i = 0; i < 5; i++) t.put(i, "v");
        StringBuilder sb = new StringBuilder();
        try { t.forEach((k, v) -> { sb.append(k); if (k == 1) t.put(10, "n"); }); sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + t.size();
    }

    public static String mapForEachLastRemove() {
        TreeMap<Integer, String> t = new TreeMap<>(Map.of(1, "a", 2, "b", 3, "c"));
        StringBuilder sb = new StringBuilder();
        try { t.forEach((k, v) -> { sb.append(k); if (k == 3) t.remove(1); }); sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + t;
    }

    public static String mapForEachPlain() {
        TreeMap<String, Integer> t = new TreeMap<>(Map.of("b", 2, "a", 1, "c", 3));
        StringBuilder sb = new StringBuilder();
        t.forEach((k, v) -> sb.append(k).append(v.intValue()));
        return sb.toString();
    }

    /// `TreeSet.forEach` is `Iterable.forEach`: a loop.
    public static String setForEachAdd() {
        TreeSet<Integer> t = new TreeSet<>(List.of(1, 3, 5));
        StringBuilder sb = new StringBuilder();
        try { t.forEach(k -> { sb.append(k.intValue()); if (k == 1) t.add(2); }); sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + t;
    }

    public static String valuesLoopRemove() {
        TreeMap<Integer, String> t = new TreeMap<>(Map.of(1, "a", 2, "b", 3, "c"));
        StringBuilder sb = new StringBuilder();
        try { for (String v : t.values()) { sb.append(v); if (v.equals("a")) t.remove(3); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + t;
    }

    public static String entryLoopPutExisting() {
        TreeMap<Integer, String> t = new TreeMap<>(Map.of(1, "a", 2, "b"));
        StringBuilder sb = new StringBuilder();
        try { for (Map.Entry<Integer, String> e : t.entrySet()) { sb.append(e.getKey()); t.put(2, "z"); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + t;
    }

    /// A change at the last element ends the loop without a check.
    public static String setForEachLastAdd() {
        TreeSet<Integer> t = new TreeSet<>(List.of(1, 3, 5));
        StringBuilder sb = new StringBuilder();
        try { t.forEach(k -> { sb.append(k.intValue()); if (k == 5) t.add(9); }); sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + t;
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

/// The reviewer's programs on #111: `firstEntry` (lowercase `f`) is the
/// first entry (`13112`, `za`), on maps of more than one entry.
#[test]
fn first_and_last_lookups_take_the_right_end() {
    assert_exact(
        "first-and-last-lookups-take-the-right-en",
        &[
            ("firstEntryKeys", "13112"),
            ("firstEntryValues", "v0,v0,w1/1,v5,5,4"),
            ("firstEntryRecords", "za#c11 c21 c11 "),
            ("treeSetEnds", "axaxcmmcmc"),
        ],
    );
}

/// The reviewer's `merge` program on #111 (`de de n null 2`), and the
/// `compute` family: the `put` walk's comparisons, `addEntryToEmptyMap`'s
/// `compare(key, key)` after the function, removal on `null`, and a
/// function that changes the map's structure throws.
#[test]
fn compute_and_merge_run_the_function_on_the_tree() {
    assert_exact(
        "compute-and-merge-run-the-function-on-th",
        &[
            ("mergeStrings", "de de n null 2 {3=de, 7=n}"),
            (
                "computeStrings",
                "11 7 null 22 null 3 22 null null {b=22, c=3}",
            ),
            (
                "computeRecords",
                "[1, 102, 30, 40, 5, 70, 9]#c25 c22 | c75 c78 | c35 c32 | c85 c88 | c45 c42 c43 ",
            ),
            ("computeEmpty", "a1b1#f c44 c66 c44 "),
            ("computeChangesMap", "CME{1=1, 2=2, 9=9}"),
            ("mergeChangesMap", "CME{2=2}"),
            ("computeReplacesValue", "no{1=10, 2=3}"),
        ],
    );
}

/// The reviewer's loops on #111 (`02` then CME, `a` then CME), and
/// `forEach`: `TreeMap.forEach` throws right after the changing action,
/// even at the last entry; `TreeSet.forEach` is a loop, which does not
/// check after the last element; replacing a value is not structural.
#[test]
fn tree_iteration_is_fail_fast() {
    assert_exact(
        "tree-iteration-is-fail-fast",
        &[
            ("keySetLoopPut", "02CME"),
            ("treeSetLoopRemove", "aCME[a, c]"),
            ("mapForEachPut", "01CME6"),
            ("mapForEachLastRemove", "123CME{2=b, 3=c}"),
            ("mapForEachPlain", "a1b2c3"),
            ("setForEachAdd", "1CME[1, 2, 3, 5]"),
            ("setForEachLastAdd", "135no[1, 3, 5, 9]"),
            ("valuesLoopRemove", "aCME{1=a, 2=b}"),
            ("entryLoopPutExisting", "12no{1=a, 2=z}"),
        ],
    );
}
