//! The independent review's cases for #118, as JDK 25 differential tests:
//! `toArray((Object[]) null)` calls `size()` before its catchable NPE, a
//! `List.of` argument's `contains(null)` throws the JDK's NPE out of
//! `retainAll`, and a host map entry (`Map.entry`, a `HashMap` entry) equals
//! a jar entry by reading its `getKey`/`getValue`.
//!
//! Each expected line is OpenJDK 25.0.5's output for the same probe method
//! (`support/jdk_differential.rs`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
#[path = "support/jdk_differential.rs"]
mod jdk_differential;

fn check(cases: &[(&str, &str)]) {
    jdk_differential::check("jdk-skeleton-review-118", PROBE, cases);
}

const PROBE: &str = r#"package demo;

import java.util.AbstractCollection;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;

/// The independent review's #118 cases, rewritten as probe methods.
public final class Probe {
    static final StringBuilder LOG = new StringBuilder();

    static void log(String event) { LOG.append(event).append(' '); }

    static String take(Object result) {
        String text = LOG.toString().trim() + " => " + result;
        LOG.setLength(0);
        return text;
    }

    /// A collection over a fixed array, logging `size`/`iterator`/`next`/`remove`.
    static final class Coll extends AbstractCollection<Object> {
        Object[] items;
        int count;
        Coll(Object... items) { this.items = items.clone(); this.count = items.length; }
        @Override public int size() { log("sz"); return count; }
        @Override public Iterator<Object> iterator() {
            log("it");
            return new Iterator<>() {
                int next;
                @Override public boolean hasNext() { log("hn" + next); return next < count; }
                @Override public Object next() { log("n" + next); return items[next++]; }
                @Override public void remove() {
                    log("rm" + (next - 1));
                    for (int i = next - 1; i < count - 1; i++) items[i] = items[i + 1];
                    count--;
                    next--;
                }
            };
        }
    }

    /// A jar `Map.Entry` whose accessors log.
    static final class Pair implements Map.Entry<Object, Object> {
        final Object key;
        final Object value;
        Pair(Object key, Object value) { this.key = key; this.value = value; }
        @Override public Object getKey() { log("gk" + key); return key; }
        @Override public Object getValue() { log("gv" + value); return value; }
        @Override public Object setValue(Object value) { throw new UnsupportedOperationException(); }
    }

    public static String toArrayNull() {
        try {
            new Coll("x").toArray((Object[]) null);
            return take("no");
        } catch (NullPointerException e) {
            return take("npe-caught");
        }
    }

    public static String retainAllListOfWithNull() {
        Coll c = new Coll("a", "x", null);
        try {
            return take(c.retainAll(List.of("x")) + ":" + c.count);
        } catch (NullPointerException e) {
            return take("NPE:" + e.getMessage() + ":" + c.count);
        }
    }

    public static String containsAllListOfWithNull() {
        try {
            return take(List.of("a").containsAll(new Coll("a", null)));
        } catch (NullPointerException e) {
            return take("NPE:" + e.getMessage());
        }
    }

    public static String containsHostEntry() {
        Coll entries = new Coll(new Pair(1, "a"));
        return take(entries.contains(Map.entry(1, "a")) + "," + entries.contains(Map.entry(1, "b")));
    }

    public static String containsHashMapEntry() {
        HashMap<Object, Object> map = new HashMap<>();
        map.put(2, "b");
        Coll entries = new Coll(new Pair(2, "b"));
        return take(entries.contains(map.entrySet().iterator().next()));
    }

    /// Only `List.of`/`List.copyOf` lists reject a `null` lookup; the other
    /// unmodifiable lists the host keeps answer it like `AbstractList`.
    static String nullLookup(java.util.function.Supplier<Object> lookup) {
        try {
            return take(lookup.get());
        } catch (NullPointerException e) {
            return take("NPE:" + e.getMessage());
        }
    }

    public static String emptyListContainsNull() {
        return nullLookup(() -> java.util.Collections.emptyList().contains(null));
    }

    public static String emptyListIndexOfNull() {
        return nullLookup(() -> java.util.Collections.emptyList().indexOf(null));
    }

    public static String singletonListContainsNull() {
        return nullLookup(() -> java.util.Collections.singletonList("x").contains(null));
    }

    public static String retainAllSingletonListWithNull() {
        Coll c = new Coll("a", "x", null);
        return nullLookup(() -> c.retainAll(java.util.Collections.singletonList("x")) + ":" + c.count);
    }

    public static String listOfIndexOfNull() {
        return nullLookup(() -> List.of("x", "y", "z").indexOf(null));
    }

    public static String listCopyOfContainsNull() {
        return nullLookup(() -> List.copyOf(List.of("x")).contains(null));
    }

    public static String[] all() {
        return new String[] {"toArrayNull", "retainAllListOfWithNull", "containsHostEntry",
                "containsHashMapEntry", "emptyListContainsNull", "emptyListIndexOfNull",
                "singletonListContainsNull", "retainAllSingletonListWithNull", "listOfIndexOfNull",
                "listCopyOfContainsNull"};
    }

    public static void main(String[] args) throws Exception {
        for (String name : all()) System.out.println(name + "\t" + Probe.class.getMethod(name).invoke(null));
    }
}
"#;

#[test]
fn review_118_runs_as_jdk_25_does() {
    check(&[
        ("toArrayNull", "sz => npe-caught"),
        (
            "retainAllListOfWithNull",
            "it hn0 n0 rm0 hn0 n0 hn1 n1 => NPE:null:2",
        ),
        (
            "containsHostEntry",
            "it hn0 n0 gk1 gva it hn0 n0 gk1 gva hn1 => true,false",
        ),
        ("containsHashMapEntry", "it hn0 n0 gk2 gvb => true"),
    ]);
}

/// The merge of #118 with main's `list_equality.rs` keeps one null-hostile
/// mechanism: only a JDK `List.of`/`List.copyOf` list (`List12`/`ListN`)
/// rejects a `null` lookup, while `Collections.emptyList()` and
/// `singletonList` answer it like `AbstractList`. Expected lines: OpenJDK
/// 25.0.5, two runs with the same output.
#[test]
fn only_list_of_lists_reject_a_null_lookup() {
    check(&[
        ("emptyListContainsNull", " => false"),
        ("emptyListIndexOfNull", " => -1"),
        ("singletonListContainsNull", " => false"),
        (
            "retainAllSingletonListWithNull",
            "it hn0 n0 rm0 hn0 n0 hn1 n1 rm1 hn1 => true:1",
        ),
        ("listOfIndexOfNull", " => NPE:null"),
        ("listCopyOfContainsNull", " => NPE:null"),
    ]);
}
