//! The re-review's cases for jar exceptions (`jar_throwables.rs`): a later
//! catch never receives an earlier, unrelated throw's object; the host throws a
//! jar exception object as itself (`Optional.orElseThrow`); `instanceof` and
//! `checkcast` walk the whole JDK throwable hierarchy; `Throwable.toString`
//! dispatches `getLocalizedMessage`/`getMessage` overrides; and an exception
//! from a comparator under a list sort reaches the calling frame's handlers.
//!
//! Each expected line is OpenJDK 25.0.5's output for the same probe method
//! (`support/jdk_differential.rs`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
#[path = "support/jdk_differential.rs"]
mod jdk_differential;

fn check(cases: &[(&str, &str)]) {
    jdk_differential::check("jar-throwables-review", PROBE, cases);
}

const PROBE: &str = r#"package demo;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;
import java.util.Objects;
import java.util.Optional;

/// The re-review's #129 cases, rewritten as probe methods.
public final class Probe {
    static class MidIAE extends IllegalArgumentException { MidIAE(String m) { super(m); } }
    static final class LeafIAE extends MidIAE { LeafIAE(String m) { super(m); } }
    static class Base extends RuntimeException { Base(String m) { super(m); } }
    static final class Sub extends Base { Sub(String m) { super(m); } }
    static final class NFE2 extends NumberFormatException { NFE2(String m) { super(m); } }
    static final class Fatal extends Error { Fatal(String m) { super(m); } }
    static final class Oops extends Exception { Oops(String m) { super(m); } }

    static final class MsgOverride extends RuntimeException {
        MsgOverride(String m) { super(m); }
        @Override public String getMessage() { return "over:" + super.getMessage() + "/y"; }
    }

    static final class LocalOverride extends RuntimeException {
        LocalOverride(String m) { super(m); }
        @Override public String getLocalizedMessage() { return "local"; }
    }

    static String name(Throwable t) { return t.getClass().getName() + ":" + t.getMessage(); }

    /// Run on its own, uncaught: the next case must not see this object.
    public static String throwUncaughtNpe() { throw new NullPointerException(); }

    public static String hostNpeAfterAnUncaughtOne() {
        NullPointerException mine = new NullPointerException("mine");
        try {
            Objects.requireNonNull(null);
            return "no";
        } catch (NullPointerException e) {
            return e.getMessage() + "|" + (e == mine);
        }
    }

    public static String orElseThrowJarException() {
        try {
            Optional.empty().orElseThrow(() -> new LeafIAE("oet"));
            return "no";
        } catch (MidIAE e) {
            return name(e);
        }
    }

    public static String instanceOfFullChain() {
        Object sub = new Sub("s");
        Object nfe = new NFE2("n");
        Object fatal = new Fatal("f");
        Object oops = new Oops("o");
        String caught;
        try {
            throw new Sub("t");
        } catch (Throwable t) {
            caught = (t instanceof Exception) + "," + (t instanceof RuntimeException) + "," + (t instanceof Base);
        }
        return (sub instanceof RuntimeException) + "," + (sub instanceof Exception) + "," + (sub instanceof Throwable)
                + "," + (sub instanceof Error) + "|" + (nfe instanceof IllegalArgumentException) + ","
                + (nfe instanceof RuntimeException) + "|" + (fatal instanceof Throwable) + "," + (fatal instanceof Exception)
                + "|" + (oops instanceof Exception) + "," + (oops instanceof RuntimeException) + "|" + caught;
    }

    public static String checkcastRejects() {
        StringBuilder out = new StringBuilder();
        Object leaf = new LeafIAE("l");
        Object sub = new Sub("s");
        try { IllegalStateException x = (IllegalStateException) leaf; out.append("cast"); } catch (ClassCastException e) { out.append("CCE"); }
        try { Error x = (Error) sub; out.append("|cast"); } catch (ClassCastException e) { out.append("|CCE"); }
        try { RuntimeException x = (RuntimeException) leaf; out.append("|ok:").append(x.getMessage()); } catch (ClassCastException e) { out.append("|CCE"); }
        return out.toString();
    }

    public static String toStringDispatchesMessage() {
        String text;
        try {
            throw new MsgOverride("m");
        } catch (RuntimeException e) {
            text = "" + e;
        }
        return text + "|" + new LocalOverride("x") + "|" + new LocalOverride("x").getMessage() + "|" + new Sub("s");
    }

    static final Comparator<String> BAD = (a, b) -> { throw new IllegalStateException("plain"); };

    public static String sortComparatorThrows() {
        StringBuilder out = new StringBuilder();
        try {
            new ArrayList<>(List.of("b", "a")).sort(BAD);
            out.append("no");
        } catch (IllegalStateException e) {
            out.append(name(e));
        } finally {
            out.append("|finally");
        }
        try {
            Collections.sort(new ArrayList<>(List.of("b", "a")), BAD);
        } catch (RuntimeException e) {
            out.append("|").append(name(e));
        }
        try {
            new ArrayList<>(List.of("b", "a")).sort((a, b) -> { throw new LeafIAE("jar"); });
        } catch (MidIAE e) {
            out.append("|").append(name(e));
        }
        return out.toString();
    }

    public static String[] all() {
        return new String[] {"hostNpeAfterAnUncaughtOne", "orElseThrowJarException", "instanceOfFullChain",
                "checkcastRejects", "toStringDispatchesMessage", "sortComparatorThrows"};
    }

    public static void main(String[] args) throws Exception {
        try { throwUncaughtNpe(); } catch (NullPointerException ignored) { }
        for (String name : all()) System.out.println(name + "\t" + Probe.class.getMethod(name).invoke(null));
    }
}
"#;

#[test]
fn jar_throwables_review_runs_as_jdk_25_does() {
    check(&[
        ("hostNpeAfterAnUncaughtOne", "null|false"),
        ("orElseThrowJarException", "demo.Probe$LeafIAE:oet"),
        (
            "instanceOfFullChain",
            "true,true,true,false|true,true|true,false|true,false|true,true,true",
        ),
        ("checkcastRejects", "CCE|CCE|ok:l"),
        (
            "toStringDispatchesMessage",
            "demo.Probe$MsgOverride: over:m/y|demo.Probe$LocalOverride: local|x|demo.Probe$Sub: s",
        ),
        (
            "sortComparatorThrows",
            "java.lang.IllegalStateException:plain|finally|java.lang.IllegalStateException:plain|demo.Probe$LeafIAE:jar",
        ),
    ]);
}

/// JDK 25: `null|false`. An earlier execution on the same thread ends with an
/// uncaught `NullPointerException`; the next one's host-raised NPE must be a
/// new object, never that earlier throw's (it used to come back as the
/// next run's own `mine`).
#[test]
fn an_uncaught_throw_does_not_reach_a_later_execution() {
    let fixture =
        javac_fixture::Fixture::build("jar-throwables-stale", &[("demo/Probe.java", PROBE)]);
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
    assert!(run("throwUncaughtNpe").is_err());
    assert_eq!(
        run("hostNpeAfterAnUncaughtOne").unwrap(),
        horizon_native_java::Value::Ref(Some("null|false".into()))
    );
}
