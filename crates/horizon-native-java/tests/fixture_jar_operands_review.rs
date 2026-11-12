//! An independent review's cases for `jar_operands.rs`: inherited JDK
//! defaults reached through a call site typed by the jar class itself, jar
//! `CharSequence` elements of `String.join`, and jar subclasses of concrete
//! JDK collections whose JDK bodies call back into the jar's overrides.
//!
//! The JVM's answers are OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), quoted in
//! each test. Where the runtime cannot run the JDK body, the answer is
//! counted: the counted text is the runtime's honesty contract, **not JVM
//! behaviour**. For a concrete subclass the host's own answer from its row
//! stands beside the count. Before this change, every case below answered
//! silently (uncounted).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"package demo;

import java.util.AbstractList;
import java.util.AbstractMap;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

public final class Probe {
    /// The squares 0, 1, 4, 9, kept only in `get`.
    static final class Squares extends AbstractList<Integer> {
        @Override public Integer get(int index) { return index * index; }
        @Override public int size() { return 4; }
    }

    /// One entry, kept only in `entrySet`.
    static final class Pairs extends AbstractMap<String, Integer> {
        @Override public Set<Map.Entry<String, Integer>> entrySet() { return Set.of(Map.entry("x", 1)); }
    }

    /// A jar `CharSequence` whose text is its own `toString`.
    static final class Cs implements CharSequence {
        @Override public int length() { return 3; }
        @Override public char charAt(int index) { return "abc".charAt(index); }
        @Override public CharSequence subSequence(int start, int end) { return "abc".subSequence(start, end); }
        @Override public String toString() { return "abc"; }
    }

    /// An LRU map: `LinkedHashMap.put` calls `removeEldestEntry` back.
    static final class Lru extends LinkedHashMap<String, Integer> {
        Lru() { super(16, 0.75f, true); }
        @Override protected boolean removeEldestEntry(Map.Entry<String, Integer> eldest) { return size() > 2; }
    }

    /// `toString` and `String.join` iterate through this `iterator()`.
    static final class OneIter extends ArrayList<String> {
        @Override public Iterator<String> iterator() { return List.of("x").iterator(); }
    }

    /// `containsAll` asks this `contains`.
    static final class Always extends ArrayList<String> {
        @Override public boolean contains(Object o) { return true; }
    }

    /// `HashMap.isEmpty` reads its own count, not `size()`.
    static final class SizeLie extends HashMap<String, Integer> {
        @Override public int size() { return 42; }
    }

    /// `HashMap.putAll` stores directly; it never calls `put`.
    static final class Logged extends HashMap<String, Integer> {
        int puts;
        @Override public Integer put(String key, Integer value) { puts++; return super.put(key, value); }
    }

    public static String typedByJarClass() {
        StringBuilder out = new StringBuilder();
        Squares squares = new Squares();
        squares.forEach(out::append);
        Pairs pairs = new Pairs();
        String defaults = pairs.getOrDefault("x", 5) + "|" + pairs.getOrDefault("y", 5);
        StringBuilder each = new StringBuilder();
        pairs.forEach((k, v) -> each.append(k).append(v));
        return out + "|" + defaults + "|" + each;
    }

    public static String firstOfJarList() {
        Squares squares = new Squares();
        try {
            return String.valueOf(squares.getFirst());
        } catch (RuntimeException error) {
            return error.getClass().getName();
        }
    }

    public static String joinedCharSequences() {
        Cs cs = new Cs();
        return String.join("-", List.of(cs, cs)) + "|" + String.join("-", cs, "d");
    }

    public static String lru() {
        Lru lru = new Lru();
        lru.put("a", 1);
        lru.put("b", 2);
        lru.put("c", 3);
        return lru.size() + "|" + lru.containsKey("a");
    }

    public static String overriddenIterator() {
        OneIter list = new OneIter();
        list.add("a");
        list.add("b");
        return list + "|" + String.join(",", list);
    }

    public static String overriddenContains() {
        return String.valueOf(new Always().containsAll(List.of("zz")));
    }

    public static String overriddenSize() {
        SizeLie map = new SizeLie();
        return map.isEmpty() + "|" + map;
    }

    public static String putAllDoesNotCallPut() {
        Logged logged = new Logged();
        logged.putAll(Map.of("a", 1));
        logged.put("b", 2);
        return logged.size() + "|" + logged.puts + "|" + logged.get("a");
    }
}
"#;

/// The text `method` returns and the unknown host calls it counted.
fn run(method: &str) -> (String, Vec<(String, usize)>) {
    let fixture = Fixture::build(
        &format!("jar-operands-review-{method}"),
        &[("demo/Probe.java", PROBE)],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = fixture.text_on(&mut host, method);
    (text, host.unknown_host_calls())
}

fn calls(rows: &[(&str, usize)]) -> Vec<(String, usize)> {
    rows.iter()
        .map(|(key, n)| ((*key).to_owned(), *n))
        .collect()
}

/// JDK 25: `0149|1|5|x1`. The receiver's class decides which default runs,
/// not the call site's type (`Squares s; s.forEach(...)`).
#[test]
fn defaults_reached_through_the_jar_type_are_counted() {
    let (text, unknown) = run("typedByJarClass");
    assert_eq!(text, "|null|null|");
    assert_eq!(
        unknown,
        calls(&[
            (
                "java/lang/Iterable#forEach(Ljava/util/function/Consumer;)V",
                1
            ),
            ("java/util/Map#forEach(Ljava/util/function/BiConsumer;)V", 1),
            (
                "java/util/Map#getOrDefault(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                2
            ),
        ])
    );
}

/// JDK 25: `0`. Counted under the most specific declaring interface
/// (`List.getFirst` over `SequencedCollection.getFirst`); it used to throw
/// the host's `NoSuchElementException` from an empty row.
#[test]
fn get_first_through_the_jar_type_is_counted() {
    let (text, unknown) = run("firstOfJarList");
    assert_eq!(text, "null");
    assert_eq!(
        unknown,
        calls(&[("java/util/List#getFirst()Ljava/lang/Object;", 1)])
    );
}

/// JDK 25: `abc-abc|abc-d`, exactly: each element is `String.valueOf`'d
/// through its own `toString` (it printed the objects' internal markers).
#[test]
fn string_join_prints_jar_char_sequences_through_their_to_string() {
    let (text, unknown) = run("joinedCharSequences");
    assert_eq!(text, "abc-abc|abc-d");
    assert_eq!(unknown, Vec::new());
}

/// JDK 25: `2|false`: `removeEldestEntry` evicts. The host keeps all three
/// and every `put` is counted.
#[test]
fn an_lru_map_counts_its_puts() {
    let (text, unknown) = run("lru");
    assert_eq!(text, "3|true");
    assert_eq!(
        unknown,
        calls(&[(
            "java/util/LinkedHashMap#put(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            3
        )])
    );
}

/// JDK 25: `[x]|x`: `toString` and `String.join` iterate the override.
#[test]
fn an_overridden_iterator_counts_what_iterates_it() {
    let (text, unknown) = run("overriddenIterator");
    assert_eq!(text, "[a, b]|a,b");
    assert_eq!(
        unknown,
        calls(&[
            (
                "java/lang/String#join(Ljava/lang/CharSequence;Ljava/lang/Iterable;)Ljava/lang/String;",
                1
            ),
            ("java/util/ArrayList#toString()Ljava/lang/String;", 1),
        ])
    );
}

/// JDK 25: `true`: `containsAll` asks the overridden `contains`.
#[test]
fn an_overridden_contains_counts_contains_all() {
    let (text, unknown) = run("overriddenContains");
    assert_eq!(text, "false");
    assert_eq!(
        unknown,
        calls(&[(
            "java/util/ArrayList#containsAll(Ljava/util/Collection;)Z",
            1
        )])
    );
}

/// JDK 25: `true|{}`, exactly: `HashMap.isEmpty` reads its own count, not
/// the overridden `size()`, and a map subclass prints as a map (it used to
/// answer `false|[]`).
#[test]
fn a_concrete_map_ignores_an_overridden_size_and_prints_as_a_map() {
    let (text, unknown) = run("overriddenSize");
    assert_eq!(text, "true|{}");
    assert_eq!(unknown, Vec::new());
}

/// JDK 25: `2|1|1`, exactly: `HashMap.putAll` never calls `put`, so an
/// overridden `put` is no reason to count it.
#[test]
fn put_all_on_a_hash_map_subclass_stays_exact() {
    let (text, unknown) = run("putAllDoesNotCallPut");
    assert_eq!(text, "2|1|1");
    assert_eq!(unknown, Vec::new());
}
