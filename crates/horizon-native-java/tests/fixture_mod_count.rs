//! `modCount` (`mod_count.rs`, `iteration_guard.rs`): changes that undo
//! themselves or change nothing still make fail-fast iteration throw,
//! `LinkedList.removeIf`, `ArrayDeque`'s bulk members and `subList` views.
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `K` logs each `equals` as `e<id>:<other id>`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r##"package demo;

import java.util.*;
import java.util.function.*;

public final class Probe {
    static final StringBuilder log = new StringBuilder();
    /// Logs `e<id>:<other id>` per `equals`.
    static final class K {
        final int id; K(int id) { this.id = id; }
        @Override public boolean equals(Object o) {
            log.append('e').append(id).append(':').append(o instanceof K k ? String.valueOf(k.id) : String.valueOf(o)).append(' ');
            return o instanceof K k && k.id == id;
        }
        @Override public int hashCode() { return id; }
        @Override public String toString() { return "K" + id; }
    }

    static String ids(Collection<K> c) { StringBuilder sb = new StringBuilder("["); for (K k : c) sb.append(k.id).append(','); return sb.append(']').toString(); }

    static List<Integer> list() { return new ArrayList<>(List.of(1, 2, 3, 4)); }

    /// For-each over `l`, printing, running `act` at 2.
    static String loop(List<Integer> l, Runnable act) {
        StringBuilder sb = new StringBuilder();
        try { for (Integer x : l) { sb.append(x.intValue()); if (x == 2) act.run(); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + l;
    }

    // The reviewer's programs on #105: the change undoes itself, or changes nothing.
    public static String addThenRemove() { List<Integer> l = list(); return loop(l, () -> { l.add(9); l.remove(l.size() - 1); }); }
    public static String addAllEmpty() { List<Integer> l = list(); return loop(l, () -> l.addAll(List.of())); }
    public static String sortSorted() { List<Integer> l = list(); return loop(l, () -> l.sort(null)); }
    public static String addAtThenRemoveAt() { List<Integer> l = list(); return loop(l, () -> { l.add(0, 2); l.remove(0); }); }
    public static String clearRefill() { List<Integer> l = list(); return loop(l, () -> { List<Integer> c = new ArrayList<>(l); l.clear(); l.addAll(c); }); }
    public static String iteratorAddRemove() {
        List<Integer> l = list(); StringBuilder sb = new StringBuilder();
        try { Iterator<Integer> it = l.iterator(); while (it.hasNext()) { Integer x = it.next(); sb.append(x.intValue()); if (x == 2) { l.add(5); l.remove(Integer.valueOf(5)); } } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + l;
    }
    public static String forEachAddRemove() {
        List<Integer> l = list(); StringBuilder sb = new StringBuilder();
        try { l.forEach(x -> { sb.append(x.intValue()); if (x == 2) { l.add(9); l.remove(4); } }); sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + l;
    }
    public static String removeIfAddRemove() {
        List<Integer> l = list();
        try { boolean r = l.removeIf(x -> { if (x == 1) { l.add(7); l.remove(4); } return x == 2; }); return r + "" + l; }
        catch (ConcurrentModificationException e) { return "CME" + l; }
    }
    public static String linkedAddRemoveLast() { LinkedList<Integer> l = new LinkedList<>(List.of(1, 2, 3, 4)); return loop(l, () -> { l.add(9); l.removeLast(); }); }
    public static String linkedForEachAddRemoveLast() {
        LinkedList<Integer> l = new LinkedList<>(List.of(1, 2, 3, 4)); StringBuilder sb = new StringBuilder();
        try { l.forEach(x -> { sb.append(x.intValue()); if (x == 2) { l.add(9); l.removeLast(); } }); sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + l;
    }
    public static String vectorAddAllEmpty() { Vector<Integer> l = new Vector<>(List.of(1, 2, 3, 4)); return loop(l, () -> l.addAll(List.of())); }

    // What bumps `modCount` in JDK 25 and what does not.
    public static String setInLoop() { List<Integer> l = list(); return loop(l, () -> l.set(3, 8)); }
    public static String vectorRemoveMissing() { Vector<Integer> l = new Vector<>(List.of(1, 2, 3, 4)); return loop(l, () -> l.removeElement(7)); }
    public static String listRemoveMissing() { List<Integer> l = list(); return loop(l, () -> l.remove(Integer.valueOf(7))); }
    public static String trimInLoop() { ArrayList<Integer> l = new ArrayList<>(List.of(1, 2, 3, 4)); return loop(l, l::trimToSize); }
    public static String collectionsSortInLoop() { List<Integer> l = list(); return loop(l, () -> Collections.sort(l)); }
    public static String linkedSortInLoop() { LinkedList<Integer> l = new LinkedList<>(List.of(1, 2, 3, 4)); return loop(l, () -> l.sort(null)); }
    public static String replaceAllInLoop() { List<Integer> l = list(); return loop(l, () -> l.replaceAll(x -> x)); }
    public static String clearEmptyInLoop() {
        List<Integer> l = list(); List<Integer> other = new ArrayList<>(); StringBuilder sb = new StringBuilder();
        try { for (Integer x : l) { sb.append(x.intValue()); other.clear(); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb.toString();
    }
    public static String otherListChanges() {
        List<Integer> l = list(); List<Integer> other = list(); StringBuilder sb = new StringBuilder();
        for (Integer x : l) { sb.append(x.intValue()); other.add(x); }
        return sb + "" + other.size();
    }

    // `LinkedList.removeIf` is `Collection.removeIf`: through the iterator.
    public static String linkedRemoveIfThrows() {
        LinkedList<Integer> l = new LinkedList<>(List.of(1, 2, 3, 4)); int[] n = {0};
        try { l.removeIf(x -> { if (++n[0] == 3) throw new IllegalStateException(); return x <= 2; }); return "no" + l; }
        catch (IllegalStateException e) { return "ISE" + l; }
    }
    public static String linkedRemoveIfAdds() {
        LinkedList<Integer> l = new LinkedList<>(List.of(1, 2, 3, 4));
        try { boolean r = l.removeIf(x -> { if (x == 1) l.add(9); return x == 2; }); return r + "" + l; }
        catch (ConcurrentModificationException e) { return "CME" + l; }
    }
    public static String linkedRemoveIfPlain() {
        LinkedList<Integer> l = new LinkedList<>(List.of(1, 2, 3, 4, 5));
        boolean r = l.removeIf(x -> x % 2 == 1);
        return r + "" + l + l.removeIf(x -> x > 10);
    }

    // `ArrayDeque`'s bulk members compare with `equals`.
    static ArrayDeque<K> deque() { ArrayDeque<K> d = new ArrayDeque<>(); d.add(new K(1)); d.add(new K(2)); log.setLength(0); return d; }
    public static String dequeContainsAll() { ArrayDeque<K> d = deque(); boolean r = d.containsAll(List.of(new K(2))); return r + "#" + log; }
    public static String dequeRetainAll() { ArrayDeque<K> d = deque(); boolean r = d.retainAll(List.of(new K(2))); return r + ids(d) + "#" + log; }
    public static String dequeRemoveAll() { ArrayDeque<K> d = deque(); boolean r = d.removeAll(List.of(new K(2))); return r + ids(d) + "#" + log; }
    public static String dequeRemoveAllMany() {
        ArrayDeque<K> d = new ArrayDeque<>(); for (int i = 1; i <= 4; i++) d.add(new K(i)); log.setLength(0);
        boolean r = d.removeAll(new ArrayList<>(List.of(new K(3), new K(1)))); return r + ids(d) + "#" + log;
    }

    // `ArrayDeque` has no `modCount`: a change under its iterator.
    public static String dequeLoopPollLast() {
        ArrayDeque<Integer> d = new ArrayDeque<>(List.of(1, 2, 3, 4)); StringBuilder sb = new StringBuilder();
        try { for (Integer x : d) { sb.append(x.intValue()); if (x == 2) d.pollLast(); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb.toString();
    }

    // `subList` is a view.
    public static String subListAdd() { List<Integer> l = list(); List<Integer> s = l.subList(2, 4); s.add(5); return l + "" + s; }
    public static String subListClear() { List<Integer> l = list(); l.subList(1, 3).clear(); return l.toString(); }
    public static String subListAfterParentAdd() {
        List<Integer> l = list(); List<Integer> s = l.subList(1, 3); l.add(9);
        try { return s.toString(); } catch (ConcurrentModificationException e) { return "CME" + l; }
    }
    public static String subListSeesSet() { List<Integer> l = list(); List<Integer> s = l.subList(1, 3); l.set(1, 7); s.set(1, 8); return s + "" + l + s.size(); }
    public static String subListRemoveAndSort() {
        List<Integer> l = new ArrayList<>(List.of(5, 4, 3, 2, 1)); List<Integer> s = l.subList(1, 4);
        s.remove(Integer.valueOf(3)); s.sort(null); s.add(0, 9); return l + "" + s;
    }
    public static String subListOfSubList() {
        List<Integer> l = new ArrayList<>(List.of(1, 2, 3, 4, 5, 6)); List<Integer> s = l.subList(1, 5); List<Integer> t = s.subList(1, 3);
        t.clear(); return l + "" + s;
    }
    public static String linkedSubListClear() { LinkedList<Integer> l = new LinkedList<>(List.of(1, 2, 3, 4)); l.subList(0, 2).clear(); return l.toString(); }
    public static String subListLoopInParent() { List<Integer> l = list(); return loop(l, () -> l.subList(0, 1).clear()); }
    public static String unmodifiableLoop() {
        List<Integer> l = list(); List<Integer> u = Collections.unmodifiableList(l); StringBuilder sb = new StringBuilder();
        try { for (Integer x : u) { sb.append(x.intValue()); if (x == 2) l.add(9); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb.toString();
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

/// The reviewer's programs on #105: `modCount` moves even when the list
/// ends as it started (`add` then `remove`), and `addAll` of nothing and
/// `sort` move it in `ArrayList`/`Vector`. `removeIf` then throws instead of
/// removing. A loop over an unmodifiable view checks the list under it.
#[test]
fn changes_that_undo_themselves_still_throw() {
    assert_exact(
        "changes-that-undo-themselves-still-throw",
        &[
            ("addThenRemove", "12CME[1, 2, 3, 4]"),
            ("addAllEmpty", "12CME[1, 2, 3, 4]"),
            ("sortSorted", "12CME[1, 2, 3, 4]"),
            ("addAtThenRemoveAt", "12CME[1, 2, 3, 4]"),
            ("clearRefill", "12CME[1, 2, 3, 4]"),
            ("iteratorAddRemove", "12CME[1, 2, 3, 4]"),
            ("forEachAddRemove", "12CME[1, 2, 3, 4]"),
            ("removeIfAddRemove", "CME[1, 2, 3, 4]"),
            ("linkedAddRemoveLast", "12CME[1, 2, 3, 4]"),
            ("linkedForEachAddRemoveLast", "12CME[1, 2, 3, 4]"),
            ("vectorAddAllEmpty", "12CME[1, 2, 3, 4]"),
            ("collectionsSortInLoop", "12CME[1, 2, 3, 4]"),
            ("subListLoopInParent", "12CME[2, 3, 4]"),
            ("unmodifiableLoop", "12CME"),
        ],
    );
}

/// `set` and a `remove` that finds nothing move no `modCount`, nor does
/// `LinkedList.sort` (`List.sort` writes through `set`), nor a change to
/// another list.
#[test]
fn only_what_jdk_25_bumps_moves_mod_count() {
    assert_exact(
        "only-what-jdk-25-bumps-moves-mod-count",
        &[
            ("setInLoop", "1238no[1, 2, 3, 8]"),
            ("listRemoveMissing", "1234no[1, 2, 3, 4]"),
            ("linkedSortInLoop", "1234no[1, 2, 3, 4]"),
            ("clearEmptyInLoop", "1234no"),
            ("otherListChanges", "12348"),
        ],
    );
}

/// `LinkedList.removeIf` is `Collection.removeIf`: a predicate that throws
/// leaves 1 and 2 removed (`ISE[3, 4]`), one that adds throws at the next
/// step.
#[test]
fn linked_list_remove_if_removes_through_the_iterator() {
    assert_exact(
        "linked-list-remove-if-removes-through-th",
        &[
            ("linkedRemoveIfThrows", "ISE[3, 4]"),
            ("linkedRemoveIfAdds", "CME[1, 2, 3, 4, 9]"),
            ("linkedRemoveIfPlain", "true[2, 4]false"),
        ],
    );
}

/// The reviewer's programs on #105: `ArrayDeque.containsAll` is `contains`
/// per element, `removeAll`/`retainAll` test `c.contains(e)` head to tail.
#[test]
fn array_deque_bulk_members_compare_with_equals() {
    assert_exact(
        "array-deque-bulk-members-compare-with-eq",
        &[
            ("dequeContainsAll", "true#e2:1 e2:2 "),
            ("dequeRetainAll", "true[2,]#e1:2 e2:2 "),
            ("dequeRemoveAll", "true[1,]#e1:2 e2:2 "),
            (
                "dequeRemoveAllMany",
                "true[2,4,]#e1:3 e1:1 e2:3 e2:1 e3:3 e4:3 e4:1 ",
            ),
        ],
    );
}

/// The reviewer's `subList` programs on #105: writes reach the list
/// (`subList(1, 3).clear()` gives `[1, 4]`), a `set` on either shows in
/// both, a sub-list of a sub-list writes through both, and a structural
/// change to the list makes the view throw.
#[test]
fn sub_lists_write_through_to_their_list() {
    assert_exact(
        "sub-lists-write-through-to-their-list",
        &[
            ("subListAdd", "[1, 2, 3, 4, 5][3, 4, 5]"),
            ("subListClear", "[1, 4]"),
            ("subListAfterParentAdd", "CME[1, 2, 3, 4, 9]"),
            ("subListSeesSet", "[7, 8][1, 7, 8, 4]2"),
            ("subListRemoveAndSort", "[5, 9, 2, 4, 1][9, 2, 4]"),
            ("subListOfSubList", "[1, 2, 5, 6][2, 5]"),
            ("linkedSubListClear", "[3, 4]"),
        ],
    );
}

/// `trimToSize`, `Vector.removeElement` and `List.replaceAll` are counted
/// by the host but still move `modCount` (`12CME`, as JDK 25).
#[test]
fn members_the_host_counts_still_move_mod_count() {
    let fixture = fixture("members-the-host-counts-still-move-mod-c");
    let cases: &[(&str, &[(&str, usize)])] = &[
        ("trimInLoop", &[("java/util/ArrayList#trimToSize()V", 1)]),
        (
            "vectorRemoveMissing",
            &[("java/util/Vector#removeElement(Ljava/lang/Object;)Z", 1)],
        ),
        (
            "replaceAllInLoop",
            &[(
                "java/util/List#replaceAll(Ljava/util/function/UnaryOperator;)V",
                1,
            )],
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

/// `ArrayDeque` has no `modCount`: its iterator throws at the slot
/// `pollLast` emptied (`array_deque.rs`).
#[test]
fn array_deque_iterator_meets_the_emptied_slot() {
    assert_exact(
        "array-deque-iterator-meets-the-emptied-s",
        &[("dequeLoopPollLast", "123CME")],
    );
}
