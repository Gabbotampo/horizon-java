//! Map views (`hash_collections.rs`): `keySet()`/`values()`/`entrySet()`
//! `forEach` read the live map, and a kept view follows its map.
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
    static Map<String, Integer> fill(Map<String, Integer> m) { m.put("a", 0); m.put("b", 1); m.put("c", 2); m.put("d", 3); return m; }
    static Map<String, Integer> map() { return fill(new HashMap<>()); }

    static String run(Map<String, Integer> m, Runnable r) {
        try { r.run(); return "no" + m; } catch (ConcurrentModificationException e) { return "CME" + m; }
    }

    // The reviewer's `forEach` programs on #108.
    public static String keySetForEachRemove() {
        Map<String, Integer> m = map(); StringBuilder sb = new StringBuilder();
        return sb + run(m, () -> m.keySet().forEach(k -> { sb.append(k); if (k.equals("a")) m.remove("c"); })) + sb;
    }
    public static String valuesForEachPut() {
        Map<String, Integer> m = map(); StringBuilder sb = new StringBuilder();
        return run(m, () -> m.values().forEach(v -> { sb.append(v.intValue()); if (v == 0) m.put("c", 9); })) + sb;
    }
    public static String entrySetForEachPut() {
        Map<String, Integer> m = map(); StringBuilder sb = new StringBuilder();
        return run(m, () -> m.entrySet().forEach(e -> { sb.append(e.getKey()).append(e.getValue().intValue()); if (e.getKey().equals("a")) m.put("d", 7); })) + sb;
    }
    public static String keySetForEachPlain() { Map<String, Integer> m = map(); StringBuilder sb = new StringBuilder(); m.keySet().forEach(k -> sb.append(k)); return sb.toString(); }
    public static String linkedKeySetForEachAdd() {
        Map<String, Integer> m = fill(new LinkedHashMap<>()); StringBuilder sb = new StringBuilder();
        return run(m, () -> m.keySet().forEach(k -> { sb.append(k); if (k.equals("b")) m.put("z", 9); })) + sb;
    }
    public static String linkedKeySetForEachRemoveCurrent() {
        Map<String, Integer> m = fill(new LinkedHashMap<>()); StringBuilder sb = new StringBuilder();
        return run(m, () -> m.keySet().forEach(k -> { sb.append(k); if (k.equals("b")) m.remove("b"); })) + sb;
    }
    public static String treeKeySetForEachRemove() {
        Map<String, Integer> m = fill(new TreeMap<>()); StringBuilder sb = new StringBuilder();
        return run(m, () -> m.keySet().forEach(k -> { sb.append(k); if (k.equals("a")) m.remove("c"); })) + sb;
    }
    public static String treeValuesForEachLast() {
        Map<String, Integer> m = fill(new TreeMap<>()); StringBuilder sb = new StringBuilder();
        return run(m, () -> m.values().forEach(v -> { sb.append(v.intValue()); if (v == 3) m.put("z", 9); })) + sb;
    }

    // The reviewer's live views on #108 and #111.
    public static String keptViews() {
        Map<String, Integer> m = map(); Set<String> ks = m.keySet(); Collection<Integer> vs = m.values();
        m.put("z", 7); m.remove("a");
        return ks.contains("z") + " " + ks.size() + " " + ks.contains("a") + " " + vs.contains(7);
    }
    public static String keptKeySetLoop() { Map<String, Integer> m = map(); Set<String> ks = m.keySet(); m.put("z", 1); StringBuilder sb = new StringBuilder(); for (String k : ks) sb.append(k); return sb.toString(); }
    public static String keptValuesText() { Map<String, Integer> m = map(); Collection<Integer> vs = m.values(); m.put("a", 9); return "" + vs; }
    public static String keptEntrySetSize() { Map<String, Integer> m = map(); Set<Map.Entry<String, Integer>> es = m.entrySet(); m.remove("b"); return es.size() + ""; }
    public static String keptLinkedKeySet() { Map<String, Integer> m = new LinkedHashMap<>(); m.put("a", 1); m.put("b", 2); m.put("c", 3); Set<String> ks = m.keySet(); m.put("z", 1); return "" + ks; }
    public static String keptTreeKeySet() {
        Map<String, Integer> m = fill(new TreeMap<>()); Set<String> ks = m.keySet(); m.put("z", 1); m.remove("a");
        return ks.size() + " " + ks.contains("z") + " " + ks;
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

/// The reviewer's programs on #108: `HashMap.KeySet.forEach` skips a key
/// removed ahead and throws at the end (`abd`), `Values.forEach` sees a new
/// value (`0193`); `LinkedHashMap` visits a key added behind and stops when
/// the current one goes; `TreeMap`'s views throw at the loop's next step.
#[test]
fn view_for_each_reads_the_live_map() {
    assert_exact(
        "view-for-each-reads-the-live-map",
        &[
            ("keySetForEachRemove", "CME{a=0, b=1, d=3}abd"),
            ("valuesForEachPut", "no{a=0, b=1, c=9, d=3}0193"),
            ("entrySetForEachPut", "no{a=0, b=1, c=2, d=7}a0b1c2d7"),
            ("keySetForEachPlain", "abcd"),
            (
                "linkedKeySetForEachAdd",
                "CME{a=0, b=1, c=2, d=3, z=9}abcdz",
            ),
            ("linkedKeySetForEachRemoveCurrent", "CME{a=0, c=2, d=3}ab"),
            ("treeKeySetForEachRemove", "CME{a=0, b=1, d=3}a"),
            ("treeValuesForEachLast", "no{a=0, b=1, c=2, d=3, z=9}0123"),
        ],
    );
}

/// The reviewer's view programs on #108 and #111: a `keySet()`/`values()`/
/// `entrySet()` kept in a variable follows later changes to the map
/// (`true 4 false true`).
#[test]
fn kept_views_follow_their_map() {
    assert_exact(
        "kept-views-follow-their-map",
        &[
            ("keptViews", "true 4 false true"),
            ("keptKeySetLoop", "abcdz"),
            ("keptValuesText", "[9, 1, 2, 3]"),
            ("keptEntrySetSize", "3"),
            ("keptLinkedKeySet", "[a, b, c, z]"),
            ("keptTreeKeySet", "4 true [b, c, d, z]"),
        ],
    );
}
