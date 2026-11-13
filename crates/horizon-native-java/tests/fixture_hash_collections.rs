//! `HashMap`/`HashSet`/`LinkedHashMap`/`LinkedHashSet` as JDK 25 runs them
//! (`hash_table.rs`, `hash_collections.rs`): bucket-table iteration order,
//! lookups that run a key's `hashCode` once and its `equals` only for
//! same-hash nodes of its bin, and counting where identity hashes or a tree
//! bin decide the answer.
//!
//! Every expected value is OpenJDK 25.0.5's output for the same sources
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `RandomHash` is generated (`gen_hash.py 11 150 core`): random
//! `get`/`put`/`remove`/`putIfAbsent`/`replace`/`containsValue`/`putAll`/
//! `add`/`contains`/`addAll`/`hashCode` on random hash collections over
//! logging keys whose hashes collide (`e<id>`/`h<id>` per `equals`/
//! `hashCode`), records, strings, boxes and `null`, printing the iteration
//! order after every operation.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"package demo;

import java.util.*;
import java.util.stream.*;

public final class Probe {
    record Key(int id) { }
    /// `equals` by id, `hashCode` kept: the common mod bug.
    static final class NoHash {
        final int id; NoHash(int id) { this.id = id; }
        @Override public boolean equals(Object o) { return o instanceof NoHash n && n.id == id; }
    }
    static final class AllEq {
        final int id; AllEq(int id) { this.id = id; }
        @Override public boolean equals(Object o) { return true; }
        @Override public int hashCode() { return id; }
    }
    static final class Boom {
        final int id; Boom(int id) { this.id = id; }
        @Override public boolean equals(Object o) { throw new IllegalStateException("boom"); }
        @Override public int hashCode() { return id; }
    }
    static int equalsCalls;
    static final class Counted {
        final int id; Counted(int id) { this.id = id; }
        @Override public boolean equals(Object o) { equalsCalls++; return o instanceof Counted c && c.id == id; }
        @Override public int hashCode() { return id; }
    }
    static final class T { final int id; T(int id) { this.id = id; } @Override public String toString() { return "T" + id; } }
    enum E { A, B, C }
    static final class Same { final int id; Same(int id) { this.id = id; }
        @Override public boolean equals(Object o) { return o instanceof Same s && s.id == id; }
        @Override public int hashCode() { return 7; } }
    record Pt(int x) { }

    public static String identityMap() {
        Map<Key, String> m = new IdentityHashMap<>();
        m.put(new Key(1), "a"); m.put(new Key(1), "b");
        return m.size() + "|" + m.get(new Key(1)) + "|" + m.containsKey(new Key(1));
    }

    public static String noHash() {
        Map<NoHash, String> m = new HashMap<>();
        m.put(new NoHash(1), "a");
        Set<NoHash> s = new HashSet<>();
        s.add(new NoHash(2)); s.add(new NoHash(2));
        return m.get(new NoHash(1)) + "|" + m.containsKey(new NoHash(1)) + "|" + m.containsKey(new NoHash(3)) + "|" + s.size();
    }

    public static String allEq() {
        Set<AllEq> set = new HashSet<>();
        set.add(new AllEq(1));
        boolean added = set.add(new AllEq(2));
        Map<AllEq, String> map = new HashMap<>();
        map.put(new AllEq(5), "five");
        return added + "|" + set.size() + "|" + map.get(new AllEq(6));
    }

    public static String boom() {
        Set<Boom> set = new HashSet<>();
        set.add(new Boom(1));
        return set.contains(new Boom(2)) + "|" + set.add(new Boom(3)) + "|" + set.size();
    }

    public static String equalsCounts() {
        equalsCalls = 0;
        Map<Counted, Integer> m = new HashMap<>();
        for (int i = 1; i <= 6; i++) m.put(new Counted(i), i);
        equalsCalls = 0;
        m.get(new Counted(6));
        int a = equalsCalls;
        m.get(new Counted(60));
        return a + "|" + equalsCalls;
    }

    public static String hashSetCopy() { return String.valueOf(new HashSet<>(List.of(new Key(1), new Key(1))).size()); }
    public static String mapViews() {
        Map<String, Key> m1 = new HashMap<>(); Map<Key, String> m = new HashMap<>();
        m.put(new Key(1), "a");
        String a = m.keySet().contains(new Key(1)) + "|" + m.keySet().remove(new Key(1)) + "|" + m.size();
        m1.put("a", new Key(2));
        return a + "/" + m1.values().contains(new Key(2)) + "/" + m1.entrySet().contains(Map.entry("a", new Key(2)));
    }



    public static String hashSetStrings() {
        Set<String> s = new HashSet<>(); s.add("pear"); s.add("b"); s.add("apple"); s.add("a");
        return String.join(",", s);
    }

    public static String hashMapInts() {
        Map<Integer, String> m = new HashMap<>(); m.put(300, "x"); m.put(17, "y"); m.put(5, "z");
        return m.keySet().toString();
    }

    public static String hashMapRecords() {
        Map<Key, String> m = new HashMap<>(); m.put(new Key(20), "x"); m.put(new Key(4), "y"); m.put(new Key(1), "z");
        StringBuilder sb = new StringBuilder(); for (Key k : m.keySet()) sb.append(k.id()).append(',');
        return sb.toString();
    }

    /// Identity-hashed keys: the order is unknown, lookups are exact.
    public static String enumKeys() {
        Map<Object, Integer> m = new HashMap<>(); m.put(E.B, 1); m.put(E.A, 2); m.put("s", 3);
        return m.get(E.A) + "|" + m.containsKey(E.C) + "|" + m.size();
    }

    public static String enumKeysIterated() {
        Map<Object, Integer> m = new HashMap<>(); m.put(E.B, 1); m.put(E.A, 2);
        return String.valueOf(m.keySet());
    }

    /// Nine keys in one bin of a 64-bin table: a tree bin.
    public static String treeBin() {
        Map<Object, Integer> m = new HashMap<>(64);
        for (int i = 0; i < 9; i++) m.put(new Same(i), i);
        StringBuilder sb = new StringBuilder();
        for (Object k : m.keySet()) sb.append(((Same) k).id);
        return sb.toString();
    }

    /// The same nine in a 16-bin table: treeifyBin resizes instead.
    public static String smallTableResizes() {
        Map<Object, Integer> m = new HashMap<>();
        for (int i = 0; i < 9; i++) m.put(new Pt(i * 16), i);
        m.put("z", 0);
        StringBuilder sb = new StringBuilder();
        for (Object k : m.keySet()) sb.append(k).append(',');
        return sb.toString();
    }

    public static String accessOrder() {
        Map<String, Integer> m = new LinkedHashMap<>(16, 0.75f, true);
        m.put("a", 1); m.put("b", 2); m.get("a");
        return String.valueOf(m.keySet());
    }





    public static String random_m0() { return RandomHash.m0(); }
    public static String random_m1() { return RandomHash.m1(); }
    public static String random_m2() { return RandomHash.m2(); }
    public static String random_m3() { return RandomHash.m3(); }
    public static String random_m4() { return RandomHash.m4(); }
    public static String random_m5() { return RandomHash.m5(); }
    public static String random_m6() { return RandomHash.m6(); }
    public static String random_m7() { return RandomHash.m7(); }
    public static String random_m8() { return RandomHash.m8(); }
    public static String random_m9() { return RandomHash.m9(); }
    public static String random_m10() { return RandomHash.m10(); }
    public static String random_m11() { return RandomHash.m11(); }
    public static String random_m12() { return RandomHash.m12(); }
    public static String random_m13() { return RandomHash.m13(); }
    public static String random_m14() { return RandomHash.m14(); }
    public static String random_m15() { return RandomHash.m15(); }
}
"#;

const RANDOM_HASH: &str = r#"package demo;

import java.util.*;
import java.util.stream.*;

/// Seeded random hash-collection programs (`gen_hash.py 11 150 core`, the first 16).
final class RandomHash {
    static final StringBuilder log = new StringBuilder();
    /** Logs every equals/hashCode; equal by id, hash id % 5 (collisions). */
    static final class T {
        final int id; T(int id) { this.id = id; }
        @Override public boolean equals(Object o) { log.append('e').append(id); return o instanceof T t && t.id == id; }
        @Override public int hashCode() { log.append('h').append(id); return id % 5; }
        @Override public String toString() { return "T" + id; }
    }
    record K(int id, String s) { }
    static String it(Iterable<?> c) {
        StringBuilder sb = new StringBuilder("[");
        for (Object o : c) sb.append(o).append(',');
        return sb.append(']').toString();
    }
    static String run(java.util.function.Supplier<Object> op) {
        try { return String.valueOf(op.get()); }
        catch (RuntimeException e) { return "!" + e.getClass().getSimpleName(); }
    }

    public static String m0() {
        log.setLength(0);
        Map<Object, Object> a = new LinkedHashMap<>();
        Map<Object, Object> b = new HashMap<>();
        b.put(null, Integer.valueOf(0));
        b.put(new T(1), Integer.valueOf(0));
        b.put(null, new T(0));
        b.put(new T(0), new T(1));
        b.put(new T(7), new T(1));
        for (int i = 0; i < 25; i++) a.put("x" + i, i);
        log.append(run(() -> a.get(new T(10)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.get(new K(2, "k")))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> { a.putAll(b); return a.size(); })).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue(new T(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey(new K(0, "k")))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.getOrDefault(new T(10), "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.getOrDefault(new T(6), "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.putIfAbsent(new T(4), "v4"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.getOrDefault("s38", "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
    public static String m1() {
        log.setLength(0);
        Map<Object, Object> a = new HashMap<>(4, 1.0f);
        Map<Object, Object> b = new HashMap<>();
        b.put(new T(1), "v1");
        b.put(null, "v2");
        log.append(run(() -> a.remove(null))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.getOrDefault(new K(1, "k"), "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey("s30"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> { a.putAll(b); return a.size(); })).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.putIfAbsent(new K(3, "k"), new T(2)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.getOrDefault(new T(2), "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.remove(new T(0)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.remove(new T(5)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(null, Integer.valueOf(0)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.remove(new T(9)))).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
    public static String m2() {
        log.setLength(0);
        Map<Object, Object> a = new HashMap<>();
        Map<Object, Object> b = new HashMap<>();
        b.put(new K(2, "k"), Integer.valueOf(3));
        b.put(new T(7), Integer.valueOf(0));
        b.put(null, "v0");
        for (int i = 0; i < 13; i++) a.put("x" + i, i);
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.get(Integer.valueOf(42)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.getOrDefault(new K(0, "k"), "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.replace(new K(1, "k"), Integer.valueOf(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
    public static String m3() {
        log.setLength(0);
        Map<Object, Object> a = new HashMap<>(5);
        Map<Object, Object> b = new HashMap<>();
        b.put(new K(0, "k"), new T(0));
        b.put(new K(4, "k"), new T(0));
        b.put(new T(6), Integer.valueOf(1));
        b.put(new T(2), "v0");
        b.put(Integer.valueOf(188), Integer.valueOf(1));
        for (int i = 0; i < 70; i++) a.put("x" + i, i);
        log.append(run(() -> a.remove("s34", Integer.valueOf(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.replace(Integer.valueOf(266), Integer.valueOf(4)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.getOrDefault(new T(6), "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.get(new T(6)))).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
    public static String m4() {
        log.setLength(0);
        Map<Object, Object> a = new LinkedHashMap<>();
        Map<Object, Object> b = new HashMap<>();
        b.put(new T(4), new T(2));
        b.put(new T(5), Integer.valueOf(0));
        b.put(new K(4, "k"), "v3");
        b.put(new T(1), new T(1));
        b.put(new K(5, "k"), new T(1));
        log.append(run(() -> a.get("s37"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> { a.putAll(b); return a.size(); })).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue(new T(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey(null))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(Integer.valueOf(2), new T(0)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.get(Integer.valueOf(205)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue(Integer.valueOf(3)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(Integer.valueOf(162), new T(3)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.replace(new K(1, "k"), "v0"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
    public static String m5() {
        log.setLength(0);
        Map<Object, Object> a = new LinkedHashMap<>();
        Map<Object, Object> b = new HashMap<>();
        b.put(new T(6), new T(1));
        b.put(new T(10), new T(1));
        b.put(null, new T(1));
        b.put(new K(1, "k"), "v2");
        b.put(new T(3), "v0");
        log.append(run(() -> { a.putAll(b); return a.size(); })).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey(new T(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey(Integer.valueOf(175)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.getOrDefault(new T(0), "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey(new T(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey(null))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.getOrDefault(new K(3, "k"), "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.putIfAbsent(null, "v2"))).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
    public static String m6() {
        log.setLength(0);
        Map<Object, Object> a = new HashMap<>();
        Map<Object, Object> b = new HashMap<>();
        b.put(new T(8), new T(0));
        b.put("s36", Integer.valueOf(1));
        b.put(new K(0, "k"), Integer.valueOf(3));
        log.append(run(() -> a.remove("s11"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue("v0"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.get(new K(2, "k")))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put("s32", Integer.valueOf(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> { a.putAll(b); return a.size(); })).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue("v1"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey(new K(2, "k")))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.putIfAbsent("s4", Integer.valueOf(4)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
    public static String m7() {
        log.setLength(0);
        Map<Object, Object> a = new LinkedHashMap<>(5);
        Map<Object, Object> b = new HashMap<>();
        b.put("s17", "v4");
        b.put(new T(3), Integer.valueOf(0));
        b.put("s3", new T(0));
        b.put(new K(0, "k"), Integer.valueOf(3));
        log.append(run(() -> a.containsKey(new T(8)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(new T(9), "v3"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.remove(new K(4, "k")))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.remove("s29"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.remove(new K(5, "k")))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue(new T(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(new T(3), new T(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
    public static String m8() {
        log.setLength(0);
        Set<Object> a = new HashSet<>();
        Set<Object> b = new HashSet<>();
        b.add(new T(3));
        b.add(Integer.valueOf(142));
        for (int i = 0; i < 13; i++) a.add("x" + i);
        log.append(run(() -> a.remove(Integer.valueOf(120)))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.contains(new K(2, "k")))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.contains("s27"))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.add(new K(4, "k")))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.add("s36"))).append(';');
        log.append(it(a)).append(';');
        return log.toString();
    }
    public static String m9() {
        log.setLength(0);
        Map<Object, Object> a = new HashMap<>(33);
        Map<Object, Object> b = new HashMap<>();
        b.put(new K(2, "k"), new T(2));
        b.put(new K(2, "k"), new T(3));
        b.put(null, Integer.valueOf(3));
        b.put(new K(1, "k"), "v3");
        b.put(new K(2, "k"), new T(3));
        log.append(run(() -> a.putIfAbsent(null, new T(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(new T(1), "v3"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(Integer.valueOf(81), new T(0)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.get(null))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.remove(new K(5, "k")))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
    public static String m10() {
        log.setLength(0);
        Map<Object, Object> a = new LinkedHashMap<>(8, 0.5f);
        Map<Object, Object> b = new HashMap<>();
        b.put(new T(6), new T(0));
        b.put(new K(5, "k"), Integer.valueOf(0));
        b.put(new T(3), Integer.valueOf(3));
        b.put(new T(6), Integer.valueOf(4));
        log.append(run(() -> a.put("s19", "v0"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey("s1"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue("v4"))).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
    public static String m11() {
        log.setLength(0);
        Map<Object, Object> a = new HashMap<>(33);
        Map<Object, Object> b = new HashMap<>();
        b.put("s21", "v4");
        b.put("s3", new T(3));
        b.put(Integer.valueOf(140), Integer.valueOf(1));
        b.put(new K(3, "k"), Integer.valueOf(3));
        b.put(new T(5), new T(1));
        log.append(run(() -> a.getOrDefault(Integer.valueOf(273), "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey(new T(2)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.getOrDefault("s29", "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> { a.putAll(b); return a.size(); })).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.getOrDefault(new T(4), "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
    public static String m12() {
        log.setLength(0);
        Set<Object> a = new HashSet<>(2);
        Set<Object> b = new HashSet<>();
        b.add("s35");
        b.add(new T(4));
        b.add(new T(3));
        b.add("s7");
        log.append(run(() -> a.add(null))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.addAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.remove(new T(5)))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.remove(null))).append(';');
        log.append(it(a)).append(';');
        return log.toString();
    }
    public static String m13() {
        log.setLength(0);
        Map<Object, Object> a = new LinkedHashMap<>();
        Map<Object, Object> b = new HashMap<>();
        b.put(new K(0, "k"), new T(2));
        b.put(new K(4, "k"), new T(1));
        b.put(new T(8), new T(0));
        b.put(new T(1), new T(3));
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue(new T(0)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(new T(11), "v2"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.putIfAbsent(new K(5, "k"), "v2"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> { a.putAll(b); return a.size(); })).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(new T(10), new T(3)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> { a.putAll(b); return a.size(); })).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.remove(new T(9)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(new K(1, "k"), new T(0)))).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
    public static String m14() {
        log.setLength(0);
        Set<Object> a = new HashSet<>(4, 0.5f);
        Set<Object> b = new HashSet<>();
        b.add("s39");
        b.add(null);
        b.add("s18");
        b.add("s29");
        log.append(run(() -> a.addAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.add("s24"))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.add(new T(1)))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.add(Integer.valueOf(122)))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.contains("s36"))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.remove(new T(8)))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.addAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.add(new T(5)))).append(';');
        log.append(it(a)).append(';');
        return log.toString();
    }
    public static String m15() {
        log.setLength(0);
        Map<Object, Object> a = new HashMap<>();
        Map<Object, Object> b = new HashMap<>();
        b.put(new T(3), Integer.valueOf(0));
        b.put(new K(3, "k"), new T(1));
        b.put(new T(3), new T(2));
        b.put(new K(2, "k"), new T(2));
        b.put("s26", Integer.valueOf(1));
        for (int i = 0; i < 70; i++) a.put("x" + i, i);
        log.append(run(() -> a.remove(new K(1, "k"), Integer.valueOf(4)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.putIfAbsent(Integer.valueOf(59), Integer.valueOf(4)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.remove(Integer.valueOf(37)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.replace("s36", "v0"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(new T(4), "v4"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
}
"#;

fn fixture(test: &str) -> Fixture {
    Fixture::build(
        test,
        &[
            ("demo/Probe.java", PROBE),
            ("demo/RandomHash.java", RANDOM_HASH),
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

/// The reviewer's programs on #98: `IdentityHashMap` keeps identity, an
/// `equals` that is always `true` does not merge keys of different hashes,
/// one that throws only throws for a same-hash key, `equals` runs once per
/// lookup, a `HashSet` copy dedupes records, and strings, `Integer`s and
/// records iterate in bucket order.
#[test]
fn hash_collections_answer_the_reviewers_programs_like_jdk_25() {
    assert_exact(
        "hash-collections-answer-the-reviewers-pr",
        &[
            ("identityMap", "2|null|false"),
            ("allEq", "true|2|null"),
            ("boom", "false|true|2"),
            ("equalsCounts", "1|1"),
            ("hashSetCopy", "1"),
            ("hashSetStrings", "apple,a,pear,b"),
            ("hashMapInts", "[17, 5, 300]"),
            ("hashMapRecords", "1,20,4,"),
        ],
    );
}

/// Enum keys are found by identity, and `treeifyBin` on a 16-bin table
/// resizes instead (nine keys whose hashes share their low four bits).
#[test]
fn the_table_follows_hash_map_step_by_step() {
    assert_exact(
        "the-table-follows-hash-map-step-by-step",
        &[
            ("enumKeys", "2|false|3"),
            (
                "smallTableResizes",
                "Pt[x=0],Pt[x=32],Pt[x=64],Pt[x=96],Pt[x=128],Pt[x=16],Pt[x=48],Pt[x=80],Pt[x=112],z,",
            ),
        ],
    );
}

/// Generated programs: every answer, every logged call and every
/// iteration order matches JDK 25.
#[test]
fn random_hash_programs_match_jdk_25() {
    assert_exact(
        "random-hash-programs-match-jdk-25",
        &[
            (
                "random_m0",
                "h1h0e0h7h10null;[x0,x1,x2,x3,x4,x5,x6,x7,x8,x9,x10,x11,x12,x13,x14,x15,x16,x17,x18,x19,x20,x21,x22,x23,x24,];null;[x0,x1,x2,x3,x4,x5,x6,x7,x8,x9,x10,x11,x12,x13,x14,x15,x16,x17,x18,x19,x20,x21,x22,x23,x24,];h0e0h1h729;[x0,x1,x2,x3,x4,x5,x6,x7,x8,x9,x10,x11,x12,x13,x14,x15,x16,x17,x18,x19,x20,x21,x22,x23,x24,null,T0,T1,T7,];29;[x0,x1,x2,x3,x4,x5,x6,x7,x8,x9,x10,x11,x12,x13,x14,x15,x16,x17,x18,x19,x20,x21,x22,x23,x24,null,T0,T1,T7,];e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1true;[x0,x1,x2,x3,x4,x5,x6,x7,x8,x9,x10,x11,x12,x13,x14,x15,x16,x17,x18,x19,x20,x21,x22,x23,x24,null,T0,T1,T7,];false;[x0,x1,x2,x3,x4,x5,x6,x7,x8,x9,x10,x11,x12,x13,x14,x15,x16,x17,x18,x19,x20,x21,x22,x23,x24,null,T0,T1,T7,];29;[x0,x1,x2,x3,x4,x5,x6,x7,x8,x9,x10,x11,x12,x13,x14,x15,x16,x17,x18,x19,x20,x21,x22,x23,x24,null,T0,T1,T7,];h10e10e10d;[x0,x1,x2,x3,x4,x5,x6,x7,x8,x9,x10,x11,x12,x13,x14,x15,x16,x17,x18,x19,x20,x21,x22,x23,x24,null,T0,T1,T7,];h6e6d;[x0,x1,x2,x3,x4,x5,x6,x7,x8,x9,x10,x11,x12,x13,x14,x15,x16,x17,x18,x19,x20,x21,x22,x23,x24,null,T0,T1,T7,];h4null;[x0,x1,x2,x3,x4,x5,x6,x7,x8,x9,x10,x11,x12,x13,x14,x15,x16,x17,x18,x19,x20,x21,x22,x23,x24,null,T0,T1,T7,T4,];d;[x0,x1,x2,x3,x4,x5,x6,x7,x8,x9,x10,x11,x12,x13,x14,x15,x16,x17,x18,x19,x20,x21,x22,x23,x24,null,T0,T1,T7,T4,];",
            ),
            (
                "random_m1",
                "h1null;[];d;[];false;[];h12;[null,T1,];null;[null,K[id=3, s=k],T1,];h2d;[null,K[id=3, s=k],T1,];h2h17616;[null,K[id=3, s=k],T1,];h0e0null;[null,K[id=3, s=k],T1,];h5e5null;[null,K[id=3, s=k],T1,];v2;[null,K[id=3, s=k],T1,];h9null;[null,K[id=3, s=k],T1,];",
            ),
            (
                "random_m2",
                "h7388373;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];null;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];13;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];d;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];388373;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];null;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];13;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];388373;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];",
            ),
            (
                "random_m3",
                "h6h2false;[x50,x52,x51,x10,x54,x53,x12,x56,x11,x55,x14,x58,x13,x57,x16,x15,x59,x18,x17,x19,x61,x60,x63,x62,x21,x65,x20,x64,x0,x23,x67,x1,x22,x66,x2,x25,x69,x3,x24,x68,x4,x27,x5,x26,x6,x29,x7,x28,x8,x9,x30,x32,x31,x34,x33,x36,x35,x38,x37,x39,x41,x40,x43,x42,x45,x44,x47,x46,x49,x48,];null;[x50,x52,x51,x10,x54,x53,x12,x56,x11,x55,x14,x58,x13,x57,x16,x15,x59,x18,x17,x19,x61,x60,x63,x62,x21,x65,x20,x64,x0,x23,x67,x1,x22,x66,x2,x25,x69,x3,x24,x68,x4,x27,x5,x26,x6,x29,x7,x28,x8,x9,x30,x32,x31,x34,x33,x36,x35,x38,x37,x39,x41,x40,x43,x42,x45,x44,x47,x46,x49,x48,];h6d;[x50,x52,x51,x10,x54,x53,x12,x56,x11,x55,x14,x58,x13,x57,x16,x15,x59,x18,x17,x19,x61,x60,x63,x62,x21,x65,x20,x64,x0,x23,x67,x1,x22,x66,x2,x25,x69,x3,x24,x68,x4,x27,x5,x26,x6,x29,x7,x28,x8,x9,x30,x32,x31,x34,x33,x36,x35,x38,x37,x39,x41,x40,x43,x42,x45,x44,x47,x46,x49,x48,];h6null;[x50,x52,x51,x10,x54,x53,x12,x56,x11,x55,x14,x58,x13,x57,x16,x15,x59,x18,x17,x19,x61,x60,x63,x62,x21,x65,x20,x64,x0,x23,x67,x1,x22,x66,x2,x25,x69,x3,x24,x68,x4,x27,x5,x26,x6,x29,x7,x28,x8,x9,x30,x32,x31,x34,x33,x36,x35,x38,x37,x39,x41,x40,x43,x42,x45,x44,x47,x46,x49,x48,];",
            ),
            (
                "random_m4",
                "h4h5h1null;[];h5h1h45;[T5,T1,T4,K[id=5, s=k],K[id=4, s=k],];e1e1true;[T5,T1,T4,K[id=5, s=k],K[id=4, s=k],];false;[T5,T1,T4,K[id=5, s=k],K[id=4, s=k],];null;[T5,T1,T4,K[id=5, s=k],K[id=4, s=k],2,];null;[T5,T1,T4,K[id=5, s=k],K[id=4, s=k],2,];false;[T5,T1,T4,K[id=5, s=k],K[id=4, s=k],2,];6;[T5,T1,T4,K[id=5, s=k],K[id=4, s=k],2,];null;[T5,T1,T4,K[id=5, s=k],K[id=4, s=k],2,162,];null;[T5,T1,T4,K[id=5, s=k],K[id=4, s=k],2,162,];7;[T5,T1,T4,K[id=5, s=k],K[id=4, s=k],2,162,];",
            ),
            (
                "random_m5",
                "h6h10h3h10h6h35;[T10,null,T6,T3,K[id=1, s=k],];h1e1false;[T10,null,T6,T3,K[id=1, s=k],];false;[T10,null,T6,T3,K[id=1, s=k],];h0e0e0d;[T10,null,T6,T3,K[id=1, s=k],];h1e1false;[T10,null,T6,T3,K[id=1, s=k],];true;[T10,null,T6,T3,K[id=1, s=k],];d;[T10,null,T6,T3,K[id=1, s=k],];T1;[T10,null,T6,T3,K[id=1, s=k],];",
            ),
            (
                "random_m6",
                "h8null;[];false;[];null;[];null;[s32,];h84;[s32,T8,s36,K[id=0, s=k],];false;[s32,T8,s36,K[id=0, s=k],];false;[s32,T8,s36,K[id=0, s=k],];null;[s4,s32,T8,s36,K[id=0, s=k],];5;[s4,s32,T8,s36,K[id=0, s=k],];",
            ),
            (
                "random_m7",
                "h3false;[];h9null;[T9,];null;[T9,];null;[T9,];null;[T9,];e1false;[T9,];h3null;[T9,T3,];",
            ),
            (
                "random_m8",
                "h3false;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];false;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];false;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];13;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];true;[x8,x9,K[id=4, s=k],x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];true;[x8,x9,K[id=4, s=k],x10,s36,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];",
            ),
            (
                "random_m9",
                "null;[null,];h1null;[null,T1,];null;[null,T1,81,];T1;[null,T1,81,];null;[null,T1,81,];h1h1h03790;[null,T1,81,];3;[null,T1,81,];",
            ),
            (
                "random_m10",
                "h6h3h6e6null;[s19,];false;[s19,];1;[s19,];false;[s19,];",
            ),
            (
                "random_m11",
                "h5d;[];false;[];d;[];0;[];0;[];h55;[T5,K[id=3, s=k],140,s3,s21,];5;[T5,K[id=3, s=k],140,s3,s21,];h4d;[T5,K[id=3, s=k],140,s3,s21,];",
            ),
            (
                "random_m12",
                "h4h3true;[null,];1;[null,];0;[null,];h3h4true;[null,T3,s35,T4,s7,];h5e5false;[null,T3,s35,T4,s7,];h3h4115776;[null,T3,s35,T4,s7,];5;[null,T3,s35,T4,s7,];true;[T3,s35,T4,s7,];",
            ),
            (
                "random_m13",
                "h8h10;[];false;[];h11null;[T11,];null;[T11,K[id=5, s=k],];h1e1h86;[T11,K[id=5, s=k],T1,T8,K[id=4, s=k],K[id=0, s=k],];h10null;[T11,K[id=5, s=k],T1,T8,K[id=4, s=k],K[id=0, s=k],T10,];h1e1h87;[T11,K[id=5, s=k],T1,T8,K[id=4, s=k],K[id=0, s=k],T10,];h9null;[T11,K[id=5, s=k],T1,T8,K[id=4, s=k],K[id=0, s=k],T10,];null;[T11,K[id=5, s=k],T1,T8,K[id=4, s=k],K[id=0, s=k],T10,K[id=1, s=k],];",
            ),
            (
                "random_m14",
                "true;[null,s39,s18,s29,];336365;[null,s39,s18,s29,];true;[null,s24,s39,s18,s29,];h1true;[null,T1,s24,s39,s18,s29,];true;[null,T1,s24,s39,122,s18,s29,];7;[null,T1,s24,s39,122,s18,s29,];false;[null,T1,s24,s39,122,s18,s29,];h8false;[null,T1,s24,s39,122,s18,s29,];false;[null,T1,s24,s39,122,s18,s29,];h5e5true;[null,T5,T1,s24,s39,122,s18,s29,];",
            ),
            (
                "random_m15",
                "h3h3e3false;[x50,x52,x51,x10,x54,x53,x12,x56,x11,x55,x14,x58,x13,x57,x16,x15,x59,x18,x17,x19,x61,x60,x63,x62,x21,x65,x20,x64,x0,x23,x67,x1,x22,x66,x2,x25,x69,x3,x24,x68,x4,x27,x5,x26,x6,x29,x7,x28,x8,x9,x30,x32,x31,x34,x33,x36,x35,x38,x37,x39,x41,x40,x43,x42,x45,x44,x47,x46,x49,x48,];null;[x50,x52,x51,x10,x54,x53,x12,x56,x11,x55,x14,x58,x13,x57,x16,x15,x59,x18,x17,x19,x61,x60,x63,x62,x21,x65,x20,x64,x0,x23,x67,x1,x22,x66,x2,x25,x69,x3,x24,x68,59,x4,x27,x5,x26,x6,x29,x7,x28,x8,x9,x30,x32,x31,x34,x33,x36,x35,x38,x37,x39,x41,x40,x43,x42,x45,x44,x47,x46,x49,x48,];7055891;[x50,x52,x51,x10,x54,x53,x12,x56,x11,x55,x14,x58,x13,x57,x16,x15,x59,x18,x17,x19,x61,x60,x63,x62,x21,x65,x20,x64,x0,x23,x67,x1,x22,x66,x2,x25,x69,x3,x24,x68,59,x4,x27,x5,x26,x6,x29,x7,x28,x8,x9,x30,x32,x31,x34,x33,x36,x35,x38,x37,x39,x41,x40,x43,x42,x45,x44,x47,x46,x49,x48,];null;[x50,x52,x51,x10,x54,x53,x12,x56,x11,x55,x14,x58,x13,x57,x16,x15,x59,x18,x17,x19,x61,x60,x63,x62,x21,x65,x20,x64,x0,x23,x67,x1,x22,x66,x2,x25,x69,x3,x24,x68,59,x4,x27,x5,x26,x6,x29,x7,x28,x8,x9,x30,x32,x31,x34,x33,x36,x35,x38,x37,x39,x41,x40,x43,x42,x45,x44,x47,x46,x49,x48,];null;[x50,x52,x51,x10,x54,x53,x12,x56,x11,x55,x14,x58,x13,x57,x16,x15,x59,x18,x17,x19,x61,x60,x63,x62,x21,x65,x20,x64,x0,x23,x67,x1,x22,x66,x2,x25,x69,x3,x24,x68,59,x4,x27,x5,x26,x6,x29,x7,x28,x8,x9,x30,x32,x31,x34,x33,x36,x35,x38,x37,x39,x41,x40,x43,x42,x45,x44,x47,x46,x49,x48,];h4null;[T4,x50,x52,x51,x10,x54,x53,x12,x56,x11,x55,x14,x58,x13,x57,x16,x15,x59,x18,x17,x19,x61,x60,x63,x62,x21,x65,x20,x64,x0,x23,x67,x1,x22,x66,x2,x25,x69,x3,x24,x68,59,x4,x27,x5,x26,x6,x29,x7,x28,x8,x9,x30,x32,x31,x34,x33,x36,x35,x38,x37,x39,x41,x40,x43,x42,x45,x44,x47,x46,x49,x48,];72;[T4,x50,x52,x51,x10,x54,x53,x12,x56,x11,x55,x14,x58,x13,x57,x16,x15,x59,x18,x17,x19,x61,x60,x63,x62,x21,x65,x20,x64,x0,x23,x67,x1,x22,x66,x2,x25,x69,x3,x24,x68,59,x4,x27,x5,x26,x6,x29,x7,x28,x8,x9,x30,x32,x31,x34,x33,x36,x35,x38,x37,x39,x41,x40,x43,x42,x45,x44,x47,x46,x49,x48,];",
            ),
        ],
    );
}

/// What no run can reproduce is counted, under the call that shows it: a
/// lookup by a key whose class keeps `Object.hashCode` but has its own
/// `equals`, the order of enum (identity-hashed) keys, of a tree bin and of
/// an access-order `LinkedHashMap`, and a removal through `keySet()`, which
/// the host answers with a copy. (OpenJDK 25.0.5 printed `[A, B]` and
/// `[B, A]` for the enum keys, `201345678` and `012345678` for the tree bin,
/// on two runs.)
#[test]
fn identity_hashes_tree_bins_and_view_copies_are_counted() {
    let fixture = fixture("identity-hashes-tree-bins-and-view-copie");
    let cases: &[(&str, &[(&str, usize)])] = &[
        (
            "noHash",
            &[
                ("java/util/Map#containsKey(Ljava/lang/Object;)Z", 2),
                ("java/util/Map#get(Ljava/lang/Object;)Ljava/lang/Object;", 1),
                ("java/util/Set#add(Ljava/lang/Object;)Z", 1),
            ],
        ),
        (
            "mapViews",
            &[("java/util/Set#remove(Ljava/lang/Object;)Z", 1)],
        ),
        (
            "enumKeysIterated",
            &[("java/util/Map#keySet()Ljava/util/Set;", 1)],
        ),
        ("treeBin", &[("java/util/Map#keySet()Ljava/util/Set;", 1)]),
        (
            "accessOrder",
            &[("java/util/Map#keySet()Ljava/util/Set;", 1)],
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
