//! A host list's equality members with jar elements (`list_equality.rs`):
//! `ArrayList`, `LinkedList`, `List.of` and unmodifiable lists call a jar's
//! own `equals`/`hashCode` exactly when, as often and in the order JDK 25's
//! bodies do.
//!
//! Every expected value is OpenJDK 25.0.5's output for the same sources
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `T` logs each `equals` (`e<name>`) and `hashCode` (`h<name>`)
//! call, so a value pins the call sequence as well as the answer.
//! `RandomLists` is generated: random operations on random
//! `ArrayList`/`LinkedList`/`List.of`/unmodifiable lists of logging objects,
//! records, strings, boxes and `null`, from a fixed seed.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"package demo;

import java.util.*;

public final class Probe {
    static final StringBuilder log = new StringBuilder();
    /// Logs each call; equal by name, hashed by the name's hash.
    static final class T {
        final String n; T(String n) { this.n = n; }
        @Override public boolean equals(Object o) { log.append('e').append(n); return o instanceof T t && t.n.equals(n); }
        @Override public int hashCode() { log.append('h').append(n); return n.hashCode(); }
        @Override public String toString() { log.append('s').append(n); return n; }
    }
    /// Keeps `Object.equals` and `Object.hashCode`.
    static final class Plain { }
    record Key(int id) { }
    record Named(String name, List<String> tags) { }

    static String run(java.util.function.Supplier<Object> op) {
        try { return String.valueOf(op.get()); }
        catch (RuntimeException e) { return "!" + e.getClass().getSimpleName(); }
    }

    /// The reviewer's list cases on #98 (`false|false|4|0` before).
    public static String reviewerCases() {
        List<Key> removed = new ArrayList<>(List.of(new Key(1), new Key(2), new Key(1), new Key(3)));
        removed.removeAll(List.of(new Key(1)));
        List<Key> retained = new ArrayList<>(List.of(new Key(1), new Key(2)));
        retained.retainAll(Set.of(new Key(2)));
        return new ArrayList<>(List.of(new Key(1))).equals(List.of(new Key(1)))
            + "|" + new ArrayList<>(List.of(new Key(2))).containsAll(List.of(new Key(2)))
            + "|" + removed.size() + "|" + retained.size();
    }

    public static String listCallOrder() {
        log.setLength(0);
        T a = new T("a");
        List<T> al = new ArrayList<>(List.of(a, new T("b"), new T("c")));
        log.append(al.indexOf(new T("c"))).append(al.lastIndexOf(new T("a"))).append(al.contains(a)).append('|');
        log.append(al.equals(new ArrayList<>(List.of(a, new T("b"), new T("z"))))).append('|');
        log.append(al.equals(List.of(a, new T("b"), new T("c")))).append('|');
        log.append(List.of(a, new T("b")).equals(new ArrayList<>(List.of(a, new T("q"))))).append('|');
        log.append(al.hashCode()).append('|');
        log.append(al.remove(new T("b"))).append(al.size()).append('|');
        log.append(new LinkedList<>(List.of(a)).equals(List.of(a))).append('|');
        log.append(List.of(a).contains(a)).append(List.of(a, a, a).contains(a));
        return log.toString();
    }

    public static String bulkCallOrder() {
        log.setLength(0);
        List<T> al = new ArrayList<>(List.of(new T("a"), new T("b"), new T("c")));
        log.append(al.containsAll(List.of(new T("c"), new T("a")))).append('|');
        log.append(al.removeAll(List.of(new T("b"), new T("q")))).append(al.size()).append('|');
        List<T> bl = new ArrayList<>(List.of(new T("a"), new T("b"), new T("c")));
        log.append(bl.retainAll(new ArrayList<>(List.of(new T("c"))))).append(bl.size());
        return log.toString();
    }

    /// `List.of` refuses `null` lookups; `removeAll` keeps its partial result
    /// when the argument's `contains` throws.
    public static String nullsAndPartialRemoval() {
        log.setLength(0);
        List<Object> list = new ArrayList<>();
        list.add(new T("a"));
        list.add(new T("a"));
        list.add(null);
        List<Object> of = List.of(new T("a"), "x");
        log.append(run(() -> of.contains(null))).append(';');
        log.append(run(() -> list.retainAll(of))).append(list.size()).append(';');
        log.append(run(() -> list.removeAll(of))).append(list.size()).append(';');
        return log.toString();
    }

    /// A record's list component compares with `List.equals`.
    public static String recordWithListComponent() {
        return String.valueOf(new Named("a", List.of("z")).equals(new Named("a", new ArrayList<>(List.of("z")))));
    }

    public static String listHashes() {
        return List.of("x", "y").hashCode() + "|" + new ArrayList<>(List.of("x", "y")).hashCode()
            + "|" + List.of(new Key(2), new Key(3)).hashCode();
    }

    /// A list of a class keeping `Object.hashCode` hashes by identity.
    public static String identityListHash() { return String.valueOf(List.of(new Plain()).hashCode()); }

    public static String random_m0() { return RandomLists.m0(); }
    public static String random_m1() { return RandomLists.m1(); }
    public static String random_m2() { return RandomLists.m2(); }
    public static String random_m3() { return RandomLists.m3(); }
    public static String random_m4() { return RandomLists.m4(); }
    public static String random_m5() { return RandomLists.m5(); }
    public static String random_m6() { return RandomLists.m6(); }
    public static String random_m7() { return RandomLists.m7(); }
    public static String random_m8() { return RandomLists.m8(); }
    public static String random_m9() { return RandomLists.m9(); }
    public static String random_m10() { return RandomLists.m10(); }
    public static String random_m11() { return RandomLists.m11(); }
    public static String random_m12() { return RandomLists.m12(); }
    public static String random_m13() { return RandomLists.m13(); }
}
"#;

const RANDOM_LISTS: &str = r#"package demo;

import java.util.*;

/// Seeded random list programs (`gen_lists.py`, seed 4, the first 14).
final class RandomLists {
    static final StringBuilder log = new StringBuilder();
    /** Logs every equals/hashCode; equal by id, hash id % 3 (collisions). */
    static final class T {
        final int id; T(int id) { this.id = id; }
        @Override public boolean equals(Object o) { log.append('e').append(id); return o instanceof T t && t.id == id; }
        @Override public int hashCode() { log.append('h').append(id); return id % 3; }
        @Override public String toString() { return "T" + id; }
    }
    record K(int id) { }
    record RT(T t, String s) { }
    static final T SHARED = new T(1);
    static String run(java.util.function.Supplier<Object> op) {
        try { return String.valueOf(op.get()); }
        catch (RuntimeException e) { return "!" + e.getClass().getSimpleName(); }
    }

    public static String m0() {
        log.setLength(0);
        List<Object> a0 = new LinkedList<>();
        a0.add(SHARED);
        a0.add(new T(3));
        List<Object> a = a0;
        List<Object> b0 = new ArrayList<>();
        b0.add(new T(1));
        b0.add(new T(2));
        b0.add(new RT(new T(1), "x"));
        b0.add("s1");
        List<Object> b = Collections.unmodifiableList(b0);
        log.append('[');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(run(() -> Objects.hash(new RT(new T(0), "x"), new K(1)))).append(';');
        log.append(run(() -> a.indexOf(new T(1)))).append(';');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(run(() -> Objects.hash(new K(1), new RT(new T(1), "x")))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
    public static String m1() {
        log.setLength(0);
        List<Object> a0 = new ArrayList<>();
        a0.add(new K(1));
        a0.add("s1");
        a0.add(new T(1));
        a0.add(new K(1));
        List<Object> a = a0;
        List<Object> b0 = new ArrayList<>();
        b0.add(new K(2));
        b0.add(Integer.valueOf(0));
        b0.add(new T(1));
        b0.add(new K(1));
        List<Object> b = b0;
        log.append('[');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(run(() -> a.indexOf(SHARED))).append(';');
        log.append(run(() -> a.remove((Object) new RT(new T(0), "x")))).append(';');
        log.append(run(() -> Objects.hash(new RT(new T(1), "x"), new T(1)))).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(run(() -> Objects.equals(new K(1), SHARED))).append(';');
        log.append(run(() -> a.contains(null))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
    public static String m2() {
        log.setLength(0);
        List<Object> a0 = new LinkedList<>();
        a0.add(new T(1));
        a0.add(null);
        List<Object> a = a0;
        List<Object> b0 = new ArrayList<>();
        List<Object> b = Collections.unmodifiableList(b0);
        log.append('[');
        log.append(run(() -> a.lastIndexOf(null))).append(';');
        log.append(run(() -> Objects.hash(Integer.valueOf(1), null))).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(run(() -> a.retainAll(b))).append(';');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
    public static String m3() {
        log.setLength(0);
        List<Object> a0 = new ArrayList<>();
        a0.add("s1");
        a0.add(new K(1));
        a0.add(new RT(new T(0), "x"));
        a0.add(new T(1));
        List<Object> a = Collections.unmodifiableList(a0);
        List<Object> b0 = new ArrayList<>();
        b0.add(SHARED);
        b0.add(null);
        List<Object> b = b0;
        log.append('[');
        log.append(run(() -> a.lastIndexOf(new RT(new T(0), "x")))).append(';');
        log.append(run(() -> a.retainAll(b))).append(';');
        log.append(run(() -> a.retainAll(b))).append(';');
        log.append(run(() -> a.lastIndexOf(SHARED))).append(';');
        log.append(run(() -> Objects.equals(new K(2), "s1"))).append(';');
        log.append(run(() -> a.lastIndexOf(new T(2)))).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
    public static String m4() {
        log.setLength(0);
        List<Object> a0 = new ArrayList<>();
        a0.add(new RT(new T(0), "x"));
        List<Object> a = a0;
        List<Object> b0 = new ArrayList<>();
        b0.add(new K(2));
        List<Object> b = b0;
        log.append('[');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(run(() -> a.contains(new T(3)))).append(';');
        log.append(run(() -> Objects.equals(new T(1), "s0"))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
    public static String m5() {
        log.setLength(0);
        List<Object> a = List.<Object>of(new K(1));
        List<Object> b = List.<Object>of(new RT(new T(1), "x"), new T(2), new K(1), "s0");
        log.append('[');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(run(() -> Objects.equals(Integer.valueOf(1), Integer.valueOf(1)))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
    public static String m6() {
        log.setLength(0);
        List<Object> a = List.<Object>of(new K(0), "s0", new T(0));
        List<Object> b = List.<Object>of(new K(1), SHARED, new K(1), new T(1));
        log.append('[');
        log.append(run(() -> a.contains(new T(1)))).append(';');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(run(() -> Objects.hash(SHARED, new RT(new T(1), "x")))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
    public static String m7() {
        log.setLength(0);
        List<Object> a0 = new ArrayList<>();
        a0.add(null);
        a0.add(new K(0));
        List<Object> a = a0;
        List<Object> b0 = new ArrayList<>();
        List<Object> b = b0;
        log.append('[');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(run(() -> a.remove((Object) new T(1)))).append(';');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
    public static String m8() {
        log.setLength(0);
        List<Object> a0 = new ArrayList<>();
        a0.add(new K(1));
        a0.add(null);
        List<Object> a = a0;
        List<Object> b0 = new LinkedList<>();
        b0.add("s0");
        b0.add(SHARED);
        b0.add(new RT(new T(1), "x"));
        b0.add(new K(1));
        List<Object> b = b0;
        log.append('[');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(run(() -> a.lastIndexOf(new T(1)))).append(';');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(run(() -> a.retainAll(b))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
    public static String m9() {
        log.setLength(0);
        List<Object> a0 = new ArrayList<>();
        a0.add(new RT(new T(0), "x"));
        a0.add(new K(1));
        a0.add(new T(1));
        List<Object> a = a0;
        List<Object> b = List.<Object>of(new RT(new T(0), "x"), new RT(new T(0), "x"), new RT(new T(1), "x"));
        log.append('[');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(run(() -> Objects.hash(new K(1), "s0"))).append(';');
        log.append(run(() -> a.lastIndexOf(SHARED))).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(run(() -> a.equals(b))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
    public static String m10() {
        log.setLength(0);
        List<Object> a = List.<Object>of(new K(0), new K(2), new K(2));
        List<Object> b = List.<Object>of(new T(1));
        log.append('[');
        log.append(run(() -> a.indexOf(null))).append(';');
        log.append(run(() -> a.hashCode())).append(';');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(run(() -> a.lastIndexOf(SHARED))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
    public static String m11() {
        log.setLength(0);
        List<Object> a0 = new ArrayList<>();
        a0.add(SHARED);
        a0.add(new K(1));
        a0.add(null);
        a0.add(new T(1));
        List<Object> a = a0;
        List<Object> b0 = new ArrayList<>();
        b0.add(Integer.valueOf(1));
        List<Object> b = Collections.unmodifiableList(b0);
        log.append('[');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(run(() -> a.indexOf("s0"))).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
    public static String m12() {
        log.setLength(0);
        List<Object> a0 = new ArrayList<>();
        a0.add(new T(1));
        a0.add(new K(0));
        a0.add(null);
        a0.add(new T(1));
        a0.add(new K(2));
        List<Object> a = a0;
        List<Object> b0 = new ArrayList<>();
        b0.add(new T(0));
        b0.add(new K(1));
        b0.add(new K(2));
        b0.add(Integer.valueOf(0));
        b0.add(new K(1));
        List<Object> b = b0;
        log.append('[');
        log.append(run(() -> Objects.equals(Integer.valueOf(2), new T(1)))).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(run(() -> Objects.hash(new T(0), new T(1)))).append(';');
        log.append(run(() -> Objects.hash(new K(1), Integer.valueOf(2)))).append(';');
        log.append(run(() -> a.remove((Object) new K(1)))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
    public static String m13() {
        log.setLength(0);
        List<Object> a0 = new ArrayList<>();
        a0.add(new K(1));
        a0.add("s0");
        List<Object> a = Collections.unmodifiableList(a0);
        List<Object> b0 = new ArrayList<>();
        List<Object> b = Collections.unmodifiableList(b0);
        log.append('[');
        log.append(run(() -> a.containsAll(b))).append(';');
        log.append(run(() -> a.removeAll(b))).append(';');
        log.append(run(() -> a.indexOf("s0"))).append(';');
        log.append(run(() -> a.contains(new K(0)))).append(';');
        log.append(run(() -> Objects.hash(new T(3), null))).append(';');
        log.append(a.size()).append(']');
        return log.toString();
    }
}
"#;

fn fixture(test: &str) -> Fixture {
    Fixture::build(
        test,
        &[
            ("demo/Probe.java", PROBE),
            ("demo/RandomLists.java", RANDOM_LISTS),
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

/// The reviewer's list programs on #98: `List.equals`, `containsAll`,
/// `removeAll` and `retainAll` with records.
#[test]
fn lists_of_records_compare_by_equals() {
    assert_exact(
        "list-equality-reviewer",
        &[
            ("reviewerCases", "true|true|2|1"),
            ("recordWithListComponent", "true"),
        ],
    );
}

/// `indexOf`/`lastIndexOf`/`contains`/`remove`/`equals`/`hashCode`/
/// `containsAll`/`removeAll`/`retainAll` call a jar's `equals` and
/// `hashCode` as each JDK class does; `List.of` refuses `null`, and a
/// `removeAll` whose argument throws keeps its partial result.
#[test]
fn list_members_call_jar_equality_as_the_jdk_does() {
    assert_exact(
        "list-equality-members",
        &[
            (
                "listCallOrder",
                "ececec2eaeaea0eatrue|ebecfalse|ebectrue|ebfalse|hahbhc126145|ebebtrue2|eatrue|eatruetrue",
            ),
            ("bulkCallOrder", "ecececeatrue|eaeaebecectrue2|eaebectrue1"),
            (
                "nullsAndPartialRemoval",
                "!NullPointerException;eaea!NullPointerException3;eaea!NullPointerException1;",
            ),
            ("listHashes", "4802|4802|1026"),
        ],
    );
}

/// Generated programs: every answer and every logged call matches JDK 25.
#[test]
fn random_list_programs_match_jdk_25() {
    assert_exact(
        "list-equality-random",
        &[
            (
                "random_m0",
                "[e1e2e2false;h04682;e10;e1e2e2false;e1e3false;e1e3false;h11143;2]",
            ),
            (
                "random_m1",
                "[e1e1e1true;e1-1;false;h1h15643;false;false;false;1]",
            ),
            ("random_m2", "[1;992;false;true;true;0]"),
            (
                "random_m3",
                "[e02;!UnsupportedOperationException;!UnsupportedOperationException;e13;false;e2e2e2e2-1;!UnsupportedOperationException;4]",
            ),
            ("random_m4", "[false;e3false;e1false;1]"),
            ("random_m5", "[false;!UnsupportedOperationException;true;1]"),
            (
                "random_m6",
                "[e1e1e1false;false;!UnsupportedOperationException;h1h11143;3]",
            ),
            ("random_m7", "[false;e1e1false;true;false;2]"),
            ("random_m8", "[true;e1-1;false;31;false;true;0]"),
            (
                "random_m9",
                "[h0h1145143;e0false;4605;e12;e0e1e1e1true;false;2]",
            ),
            (
                "random_m10",
                "[!NullPointerException;29855;e1e1e1false;e1e1e1-1;3]",
            ),
            ("random_m11", "[false;-1;e1e1false;e1e1false;4]"),
            (
                "random_m12",
                "[false;e1e1e1e1e1e1e1e1e1e1true;h0h1962;994;false;4]",
            ),
            (
                "random_m13",
                "[true;!UnsupportedOperationException;1;false;h3961;2]",
            ),
        ],
    );
}

/// A list holding an object that keeps `Object.hashCode` has an identity
/// hash: counted, not answered.
#[test]
fn a_list_hash_over_an_identity_hash_is_counted() {
    let fixture = fixture("list-equality-identity");
    let mut host = fixture.jar.seed_host(StdHost::new());
    fixture.text_on(&mut host, "identityListHash");
    assert_eq!(
        host.unknown_host_calls(),
        [("java/util/List#hashCode()I".to_owned(), 1)]
    );
}
