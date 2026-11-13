//! `TreeSet`'s bulk members over the tree, `TreeSet.removeIf`,
//! `TreeMap.replaceAll`, and `modCount` in tree loops (`tree_map.rs`,
//! `mod_count.rs`).
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `K` logs `c<id>,<other>` per `compareTo`, `e<id>:<other>` per
//! `equals`, `h<id>` per `hashCode`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r##"package demo;

import java.util.*;

public final class Probe {
    static final StringBuilder log = new StringBuilder();
    /// Ordered and equal by `id`; logs `c<id>,<other>`, `e<id>:<other>`,
    /// `h<id>`.
    static final class K implements Comparable<K> {
        final int id; K(int id) { this.id = id; }
        public int compareTo(K o) { log.append('c').append(id).append(',').append(o.id).append(' '); return Integer.compare(id, o.id); }
        @Override public boolean equals(Object o) {
            log.append('e').append(id).append(':').append(o instanceof K k ? String.valueOf(k.id) : String.valueOf(o)).append(' ');
            return o instanceof K k && k.id == id;
        }
        @Override public int hashCode() { log.append('h').append(id).append(' '); return id; }
    }
    static String ids(Collection<K> c) { StringBuilder sb = new StringBuilder("["); for (K k : c) sb.append(k.id).append(','); return sb.append(']').toString(); }
    static TreeSet<K> set(int... ids) { TreeSet<K> s = new TreeSet<>(); for (int i : ids) s.add(new K(i)); log.setLength(0); return s; }

    // The reviewer's `TreeSet` bulk programs on #111.
    public static String removeAllList() { TreeSet<K> s = set(1, 2); boolean r = s.removeAll(new ArrayList<>(List.of(new K(1), new K(5), new K(6)))); return r + ids(s) + "#" + log; }
    public static String retainAllList() { TreeSet<K> s = set(1, 2); boolean r = s.retainAll(List.of(new K(2), new K(7))); return r + ids(s) + "#" + log; }
    public static String containsAllList() { TreeSet<K> s = set(1, 2); boolean r = s.containsAll(List.of(new K(2), new K(1))); return r + "#" + log; }
    public static String equalsHashSet() { TreeSet<K> s = set(1, 2); HashSet<K> h = new HashSet<>(s); log.setLength(0); boolean r = s.equals(h); return r + "#" + log; }
    public static String removeAllLargerSet() { TreeSet<K> s = set(4, 2, 6, 1, 3); boolean r = s.removeAll(List.of(new K(3), new K(9))); return r + ids(s) + "#" + log; }
    public static String retainAllMany() { TreeSet<K> s = set(4, 2, 6, 1, 3, 5, 7); boolean r = s.retainAll(new ArrayList<>(List.of(new K(2), new K(6), new K(7)))); return r + ids(s) + "#" + log; }
    public static String equalsList() { TreeSet<K> s = set(1, 2); boolean r = s.equals(new ArrayList<>(s)); return r + "#" + log; }
    public static String equalsSmaller() { TreeSet<K> s = set(1, 2); HashSet<K> h = new HashSet<>(); h.add(new K(1)); log.setLength(0); return s.equals(h) + "#" + log; }

    // The reviewer's loops on #111: change the tree and change it back.
    static TreeMap<Integer, String> tree() { TreeMap<Integer, String> t = new TreeMap<>(); for (int i = 0; i < 8; i += 2) t.put(i, "v"); return t; }
    public static String keyLoopRemovePut() {
        TreeMap<Integer, String> t = tree(); StringBuilder sb = new StringBuilder();
        try { for (Integer k : t.keySet()) { sb.append(k.intValue()); if (k == 2) { t.remove(4); t.put(4, "w"); } } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + t;
    }
    public static String keyLoopMergeNullPut() {
        TreeMap<Integer, String> t = tree(); StringBuilder sb = new StringBuilder();
        try { for (Integer k : t.keySet()) { sb.append(k.intValue()); if (k == 2) { t.merge(4, "x", (a, b) -> null); t.put(4, "w"); } } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + t;
    }
    public static String entryLoopRemovePut() {
        TreeMap<Integer, String> t = new TreeMap<>(); t.put(1, "a"); t.put(2, "b"); t.put(3, "c"); StringBuilder sb = new StringBuilder();
        try { for (Map.Entry<Integer, String> e : t.entrySet()) { sb.append(e.getKey().intValue()); if (e.getKey() == 1) { t.remove(3); t.put(3, "z"); } } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + t;
    }

    // `TreeSet.removeIf` and `TreeMap.replaceAll`.
    public static String setRemoveIfThrows() {
        TreeSet<Integer> s = new TreeSet<>(List.of(1, 2, 3, 4)); int[] n = {0};
        try { s.removeIf(x -> { if (++n[0] == 3) throw new IllegalStateException(); return x <= 2; }); return "no" + s; }
        catch (IllegalStateException e) { return "ISE" + s; }
    }
    public static String setRemoveIfPlain() { TreeSet<Integer> s = new TreeSet<>(List.of(5, 1, 4, 2, 3, 6)); boolean r = s.removeIf(x -> x % 2 == 0); return r + "" + s + s.removeIf(x -> x > 9); }
    public static String setRemoveIfAdds() {
        TreeSet<Integer> s = new TreeSet<>(List.of(1, 2, 3));
        try { boolean r = s.removeIf(x -> { if (x == 1) s.add(9); return x == 2; }); return r + "" + s; }
        catch (ConcurrentModificationException e) { return "CME" + s; }
    }
    public static String setRemoveIfAddsAtLast() {
        TreeSet<Integer> s = new TreeSet<>(List.of(1, 2, 3));
        try { boolean r = s.removeIf(x -> { if (x == 3) s.add(9); return false; }); return r + "" + s; }
        catch (ConcurrentModificationException e) { return "CME" + s; }
    }
    public static String setRemoveIfKeys() { TreeSet<K> s = set(3, 1, 2); boolean r = s.removeIf(k -> k.id != 2); return r + ids(s) + "#" + log; }
    public static String replaceAllTree() { TreeMap<String, Integer> m = new TreeMap<>(); m.put("b", 1); m.put("a", 0); m.replaceAll((k, v) -> v + 10); return m.toString(); }
    public static String replaceAllTreePuts() {
        TreeMap<String, Integer> m = new TreeMap<>(); m.put("a", 0); m.put("b", 1); m.put("c", 2);
        try { m.replaceAll((k, v) -> { if (k.equals("a")) m.put("n", 0); return v + 10; }); return "ok" + m; }
        catch (ConcurrentModificationException e) { return "CME" + m; }
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

/// The reviewer's programs on #111: `containsAll` is `contains` per element
/// (a `compareTo` walk), `removeAll`/`retainAll` test `c.contains(e)` through
/// the iterator (or `remove(e)` per element of a smaller `c`), and
/// `equals` is `AbstractSet.equals`.
#[test]
fn tree_set_bulk_members_walk_the_tree() {
    assert_exact(
        "tree-set-bulk-members-walk-the-tree",
        &[
            ("removeAllList", "true[2,]#e1:1 e2:1 e2:5 e2:6 "),
            ("retainAllList", "true[2,]#e1:2 e1:7 e2:2 "),
            ("containsAllList", "true#c2,1 c2,2 c1,1 "),
            ("equalsHashSet", "true#c1,1 c2,1 c2,2 "),
            (
                "removeAllLargerSet",
                "true[1,2,4,6,]#c3,4 c3,2 c3,3 c9,4 c9,6 ",
            ),
            (
                "retainAllMany",
                "true[2,6,7,]#e1:2 e1:6 e1:7 e2:2 e3:2 e3:6 e3:7 e4:2 e4:6 e4:7 e5:2 e5:6 e5:7 e6:2 e6:6 e7:2 e7:6 e7:7 ",
            ),
            ("equalsList", "false#"),
            ("equalsSmaller", "false#"),
        ],
    );
}

/// The reviewer's loops on #111: removing and re-putting a key throws at
/// the next step (`02` then CME).
#[test]
fn tree_loops_check_mod_count() {
    assert_exact(
        "tree-loops-check-mod-count",
        &[
            ("keyLoopRemovePut", "02CME{0=v, 2=v, 4=w, 6=v}"),
            ("keyLoopMergeNullPut", "02CME{0=v, 2=v, 4=w, 6=v}"),
            ("entryLoopRemovePut", "1CME{1=a, 2=b, 3=z}"),
        ],
    );
}

/// `TreeSet.removeIf` removes through the iterator (a throwing predicate
/// leaves `[3, 4]`); `TreeMap.replaceAll` runs the function and throws right
/// after the call that changed the structure.
#[test]
fn tree_set_remove_if_and_tree_map_replace_all() {
    assert_exact(
        "tree-set-remove-if-and-tree-map-replace-",
        &[
            ("setRemoveIfThrows", "ISE[3, 4]"),
            ("setRemoveIfPlain", "true[1, 3, 5]false"),
            ("setRemoveIfAdds", "CME[1, 2, 3, 9]"),
            ("setRemoveIfAddsAtLast", "false[1, 2, 3, 9]"),
            ("setRemoveIfKeys", "true[2,]#"),
            ("replaceAllTree", "{a=10, b=11}"),
            ("replaceAllTreePuts", "CME{a=10, b=1, c=2, n=0}"),
        ],
    );
}
