//! `PriorityQueue`'s fail-fast iterator, `forEach` and `removeIf`
//! (`priority_queue.rs`, `iteration_guard.rs`).
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
    /// The reviewer's program on #110.
    public static String pqLoopAdd() {
        PriorityQueue<Integer> pq = new PriorityQueue<>(List.of(5, 3, 1));
        StringBuilder sb = new StringBuilder();
        try { for (Integer x : pq) { sb.append(x); pq.add(0); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + pq.size();
    }

    /// A poll inside the loop: also structural.
    public static String pqLoopPoll() {
        PriorityQueue<Integer> pq = new PriorityQueue<>(List.of(4, 2, 6, 1));
        StringBuilder sb = new StringBuilder();
        try { for (Integer x : pq) { sb.append(x); if (x == 2) pq.poll(); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + pq;
    }

    /// Adding during the last step: `hasNext` reads the live size, so the
    /// next `next` throws.
    public static String pqLoopAddLast() {
        PriorityQueue<Integer> pq = new PriorityQueue<>(List.of(1, 2));
        StringBuilder sb = new StringBuilder();
        try { for (Integer x : pq) { sb.append(x); if (x == 2) pq.add(9); } sb.append("no"); }
        catch (ConcurrentModificationException e) { sb.append("CME"); }
        return sb + "" + pq;
    }

    /// `forEach` that does not change the queue.
    public static String pqForEachPlain() {
        PriorityQueue<Integer> pq = new PriorityQueue<>(List.of(5, 3, 1, 4));
        StringBuilder sb = new StringBuilder();
        pq.forEach(x -> sb.append(x.intValue()));
        return sb.toString();
    }

    /// `removeIf` whose predicate adds: CME before compaction.
    public static String pqRemoveIfAdds() {
        PriorityQueue<Integer> pq = new PriorityQueue<>(List.of(5, 3, 1));
        try { pq.removeIf(x -> { if (x == 1) pq.add(7); return x == 3; }); return "no" + pq; }
        catch (ConcurrentModificationException e) { return "CME" + pq; }
    }

    /// `forEach` that polls: counted (the JDK reads its captured array).
    public static String pqForEachPoll() {
        PriorityQueue<Integer> pq = new PriorityQueue<>(List.of(5, 3, 1, 4));
        StringBuilder sb = new StringBuilder();
        try { pq.forEach(x -> { sb.append(x); if (x == 1) pq.poll(); }); sb.append("no"); }
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

/// The reviewer's program on #110: adding while iterating throws on the
/// next step (`1CME4`), as `Itr.next` checks `modCount`; `removeIf` checks
/// it before compacting.
#[test]
fn priority_queue_iteration_is_fail_fast() {
    assert_exact(
        "priority-queue-iteration-is-fail-fast",
        &[
            ("pqLoopAdd", "1CME4"),
            ("pqLoopPoll", "12CME[2, 4, 6]"),
            ("pqLoopAddLast", "12CME[1, 2, 9]"),
            ("pqForEachPlain", "1354"),
            ("pqRemoveIfAdds", "CME[1, 3, 5, 7]"),
        ],
    );
}

/// `forEach` reads the array it captured, which a poll has sifted (JDK 25
/// threw a `NullPointerException` on the emptied slot): counted.
#[test]
fn for_each_that_polls_is_counted() {
    let fixture = fixture("for-each-that-polls-is-counted");
    let cases: &[(&str, &[(&str, usize)])] = &[(
        "pqForEachPoll",
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
