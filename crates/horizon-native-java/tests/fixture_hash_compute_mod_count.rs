//! `compute`/`merge` check `modCount` (`hash_bulk.rs`, `mod_count.rs`),
//! and `Map.replaceAll` runs its function.
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `K` logs `e<id>:<other id>` per `equals`, `h<id>` per
//! `hashCode`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r##"package demo;

import java.util.*;

public final class Probe {
    static final StringBuilder log = new StringBuilder();
    /// Logs `e<id>:<other id>` per `equals` and `h<id>` per `hashCode`.
    static final class K {
        final int id; K(int id) { this.id = id; }
        @Override public boolean equals(Object o) {
            log.append('e').append(id).append(':').append(o instanceof K k ? String.valueOf(k.id) : String.valueOf(o)).append(' ');
            return o instanceof K k && k.id == id;
        }
        @Override public int hashCode() { log.append('h').append(id).append(' '); return id; }
    }

    // The reviewer's programs on #109: the function changes the map and back.
    public static String computeRemovePut() {
        Map<String, Integer> m = new HashMap<>(); m.put("a", 1);
        try { Integer r = m.compute("a", (k, v) -> { m.remove("a"); m.put("a", 5); return 9; }); return r + "" + m; }
        catch (ConcurrentModificationException e) { return "CME" + m; }
    }
    public static String mergeClearPut() {
        Map<String, Integer> m = new HashMap<>(); m.put("a", 1);
        try { Integer r = m.merge("a", 1, (x, y) -> { m.clear(); m.put("a", 1); return x + y; }); return r + "" + m; }
        catch (ConcurrentModificationException e) { return "CME" + m; }
    }
    public static String computeIfAbsentPutRemove() {
        Map<String, Integer> m = new HashMap<>(); m.put("a", 0); m.put("b", 1);
        try { Integer r = m.computeIfAbsent("z", k -> { m.put("q", 5); m.remove("q"); return 9; }); return r + "" + m; }
        catch (ConcurrentModificationException e) { return "CME" + m; }
    }
    public static String computeIfAbsentKeys() {
        Map<K, Integer> m = new HashMap<>(); m.put(new K(1), 1); m.put(new K(2), 2); log.setLength(0);
        try { Integer r = m.computeIfAbsent(new K(9), k -> { m.put(new K(3), 3); m.remove(new K(3)); return 9; }); return r + "" + m.size() + "#" + log; }
        catch (ConcurrentModificationException e) { return "CME" + m.size() + "#" + log; }
    }
    public static String computeValueOnly() {
        Map<String, Integer> m = new HashMap<>(); m.put("a", 1); m.put("b", 2);
        try { Integer r = m.compute("a", (k, v) -> { m.put("b", 7); return v + 1; }); return r + "" + m; }
        catch (ConcurrentModificationException e) { return "CME" + m; }
    }
    /// A key loop where `merge` removes a key and `put` restores it.
    public static String mergeNullThenPut() {
        Map<String, Integer> m = new HashMap<>(); m.put("a", 0); m.put("b", 1); m.put("c", 2); m.put("d", 3);
        StringBuilder sb = new StringBuilder();
        try { for (String k : m.keySet()) { sb.append(k); if (k.equals("a")) { m.merge("b", 1, (x, y) -> null); m.put("b", 1); } } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + m;
    }

    // `Map.replaceAll`.
    public static String replaceAllHash() { Map<String, Integer> m = new HashMap<>(); m.put("a", 0); m.put("b", 1); m.replaceAll((k, v) -> v + 10); return m.toString(); }
    public static String replaceAllConcat() { Map<Integer, String> m = new HashMap<>(); m.put(1, "x"); m.put(2, "y"); m.replaceAll((k, v) -> v + k); return m.toString(); }
    public static String replaceAllLinked() { Map<String, Integer> m = new LinkedHashMap<>(); m.put("b", 1); m.put("a", 0); m.replaceAll((k, v) -> v + 10); return m.toString(); }
    public static String replaceAllSeesEarlierPut() {
        Map<String, Integer> m = new LinkedHashMap<>(); m.put("a", 0); m.put("b", 1);
        m.replaceAll((k, v) -> { if (k.equals("a")) m.put("b", 100); return v + 1; }); return m.toString();
    }
    public static String replaceAllPutsKey() {
        Map<String, Integer> m = new HashMap<>(); m.put("a", 0); m.put("b", 1); m.put("c", 2);
        try { m.replaceAll((k, v) -> { if (k.equals("a")) m.put("n", 0); return v + 10; }); return "ok" + m; }
        catch (ConcurrentModificationException e) { return "CME" + m; }
    }
    public static String replaceAllTree() {
        Map<String, Integer> m = new TreeMap<>(); m.put("a", 0); m.put("b", 1); m.replaceAll((k, v) -> v + 10); return m.toString();
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

/// The reviewer's programs on #109: `mc != modCount` after the function,
/// even when it restored the map (`CME{a=5}`); a value-only `put` moves no
/// `modCount`. A `merge` to `null` removes the key, so a key loop that
/// re-puts it throws.
#[test]
fn a_function_that_changes_the_map_and_back_throws() {
    assert_exact(
        "a-function-that-changes-the-map-and-back",
        &[
            ("computeRemovePut", "CME{a=5}"),
            ("mergeClearPut", "CME{a=1}"),
            ("computeIfAbsentPutRemove", "CME{a=0, b=1}"),
            ("computeIfAbsentKeys", "CME2#h9 h3 h3 e3:3 "),
            ("computeValueOnly", "2{a=2, b=7}"),
            ("mergeNullThenPut", "aCME{a=0, b=1, c=2, d=3}"),
        ],
    );
}

/// The reviewer's `replaceAll` programs on #109: every entry in order, each
/// reading its value as it is then.
#[test]
fn replace_all_runs_the_function() {
    assert_exact(
        "replace-all-runs-the-function",
        &[
            ("replaceAllHash", "{a=10, b=11}"),
            ("replaceAllConcat", "{1=x1, 2=y2}"),
            ("replaceAllLinked", "{b=11, a=10}"),
            ("replaceAllSeesEarlierPut", "{a=1, b=101}"),
        ],
    );
}

/// A function that adds a key (JDK 25 walks on into its own table, then
/// throws) is counted. (`TreeMap.replaceAll` is exact since #111:
/// `fixture_tree_set_bulk.rs`.)
#[test]
fn replace_all_beyond_the_model_is_counted() {
    let fixture = fixture("replace-all-beyond-the-model-is-counted");
    let cases: &[(&str, &[(&str, usize)])] = &[(
        "replaceAllPutsKey",
        &[(
            "java/util/Map#replaceAll(Ljava/util/function/BiFunction;)V",
            1,
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
