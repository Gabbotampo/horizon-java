//! `Iterator.forEachRemaining` on a host collection's iterator runs the jar's
//! action once per remaining element (`collection_callback.rs`); when the
//! action throws, the iterator is left where its JDK 25 class leaves it.
//!
//! Each expected line is OpenJDK 25.0.5's output for the same probe method
//! (`javac --release 21`, `java -cp out demo.Probe`), pasted verbatim; a case
//! must also count no unknown host call. Before this, the host drained the
//! iterator without running the action.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

const PROBE: &str = r#"package demo;

import java.util.*;
import java.util.function.*;

public final class Probe {
    static final StringBuilder LOG = new StringBuilder();

    static void log(String event) { LOG.append(event).append(' '); }

    static String take(Object result) {
        String text = LOG.toString().trim() + " => " + result;
        LOG.setLength(0);
        return text;
    }

    static String err(Throwable e) { return e.getClass().getName() + ":" + e.getMessage(); }

    /// forEachRemaining with an action that throws on 2, then the iterator's state.
    static String throwing(Iterator<Integer> it) {
        try {
            it.forEachRemaining(x -> { log("e" + x); if (x == 2) throw new IllegalStateException("x"); });
        } catch (IllegalStateException e) { log("caught"); }
        String more = String.valueOf(it.hasNext());
        String next;
        try { next = String.valueOf(it.next()); } catch (NoSuchElementException e) { next = "nsee"; }
        return take(more + "," + next);
    }

    public static String listOf() {
        List.of(1, 2).iterator().forEachRemaining(x -> log("e" + x));
        return take("done");
    }

    public static String arrayListAfterNext() {
        Iterator<Integer> it = new ArrayList<>(List.of(1, 2, 3)).iterator();
        it.next();
        it.forEachRemaining(x -> log("e" + x));
        return take(it.hasNext());
    }

    public static String exhaustedAndNull() {
        Iterator<Integer> it = new ArrayList<>(List.of(1)).iterator();
        it.next();
        it.forEachRemaining(x -> log("never"));
        String npe;
        try { new ArrayList<>(List.of(1)).iterator().forEachRemaining(null); npe = "ok"; } catch (NullPointerException e) { npe = "npe"; }
        String npe2;
        try { List.of(1).iterator().forEachRemaining(null); npe2 = "ok"; } catch (NullPointerException e) { npe2 = "npe"; }
        return take(npe + "," + npe2);
    }

    public static String sets() {
        new LinkedHashSet<>(List.of("b", "a")).iterator().forEachRemaining(x -> log("s" + x));
        Map<String, Integer> m = new LinkedHashMap<>();
        m.put("k", 1); m.put("j", 2);
        m.keySet().iterator().forEachRemaining(x -> log("k" + x));
        m.values().iterator().forEachRemaining(x -> log("v" + x));
        return take("done");
    }

    public static String throwArrayList() { return throwing(new ArrayList<>(List.of(1, 2, 3)).iterator()); }
    public static String throwLinkedList() { return throwing(new LinkedList<>(List.of(1, 2, 3)).iterator()); }
    public static String throwListOf() { return throwing(List.of(1, 2, 3).iterator()); }
    public static String throwLinkedHashSet() { return throwing(new LinkedHashSet<>(List.of(1, 2, 3)).iterator()); }
    public static String throwArrayDeque() { return throwing(new ArrayDeque<>(List.of(1, 2, 3)).iterator()); }
    public static String throwTreeSet() { return throwing(new TreeSet<>(List.of(1, 2, 3)).iterator()); }

}
"#;

/// Run `method` on a fresh host: its answer and the calls it counted.
fn run(fixture: &javac_fixture::Fixture, method: &str) -> (String, Vec<(String, usize)>) {
    let mut host = fixture.jar.seed_host(StdHost::new());
    let answer = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        method,
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    let text = match answer {
        Ok(Value::Ref(Some(text))) => text,
        other => format!("{other:?}"),
    };
    (text, host.unknown_host_calls())
}

#[test]
fn host_iterators_run_for_each_remaining_as_jdk_25_does() {
    let fixture = javac_fixture::Fixture::build("iterator-each", &[("demo/Probe.java", PROBE)]);
    let cases: &[(&str, &str)] = &[
        ("listOf", "e1 e2 => done"),
        ("arrayListAfterNext", "e2 e3 => false"),
        ("exhaustedAndNull", " => npe,npe"),
        ("sets", "sb sa kk kj v1 v2 => done"),
        ("throwArrayList", "e1 e2 caught => true,1"),
        ("throwLinkedList", "e1 e2 caught => true,2"),
        ("throwLinkedHashSet", "e1 e2 caught => true,3"),
        ("throwArrayDeque", "e1 e2 caught => false,nsee"),
        ("throwTreeSet", "e1 e2 caught => true,3"),
    ];
    let mut failures = Vec::new();
    for (method, expected) in cases {
        let (text, unknown) = run(&fixture, method);
        if text != *expected || !unknown.is_empty() {
            failures.push(format!(
                "{method}\n  jdk 25: {expected}\n  native: {text}\n  unknown: {unknown:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "differ from JDK 25:\n{}",
        failures.join("\n")
    );
}

/// Counted, not answered: a `List.of` value's host form does not record
/// which JDK list it is (an `ArrayList` from a collector looks the same), and
/// the two leave their cursor in different places when the action throws.
#[test]
fn an_unrecorded_iterator_whose_action_throws_is_counted() {
    let fixture =
        javac_fixture::Fixture::build("iterator-each-counted", &[("demo/Probe.java", PROBE)]);
    let (_, unknown) = run(&fixture, "throwListOf");
    assert_eq!(
        unknown,
        vec![(
            "java/util/Iterator#forEachRemaining(Ljava/util/function/Consumer;)V".to_owned(),
            1
        )]
    );
}
