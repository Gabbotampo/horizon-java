//! A host model handed a jar object it holds no state for counts the call
//! instead of answering it (`jar_operands.rs`).
//!
//! Each `Probe` object below keeps its data in its own methods or in a JDK
//! skeleton superclass, so the JDK's answer comes from bodies the host cannot
//! run. Before this, the runtime answered every one of these neutrally and
//! uncounted (`contains` → `false`, `toArray` → length 0, a copy → empty,
//! `"" + set` → `[]`, `getOrDefault` → the default although the map holds the
//! key), or refused the whole entrypoint as an unknown marker.
//!
//! The JVM's answers are OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), quoted in each
//! test. The runtime's answers are the counted neutral defaults. They are the
//! runtime's honesty contract, **not JVM behaviour**.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"package demo;

import java.util.AbstractMap;
import java.util.AbstractSet;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.BiPredicate;
import java.util.function.ToIntFunction;

public final class Probe {
    /// A set whose elements live only in its own iterator.
    static final class Trio extends AbstractSet<String> {
        @Override public Iterator<String> iterator() { return List.of("a", "b", "c").iterator(); }
        @Override public int size() { return 3; }
    }

    /// An iterable with no JDK superclass.
    static final class Gen implements Iterable<String> {
        @Override public Iterator<String> iterator() { return List.of("x", "y").iterator(); }
    }

    /// An entry whose key and value the JDK superclass keeps.
    static final class Pair extends AbstractMap.SimpleImmutableEntry<String, Integer> {
        Pair() { super("k", 7); }
    }

    /// A map whose one entry lives in its own `get`.
    static final class OnlyX extends AbstractMap<String, Integer> {
        @Override public Set<Map.Entry<String, Integer>> entrySet() { return Set.of(); }
        @Override public Integer get(Object key) { return "x".equals(key) ? 1 : null; }
        @Override public boolean containsKey(Object key) { return "x".equals(key); }
    }

    /// A concrete JDK collection subclass: its state is the `ArrayList`'s.
    static final class Names extends ArrayList<String> { }

    public static String inheritedTemplates() {
        Set<String> trio = new Trio();
        return trio.contains("b") + "|" + trio.isEmpty() + "|" + trio.size();
    }

    public static String toArrayLength() {
        Set<String> trio = new Trio();
        Object[] copy = trio.toArray(new String[0]);
        return copy == null ? "null" : String.valueOf(copy.length);
    }

    public static String copiedByAModel() {
        List<String> copy = new ArrayList<>(new Trio());
        return String.valueOf(copy.size());
    }

    public static String printed() {
        return "" + new Trio();
    }

    public static String iterableDefault() {
        StringBuilder out = new StringBuilder();
        Iterable<String> gen = new Gen();
        gen.forEach(out::append);
        return out.toString();
    }

    public static String entryState() {
        Map.Entry<String, Integer> pair = new Pair();
        return pair.getKey() + "=" + pair.getValue();
    }

    public static String mapDefault() {
        Map<String, Integer> map = new OnlyX();
        return map.getOrDefault("x", 5) + "|" + map.getOrDefault("y", 5);
    }

    public static String concreteSubclass() {
        Names names = new Names();
        names.add("p");
        names.add("q");
        List<String> copy = new ArrayList<>(names);
        return copy.size() + "|" + names.contains("q") + "|" + names;
    }

    public static String methodReferences() {
        ToIntFunction<Collection<String>> size = Collection::size;
        BiPredicate<Collection<String>, Object> contains = Collection::contains;
        return size.applyAsInt(new Trio()) + "|" + contains.test(new Trio(), "b");
    }
}
"#;

/// The text `method` returns and the unknown host calls it counted.
fn run(method: &str) -> (String, Vec<(String, usize)>) {
    let fixture = Fixture::build(
        &format!("jar-operands-{method}"),
        &[("demo/Probe.java", PROBE)],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = fixture.text_on(&mut host, method);
    (text, host.unknown_host_calls())
}

fn counted(key: &str, times: usize) -> Vec<(String, usize)> {
    vec![(key.to_owned(), times)]
}

/// JDK 25: `true|false|3`. `contains` is `AbstractCollection`'s template over
/// the set's own iterator; `isEmpty` (`size() == 0`) and `size` run exactly.
#[test]
fn an_inherited_template_is_counted() {
    let (text, unknown) = run("inheritedTemplates");
    assert_eq!(text, "false|false|3");
    assert_eq!(
        unknown,
        counted(
            "java/util/AbstractCollection#contains(Ljava/lang/Object;)Z",
            1
        )
    );
}

/// JDK 25: `3`. The fastutil `EntrySet` shape from the DFU investigation.
#[test]
fn to_array_on_a_skeleton_subclass_is_counted() {
    let (text, unknown) = run("toArrayLength");
    assert_eq!(text, "0");
    assert_eq!(
        unknown,
        counted(
            "java/util/AbstractCollection#toArray([Ljava/lang/Object;)[Ljava/lang/Object;",
            1
        )
    );
}

/// JDK 25: `3`. The copy constructor reads its argument's elements.
#[test]
fn a_jar_collection_argument_is_counted() {
    let (text, unknown) = run("copiedByAModel");
    assert_eq!(text, "0");
    assert_eq!(
        unknown,
        counted("java/util/ArrayList#<init>(Ljava/util/Collection;)V", 1)
    );
}

/// JDK 25: `[a, b, c]`.
#[test]
fn a_skeleton_to_string_is_counted() {
    let (text, unknown) = run("printed");
    assert_eq!(text, "null");
    assert_eq!(
        unknown,
        counted(
            "java/util/AbstractCollection#toString()Ljava/lang/String;",
            1
        )
    );
}

/// JDK 25: `xy`. Refused before (no host row for the iterable).
#[test]
fn an_interface_default_on_a_jar_iterable_is_counted() {
    let (text, unknown) = run("iterableDefault");
    assert_eq!(text, "");
    assert_eq!(
        unknown,
        counted(
            "java/lang/Iterable#forEach(Ljava/util/function/Consumer;)V",
            1
        )
    );
}

/// JDK 25: `k=7`. The key and value are `SimpleImmutableEntry`'s state.
#[test]
fn jdk_superclass_state_is_counted() {
    let (text, unknown) = run("entryState");
    assert_eq!(text, "null=null");
    assert_eq!(
        unknown,
        vec![
            (
                "java/util/AbstractMap$SimpleImmutableEntry#getKey()Ljava/lang/Object;".to_owned(),
                1
            ),
            (
                "java/util/AbstractMap$SimpleImmutableEntry#getValue()Ljava/lang/Object;"
                    .to_owned(),
                1
            ),
        ]
    );
}

/// JDK 25: `1|5`. `Map.getOrDefault` calls the map's own `get`.
#[test]
fn a_map_default_on_a_jar_map_is_counted() {
    let (text, unknown) = run("mapDefault");
    assert_eq!(text, "null|null");
    assert_eq!(
        unknown,
        counted(
            "java/util/Map#getOrDefault(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            2
        )
    );
}

/// JDK 25: `2|true|[p, q]`. A subclass of a concrete JDK collection keeps
/// its elements in the host's row, so the models answer it exactly.
#[test]
fn a_concrete_jdk_collection_subclass_is_still_answered() {
    let (text, unknown) = run("concreteSubclass");
    assert_eq!(text, "2|true|[p, q]");
    assert_eq!(unknown, Vec::new());
}

/// JDK 25: `3|true`. A method reference to a JDK interface method runs the
/// jar object's own `size` (it used to be the host's, from an empty row) and
/// counts the inherited `contains` template, exactly as the direct calls do.
#[test]
fn method_references_dispatch_and_count_like_the_direct_call() {
    let (text, unknown) = run("methodReferences");
    assert_eq!(text, "3|false");
    assert_eq!(
        unknown,
        counted(
            "java/util/AbstractCollection#contains(Ljava/lang/Object;)Z",
            1
        )
    );
}
