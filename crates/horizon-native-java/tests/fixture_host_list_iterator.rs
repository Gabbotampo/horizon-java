//! `listIterator()` on the host's `ArrayList` and `LinkedList`
//! (`host_list_iterator.rs`): walking both ways, `set`/`add`/`remove`
//! writing through to the list, and the JDK's exceptions.
//!
//! Each expected line is OpenJDK 25.0.5's output for the same probe method
//! (`javac --release 21`, `java -cp out demo.Probe`), pasted verbatim; a case
//! must also count no unknown host call. Before this, `listIterator` on a host list
//! was unmodelled.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

const PROBE: &str = r#"package demo;

import java.util.*;

public final class Probe {
    static final StringBuilder LOG = new StringBuilder();

    static void log(String event) { LOG.append(event).append(' '); }

    static String take(Object result) {
        String text = LOG.toString().trim() + " => " + result;
        LOG.setLength(0);
        return text;
    }

    public static String listIteratorWalk() {
        List<String> list = new ArrayList<>(List.of("a", "b", "c"));
        ListIterator<String> it = list.listIterator();
        StringBuilder out = new StringBuilder();
        while (it.hasNext()) { int i = it.nextIndex(); out.append(i).append(it.next()); }
        while (it.hasPrevious()) { int i = it.previousIndex(); out.append(i).append(it.previous()); }
        return take(out);
    }

    public static String listIteratorMutate() {
        List<String> list = new ArrayList<>(List.of("a", "b", "c"));
        ListIterator<String> it = list.listIterator(1);
        it.next();
        it.set("B");
        it.add("x");
        it.next();
        it.remove();
        String state = it.nextIndex() + "," + it.previousIndex() + "," + it.hasNext();
        String illegal;
        try { it.set("y"); illegal = "set"; } catch (IllegalStateException e) { illegal = e.getClass().getName(); }
        return take(list + "," + state + "," + illegal);
    }

    public static String listIteratorErrors() {
        List<String> list = new ArrayList<>(List.of("a"));
        String bad;
        try { list.listIterator(2); bad = "ok"; } catch (IndexOutOfBoundsException e) { bad = e.getClass().getName() + ":" + e.getMessage(); }
        ListIterator<String> it = list.listIterator();
        String prev;
        try { it.previous(); prev = "ok"; } catch (NoSuchElementException e) { prev = e.getClass().getName() + ":" + e.getMessage(); }
        ListIterator<String> cme = list.listIterator();
        list.add("z");
        String mod;
        try { cme.next(); mod = "ok"; } catch (ConcurrentModificationException e) { mod = e.getClass().getName() + ":" + e.getMessage(); }
        return take(bad + "," + prev + "," + mod);
    }

    public static String linkedListIterator() {
        List<String> list = new LinkedList<>(List.of("a", "b"));
        ListIterator<String> it = list.listIterator(2);
        it.previous();
        it.add("m");
        it.previous();
        it.set("M");
        return take(list + "," + it.nextIndex());
    }

    static String err(Throwable e) { return e.getClass().getName() + ":" + e.getMessage(); }


    public static String linkedErrors() {
        List<String> list = new LinkedList<>(List.of("a"));
        String bad;
        try { list.listIterator(2); bad = "ok"; } catch (IndexOutOfBoundsException e) { bad = err(e); }
        ListIterator<String> it = list.listIterator();
        it.next();
        String past;
        try { it.next(); past = "ok"; } catch (NoSuchElementException e) { past = err(e); }
        String before;
        try { list.listIterator().previous(); before = "ok"; } catch (NoSuchElementException e) { before = err(e); }
        ListIterator<String> cme = list.listIterator();
        list.add("z");
        String mod;
        try { cme.next(); mod = "ok"; } catch (ConcurrentModificationException e) { mod = err(e); }
        return take(bad + "," + past + "," + before + "," + mod);
    }

    public static String iteratorState() {
        List<String> list = new ArrayList<>(List.of("a", "b"));
        ListIterator<String> it = list.listIterator();
        it.add("x");
        String afterAdd;
        try { it.remove(); afterAdd = "ok"; } catch (IllegalStateException e) { afterAdd = err(e); }
        it.next();
        it.set("A");
        it.set("AA");
        it.add("y");
        String setAfterAdd;
        try { it.set("q"); setAfterAdd = "ok"; } catch (IllegalStateException e) { setAfterAdd = err(e); }
        String prev = it.previous();
        it.remove();
        return take(list + "," + afterAdd + "," + setAfterAdd + "," + prev + "," + it.nextIndex() + "," + it.previousIndex());
    }

    public static String previousThenRemove() {
        List<String> list = new ArrayList<>(List.of("a", "b", "c"));
        ListIterator<String> it = list.listIterator(3);
        String p = it.previous();
        it.remove();
        return take(list + "," + p + "," + it.nextIndex() + "," + it.hasNext());
    }

    public static String nonStructuralWrite() {
        List<String> list = new ArrayList<>(List.of("a", "b"));
        ListIterator<String> it = list.listIterator();
        list.set(0, "z");
        String first = it.next();
        list.set(1, "w");
        return take(first + "," + it.next() + "," + list);
    }

    public static String listOfIterator() {
        ListIterator<String> it = List.of("a", "b").listIterator();
        return take(it == null ? "null" : "made");
    }

    static String nextOrCme(ListIterator<Integer> it) {
        try { return String.valueOf(it.next()); } catch (ConcurrentModificationException e) { return "CME"; }
    }

    static String hasNextAfterRemove(List<Integer> l) {
        ListIterator<Integer> it = l.listIterator();
        it.next(); it.next(); it.next();
        l.remove(0);
        log(String.valueOf(it.hasNext()));
        return take(nextOrCme(it));
    }

    public static String hasNextArrayList() { return hasNextAfterRemove(new ArrayList<>(List.of(1, 2, 3))); }
    public static String hasNextLinkedList() { return hasNextAfterRemove(new LinkedList<>(List.of(1, 2, 3))); }

    static String restoring(List<Integer> l, boolean clear) {
        ListIterator<Integer> it = l.listIterator();
        it.next();
        if (clear) { l.clear(); l.addAll(List.of(1, 2, 3)); } else { l.add(9); l.remove(3); }
        return take(l + "," + nextOrCme(it));
    }

    public static String restoreArrayList() { return restoring(new ArrayList<>(List.of(1, 2, 3)), false); }
    public static String restoreLinkedList() { return restoring(new LinkedList<>(List.of(1, 2, 3)), false); }
    public static String restoreClearAddAll() { return restoring(new ArrayList<>(List.of(1, 2, 3)), true); }

    static String throwingEach(List<Integer> l) {
        ListIterator<Integer> it = l.listIterator();
        try {
            it.forEachRemaining(x -> { log("e" + x); if (x == 2) throw new IllegalStateException(); });
        } catch (IllegalStateException e) { log("caught"); }
        return take(it.nextIndex() + "," + it.hasNext() + "," + it.next());
    }

    public static String throwingArrayList() { return throwingEach(new ArrayList<>(List.of(1, 2, 3))); }
    public static String throwingLinkedList() { return throwingEach(new LinkedList<>(List.of(1, 2, 3))); }
    public static String throwingVector() { return throwingEach(new Vector<>(List.of(1, 2, 3))); }

    public static String outsideSetAndReplaceAll() {
        List<Integer> l = new ArrayList<>(List.of(1, 2, 3));
        ListIterator<Integer> it = l.listIterator();
        it.next();
        l.set(1, 9);
        String a = nextOrCme(it);
        return take(a);
    }

    public static String forEachRemainingOutsideAdd() {
        List<Integer> l = new ArrayList<>(List.of(1, 2, 3));
        ListIterator<Integer> it = l.listIterator();
        String r;
        try { it.forEachRemaining(x -> { log("e" + x); if (x == 1) l.add(7); }); r = "ok"; } catch (ConcurrentModificationException e) { r = "CME"; }
        List<Integer> k = new LinkedList<>(List.of(1, 2, 3));
        ListIterator<Integer> kt = k.listIterator();
        String s;
        try { kt.forEachRemaining(x -> { log("k" + x); if (x == 1) k.add(7); }); s = "ok"; } catch (ConcurrentModificationException e) { s = "CME"; }
        return take(r + "," + it.nextIndex() + "," + s + "," + kt.nextIndex());
    }


    public static String listIteratorForEach() {
        List<String> list = new ArrayList<>(List.of("a", "b", "c"));
        ListIterator<String> it = list.listIterator();
        it.next();
        StringBuilder out = new StringBuilder();
        it.forEachRemaining(x -> out.append(x));
        return take(out + "," + it.nextIndex() + "," + it.hasNext() + "," + it.previous());
    }

}
"#;

#[test]
fn host_list_iterators_run_as_jdk_25_does() {
    let fixture =
        javac_fixture::Fixture::build("host-list-iterator", &[("demo/Probe.java", PROBE)]);
    let cases: &[(&str, &str)] = &[
        ("listIteratorWalk", " => 0a1b2c2c1b0a"),
        (
            "listIteratorMutate",
            " => [a, B, x],3,2,false,java.lang.IllegalStateException",
        ),
        (
            "listIteratorErrors",
            " => java.lang.IndexOutOfBoundsException:Index: 2, Size: 1,java.util.NoSuchElementException:null,java.util.ConcurrentModificationException:null",
        ),
        ("linkedListIterator", " => [a, M, b],1"),
        (
            "linkedErrors",
            " => java.lang.IndexOutOfBoundsException:Index: 2, Size: 1,java.util.NoSuchElementException:null,java.util.NoSuchElementException:null,java.util.ConcurrentModificationException:null",
        ),
        (
            "iteratorState",
            " => [x, AA, b],java.lang.IllegalStateException:null,java.lang.IllegalStateException:null,y,2,1",
        ),
        ("previousThenRemove", " => [a, b],c,2,false"),
        ("listIteratorForEach", " => bc,3,false,c"),
        ("forEachRemainingOutsideAdd", "e1 k1 => CME,1,CME,1"),
        ("hasNextArrayList", "true => CME"),
        ("hasNextLinkedList", "false => CME"),
        ("outsideSetAndReplaceAll", " => 9"),
        ("restoreArrayList", " => [1, 2, 3],CME"),
        ("restoreClearAddAll", " => [1, 2, 3],CME"),
        ("restoreLinkedList", " => [1, 2, 3],CME"),
        ("throwingArrayList", "e1 e2 caught => 0,true,1"),
        ("throwingLinkedList", "e1 e2 caught => 1,true,2"),
        ("throwingVector", "e1 e2 caught => 0,true,1"),
        ("nonStructuralWrite", " => z,w,[z, w]"),
    ];
    let mut failures = Vec::new();
    for (method, expected) in cases {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let answer = fixture.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        let unknown = host.unknown_host_calls();
        if !matches!(&answer, Ok(Value::Ref(Some(text))) if text == expected && unknown.is_empty())
        {
            failures.push(format!(
                "{method}\n  jdk 25: {expected}\n  native: {answer:?}\n  unknown: {unknown:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "differ from JDK 25:\n{}",
        failures.join("\n")
    );
}

/// Runs `method` and answers the unknown host calls it counted.
fn counted(method: &str) -> Vec<(String, usize)> {
    let fixture =
        javac_fixture::Fixture::build("host-list-iterator-counted", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let _ = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        method,
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    host.unknown_host_calls()
}

/// `List.of(...).listIterator()` is not modelled (the host's value does not
/// say which JDK list it is): the call is counted, not answered.
#[test]
fn list_of_list_iterators_are_counted() {
    assert_eq!(
        counted("listOfIterator"),
        vec![(
            "java/util/List#listIterator()Ljava/util/ListIterator;".to_owned(),
            1
        )]
    );
}
