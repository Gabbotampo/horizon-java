//! The re-check's collision cases (`jar_throwables.rs`): an exception the
//! host raises at the same pc as an earlier `athrow` (in an overload, or a
//! `throw null` at the `athrow` itself), within one execution after a static
//! initializer threw or in the next execution on the same thread, is a new
//! object, never the earlier throw's. Every throw's key is unique.
//!
//! Each expected line is OpenJDK 25.0.5's output for the same probe method
//! (`support/jdk_differential.rs`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
#[path = "support/jdk_differential.rs"]
mod jdk_differential;

fn check(cases: &[(&str, &str)]) {
    jdk_differential::check("jar-throwables-collisions", PROBE, cases);
}

const PROBE: &str = r#"package demo;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.NoSuchElementException;

/// The re-check's collision cases: a later, unrelated exception must never
/// receive an earlier throw's object.
public final class Probe {
    static void t(RuntimeException e) { throw e; }
    static void t(List<String> l) { l.clear(); }
    static void t(Iterator<String> it) { it.next(); }
    static void npeThrower(NullPointerException e) { throw e; }

    static UnsupportedOperationException CLINIT_UOE;
    static NoSuchElementException CLINIT_NSE;
    static NullPointerException CLINIT_NPE;

    static final class Holder3 { static int x; static { CLINIT_UOE = new UnsupportedOperationException(); t(CLINIT_UOE); } }
    static final class Holder4 { static int x; static { CLINIT_NSE = new NoSuchElementException(); t(CLINIT_NSE); } }
    static final class Holder5 { static int x; static { CLINIT_NPE = new NullPointerException("Cannot throw exception"); npeThrower(CLINIT_NPE); } }

    static String touch(Runnable r) {
        try { r.run(); return "no"; } catch (ExceptionInInitializerError e) { return "EIIE"; }
    }

    public static String overloadSamePcUoe() {
        String first = touch(() -> { int x = Holder3.x; });
        try {
            t(List.of("a"));
            return "no";
        } catch (UnsupportedOperationException e) {
            return first + "|" + (e == CLINIT_UOE) + "|" + e.getMessage();
        }
    }

    public static String overloadSamePcNse() {
        String first = touch(() -> { int x = Holder4.x; });
        try {
            t(new ArrayList<String>().iterator());
            return "no";
        } catch (NoSuchElementException e) {
            return first + "|" + (e == CLINIT_NSE) + "|" + e.getMessage();
        }
    }

    public static String staleFromClinitThenThrowNull() {
        String first = touch(() -> { int x = Holder5.x; });
        try {
            npeThrower(null);
            return "no";
        } catch (NullPointerException e) {
            return first + "|" + (e == CLINIT_NPE);
        }
    }

    /// Run E: uncaught, on its own.
    public static String throwUncaughtUoe() { t(new UnsupportedOperationException()); return "no"; }

    /// Run F, after E on the same thread.
    public static String hostUoeAtTheSamePc() {
        UnsupportedOperationException mine = new UnsupportedOperationException();
        try {
            t(List.of("a"));
            return "no";
        } catch (UnsupportedOperationException e) {
            return e.getMessage() + "|" + (e == mine);
        }
    }

    /// Run A: uncaught, on its own.
    public static String throwUncaughtNpe() { npeThrower(new NullPointerException("Cannot throw exception")); return "no"; }

    /// Run B, after A on the same thread: `throw null` at the same `athrow`.
    public static String throwNullAtTheSameAthrow() {
        NullPointerException mine = new NullPointerException("mine");
        try {
            npeThrower(null);
            return "no";
        } catch (NullPointerException e) {
            return String.valueOf(e == mine);
        }
    }

    public static String[] all() {
        return new String[] {"overloadSamePcUoe", "overloadSamePcNse", "staleFromClinitThenThrowNull", "hostUoeAtTheSamePc",
                "throwNullAtTheSameAthrow"};
    }

    public static void main(String[] args) throws Exception {
        try { throwUncaughtUoe(); } catch (RuntimeException ignored) { }
        try { throwUncaughtNpe(); } catch (RuntimeException ignored) { }
        for (String name : all()) System.out.println(name + "\t" + Probe.class.getMethod(name).invoke(null));
    }
}
"#;

#[test]
fn in_one_execution_runs_as_jdk_25_does() {
    check(&[
        ("overloadSamePcUoe", "EIIE|false|null"),
        ("overloadSamePcNse", "EIIE|false|null"),
        ("staleFromClinitThenThrowNull", "EIIE|false"),
    ]);
}

/// Run `first` (an uncaught throw) and then `second` on fresh hosts, one
/// after the other on this thread, as entrypoints run.
fn after_uncaught(first: &str, second: &str) -> horizon_native_java::Value {
    let fixture = javac_fixture::Fixture::build(
        "jar-throwables-collisions-runs",
        &[("demo/Probe.java", PROBE)],
    );
    let run = |method: &str| {
        let mut host = fixture.jar.seed_host(horizon_native_java::StdHost::new());
        fixture.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
    };
    assert!(run(first).is_err(), "{first} should end uncaught");
    run(second).unwrap()
}

/// JDK 25: `null|false`. The earlier run's uncaught UOE came from `t`'s
/// `athrow` at pc 1; this run's host UOE surfaces at pc 1 of an overload.
#[test]
fn a_host_exception_at_an_earlier_throws_pc_is_a_new_object() {
    assert_eq!(
        after_uncaught("throwUncaughtUoe", "hostUoeAtTheSamePc"),
        horizon_native_java::Value::Ref(Some("null|false".into()))
    );
}

/// JDK 25: `false`. `throw null` at the very `athrow` an earlier run's
/// uncaught NPE left from.
#[test]
fn throw_null_at_an_earlier_throws_athrow_is_a_new_object() {
    assert_eq!(
        after_uncaught("throwUncaughtNpe", "throwNullAtTheSameAthrow"),
        horizon_native_java::Value::Ref(Some("false".into()))
    );
}
