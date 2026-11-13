//! `Collections.emptyList()`/`emptySet()`/`emptyMap()` mutators JDK 25 does
//! not refuse (`java_immutable.rs`).
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
import java.util.function.*;

public final class Probe {
    static String run(Supplier<Object> call) {
        try { return String.valueOf(call.get()); }
        catch (UnsupportedOperationException e) { return "UOE"; }
        catch (NullPointerException e) { return "NPE"; }
    }

    // The reviewer's program on #109, and its neighbours.
    public static String emptyMapReplaceAll() { Map<String, Integer> m = Collections.emptyMap(); return run(() -> { m.replaceAll((k, v) -> v); return m; }); }
    public static String emptyMapReplaceAllNull() { Map<String, Integer> m = Collections.emptyMap(); return run(() -> { m.replaceAll(null); return m; }); }
    public static String emptyMapRemoveClear() { Map<String, Integer> m = Collections.emptyMap(); return run(() -> { Integer r = m.remove("a"); m.clear(); return r + "" + m; }); }
    public static String emptyMapPut() { Map<String, Integer> m = Collections.emptyMap(); return run(() -> m.put("a", 1)); }
    public static String emptyMapComputeIfPresent() { Map<String, Integer> m = Collections.emptyMap(); return run(() -> m.computeIfPresent("a", (k, v) -> v)); }
    public static String emptyListMembers() {
        List<Integer> l = Collections.emptyList();
        return run(() -> { l.replaceAll(x -> x); l.sort(null); boolean a = l.removeIf(x -> true); boolean b = l.remove((Object) 1); boolean c = l.removeAll(List.of(1)); l.clear(); return a + "" + b + c + l; });
    }
    public static String emptyListAdd() { List<Integer> l = Collections.emptyList(); return run(() -> l.add(1)); }
    public static String emptySetMembers() { Set<String> s = Collections.emptySet(); return run(() -> s.removeIf(x -> true) + "" + s.retainAll(List.of("a")) + s); }
    public static String listOfReplaceAll() { List<Integer> l = List.of(); return run(() -> { l.replaceAll(x -> x); return l; }); }
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

/// The reviewer's program on #109: `Collections.emptyMap().replaceAll` only
/// checks its function; `removeIf`/`sort`/`remove`/`clear` on the empty
/// collections find nothing to do; `put`, `add` and `computeIfPresent`
/// refuse, and so does `List.of().replaceAll`.
#[test]
fn empty_collections_refuse_only_what_jdk_25_refuses() {
    assert_exact(
        "empty-collections-refuse-only-what-jdk-2",
        &[
            ("emptyMapReplaceAll", "{}"),
            ("emptyMapReplaceAllNull", "NPE"),
            ("emptyMapRemoveClear", "null{}"),
            ("emptyMapPut", "UOE"),
            ("emptyMapComputeIfPresent", "UOE"),
            ("emptyListMembers", "falsefalsefalse[]"),
            ("emptyListAdd", "UOE"),
            ("emptySetMembers", "falsefalse[]"),
            ("listOfReplaceAll", "UOE"),
        ],
    );
}
