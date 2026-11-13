//! Hash collections' `modCount` (`mod_count.rs`, `hash_collections.rs`):
//! a remove and a re-put of the same key still makes a key loop throw.
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
    static Map<String, Integer> map() { Map<String, Integer> m = new HashMap<>(); m.put("a", 0); m.put("b", 1); m.put("c", 2); m.put("d", 3); return m; }

    /// For-each over `m.keySet()`, printing, running `act` at "a".
    static String loop(Map<String, Integer> m, Runnable act) {
        StringBuilder sb = new StringBuilder();
        try { for (String k : m.keySet()) { sb.append(k); if (k.equals("a")) act.run(); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + m;
    }

    // The reviewer's programs on #108.
    public static String removeThenPut() { Map<String, Integer> m = map(); return loop(m, () -> { m.remove("c"); m.put("c", 2); }); }
    public static String clearPutAll() { Map<String, Integer> m = map(); return loop(m, () -> { Map<String, Integer> c = new HashMap<>(m); m.clear(); m.putAll(c); }); }
    public static String mergeNullThenPut() { Map<String, Integer> m = map(); return loop(m, () -> { m.merge("b", 1, (x, y) -> null); m.put("b", 1); }); }
    public static String setRemoveAdd() {
        Set<String> s = new HashSet<>(List.of("a", "b", "c", "d")); StringBuilder sb = new StringBuilder();
        try { for (String k : s) { sb.append(k); if (k.equals("a")) { s.remove("c"); s.add("c"); } } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb.toString();
    }
    public static String resizeThenRemove() {
        Map<String, Integer> m = new HashMap<>(); for (int i = 0; i < 12; i++) m.put("k" + i, i);
        StringBuilder sb = new StringBuilder();
        try { for (String k : m.keySet()) { sb.append(k); if (k.equals("k3")) { m.put("k12", 12); m.remove("k12"); } } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb.toString();
    }
    public static String linkedRemoveThenPut() {
        Map<String, Integer> m = new LinkedHashMap<>(map()); return loop(m, () -> { m.remove("c"); m.put("c", 2); });
    }
    public static String entryLoopRemoveThenPut() {
        Map<String, Integer> m = map(); StringBuilder sb = new StringBuilder();
        try { for (Map.Entry<String, Integer> e : m.entrySet()) { sb.append(e.getKey()); if (e.getKey().equals("a")) { m.remove("d"); m.put("d", 3); } } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb.toString();
    }

    // What does not move `modCount`.
    public static String putExisting() { Map<String, Integer> m = map(); return loop(m, () -> m.put("b", 7)); }
    public static String removeMissing() { Map<String, Integer> m = map(); return loop(m, () -> m.remove("zz")); }
    public static String clearOtherEmpty() { Map<String, Integer> m = map(); Map<String, Integer> o = new HashMap<>(); return loop(m, o::clear); }
    public static String replaceAllNonStructural() { Map<String, Integer> m = map(); return loop(m, () -> m.replace("b", 9)); }

    // `HashSet.removeIf`, where a predicate removes and re-adds.
    public static String removeIfReAdds() {
        Set<String> s = new HashSet<>(List.of("a", "b", "c"));
        try { boolean r = s.removeIf(x -> { if (x.equals("a")) { s.remove("c"); s.add("c"); } return false; }); return r + "" + s; }
        catch (ConcurrentModificationException e) { return "CME" + s; }
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

/// The reviewer's programs on #108: removing and re-putting a key,
/// `clear` then `putAll`, a `put` that resizes then a `remove`, all inside
/// a loop over the keys: JDK 25 throws at the next step.
#[test]
fn a_change_that_undoes_itself_still_throws() {
    assert_exact(
        "a-change-that-undoes-itself-still-throws",
        &[
            ("removeThenPut", "aCME{a=0, b=1, c=2, d=3}"),
            ("clearPutAll", "aCME{a=0, b=1, c=2, d=3}"),
            ("setRemoveAdd", "aCME"),
            ("resizeThenRemove", "k0k1k2k3CME"),
            ("linkedRemoveThenPut", "aCME{a=0, b=1, d=3, c=2}"),
            ("entryLoopRemoveThenPut", "aCME"),
            ("removeIfReAdds", "CME[a, b, c]"),
        ],
    );
}

/// `put` of a present key, `remove` of a missing one and `replace` move no
/// `modCount`; clearing another map does not touch this one's.
#[test]
fn members_that_change_no_structure_do_not_throw() {
    assert_exact(
        "members-that-change-no-structure-do-not-",
        &[
            ("putExisting", "abcdno{a=0, b=7, c=2, d=3}"),
            ("removeMissing", "abcdno{a=0, b=1, c=2, d=3}"),
            ("clearOtherEmpty", "abcdno{a=0, b=1, c=2, d=3}"),
            ("replaceAllNonStructural", "abcdno{a=0, b=9, c=2, d=3}"),
        ],
    );
}
