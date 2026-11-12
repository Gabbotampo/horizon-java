//! `Set.of`/`Map.of`/`Map.ofEntries` and `IdentityHashMap` as JDK 25 runs
//! them (`immutable_collections.rs`): exact duplicate checks and lookups,
//! counted iteration order and string identity.
//!
//! Every expected value is OpenJDK 25.0.5's output for the same sources
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `RandomImm` is generated (`gen_imm.py 1 150`): random
//! `Set.of`/`Map.of`/`Map.ofEntries` over logging keys whose hashes collide
//! (`e<id>`/`h<id>` per call), records, strings and boxes, with lookups.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"package demo;

import java.util.*;
import java.util.stream.*;

public final class Probe {
    static final StringBuilder log = new StringBuilder();
    /// Logs each call; equal by name, hashed by the name's hash.
    static final class T {
        final String n; T(String n) { this.n = n; }
        @Override public boolean equals(Object o) { log.append('e').append(n); return o instanceof T t && t.n.equals(n); }
        @Override public int hashCode() { log.append('h').append(n); return n.hashCode(); }
        @Override public String toString() { log.append('s').append(n); return n; }
    }
    record Key(int id) { }

    /// The reviewer's case on #98 (`2` before), and `Set12`'s calls.
    public static String setOfDuplicates() {
        String dup;
        try { dup = String.valueOf(Set.of(new Key(1), new Key(1)).size()); }
        catch (IllegalArgumentException e) { dup = "threw:" + e.getMessage(); }
        log.setLength(0);
        try { Set.of(new T("a"), new T("a")); } catch (IllegalArgumentException e) { log.append("|IAE:" + e.getMessage()); }
        log.append("|");
        Set<T> s = Set.of(new T("a"), new T("b"));
        log.append(s.contains(new T("b")));
        String map;
        try { map = String.valueOf(Map.of(new Key(2), "x", new Key(2), "y").size()); }
        catch (IllegalArgumentException e) { map = e.getMessage(); }
        return dup + log + "|" + map;
    }

    /// `SetN`/`MapN` probe deterministically: lookups are exact.
    public static String probes() {
        log.setLength(0);
        Set<T> s = Set.of(new T("a"), new T("b"), new T("c"));
        log.append(s.contains(new T("c"))).append(s.contains(new T("z"))).append(s.hashCode()).append('|');
        Map<Key, String> m = Map.of(new Key(1), "a", new Key(2), "b", new Key(3), "c");
        log.append(m.get(new Key(2))).append(m.containsKey(new Key(9))).append(m.getOrDefault(new Key(9), "d"))
            .append(m.containsValue("c")).append(m.hashCode());
        return log.toString();
    }

    /// `List.of`/`Set.of`/`Map.ofEntries` with their varargs (more than ten
    /// elements were one element, the array), and `toUnmodifiableSet`.
    public static String varargsFactories() {
        List<Integer> list = List.of(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12);
        Set<Integer> set = Set.of(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11);
        Map<String, Integer> map = Map.ofEntries(Map.entry("a", 1), Map.entry("b", 2));
        Set<Key> collected = Stream.of(new Key(1), new Key(1), new Key(2)).collect(Collectors.toUnmodifiableSet());
        return list.size() + "|" + list.get(11) + "|" + set.size() + "|" + set.contains(11) + "|" + map.get("b")
            + "|" + collected.size() + "|" + collected.contains(new Key(2));
    }

    /// One element: the order is fixed.
    public static String singleIteration() {
        return Set.of("x") + "|" + Map.of("k", 1);
    }

    /// Two or more: `SALT32L`/`REVERSE` order, random per JVM run.
    public static String randomIteration() {
        return Set.of("a", "b", "c") + "|" + Map.of("a", 1, "b", 2);
    }

    /// `IdentityHashMap` keeps identity for jar objects (the reviewer's
    /// case; #98 answered `1|b|true`).
    public static String identityKeys() {
        Map<Key, String> m = new IdentityHashMap<>();
        m.put(new Key(1), "a"); m.put(new Key(1), "b");
        return m.size() + "|" + m.get(new Key(1)) + "|" + m.containsKey(new Key(1));
    }

    /// Two equal strings may or may not be one object.
    public static String identityStrings() {
        Map<String, Integer> m = new IdentityHashMap<>();
        m.put("k", 1);
        return String.valueOf(m.get(new String("k")));
    }

    /// An equal string key is present: whether it is the same object is not
    /// known here (a literal is interned; a built string is not).
    public static String identityStringPresent() {
        Map<String, Integer> m = new IdentityHashMap<>();
        m.put("k", 1);
        return String.valueOf(m.get("k"));
    }

    public static String random_m0() { return RandomImm.m0(); }
    public static String random_m1() { return RandomImm.m2(); }
    public static String random_m2() { return RandomImm.m3(); }
    public static String random_m3() { return RandomImm.m4(); }
    public static String random_m4() { return RandomImm.m5(); }
    public static String random_m5() { return RandomImm.m6(); }
    public static String random_m6() { return RandomImm.m7(); }
    public static String random_m7() { return RandomImm.m8(); }
    public static String random_m8() { return RandomImm.m9(); }
    public static String random_m9() { return RandomImm.m10(); }
    public static String random_m10() { return RandomImm.m11(); }
    public static String random_m11() { return RandomImm.m12(); }
    public static String random_m12() { return RandomImm.m13(); }
    public static String random_m13() { return RandomImm.m15(); }
    public static String random_m14() { return RandomImm.m16(); }
    public static String random_m15() { return RandomImm.m17(); }
}
"#;

const RANDOM_IMM: &str = r#"package demo;

import java.util.*;

/// Seeded random `Set.of`/`Map.of` programs (`gen_imm.py 1 150`, the first 16 with no counted call).
final class RandomImm {
    static final StringBuilder log = new StringBuilder();
    /** Logs every equals/hashCode; equal by id, hash id % 4 (collisions). */
    static final class T {
        final int id; T(int id) { this.id = id; }
        @Override public boolean equals(Object o) { log.append('e').append(id); return o instanceof T t && t.id == id; }
        @Override public int hashCode() { log.append('h').append(id); return id % 4; }
        @Override public String toString() { return "T" + id; }
    }
    record K(int id) { }
    static String run(java.util.function.Supplier<Object> op) {
        try { return String.valueOf(op.get()); }
        catch (RuntimeException e) { return "!" + e.getClass().getSimpleName() + (e.getMessage() == null ? "" : ":" + e.getMessage()); }
    }

    static Object a0;
    public static String m0() {
        log.setLength(0);
        String made = run(() -> { a0 = Set.of(new Object[] {new T(4), new T(7)}); return ((Set<Object>) a0).size(); });
        log.append(made).append('|');
        if (a0 == null) return log.toString();
        log.append(run(() -> ((Set<Object>) a0).contains(new K(1)))).append(';');
        log.append(run(() -> ((Set<Object>) a0).containsAll(List.of(new K(3), Integer.valueOf(0))))).append(';');
        log.append(run(() -> ((Set<Object>) a0).hashCode())).append(';');
        log.append(run(() -> ((Set<Object>) a0).contains(new T(0)))).append(';');
        log.append(run(() -> ((Set<Object>) a0).equals(Set.of(new K(1))))).append(';');
        log.append(run(() -> ((Set<Object>) a0).hashCode())).append(';');
        if (((Set<Object>) a0).size() <= 1) log.append(String.valueOf(((Set<Object>) a0).isEmpty() ? "[]" : (a0 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a0).iterator().next())));
        return log.toString();
    }
    static Object a2;
    public static String m2() {
        log.setLength(0);
        String made = run(() -> { a2 = Map.ofEntries(); return ((Map<Object, Integer>) a2).size(); });
        log.append(made).append('|');
        if (a2 == null) return log.toString();
        log.append(run(() -> ((Map<Object, Integer>) a2).containsKey(Integer.valueOf(3)))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a2).get(new T(8)))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a2).containsKey(new T(8)))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a2).getOrDefault(new K(2), -1))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a2).hashCode())).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a2).containsKey(null))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a2).containsKey(null))).append(';');
        if (((Map<Object, Integer>) a2).size() <= 1) log.append(String.valueOf(((Map<Object, Integer>) a2).isEmpty() ? "[]" : (a2 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a2).iterator().next())));
        return log.toString();
    }
    static Object a3;
    public static String m3() {
        log.setLength(0);
        String made = run(() -> { a3 = Map.of(new T(7), 0, new T(8), 1, new T(8), 2, new K(3), 3, new T(6), 4, new T(0), 5, "s2", 6, new K(0), 7, new T(2), 8); return ((Map<Object, Integer>) a3).size(); });
        log.append(made).append('|');
        if (a3 == null) return log.toString();
        log.append(run(() -> ((Map<Object, Integer>) a3).get(new T(8)))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a3).containsValue(0))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a3).getOrDefault(new T(4), -1))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a3).getOrDefault(new T(2), -1))).append(';');
        if (((Map<Object, Integer>) a3).size() <= 1) log.append(String.valueOf(((Map<Object, Integer>) a3).isEmpty() ? "[]" : (a3 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a3).iterator().next())));
        return log.toString();
    }
    static Object a4;
    public static String m4() {
        log.setLength(0);
        String made = run(() -> { a4 = Map.ofEntries(Map.entry(new T(8), 0)); return ((Map<Object, Integer>) a4).size(); });
        log.append(made).append('|');
        if (a4 == null) return log.toString();
        log.append(run(() -> ((Map<Object, Integer>) a4).getOrDefault(new T(7), -1))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a4).get(new K(3)))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a4).containsValue(2))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a4).getOrDefault(new T(4), -1))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a4).containsValue(4))).append(';');
        if (((Map<Object, Integer>) a4).size() <= 1) log.append(String.valueOf(((Map<Object, Integer>) a4).isEmpty() ? "[]" : (a4 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a4).iterator().next())));
        return log.toString();
    }
    static Object a5;
    public static String m5() {
        log.setLength(0);
        String made = run(() -> { a5 = Map.of(); return ((Map<Object, Integer>) a5).size(); });
        log.append(made).append('|');
        if (a5 == null) return log.toString();
        log.append(run(() -> ((Map<Object, Integer>) a5).hashCode())).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a5).containsKey(null))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a5).hashCode())).append(';');
        if (((Map<Object, Integer>) a5).size() <= 1) log.append(String.valueOf(((Map<Object, Integer>) a5).isEmpty() ? "[]" : (a5 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a5).iterator().next())));
        return log.toString();
    }
    static Object a6;
    public static String m6() {
        log.setLength(0);
        String made = run(() -> { a6 = Map.of(); return ((Map<Object, Integer>) a6).size(); });
        log.append(made).append('|');
        if (a6 == null) return log.toString();
        log.append(run(() -> ((Map<Object, Integer>) a6).getOrDefault(new K(0), -1))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a6).getOrDefault(new T(3), -1))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a6).getOrDefault(new T(1), -1))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a6).getOrDefault(new T(6), -1))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a6).containsKey(null))).append(';');
        if (((Map<Object, Integer>) a6).size() <= 1) log.append(String.valueOf(((Map<Object, Integer>) a6).isEmpty() ? "[]" : (a6 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a6).iterator().next())));
        return log.toString();
    }
    static Object a7;
    public static String m7() {
        log.setLength(0);
        String made = run(() -> { a7 = Set.of(); return ((Set<Object>) a7).size(); });
        log.append(made).append('|');
        if (a7 == null) return log.toString();
        log.append(run(() -> ((Set<Object>) a7).equals(new HashSet<>((Set<Object>) a7)))).append(';');
        log.append(run(() -> ((Set<Object>) a7).equals(Set.of(new T(5))))).append(';');
        log.append(run(() -> ((Set<Object>) a7).containsAll(List.of(new K(1), new K(0))))).append(';');
        log.append(run(() -> ((Set<Object>) a7).contains(new K(2)))).append(';');
        log.append(run(() -> ((Set<Object>) a7).contains(new T(6)))).append(';');
        log.append(run(() -> ((Set<Object>) a7).hashCode())).append(';');
        log.append(run(() -> ((Set<Object>) a7).contains(null))).append(';');
        if (((Set<Object>) a7).size() <= 1) log.append(String.valueOf(((Set<Object>) a7).isEmpty() ? "[]" : (a7 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a7).iterator().next())));
        return log.toString();
    }
    static Object a8;
    public static String m8() {
        log.setLength(0);
        String made = run(() -> { a8 = Set.of(new Object[] {new T(8), new K(1), new T(0), new T(2), new T(2)}); return ((Set<Object>) a8).size(); });
        log.append(made).append('|');
        if (a8 == null) return log.toString();
        log.append(run(() -> ((Set<Object>) a8).hashCode())).append(';');
        log.append(run(() -> ((Set<Object>) a8).contains(new T(5)))).append(';');
        log.append(run(() -> ((Set<Object>) a8).contains(null))).append(';');
        log.append(run(() -> ((Set<Object>) a8).contains(new T(5)))).append(';');
        if (((Set<Object>) a8).size() <= 1) log.append(String.valueOf(((Set<Object>) a8).isEmpty() ? "[]" : (a8 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a8).iterator().next())));
        return log.toString();
    }
    static Object a9;
    public static String m9() {
        log.setLength(0);
        String made = run(() -> { a9 = Set.of(new Object[] {new T(2), new T(1), Integer.valueOf(3), new T(8), new T(1)}); return ((Set<Object>) a9).size(); });
        log.append(made).append('|');
        if (a9 == null) return log.toString();
        log.append(run(() -> ((Set<Object>) a9).hashCode())).append(';');
        log.append(run(() -> ((Set<Object>) a9).equals(Set.of(new T(0))))).append(';');
        log.append(run(() -> ((Set<Object>) a9).contains(null))).append(';');
        log.append(run(() -> ((Set<Object>) a9).contains(new T(3)))).append(';');
        log.append(run(() -> ((Set<Object>) a9).contains(new T(7)))).append(';');
        if (((Set<Object>) a9).size() <= 1) log.append(String.valueOf(((Set<Object>) a9).isEmpty() ? "[]" : (a9 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a9).iterator().next())));
        return log.toString();
    }
    static Object a10;
    public static String m10() {
        log.setLength(0);
        String made = run(() -> { a10 = Map.of(new K(2), 0); return ((Map<Object, Integer>) a10).size(); });
        log.append(made).append('|');
        if (a10 == null) return log.toString();
        log.append(run(() -> ((Map<Object, Integer>) a10).size())).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a10).get(new T(0)))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a10).containsValue(3))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a10).get(new T(6)))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a10).getOrDefault(new T(7), -1))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a10).containsValue(5))).append(';');
        if (((Map<Object, Integer>) a10).size() <= 1) log.append(String.valueOf(((Map<Object, Integer>) a10).isEmpty() ? "[]" : (a10 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a10).iterator().next())));
        return log.toString();
    }
    static Object a11;
    public static String m11() {
        log.setLength(0);
        String made = run(() -> { a11 = Map.of("s1", 0, new T(3), 1, new T(5), 2, new T(4), 3); return ((Map<Object, Integer>) a11).size(); });
        log.append(made).append('|');
        if (a11 == null) return log.toString();
        log.append(run(() -> ((Map<Object, Integer>) a11).containsValue(1))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a11).containsKey(new T(0)))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a11).getOrDefault(new T(4), -1))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a11).hashCode())).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a11).containsKey(Integer.valueOf(0)))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a11).get(new T(3)))).append(';');
        if (((Map<Object, Integer>) a11).size() <= 1) log.append(String.valueOf(((Map<Object, Integer>) a11).isEmpty() ? "[]" : (a11 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a11).iterator().next())));
        return log.toString();
    }
    static Object a12;
    public static String m12() {
        log.setLength(0);
        String made = run(() -> { a12 = Set.of(new T(1), new T(0), new T(5)); return ((Set<Object>) a12).size(); });
        log.append(made).append('|');
        if (a12 == null) return log.toString();
        log.append(run(() -> ((Set<Object>) a12).equals(Set.of(new T(8))))).append(';');
        log.append(run(() -> ((Set<Object>) a12).contains(null))).append(';');
        log.append(run(() -> ((Set<Object>) a12).contains(null))).append(';');
        log.append(run(() -> ((Set<Object>) a12).contains(Integer.valueOf(2)))).append(';');
        if (((Set<Object>) a12).size() <= 1) log.append(String.valueOf(((Set<Object>) a12).isEmpty() ? "[]" : (a12 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a12).iterator().next())));
        return log.toString();
    }
    static Object a13;
    public static String m13() {
        log.setLength(0);
        String made = run(() -> { a13 = Set.of(new Object[] {new T(5), Integer.valueOf(1), new T(4), new K(1), new T(3), new T(1), new K(3), "s2", "s3"}); return ((Set<Object>) a13).size(); });
        log.append(made).append('|');
        if (a13 == null) return log.toString();
        log.append(run(() -> ((Set<Object>) a13).hashCode())).append(';');
        log.append(run(() -> ((Set<Object>) a13).contains(new K(0)))).append(';');
        log.append(run(() -> ((Set<Object>) a13).contains(new T(2)))).append(';');
        log.append(run(() -> ((Set<Object>) a13).contains(new T(4)))).append(';');
        log.append(run(() -> ((Set<Object>) a13).contains(Integer.valueOf(0)))).append(';');
        log.append(run(() -> ((Set<Object>) a13).containsAll(List.of(new T(8), new T(8))))).append(';');
        if (((Set<Object>) a13).size() <= 1) log.append(String.valueOf(((Set<Object>) a13).isEmpty() ? "[]" : (a13 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a13).iterator().next())));
        return log.toString();
    }
    static Object a15;
    public static String m15() {
        log.setLength(0);
        String made = run(() -> { a15 = Map.ofEntries(Map.entry(new T(8), 0), Map.entry(new T(8), 1), Map.entry(new T(4), 2), Map.entry(new T(4), 3)); return ((Map<Object, Integer>) a15).size(); });
        log.append(made).append('|');
        if (a15 == null) return log.toString();
        log.append(run(() -> ((Map<Object, Integer>) a15).containsKey(null))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a15).containsValue(3))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a15).containsKey(new T(5)))).append(';');
        if (((Map<Object, Integer>) a15).size() <= 1) log.append(String.valueOf(((Map<Object, Integer>) a15).isEmpty() ? "[]" : (a15 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a15).iterator().next())));
        return log.toString();
    }
    static Object a16;
    public static String m16() {
        log.setLength(0);
        String made = run(() -> { a16 = Set.of(new T(6), Integer.valueOf(3), new T(4), new T(3), new K(0), new T(8), new K(0)); return ((Set<Object>) a16).size(); });
        log.append(made).append('|');
        if (a16 == null) return log.toString();
        log.append(run(() -> ((Set<Object>) a16).hashCode())).append(';');
        log.append(run(() -> ((Set<Object>) a16).hashCode())).append(';');
        log.append(run(() -> ((Set<Object>) a16).equals(Set.of(Integer.valueOf(2))))).append(';');
        log.append(run(() -> ((Set<Object>) a16).contains(null))).append(';');
        log.append(run(() -> ((Set<Object>) a16).contains(null))).append(';');
        log.append(run(() -> ((Set<Object>) a16).equals(Set.of(new T(0))))).append(';');
        log.append(run(() -> ((Set<Object>) a16).hashCode())).append(';');
        if (((Set<Object>) a16).size() <= 1) log.append(String.valueOf(((Set<Object>) a16).isEmpty() ? "[]" : (a16 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a16).iterator().next())));
        return log.toString();
    }
    static Object a17;
    public static String m17() {
        log.setLength(0);
        String made = run(() -> { a17 = Map.ofEntries(Map.entry(new T(8), 0), Map.entry(new T(0), 1), Map.entry(new T(7), 2)); return ((Map<Object, Integer>) a17).size(); });
        log.append(made).append('|');
        if (a17 == null) return log.toString();
        log.append(run(() -> ((Map<Object, Integer>) a17).getOrDefault(new T(8), -1))).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a17).size())).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a17).hashCode())).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a17).hashCode())).append(';');
        log.append(run(() -> ((Map<Object, Integer>) a17).hashCode())).append(';');
        if (((Map<Object, Integer>) a17).size() <= 1) log.append(String.valueOf(((Map<Object, Integer>) a17).isEmpty() ? "[]" : (a17 instanceof Map<?, ?> mm ? mm.keySet().iterator().next() : ((Set<?>) a17).iterator().next())));
        return log.toString();
    }
}
"#;

fn fixture(test: &str) -> Fixture {
    Fixture::build(
        test,
        &[
            ("demo/Probe.java", PROBE),
            ("demo/RandomImm.java", RANDOM_IMM),
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

/// The reviewer's case on #98: `Set.of` of equal records throws
/// "duplicate element" (`Set12` calls `e0.equals(e1)`), as does `Map.of`
/// with "duplicate key". `SetN`/`MapN` lookups probe as the JDK does
/// (`hashCode`, then `equals` slot by slot), the varargs factories keep
/// their elements, `toUnmodifiableSet` dedupes, and one element iterates
/// in its fixed order.
#[test]
fn set_of_and_map_of_check_duplicates_and_probe_like_jdk_25() {
    assert_exact(
        "set-of-and-map-of-check-duplicates-and-p",
        &[
            (
                "setOfDuplicates",
                "threw:duplicate element: Key[id=1]easa|IAE:duplicate element: a|eaebebtrue|duplicate key: Key[id=2]",
            ),
            (
                "probes",
                "hahbhchcectruehzezezfalsehahbhc294|bfalsedtrue288",
            ),
            ("varargsFactories", "12|12|11|true|2|2|true"),
            ("singleIteration", "[x]|{k=1}"),
        ],
    );
}

/// `IdentityHashMap` compares jar objects by identity, and a key no equal
/// value matches is absent.
#[test]
fn identity_hash_maps_keep_identity() {
    assert_exact(
        "identity-hash-maps-keep-identity",
        &[
            ("identityKeys", "2|null|false"),
            ("identityStrings", "null"),
        ],
    );
}

/// Generated programs: every duplicate check, lookup, `hashCode` and
/// `equals`, with their `hashCode`/`equals` calls, matches JDK 25.
#[test]
fn random_set_of_and_map_of_programs_match_jdk_25() {
    assert_exact(
        "random-set-of-and-map-of-programs-match-",
        &[
            (
                "random_m0",
                "e42|e7false;e7false;h4h73;e0e7false;false;h4h73;",
            ),
            (
                "random_m1",
                "0|false;null;false;-1;0;!NullPointerException;!NullPointerException;[]",
            ),
            (
                "random_m2",
                "h7h8h8e8!IllegalArgumentException:duplicate key: T8|",
            ),
            ("random_m3", "1|e7-1;null;false;e4-1;false;T8"),
            ("random_m4", "0|0;!NullPointerException;0;[]"),
            ("random_m5", "0|-1;-1;-1;-1;!NullPointerException;[]"),
            (
                "random_m6",
                "0|true;false;false;false;false;0;!NullPointerException;[]",
            ),
            (
                "random_m7",
                "h8h0e0e0h2e2h2e2e2!IllegalArgumentException:duplicate element: T2|",
            ),
            (
                "random_m8",
                "h2h1h8h1e1!IllegalArgumentException:duplicate element: T1|",
            ),
            ("random_m9", "1|1;e0null;false;e6null;e7-1;false;K[id=2]"),
            (
                "random_m10",
                "h3h5h44|true;h0e0e0false;h4e43;h4h5h33622;false;h3e31;",
            ),
            (
                "random_m11",
                "h1h0h5e53|false;!NullPointerException;!NullPointerException;false;",
            ),
            (
                "random_m12",
                "h5h4h3e3h1e1e1e1e19|h4h5h3h17241;false;h2e2e2e2e2e2false;h4e4true;false;h8e8e8e8e8e8e8e8false;",
            ),
            (
                "random_m13",
                "h8h8e8!IllegalArgumentException:duplicate key: T8|",
            ),
            (
                "random_m14",
                "h6h4h3e3h8e8e8e8e8e8!IllegalArgumentException:duplicate element: K[id=0]|",
            ),
            ("random_m15", "h8h0e0h73|h8e80;3;h8h0h72;h8h0h72;h8h0h72;"),
        ],
    );
}

/// What no run can reproduce is counted: the iteration order of a
/// `Set.of`/`Map.of` of two or more (JDK 25 printed `[c, b, a]|{b=2, a=1}`
/// for this one), and an `IdentityHashMap` lookup by a string whose equal
/// value is a key.
#[test]
fn random_orders_and_ambiguous_identities_are_counted() {
    let fixture = fixture("random-orders-and-ambiguous-identities-a");
    let cases: &[(&str, &[(&str, usize)])] = &[
        (
            "randomIteration",
            &[(
                "java/lang/String#valueOf(Ljava/lang/Object;)Ljava/lang/String;",
                4,
            )],
        ),
        (
            "identityStringPresent",
            &[("java/util/Map#get(Ljava/lang/Object;)Ljava/lang/Object;", 1)],
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
