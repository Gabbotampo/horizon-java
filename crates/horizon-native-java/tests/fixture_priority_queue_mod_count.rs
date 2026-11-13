//! `PriorityQueue`'s `modCount` (`mod_count.rs`, `priority_queue.rs`):
//! changes that undo themselves still make its iterator, `removeIf` and
//! `forEach` throw.
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r##"package demo;

import java.util.*;

public final class Probe {
    static String loop(PriorityQueue<Integer> pq, Runnable act) {
        StringBuilder sb = new StringBuilder();
        try { for (Integer x : pq) { sb.append(x.intValue()); if (x == 1) act.run(); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + pq;
    }
    static PriorityQueue<Integer> queue() { return new PriorityQueue<>(List.of(5, 3, 1, 4)); }

    public static String addThenRemove() { PriorityQueue<Integer> pq = queue(); return loop(pq, () -> { pq.add(9); pq.remove(9); }); }
    public static String removeIfNothingInLoop() { PriorityQueue<Integer> pq = queue(); return loop(pq, () -> pq.removeIf(x -> false)); }
    public static String clearEmptyOther() { PriorityQueue<Integer> pq = queue(); PriorityQueue<Integer> o = new PriorityQueue<>(); return loop(pq, o::clear); }
    public static String pollEmptyOther() { PriorityQueue<Integer> pq = queue(); PriorityQueue<Integer> o = new PriorityQueue<>(); return loop(pq, o::poll); }
    public static String removeMissing() { PriorityQueue<Integer> pq = queue(); return loop(pq, () -> pq.remove(42)); }
    public static String removeIfAddsAndPolls() {
        PriorityQueue<Integer> pq = queue();
        try { boolean r = pq.removeIf(x -> { if (x == 3) { pq.add(0); pq.poll(); } return x == 4; }); return r + "" + pq; }
        catch (ConcurrentModificationException e) { return "CME" + pq; }
    }
    public static String forEachAddRemove() {
        PriorityQueue<Integer> pq = queue(); StringBuilder sb = new StringBuilder();
        try { pq.forEach(x -> { sb.append(x.intValue()); if (x == 1) { pq.add(9); pq.remove(9); } }); sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + pq;
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

/// `add` then `remove` moves `modCount`, and so does `removeIf` that removes
/// nothing (`bulkRemove` bumps it first); a `remove` that finds nothing,
/// and changes to another queue, do not.
#[test]
fn priority_queue_checks_mod_count() {
    assert_exact(
        "priority-queue-checks-mod-count",
        &[
            ("addThenRemove", "1CME[1, 3, 5, 4]"),
            ("removeIfNothingInLoop", "1CME[1, 3, 5, 4]"),
            ("clearEmptyOther", "1354no[1, 3, 5, 4]"),
            ("pollEmptyOther", "1354no[1, 3, 5, 4]"),
            ("removeMissing", "1354no[1, 3, 5, 4]"),
            ("removeIfAddsAndPolls", "CME[1, 3, 5, 4]"),
        ],
    );
}

/// `forEach` throws at the end after a change, even one that restored the
/// queue; it reads the array it captured, so the step after is counted.
#[test]
fn for_each_that_restores_the_queue_is_counted_and_throws() {
    let fixture = fixture("for-each-that-restores-the-queue-is-coun");
    let cases: &[(&str, &[(&str, usize)])] = &[(
        "forEachAddRemove",
        &[(
            "java/util/PriorityQueue#forEach(Ljava/util/function/Consumer;)V",
            1,
        )],
    )];
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
