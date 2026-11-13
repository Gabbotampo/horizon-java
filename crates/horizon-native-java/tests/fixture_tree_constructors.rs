//! `TreeSet`/`TreeMap` constructors take a comparator by overload, and
//! synchronized views write through the tree's model (`tree_map.rs`).
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `K` logs `c<id>,<other>` per `compareTo`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r##"package demo;

import java.util.*;

public final class Probe {
    static final StringBuilder log = new StringBuilder();
    static final class K implements Comparable<K> {
        final int id; K(int id) { this.id = id; }
        public int compareTo(K o) { log.append('c').append(id).append(',').append(o.id).append(' '); return Integer.compare(id, o.id); }
    }
    static final Comparator<K> DESC = (a, b) -> { log.append('d').append(a.id).append(b.id).append(' '); return Integer.compare(b.id, a.id); };
    static String ids(Collection<K> c) { StringBuilder sb = new StringBuilder("["); for (K k : c) sb.append(k.id).append(','); return sb.append(']').toString(); }
    static String text(Collection<?> c) { StringBuilder sb = new StringBuilder("["); for (Object o : c) sb.append(o).append(','); return sb.append(']').toString(); }

    // The reviewer's R1 programs: the overload, not the argument's class.
    public static String treeSetFromCollection() {
        Collection<Integer> src = new TreeSet<>(Comparator.reverseOrder()); src.addAll(List.of(1, 2, 3));
        return text(new TreeSet<>(src));
    }
    public static String treeMapFromMap() {
        Map<Integer, Integer> src = new TreeMap<>(Comparator.reverseOrder()); for (int i = 1; i <= 3; i++) src.put(i, i);
        TreeMap<Integer, Integer> o = new TreeMap<>(src);
        return text(o.keySet()) + " " + o.firstKey() + " " + (o.comparator() == null);
    }
    public static String treeMapFromMapKeys() {
        TreeMap<K, Integer> d = new TreeMap<>(DESC); d.put(new K(8), 3); d.put(new K(3), 1); d.put(new K(2), 2);
        Map<K, Integer> c = d; log.setLength(0);
        TreeMap<K, Integer> o = new TreeMap<>(c);
        return log + ids(o.keySet());
    }
    // `(SortedSet)`/`(SortedMap)` keep it.
    public static String treeSetFromSortedSet() {
        SortedSet<Integer> src = new TreeSet<>(Comparator.reverseOrder()); src.addAll(List.of(1, 2, 3));
        return text(new TreeSet<>(src));
    }
    public static String treeMapFromSortedMap() {
        SortedMap<Integer, Integer> src = new TreeMap<>(Comparator.reverseOrder()); src.put(1, 1); src.put(2, 2);
        TreeMap<Integer, Integer> o = new TreeMap<>(src);
        return text(o.keySet()) + (o.comparator() == src.comparator());
    }
    // `TreeSet(Collection)` of a natural `TreeSet`: `addAll`'s bulk path, no
    // comparisons.
    public static String treeSetFromNaturalSorted() {
        Collection<K> src = new TreeSet<>(); src.add(new K(5)); src.add(new K(1)); src.add(new K(3)); log.setLength(0);
        TreeSet<K> o = new TreeSet<>(src); return log + ids(o);
    }
    public static String treeSetNullComparator() { TreeSet<Integer> t = new TreeSet<>((Comparator<Integer>) null); t.addAll(List.of(3, 1, 2)); return text(t) + (t.comparator() == null); }
    public static String queueFromReversedTreeSet() {
        Collection<Integer> src = new TreeSet<>(Comparator.reverseOrder()); src.addAll(List.of(1, 2, 3));
        return new PriorityQueue<>(src).peek() + "";
    }

    // R2 on trees: a synchronized view writes through the tree's model.
    public static String syncTreeSetAddAll() {
        TreeSet<Integer> t = new TreeSet<>(List.of(9, 3));
        boolean r = Collections.synchronizedSet(t).addAll(List.of(6, 3, 1));
        return r + " " + text(t) + " " + t.first();
    }
    public static String syncTreeSetAdd() { TreeSet<Integer> t = new TreeSet<>(List.of(9, 3)); Collections.synchronizedSet(t).add(1); return text(t); }
    public static String syncTreeMapPut() {
        TreeMap<Integer, Integer> t = new TreeMap<>(); Map<Integer, Integer> v = Collections.synchronizedMap(t);
        v.put(9, 9); v.put(1, 1); v.putAll(Map.of(5, 5)); return text(t.keySet());
    }
    public static String syncHashSetToArray() {
        Set<Integer> c = Collections.synchronizedSet(new HashSet<>(List.of(9, 7, 0)));
        c.addAll(Arrays.asList(4, 6, 9)); c.addAll(new LinkedHashSet<>(List.of(9)));
        StringBuilder sb = new StringBuilder(); for (Object o : c.toArray()) sb.append(o).append(','); return sb.toString();
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

/// The reviewer's R1 programs: `TreeSet(Collection)`/`TreeMap(Map)` start
/// in natural order whatever the argument is (`[1, 2, 3]`, and the copy's
/// `compareTo` walk `c8,8 c3,8 c2,8 c2,3`); `(SortedSet)`/`(SortedMap)`
/// keep its comparator; a natural sorted source copies with no comparisons.
#[test]
fn tree_constructors_take_the_comparator_by_overload() {
    assert_exact(
        "tree-constructors-take-the-comparator-by",
        &[
            ("treeSetFromCollection", "[1,2,3,]"),
            ("treeMapFromMap", "[1,2,3,] 1 true"),
            ("treeMapFromMapKeys", "c8,8 c3,8 c2,8 c2,3 [2,3,8,]"),
            ("treeSetFromSortedSet", "[3,2,1,]"),
            ("treeMapFromSortedMap", "[2,1,]true"),
            ("treeSetFromNaturalSorted", "[1,3,5,]"),
            ("treeSetNullComparator", "[1,2,3,]true"),
            ("queueFromReversedTreeSet", "3"),
        ],
    );
}

/// The reviewer's R2 programs: `Collections.synchronizedSet(treeSet).addAll`
/// leaves the tree sorted (`[1, 3, 6, 9]`), as do `add` and a synchronized
/// map's `put`; a synchronized `HashSet` keeps its table order.
#[test]
fn synchronized_views_write_through_the_tree() {
    assert_exact(
        "synchronized-views-write-through-the-tre",
        &[
            ("syncTreeSetAddAll", "true [1,3,6,9,] 1"),
            ("syncTreeSetAdd", "[1,3,9,]"),
            ("syncTreeMapPut", "[1,5,9,]"),
            ("syncHashSetToArray", "0,4,6,7,9,"),
        ],
    );
}
