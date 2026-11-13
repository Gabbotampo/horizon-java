//! `Field#canAccess(Object)` run in the interpreter against the JDK's own answers.
//!
//! `PROBE` and the `demo/` classes beside it (a holder with public, private,
//! protected and package-private fields, a nested class, a package-private class,
//! a class in another package and a subclass there) were compiled with `javac
//! --release 21` and run with `java` (OpenJDK 27); the output is pasted verbatim
//! into `EXPECTED`. No expected value is derived from the runtime.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

const PROBE: &str = r##"package demo;

import java.lang.reflect.Field;

public final class Probe {
    static String failure(Throwable error) {
        return error.getClass().getSimpleName() + ":" + error.getMessage();
    }

    public static String canAccessHere() throws Exception {
        Holder2 holder = new Holder2();
        StringBuilder out = new StringBuilder();
        for (String name : new String[] {"pub", "priv", "prot", "pkg"}) {
            out.append(Holder2.class.getDeclaredField(name).canAccess(holder)).append(',');
        }
        out.append(Holder2.class.getDeclaredField("spriv").canAccess(null)).append(',');
        out.append(Hidden.class.getDeclaredField("open").canAccess(new Hidden())).append(',');
        out.append(Holder2.Nest.peek());
        return out.toString();
    }

    public static String canAccessElsewhere() throws Exception {
        return demo.sub.Other.check(new Holder2(), new Hidden())
            + "|" + demo.sub.Child.check();
    }

    public static String canAccessErrors() throws Exception {
        StringBuilder out = new StringBuilder();
        Field pub = Holder2.class.getDeclaredField("pub");
        Field spub = Holder2.class.getDeclaredField("spub");
        try { pub.canAccess(null); } catch (IllegalArgumentException e) { out.append(failure(e)); }
        out.append('|');
        try { spub.canAccess(new Holder2()); } catch (IllegalArgumentException e) { out.append(failure(e)); }
        out.append('|');
        try { pub.canAccess(new Hidden()); } catch (IllegalArgumentException e) { out.append(failure(e)); }
        return out.toString();
    }

    /** `setAccessible` on one copy leaves a fresh copy's `override` unset. */
    public static String touched() throws Exception {
        Holder2.class.getDeclaredField("priv").setAccessible(true);
        return String.valueOf(Holder2.class.getDeclaredField("priv").canAccess(new Holder2()));
    }
}
"##;

const HOLDER: &str = r##"package demo;

public class Holder2 {
    public int pub;
    private int priv;
    protected int prot;
    int pkg;
    public static int spub;
    private static int spriv;
    protected static int sprot;

    static final class Nest {
        static String peek() throws Exception {
            return String.valueOf(Holder2.class.getDeclaredField("priv").canAccess(new Holder2()));
        }
    }
}
"##;

const HIDDEN: &str = r##"package demo;

class Hidden {
    public int open;
}
"##;

const OTHER: &str = r##"package demo.sub;

import demo.Holder2;

public final class Other {
    public static String check(Holder2 holder, Object hidden) throws Exception {
        StringBuilder out = new StringBuilder();
        for (String name : new String[] {"pub", "priv", "prot", "pkg"}) {
            out.append(Holder2.class.getDeclaredField(name).canAccess(holder)).append(',');
        }
        out.append(Holder2.class.getDeclaredField("sprot").canAccess(null)).append(',');
        out.append(hidden.getClass().getDeclaredField("open").canAccess(hidden));
        return out.toString();
    }
}
"##;

const CHILD: &str = r##"package demo.sub;

import demo.Holder2;

public final class Child extends Holder2 {
    public static String check() throws Exception {
        java.lang.reflect.Field prot = Holder2.class.getDeclaredField("prot");
        java.lang.reflect.Field sprot = Holder2.class.getDeclaredField("sprot");
        java.lang.reflect.Field pkg = Holder2.class.getDeclaredField("pkg");
        return prot.canAccess(new Child()) + "," + prot.canAccess(new Holder2()) + ","
            + sprot.canAccess(null) + "," + pkg.canAccess(new Child());
    }
}
"##;

/// `java -cp <classes> Oracle <method>...`, one line per `Probe` method,
/// verbatim.
const EXPECTED: &[(&str, &str)] = &[
    ("canAccessHere", r#"true,false,true,true,false,true,true"#),
    (
        "canAccessElsewhere",
        r#"true,false,false,false,false,false|true,false,true,false"#,
    ),
    (
        "canAccessErrors",
        r#"IllegalArgumentException:null object for public int demo.Holder2.pub|IllegalArgumentException:non-null object for public static int demo.Holder2.spub|IllegalArgumentException:object is not an instance of demo.Holder2"#,
    ),
];

fn fixture(test: &str) -> javac_fixture::Fixture {
    javac_fixture::Fixture::build(
        test,
        &[
            ("demo/Probe.java", PROBE),
            ("demo/Holder2.java", HOLDER),
            ("demo/Hidden.java", HIDDEN),
            ("demo/sub/Other.java", OTHER),
            ("demo/sub/Child.java", CHILD),
        ],
    )
}

/// Run one static `String demo.<class>.<method>()` and return its text and
/// the host's unknown calls.
fn run(
    fixture: &javac_fixture::Fixture,
    class: &str,
    method: &str,
) -> (String, Vec<(String, usize)>) {
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = match fixture
        .jar
        .invoke_method_with_args(class, method, "()Ljava/lang/String;", None, &[], &mut host)
        .unwrap_or_else(|error| panic!("{method} failed: {error}"))
    {
        Value::Ref(Some(text)) => text,
        other => panic!("{method} did not return a string: {other:?}"),
    };
    (text, host.unknown_host_calls())
}

/// Every oracle line, answered with no unknown host call left.
#[test]
fn can_access_matches_the_jdk() {
    let fixture = fixture("can-access");
    for (method, text) in EXPECTED {
        let (answer, unknown) = run(&fixture, "demo/Probe", method);
        assert_eq!(answer, *text, "Probe.{method}");
        assert!(unknown.is_empty(), "Probe.{method} counted {unknown:?}");
    }
}

/// `setAccessible(true)` on one `Field` copy does not touch another: a fresh
/// `getDeclaredField` copy of the same private field answers `false` (OpenJDK
/// 27 prints `false` for `touched()`). Before `reflect_override` told copies
/// apart, both calls were counted.
#[test]
fn can_access_after_set_accessible_on_another_copy() {
    let fixture = fixture("can-access-touched");
    let (answer, unknown) = run(&fixture, "demo/Probe", "touched");
    assert_eq!(answer, "false");
    assert!(unknown.is_empty(), "touched counted {unknown:?}");
}
