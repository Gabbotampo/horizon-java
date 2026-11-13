//! `equals` and `hashCode` as the JVM runs them (`object_methods.rs`):
//! records follow `ObjectMethods`, `Objects`' helpers call a jar's own
//! `equals`/`hashCode` as JDK 25 does, and identity hashes are counted.
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `T` logs each `equals` (`e<name>`), `hashCode` (`h<name>`) and
//! `toString` (`s<name>`) call, so a value pins the call sequence as well as
//! the answer.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"package demo;

import java.util.*;

public final class Probe {
    static final StringBuilder log = new StringBuilder();
    /// Logs each call; equal by name, hashed by the name's hash.
    static final class T {
        final String n; T(String n) { this.n = n; }
        @Override public boolean equals(Object o) { log.append('e').append(n); return o instanceof T t && t.n.equals(n); }
        @Override public int hashCode() { log.append('h').append(n); return n.hashCode(); }
        @Override public String toString() { log.append('s').append(n); return n; }
    }
    /// Keeps `Object.equals` and `Object.hashCode`.
    static final class Plain { }
    record Key(int id) { }
    record Ver(int n, String s) { }
    record R3(T a, T b, T c) { }
    record Acc(int x) { public int x() { log.append("acc"); return 99; } }
    record Mixed(int a, String b, long c, boolean d, double e, float f, char g, List<String> h) { }
    record Holder(Plain p) { }
    enum Mode { ON }

    /// The reviewer's record cases on #98 (`36` and `1089` before).
    public static String reviewerCases() {
        return Objects.equals(new Key(1), new Key(1)) + "|" + new Key(5).hashCode()
            + "|" + new Ver(1, "a").hashCode() + "|" + new Key(1).equals(new Key(2));
    }

    public static String recordCallOrder() {
        log.setLength(0);
        R3 r = new R3(new T("a"), new T("b"), new T("c"));
        R3 q = new R3(new T("a"), new T("b"), new T("c"));
        int h = r.hashCode();
        log.append('|').append(r.equals(q)).append('|').append(r).append('|').append(r.equals(r));
        log.append('|').append(r.equals(new R3(new T("a"), new T("x"), new T("c"))));
        R3 nulls = new R3(null, new T("b"), null);
        log.append('|').append(nulls.hashCode()).append('|').append(nulls.equals(new R3(null, new T("b"), null)));
        return h + "|" + log;
    }

    public static String recordReadsFields() {
        log.setLength(0);
        Acc a = new Acc(5);
        return a.hashCode() + "|" + a + "|" + a.equals(new Acc(5)) + "|" + log;
    }

    public static String recordComponentTypes() {
        List<String> z = List.of("z");
        Mixed m = new Mixed(1, "b", 3L, true, 2.5, 1.25f, 'q', z);
        Mixed n = new Mixed(1, "b", 3L, true, 2.5, 1.25f, 'q', z);
        Mixed nan = new Mixed(1, "b", 3L, true, Double.NaN, 1.25f, 'q', z);
        return m.hashCode() + "|" + m + "|" + m.equals(n)
            + "|" + nan.equals(new Mixed(1, "b", 3L, true, Double.NaN, 1.25f, 'q', z))
            + "|" + m.equals(new Mixed(1, "b", 3L, true, -0.0, 1.25f, 'q', z))
            + "|" + m.equals(null) + "|" + m.equals("x");
    }

    public static String objectsHelpers() {
        log.setLength(0);
        T a = new T("a");
        Object i = 7, l = 7L, b = true;
        log.append(Objects.equals(new T("a"), new T("a"))).append(Objects.equals(a, a))
            .append(Objects.equals(null, a)).append('|');
        log.append(Objects.hashCode(new T("k"))).append(Objects.hash(new T("x"), 1, null)).append('|');
        log.append(Objects.hash("a", 1, null)).append(Objects.hashCode(null)).append('|');
        log.append(i.hashCode()).append(l.hashCode()).append(b.hashCode());
        return log.toString();
    }

    /// Answers that are identity hashes, which a run cannot reproduce.
    public static String identityHashes() {
        return Mode.ON.hashCode() + "|" + new Holder(new Plain()).hashCode() + "|"
            + Objects.hash(new Plain()) + "|" + System.identityHashCode(new Plain());
    }
}
"#;

/// Runs each `(method, JDK 25 answer)` and checks that nothing was counted.
fn assert_exact(test: &str, cases: &[(&str, &str)]) {
    let fixture = Fixture::build(test, &[("demo/Probe.java", PROBE)]);
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

/// The reviewer's record cases on #98: `Objects.equals` of equal records,
/// and record hashes (`5`, not `36`; `128`, not `1089`).
#[test]
fn records_compare_and_hash_like_object_methods() {
    assert_exact(
        "object-methods-reviewer",
        &[("reviewerCases", "true|5|128|false")],
    );
}

/// `ObjectMethods`: `hashCode` and `toString` read the components first to
/// last, `equals` last to first after `==` and `instanceof`, from the fields
/// rather than the accessors, with `Objects.equals`/`Objects.hashCode` for a
/// reference component and each primitive type's own hash, equality
/// (`Double.compare`: `NaN` equals `NaN`, `0.0` is not `-0.0`) and text.
#[test]
fn record_members_follow_object_methods_call_by_call() {
    assert_exact(
        "object-methods-records",
        &[
            (
                "recordCallOrder",
                "96354|hahbhc|ecebeatrue|sasbscR3[a=a, b=b, c=c]|true|ecebfalse|hb3038|ebtrue",
            ),
            ("recordReadsFields", "5|Acc[x=5]|true|"),
            (
                "recordComponentTypes",
                "1510046005|Mixed[a=1, b=b, c=3, d=true, e=2.5, f=1.25, g=q, h=[z]]|true|true|false|false|false",
            ),
        ],
    );
}

/// `Objects.equals` (`==` first), `Objects.hashCode`/`hash` and a box's
/// `hashCode` through an `Object`-typed call.
#[test]
fn objects_helpers_call_the_jar_members() {
    assert_exact(
        "object-methods-objects",
        &[(
            "objectsHelpers",
            "eatruetruefalse|hk107hx145142|1230390|771231",
        )],
    );
}

/// An enum constant's hash, a record's over a component that keeps
/// `Object.hashCode`, `Objects.hash` of one and `System.identityHashCode`
/// are identity hashes, different on every JVM run: each is counted rather
/// than answered with a made-up number.
#[test]
fn identity_hashes_are_counted() {
    let fixture = Fixture::build("object-methods-identity", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    fixture.text_on(&mut host, "identityHashes");
    let mut counted = host.unknown_host_calls();
    counted.sort();
    assert_eq!(
        counted,
        [
            ("java/lang/Object#hashCode()I".to_owned(), 2),
            (
                "java/lang/System#identityHashCode(Ljava/lang/Object;)I".to_owned(),
                1
            ),
            ("java/util/Objects#hash([Ljava/lang/Object;)I".to_owned(), 1),
        ]
    );
}
