//! The hash collections' callback and bulk members (`hash_bulk.rs`):
//! `compute*`/`merge`, `containsAll`/`removeAll`/`retainAll`, `equals`, and
//! streams' `distinct`/`toSet`/`toMap`, run as JDK 25 runs them.
//!
//! Every expected value is OpenJDK 25.0.5's output for the same sources
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `RandomHash` is generated (`gen_hash.py 1 150`): random
//! operations on random hash collections over logging keys whose hashes
//! collide (`e<id>`/`h<id>` per `equals`/`hashCode`), records, strings,
//! boxes and `null`, printing the iteration order after every operation.

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







    public static String streamDistinct() {
        long d = Stream.of(new Key(1), new Key(1), new Key(2)).distinct().count();
        int s = Stream.of(new Key(1), new Key(1)).collect(Collectors.toSet()).size();
        return d + "|" + s;
    }

    public static String streamToMapDup() {
        try {
            return String.valueOf(Stream.of(new Key(1), new Key(1)).collect(Collectors.toMap(k -> k, k -> "v")).size());
        } catch (IllegalStateException e) {
            return "threw:java.lang.IllegalStateException:" + e.getMessage();
        }
    }









    public static String computeHeads() {
        Map<Object, Integer> m = new HashMap<>();
        m.put(new Pt(0), 0); m.computeIfAbsent(new Pt(16), k -> 1); m.merge(new Pt(32), 2, Integer::sum);
        m.put(new Pt(48), 3);
        StringBuilder sb = new StringBuilder();
        for (Object k : m.keySet()) sb.append(k).append(',');
        return sb.toString();
    }

    public static String copyEquals() {
        Set<Object> a = new HashSet<>(); a.add("x"); a.add("y");
        Set<Object> c = new HashSet<>(a);
        return c.size() + "|" + c.equals(a) + "|" + a.equals(c) + "|" + c;
    }

    public static String containsAllSelf() {
        Set<Object> a = new HashSet<>(); a.add("x");
        Set<Object> b = new HashSet<>(); b.add("x");
        return a.containsAll(b) + "|" + a.equals(b);
    }

    public static String mergeJarValue() {
        Map<Object, Object> a = new HashMap<>();
        a.put("k", new T(0));
        Object r = a.merge("k", "v", (x, y) -> x + "+" + y);
        return r + "|" + a.get("k");
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

/// Seeded random hash-collection programs (`gen_hash.py`, seed 1, the first 16).
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
        Map<Object, Object> a = new HashMap<>(2, 2.0f);
        Map<Object, Object> b = new HashMap<>();
        b.put("s13", Integer.valueOf(3));
        b.put("s38", new T(2));
        b.put(new T(9), "v0");
        b.put(new T(10), new T(3));
        b.put(new T(6), Integer.valueOf(4));
        log.append(run(() -> a.equals(b))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.merge(Integer.valueOf(51), new T(0), (x, y) -> x + "+" + y))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> { a.putAll(b); return a.size(); })).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.putIfAbsent(new T(7), new T(3)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(it(Stream.of(new T(0), new T(8), Integer.valueOf(118), "s32", new K(4, "k")).distinct().collect(Collectors.toList()))).append(';');
        return log.toString();
    }
    public static String m1() {
        log.setLength(0);
        Set<Object> a = new LinkedHashSet<>(8, 0.5f);
        Set<Object> b = new HashSet<>();
        b.add(Integer.valueOf(287));
        b.add(new T(6));
        b.add(new T(7));
        log.append(run(() -> a.add(Integer.valueOf(211)))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.removeAll(a))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.retainAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.contains(new T(4)))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(it(Stream.of(new K(3, "k"), new K(3, "k"), new T(4), new T(4), Integer.valueOf(107)).collect(Collectors.toSet()))).append(';');
        return log.toString();
    }
    public static String m2() {
        log.setLength(0);
        Set<Object> a = new HashSet<>(2, 0.5f);
        Set<Object> b = new HashSet<>();
        b.add(new T(11));
        b.add(new T(7));
        b.add(Integer.valueOf(218));
        log.append(run(() -> a.size())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.addAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.addAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.add(new K(5, "k")))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.retainAll(a))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.add(new T(3)))).append(';');
        log.append(it(a)).append(';');
        return log.toString();
    }
    public static String m3() {
        log.setLength(0);
        Map<Object, Object> a = new HashMap<>(2);
        Map<Object, Object> b = new HashMap<>();
        b.put(new T(3), Integer.valueOf(4));
        b.put("s13", new T(3));
        log.append(run(() -> a.putIfAbsent(Integer.valueOf(120), "v0"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue(new T(2)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> { a.putAll(b); return a.size(); })).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.remove(new T(9), Integer.valueOf(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey(new K(4, "k")))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey(new T(0)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> { a.putAll(b); return a.size(); })).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey(Integer.valueOf(150)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(it(Stream.of(new K(0, "k"), new T(0), new K(5, "k"), "n", new K(3, "k")).distinct().collect(Collectors.toList()))).append(';');
        return log.toString();
    }
    public static String m4() {
        log.setLength(0);
        Map<Object, Object> a = new HashMap<>();
        Map<Object, Object> b = new HashMap<>();
        b.put(new T(9), Integer.valueOf(3));
        b.put(new T(8), "v1");
        b.put(new K(0, "k"), "v2");
        b.put(null, Integer.valueOf(1));
        for (int i = 0; i < 13; i++) a.put("x" + i, i);
        log.append(run(() -> a.get(null))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue("v0"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.remove(Integer.valueOf(36), Integer.valueOf(0)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.putIfAbsent("s30", "v1"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.computeIfAbsent(new T(2), z -> "v1"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.getOrDefault(new T(3), "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.replace(new T(4), "v3"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue("v0"))).append(';');
        log.append(it(a.keySet())).append(';');
        return log.toString();
    }
    public static String m5() {
        log.setLength(0);
        Set<Object> a = new LinkedHashSet<>();
        Set<Object> b = new HashSet<>();
        b.add(new T(4));
        b.add(new K(3, "k"));
        b.add(null);
        log.append(run(() -> a.size())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.retainAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.remove(new T(11)))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.removeAll(a))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> new HashSet<>(a).equals(a))).append(';');
        log.append(it(a)).append(';');
        return log.toString();
    }
    public static String m6() {
        log.setLength(0);
        Map<Object, Object> a = new HashMap<>();
        Map<Object, Object> b = new HashMap<>();
        b.put(new K(3, "k"), Integer.valueOf(4));
        b.put("s22", new T(1));
        b.put(new T(8), Integer.valueOf(2));
        log.append(run(() -> a.equals(b))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.computeIfAbsent(new T(1), z -> "v3"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.getOrDefault(null, "d"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(it(Stream.of(new T(8), new T(4), new K(4, "k"), new K(5, "k"), "s10").distinct().collect(Collectors.toList()))).append(';');
        return log.toString();
    }
    public static String m7() {
        log.setLength(0);
        Map<Object, Object> a = new HashMap<>();
        Map<Object, Object> b = new HashMap<>();
        b.put(new T(0), new T(0));
        b.put(new K(5, "k"), new T(2));
        b.put(new K(3, "k"), "v4");
        b.put(new T(7), new T(2));
        log.append(run(() -> a.put(new T(10), new T(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(new K(5, "k"), Integer.valueOf(3)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.get("s37"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.remove("s15"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue(Integer.valueOf(0)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsKey(new T(7)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue(Integer.valueOf(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.merge(null, "v2", (x, y) -> x + "+" + y))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(it(Stream.of(new T(11), Integer.valueOf(188), "s32", Integer.valueOf(25), new T(4)).distinct().collect(Collectors.toList()))).append(';');
        return log.toString();
    }
    public static String m8() {
        log.setLength(0);
        Set<Object> a = new LinkedHashSet<>(4, 0.75f);
        Set<Object> b = new HashSet<>();
        b.add(new T(7));
        b.add(new K(4, "k"));
        b.add(new K(5, "k"));
        b.add(new T(4));
        b.add(null);
        log.append(run(() -> a.removeAll(a))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> new HashSet<>(a).equals(a))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.removeAll(a))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.remove(new T(11)))).append(';');
        log.append(it(a)).append(';');
        log.append(it(Stream.of(new T(11), new K(0, "k"), new T(3), "s20", "s6").distinct().collect(Collectors.toList()))).append(';');
        return log.toString();
    }
    public static String m9() {
        log.setLength(0);
        Map<Object, Object> a = new LinkedHashMap<>(2, 0.5f);
        Map<Object, Object> b = new HashMap<>();
        b.put(new K(5, "k"), "v2");
        b.put(new T(6), Integer.valueOf(3));
        b.put(new T(3), new T(3));
        b.put(null, new T(3));
        b.put(new K(2, "k"), "v3");
        log.append(run(() -> a.computeIfAbsent(new T(0), z -> new T(3)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> { a.putAll(b); return a.size(); })).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.remove(new T(10)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(new T(7), new T(0)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(it(Stream.of(new K(0, "k"), "s5", new T(0), "s8", new K(5, "k")).collect(Collectors.toSet()))).append(';');
        return log.toString();
    }
    public static String m10() {
        log.setLength(0);
        Set<Object> a = new HashSet<>(33);
        Set<Object> b = new HashSet<>();
        b.add(new K(5, "k"));
        b.add(new T(3));
        b.add(new T(9));
        b.add(null);
        for (int i = 0; i < 25; i++) a.add("x" + i);
        log.append(run(() -> a.equals(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.contains("s4"))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.add(new T(2)))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(it(Stream.of(Integer.valueOf(16), "s8", "s28", new T(11), Integer.valueOf(138)).distinct().collect(Collectors.toList()))).append(';');
        log.append(it(Stream.of(new K(0, "k"), new K(0, "k"), "s3", new K(2, "k"), new T(4)).collect(Collectors.toSet()))).append(';');
        return log.toString();
    }
    public static String m11() {
        log.setLength(0);
        Set<Object> a = new HashSet<>(4, 0.5f);
        Set<Object> b = new HashSet<>();
        b.add(Integer.valueOf(285));
        b.add(new T(5));
        b.add(new K(4, "k"));
        log.append(run(() -> new HashSet<>(a).equals(a))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.retainAll(a))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.addAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.add(new K(5, "k")))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.addAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.removeAll(a))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.retainAll(a))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.contains(new T(0)))).append(';');
        log.append(it(a)).append(';');
        log.append(it(Stream.of("s39", new T(1), Integer.valueOf(136), new T(3), new K(0, "k")).collect(Collectors.toSet()))).append(';');
        return log.toString();
    }
    public static String m12() {
        log.setLength(0);
        Set<Object> a = new LinkedHashSet<>();
        Set<Object> b = new HashSet<>();
        b.add(Integer.valueOf(221));
        b.add(new T(9));
        b.add(new K(3, "k"));
        log.append(run(() -> a.retainAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.add("s0"))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.addAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.removeAll(a))).append(';');
        log.append(it(a)).append(';');
        log.append(it(Stream.of(new T(6), "n", new T(5), "s25", new K(5, "k")).collect(Collectors.toSet()))).append(';');
        return log.toString();
    }
    public static String m13() {
        log.setLength(0);
        Set<Object> a = new LinkedHashSet<>();
        Set<Object> b = new HashSet<>();
        b.add(new K(3, "k"));
        b.add(new K(2, "k"));
        b.add("s21");
        b.add("s29");
        b.add(new T(7));
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.retainAll(a))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.remove("s25"))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.retainAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.add(new T(8)))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.size())).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(it(a)).append(';');
        return log.toString();
    }
    public static String m14() {
        log.setLength(0);
        Set<Object> a = new HashSet<>();
        Set<Object> b = new HashSet<>();
        b.add(Integer.valueOf(131));
        b.add("s34");
        b.add(new K(3, "k"));
        log.append(run(() -> a.removeAll(a))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.removeAll(a))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.remove(new T(3)))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.addAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.retainAll(b))).append(';');
        log.append(it(a)).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(it(a)).append(';');
        return log.toString();
    }
    public static String m15() {
        log.setLength(0);
        Map<Object, Object> a = new HashMap<>(16);
        Map<Object, Object> b = new HashMap<>();
        b.put(new T(5), Integer.valueOf(3));
        b.put(new T(2), "v1");
        b.put(Integer.valueOf(25), "v4");
        log.append(run(() -> a.replace("s20", new T(2)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> { a.putAll(b); return a.size(); })).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.containsValue("v2"))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.get(new T(10)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.put(new T(4), new T(2)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.replace(new K(0, "k"), Integer.valueOf(1)))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.merge(new T(10), new T(0), (x, y) -> x + "+" + y))).append(';');
        log.append(it(a.keySet())).append(';');
        log.append(run(() -> a.get(new T(5)))).append(';');
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

/// `computeIfAbsent`/`merge` insert a new key at its bin's head (records
/// `Pt(0)`, `Pt(16)`, `Pt(32)`, `Pt(48)` share a bin), and `merge` runs its
/// function over a jar value (the host's `merge` answered the new value).
#[test]
fn compute_and_merge_insert_at_the_bin_head_and_run_jar_functions() {
    assert_exact(
        "compute-and-merge-insert-at-the-bin-head",
        &[
            ("computeHeads", "Pt[x=32],Pt[x=16],Pt[x=0],Pt[x=48],"),
            ("mergeJarValue", "T0+v|T0+v"),
        ],
    );
}

/// `AbstractSet.equals` and `containsAll` on hash sets.
#[test]
fn set_equals_and_contains_all_look_up_by_hash() {
    assert_exact(
        "set-equals-and-contains-all-look-up-by-h",
        &[
            ("copyEquals", "2|true|true|[x, y]"),
            ("containsAllSelf", "true|true"),
        ],
    );
}

/// The reviewer's stream programs on #98: `distinct()` and `toSet()` dedupe
/// records through a `HashSet`, `toMap` throws on a duplicate record key.
#[test]
fn streams_collect_into_hash_collections() {
    assert_exact(
        "streams-collect-into-hash-collections",
        &[
            ("streamDistinct", "2|1"),
            (
                "streamToMapDup",
                "threw:java.lang.IllegalStateException:Duplicate key Key[id=1] (attempted merging values v and v)",
            ),
        ],
    );
}

/// Generated programs, with `computeIfAbsent`/`merge`/`remove(k, v)`/
/// `equals`/`containsAll`/`removeAll`/`retainAll` and stream collection:
/// every answer, every logged call and every iteration order matches JDK 25.
#[test]
fn random_hash_programs_match_jdk_25() {
    assert_exact(
        "random-hash-programs-match-jdk-25",
        &[
            (
                "random_m0",
                "h9h10h6false;[];T0;[51,];1;[51,];h10h6h96;[T10,s13,T9,T6,s38,51,];h10h3h9h6h2h0228009;[T10,s13,T9,T6,s38,51,];6;[T10,s13,T9,T6,s38,51,];h7null;[T10,s13,T9,T6,s38,T7,51,];h0h8[T0,T8,118,s32,K[id=4, s=k],];",
            ),
            (
                "random_m1",
                "h6h7true;[211,];211;[211,];true;[];false;[];h4false;[];false;[];h6false;[];h4h4e4[T4,K[id=3, s=k],107,];",
            ),
            (
                "random_m2",
                "h11h70;[];h11h7true;[T11,T7,218,];h11h7false;[T11,T7,218,];true;[T11,T7,218,K[id=5, s=k],];h11h7false;[T11,T7,218,K[id=5, s=k],];h11h7true;[T11,T7,218,K[id=5, s=k],];h3true;[T11,T7,T3,K[id=5, s=k],218,];",
            ),
            (
                "random_m3",
                "h3null;[120,];e2false;[120,];1;[120,];false;[120,];h33;[120,s13,T3,];h9false;[120,s13,T3,];false;[120,s13,T3,];false;[120,s13,T3,];h0false;[120,s13,T3,];h33;[120,s13,T3,];false;[120,s13,T3,];h0[K[id=0, s=k],T0,K[id=5, s=k],n,K[id=3, s=k],];",
            ),
            (
                "random_m4",
                "h9h8null;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];false;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];false;[x8,x9,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];null;[x8,x9,s30,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];h2v1;[x8,x9,T2,s30,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];h3d;[x8,x9,T2,s30,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];h4null;[x8,x9,T2,s30,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];h2504825;[x8,x9,T2,s30,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];false;[x8,x9,T2,s30,x10,x0,x12,x1,x11,x2,x3,x4,x5,x6,x7,];",
            ),
            ("random_m5", "h40;[];false;[];h11false;[];false;[];true;[];"),
            (
                "random_m6",
                "h8false;[];0;[];h1v3;[T1,];h13708;[T1,];d;[T1,];h8h4[T8,T4,K[id=4, s=k],K[id=5, s=k],s10,];",
            ),
            (
                "random_m7",
                "h0h7h10null;[T10,];false;[T10,];null;[T10,K[id=5, s=k],];null;[T10,K[id=5, s=k],];null;[T10,K[id=5, s=k],];false;[T10,K[id=5, s=k],];2;[T10,K[id=5, s=k],];h7false;[T10,K[id=5, s=k],];h10h1262;[T10,K[id=5, s=k],];false;[T10,K[id=5, s=k],];v2;[null,T10,K[id=5, s=k],];h11h4[T11,188,s32,25,T4,];",
            ),
            (
                "random_m8",
                "h7h4false;[];true;[];false;[];h11false;[];h11h3[T11,K[id=0, s=k],T3,s20,s6,];",
            ),
            (
                "random_m9",
                "h6h3h0T3;[T0,];h6h36;[T0,null,T6,T3,K[id=5, s=k],K[id=2, s=k],];h10e10e10null;[T0,null,T6,T3,K[id=5, s=k],K[id=2, s=k],];h7null;[T0,null,T6,T3,K[id=5, s=k],K[id=2, s=k],T7,];h0[T0,s5,s8,K[id=5, s=k],K[id=0, s=k],];",
            ),
            (
                "random_m10",
                "h3h9false;[x8,x9,x10,x12,x11,x14,x13,x16,x15,x18,x17,x19,x21,x20,x0,x23,x1,x22,x2,x3,x24,x4,x5,x6,x7,];false;[x8,x9,x10,x12,x11,x14,x13,x16,x15,x18,x17,x19,x21,x20,x0,x23,x1,x22,x2,x3,x24,x4,x5,x6,x7,];h2true;[x8,x9,T2,x10,x12,x11,x14,x13,x16,x15,x18,x17,x19,x21,x20,x0,x23,x1,x22,x2,x3,x24,x4,x5,x6,x7,];false;[x8,x9,T2,x10,x12,x11,x14,x13,x16,x15,x18,x17,x19,x21,x20,x0,x23,x1,x22,x2,x3,x24,x4,x5,x6,x7,];h11[16,s8,s28,T11,138,];h4[s3,T4,K[id=2, s=k],K[id=0, s=k],];",
            ),
            (
                "random_m11",
                "h5true;[];false;[];false;[];h5true;[T5,285,K[id=4, s=k],];true;[T5,285,K[id=5, s=k],K[id=4, s=k],];h5false;[T5,285,K[id=5, s=k],K[id=4, s=k],];h5true;[];false;[];h5false;[];false;[];h0false;[];h1h3[T1,T3,s39,136,K[id=0, s=k],];",
            ),
            (
                "random_m12",
                "h9false;[];false;[];false;[];false;[];true;[s0,];false;[s0,];h9true;[s0,T9,K[id=3, s=k],221,];h9true;[];h6h5[T5,T6,K[id=5, s=k],s25,n,];",
            ),
            (
                "random_m13",
                "h70;[];false;[];false;[];false;[];false;[];0;[];h8true;[T8,];1;[T8,];1;[T8,];false;[T8,];",
            ),
            (
                "random_m14",
                "false;[];false;[];h3false;[];false;[];true;[131,s34,K[id=3, s=k],];false;[131,s34,K[id=3, s=k],];112479;[131,s34,K[id=3, s=k],];",
            ),
            (
                "random_m15",
                "h5h2null;[];h5h23;[T5,T2,25,];false;[T5,T2,25,];h10e10null;[T5,T2,25,];h4null;[T5,T2,T4,25,];null;[T5,T2,T4,25,];h10e10T0;[T10,T5,T2,T4,25,];h5e5e53;[T10,T5,T2,T4,25,];",
            ),
        ],
    );
}
