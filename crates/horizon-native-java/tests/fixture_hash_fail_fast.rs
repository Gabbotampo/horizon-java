//! Hash collections' `removeIf`, fail-fast iteration and `forEach`, and
//! their counted cases (`hash_collections.rs`, `iteration_guard.rs`).
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
    /// Logs `e<id>:<other id>` per `equals`; equal by id, hashed by `h`.
    static final class K {
        final int id, h; K(int id, int h) { this.id = id; this.h = h; }
        @Override public boolean equals(Object o) {
            log.append('e').append(id).append(':').append(o instanceof K k ? String.valueOf(k.id) : String.valueOf(o)).append(' ');
            return o instanceof K k && k.id == id;
        }
        @Override public int hashCode() { return h; }
        @Override public String toString() { return "K" + id; }
    }
    static String it(Iterable<?> c) { StringBuilder sb = new StringBuilder("["); for (Object o : c) sb.append(o).append(','); return sb.append(']').toString(); }

    /// The reviewer's `removeIf` programs on #108.
    public static String removeIfHashSet() {
        log.setLength(0);
        Set<K> s = new HashSet<>();
        for (int i = 0; i < 8; i++) s.add(new K(i, i % 3));
        log.setLength(0);
        boolean r = s.removeIf(k -> k.id % 4 == 0);
        return r + it(s) + "|" + log;
    }

    public static String removeIfLinkedHashSet() {
        log.setLength(0);
        Set<K> s = new LinkedHashSet<>();
        for (int i = 0; i < 6; i++) s.add(new K(i, 1));
        log.setLength(0);
        boolean r = s.removeIf(k -> k.id % 2 == 1);
        return r + it(s) + "|" + log;
    }

    /// A predicate that adds to the set: the next step throws.
    public static String removeIfAdds() {
        Set<String> s = new HashSet<>(List.of("a", "b", "c"));
        try { s.removeIf(x -> { if (x.equals("a")) s.add("zz"); return false; }); return "no" + s.size(); }
        catch (ConcurrentModificationException e) { return "CME" + s.size(); }
    }

    /// The reviewer's CME programs on #108.
    public static String mapForEachPut() {
        Map<Integer, String> m = new HashMap<>();
        for (int i = 0; i < 5; i++) m.put(i, "v");
        StringBuilder sb = new StringBuilder();
        try { m.forEach((k, v) -> { sb.append(k); if (k == 1) m.put(101, "n"); }); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb.toString();
    }

    public static String setLoopAdd() {
        Set<String> s = new HashSet<>(List.of("a", "b", "c"));
        StringBuilder sb = new StringBuilder();
        try { for (String x : s) { sb.append(x); if (x.equals("a")) s.add("zz"); } }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb.toString();
    }

    public static String keySetLoopRemove() {
        Map<String, Integer> m = new HashMap<>();
        for (String k : List.of("a", "b", "c", "d")) m.put(k, 1);
        StringBuilder sb = new StringBuilder();
        try { for (String k : m.keySet()) { sb.append(k); if (k.equals("a")) m.remove("d"); } }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb.toString();
    }

    public static String linkedKeySetLoopRemove() {
        Map<String, Integer> m = new LinkedHashMap<>();
        for (String k : List.of("p", "q", "r")) m.put(k, 1);
        StringBuilder sb = new StringBuilder();
        try { for (String k : m.keySet()) { sb.append(k); if (k.equals("p")) m.remove("q"); } }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb.toString();
    }

    public static String valuesLoopPut() {
        Map<String, Integer> m = new HashMap<>();
        m.put("a", 9); m.put("b", 2);
        StringBuilder sb = new StringBuilder();
        try { for (Integer v : m.values()) { sb.append(v); m.put("z", 9); } }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb.toString();
    }

    /// A change in the last call: every entry was visited, then CME.
    public static String forEachLastPut() {
        Map<String, Integer> m = new LinkedHashMap<>();
        m.put("a", 1); m.put("b", 2);
        StringBuilder sb = new StringBuilder();
        try { m.forEach((k, v) -> { sb.append(k); if (k.equals("b")) m.put("c", 3); }); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + m.size();
    }

    /// A value replaced in a loop is not structural.
    public static String valueReplaceInLoop() {
        Map<String, Integer> m = new HashMap<>();
        m.put("a", 1); m.put("b", 2);
        StringBuilder sb = new StringBuilder();
        for (String k : m.keySet()) { sb.append(k); m.put(k, 5); }
        return sb + "" + m.values();
    }

    /// The reviewer's `equals` with jar keys.
    public static String mapEqualsJarKeys() {
        Map<K, String> a = new HashMap<>(), b = new HashMap<>();
        for (int i = 0; i < 4; i++) a.put(new K(i, i), "v" + i);
        for (int i = 3; i >= 0; i--) b.put(new K(i, i), "v" + i);
        return a.equals(b) + "|" + a.keySet().equals(b.keySet());
    }

    /// `LinkedHashMap.forEach` removing a later key (counted: the JDK walks
    /// the live list and skips it).
    public static String linkedForEachRemove() {
        Map<String, Integer> m = new LinkedHashMap<>();
        for (String k : List.of("a", "b", "c")) m.put(k, 1);
        StringBuilder sb = new StringBuilder();
        try { m.forEach((k, v) -> { sb.append(k); if (k.equals("a")) m.remove("c"); }); }
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

/// The reviewer's programs on #108: `removeIf` on a `HashSet`/
/// `LinkedHashSet` removes through the iterator, whose `removeNode`
/// re-walks the bin (`e4:1`; `e1:0 e3:0 e3:2 e5:0 e5:2 e5:4`), and a
/// predicate that adds makes the next step throw.
#[test]
fn hash_set_remove_if_rewalks_the_bin() {
    assert_exact(
        "hash-set-remove-if-rewalks-the-bin",
        &[
            ("removeIfHashSet", "true[K3,K6,K1,K7,K2,K5,]|e4:1 "),
            (
                "removeIfLinkedHashSet",
                "true[K0,K2,K4,]|e1:0 e3:0 e3:2 e5:0 e5:2 e5:4 ",
            ),
            ("removeIfAdds", "CME4"),
        ],
    );
}

/// The reviewer's programs on #108: a loop over a hash collection, or
/// over its `keySet()`/`values()`, throws on the next step after a
/// structural change, instead of visiting a removed key; replacing a value
/// is not structural.
#[test]
fn hash_iteration_is_fail_fast() {
    assert_exact(
        "hash-iteration-is-fail-fast",
        &[
            ("setLoopAdd", "aCME"),
            ("keySetLoopRemove", "aCME"),
            ("linkedKeySetLoopRemove", "pCME"),
            ("valuesLoopPut", "9CME"),
            ("valueReplaceInLoop", "ab[5, 5]"),
        ],
    );
}

/// `forEach` walks the live table after a change (JDK 25 printed
/// `01234101CME`, `abcCME3`, `abCME`), which is not modelled: counted, with
/// the exception at the end. `equals` of hash maps or their key sets with
/// jar keys: the maps' `equals` is answered (#109), their key-set copies' is
/// counted.
#[test]
fn changes_inside_for_each_and_jar_key_equals_are_counted() {
    let fixture = fixture("changes-inside-for-each-and-jar-key-equa");
    let cases: &[(&str, &[(&str, usize)])] = &[
        (
            "mapForEachPut",
            &[("java/util/Map#forEach(Ljava/util/function/BiConsumer;)V", 1)],
        ),
        (
            "forEachLastPut",
            &[("java/util/Map#forEach(Ljava/util/function/BiConsumer;)V", 1)],
        ),
        (
            "linkedForEachRemove",
            &[("java/util/Map#forEach(Ljava/util/function/BiConsumer;)V", 1)],
        ),
        (
            "mapEqualsJarKeys",
            &[("java/util/Set#equals(Ljava/lang/Object;)Z", 1)],
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
