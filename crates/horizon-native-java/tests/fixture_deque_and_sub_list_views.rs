//! `ArrayDeque`'s layout (`array_deque.rs`): `forEach` and its iterator
//! read the live array; `subList` views read through to their list and check
//! `modCount` where JDK 25's `SubList` does (`mod_count.rs`).
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `n` prints a `null` the action was handed.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r##"package demo;

import java.util.*;
import java.util.function.*;

public final class Probe {
    static final class K {
        final int id; K(int id) { this.id = id; }
        @Override public boolean equals(Object o) { return o instanceof K k && k.id == id; }
        @Override public int hashCode() { return id; }
    }
    static String ids(Collection<K> c) { StringBuilder sb = new StringBuilder("["); for (K k : c) sb.append(k.id).append(','); return sb.append(']').toString(); }
    static String show(Integer x) { return x == null ? "n" : String.valueOf(x.intValue()); }
    static ArrayDeque<Integer> small() { return new ArrayDeque<>(List.of(1, 2, 3, 4)); }
    static ArrayDeque<Integer> roomy() { ArrayDeque<Integer> d = new ArrayDeque<>(); for (int i = 1; i <= 4; i++) d.add(i); return d; }

    static String forEach(ArrayDeque<Integer> d, Consumer<ArrayDeque<Integer>> at2) {
        StringBuilder sb = new StringBuilder();
        try { d.forEach(x -> { sb.append(show(x)); if (x != null && x == 2) at2.accept(d); }); sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        catch (NullPointerException e) { sb.append("NPE"); }
        return sb + "" + d;
    }
    static String loop(ArrayDeque<Integer> d, Consumer<ArrayDeque<Integer>> at2) {
        StringBuilder sb = new StringBuilder();
        try { for (Integer x : d) { sb.append(show(x)); if (x == 2) at2.accept(d); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + d;
    }

    // The reviewer's `ArrayDeque.forEach` programs on #105.
    public static String forEachPollLastSmall() { return forEach(small(), ArrayDeque::pollLast); }
    public static String forEachPollLastRoomy() { return forEach(roomy(), ArrayDeque::pollLast); }
    public static String forEachClearSmall() { return forEach(small(), ArrayDeque::clear); }
    public static String forEachClearRoomy() { return forEach(roomy(), ArrayDeque::clear); }
    public static String forEachPollFirst() { return forEach(roomy(), ArrayDeque::pollFirst); }
    public static String forEachAddLastRoomy() { return forEach(roomy(), d -> d.addLast(9)); }
    public static String forEachAddFirstRoomy() { return forEach(roomy(), d -> d.addFirst(9)); }
    public static String forEachRemoveMiddle() { return forEach(roomy(), d -> d.remove(3)); }
    public static String forEachPlain() { return forEach(small(), d -> {}); }
    public static String randomF136() {
        ArrayDeque<K> d = new ArrayDeque<>(); d.add(new K(2)); d.add(new K(4)); d.add(new K(1)); d.add(new K(4));
        d.remove(new K(1)); StringBuilder sb = new StringBuilder();
        try { d.forEach(x -> { sb.append(x.id); if (x.id == 4) d.pollLast(); }); sb.append("no"); }
        catch (NullPointerException e) { sb.append("NPE"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + ids(d);
    }

    // For-each over a deque (`DeqIterator`).
    public static String loopPollLast() { return loop(small(), ArrayDeque::pollLast); }
    public static String loopPollFirst() { return loop(roomy(), ArrayDeque::pollFirst); }
    public static String loopAddLastSmall() { return loop(small(), d -> d.addLast(9)); }
    public static String loopAddFirstSmall() { return loop(small(), d -> d.addFirst(9)); }
    public static String loopAddFirstRoomy() { return loop(roomy(), d -> d.addFirst(9)); }
    public static String loopRemoveMiddle() { return loop(roomy(), d -> d.remove(3)); }
    public static String loopAfterWrap() {
        ArrayDeque<Integer> d = new ArrayDeque<>(4); d.add(3); d.add(4); d.addFirst(2); d.addFirst(1);
        return loop(d, x -> { d.pollLast(); d.addFirst(0); });
    }
    public static String loopAfterGrowth() {
        ArrayDeque<Integer> d = new ArrayDeque<>(2); d.add(1); d.add(2); d.addFirst(0); d.add(3);
        return loop(d, x -> d.pollLast());
    }

    // The reviewer's `subList` reads without a call on the view.
    static List<Integer> list() { return new ArrayList<>(List.of(1, 2, 3, 4)); }
    public static String concatAfterSet() { List<Integer> l = list(); List<Integer> s = l.subList(0, 2); l.set(0, 9); return "" + s; }
    public static String copyAfterSet() { List<Integer> l = list(); List<Integer> s = l.subList(0, 2); l.set(0, 9); return new ArrayList<>(s) + "" + List.of(9, 2).equals(s); }
    public static String loopOverViewWithSet() {
        List<Integer> l = list(); List<Integer> s = l.subList(1, 4); StringBuilder sb = new StringBuilder();
        for (Integer x : s) { sb.append(x.intValue()); if (x == 2) l.set(3, 9); }
        return sb.toString();
    }
    static String afterAdd(Function<List<Integer>, Object> read) {
        List<Integer> l = list(); List<Integer> s = l.subList(0, 2); l.add(9);
        try { return "no" + read.apply(s); } catch (ConcurrentModificationException e) { return "CME"; }
    }
    public static String copyAfterAdd() { return afterAdd(s -> new ArrayList<>(s)); }
    public static String addAllAfterAdd() { return afterAdd(s -> { List<Integer> o = new ArrayList<>(); o.addAll(s); return o; }); }
    public static String concatAfterAdd() { return afterAdd(s -> "" + s); }
    public static String equalsAfterAdd() { return afterAdd(s -> List.of(1, 2).equals(s)); }
    public static String valueOfAfterAdd() { return afterAdd(String::valueOf); }
    public static String randomF239() {
        List<Integer> root = list(); List<Integer> c = root.subList(1, 2); int h = c.hashCode(); root.add(7);
        try { return h + "" + c; } catch (ConcurrentModificationException e) { return h + "CME"; }
    }

    // Where `SubList` checks `modCount`.
    static String stale(Function<List<Integer>, Object> call) {
        List<Integer> l = list(); List<Integer> s = l.subList(1, 3); l.add(9);
        try { return String.valueOf(call.apply(s)); }
        catch (ConcurrentModificationException e) { return "CME"; }
        catch (IndexOutOfBoundsException e) { return "IOOBE:" + e.getMessage(); }
    }
    public static String staleAddAllEmpty() { return stale(s -> s.addAll(List.of())); }
    public static String staleGetOutside() { return stale(s -> s.get(5)); }
    public static String staleEqualsSelf() { return stale(s -> s.equals(s)); }
    public static String staleSubListThenSize() { return stale(s -> { List<Integer> t = s.subList(0, 1); return t.size(); }); }
    public static String staleSize() { return stale(List::size); }
    public static String staleContains() { return stale(s -> s.contains(3)); }
    public static String staleHashCode() { return stale(List::hashCode); }
    public static String staleContainsAllEmpty() { return stale(s -> s.containsAll(List.of())); }
    public static String linkedStaleEqualsSelf() {
        LinkedList<Integer> l = new LinkedList<>(List.of(1, 2, 3, 4)); List<Integer> s = l.subList(1, 3); l.add(9);
        try { return s.equals(s) + "" + s.addAll(List.of()); } catch (ConcurrentModificationException e) { return "CME"; }
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

/// The reviewer's programs on #105: `forEach` hands the action an emptied
/// slot as `null` and throws if `tail` moved (`123n` then CME, `12nn` then
/// CME); random f136's action meets the `null` (`24`, then NPE).
#[test]
fn array_deque_for_each_reads_the_live_array() {
    assert_exact(
        "array-deque-for-each-reads-the-live-arra",
        &[
            ("forEachPollLastSmall", "123nCME[1, 2, 3]"),
            ("forEachPollLastRoomy", "123nCME[1, 2, 3]"),
            ("forEachClearSmall", "12nnCME[]"),
            ("forEachClearRoomy", "12nnCME[]"),
            ("forEachPollFirst", "1234no[2, 3, 4]"),
            ("forEachAddFirstRoomy", "1234no[9, 1, 2, 3, 4]"),
            ("forEachRemoveMiddle", "124nCME[1, 2, 4]"),
            ("forEachPlain", "1234no[1, 2, 3, 4]"),
            ("randomF136", "24NPE[2,4,]"),
        ],
    );
}

/// `DeqIterator.next()` reads the live array at its cursor and throws only
/// at an emptied slot: after `pollLast`, a `remove` that moved the back, on a
/// wrapped or grown array; `addFirst`/`pollFirst` leave its slots alone.
#[test]
fn array_deque_iterator_throws_at_an_emptied_slot() {
    assert_exact(
        "array-deque-iterator-throws-at-an-emptie",
        &[
            ("loopPollLast", "123CME[1, 2, 3]"),
            ("loopPollFirst", "1234no[2, 3, 4]"),
            ("loopAddFirstSmall", "1234no[9, 1, 2, 3, 4]"),
            ("loopAddFirstRoomy", "1234no[9, 1, 2, 3, 4]"),
            ("loopRemoveMiddle", "124CME[1, 2, 4]"),
            ("loopAfterWrap", "123CME[0, 1, 2, 3]"),
            ("loopAfterGrowth", "012CME[0, 1, 2]"),
        ],
    );
}

/// The reviewer's `subList` programs on #105: a view read without a call on
/// it (`"" + s`, `new ArrayList<>(s)`, `addAll(s)`, `equals(s)`) sees its
/// list's `set`, and throws after a structural change.
#[test]
fn a_sub_list_reads_through_to_its_list() {
    assert_exact(
        "a-sub-list-reads-through-to-its-list",
        &[
            ("concatAfterSet", "[9, 2]"),
            ("copyAfterSet", "[9, 2]true"),
            ("loopOverViewWithSet", "239"),
            ("copyAfterAdd", "CME"),
            ("addAllAfterAdd", "CME"),
            ("concatAfterAdd", "CME"),
            ("equalsAfterAdd", "CME"),
            ("valueOfAfterAdd", "CME"),
            ("randomF239", "33CME"),
        ],
    );
}

/// A stale view: `addAll` of nothing returns `false`, an index outside it
/// throws `IndexOutOfBoundsException`, `equals(this)` is `true`, `subList`
/// is made and its first use throws, `contains` throws after its walk.
#[test]
fn sub_list_members_check_where_jdk_25_does() {
    assert_exact(
        "sub-list-members-check-where-jdk-25-does",
        &[
            ("staleAddAllEmpty", "false"),
            (
                "staleGetOutside",
                "IOOBE:Index 5 out of bounds for length 2",
            ),
            ("staleEqualsSelf", "true"),
            ("staleSubListThenSize", "CME"),
            ("staleSize", "CME"),
            ("staleContains", "CME"),
            ("staleContainsAllEmpty", "true"),
            ("linkedStaleEqualsSelf", "truefalse"),
        ],
    );
}

/// `ArrayDeque.addLast` and `List.hashCode` are counted by the host.
#[test]
fn members_the_host_counts_on_a_deque_or_view() {
    let fixture = fixture("members-the-host-counts-on-a-deque-or-vi");
    let cases: &[(&str, &[(&str, usize)])] = &[
        (
            "forEachAddLastRoomy",
            &[("java/util/ArrayDeque#addLast(Ljava/lang/Object;)V", 1)],
        ),
        (
            "loopAddLastSmall",
            &[("java/util/ArrayDeque#addLast(Ljava/lang/Object;)V", 1)],
        ),
        ("staleHashCode", &[("java/util/List#hashCode()I", 1)]),
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
