//! A jar class that extends a JDK exception is thrown, caught and read as
//! Java does (`jar_throwables.rs`): `catch` by its own class or any JDK
//! superclass, `getClass()`/`getMessage()`/`toString()`, its identity and
//! fields across frames, and its cause. It used to arrive at every `catch`
//! as a fresh `java.lang.Throwable` with no message.
//!
//! Each expected line is OpenJDK 25.0.5's output for the same probe method
//! (`support/jdk_differential.rs`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
#[path = "support/jdk_differential.rs"]
mod jdk_differential;

fn check(cases: &[(&str, &str)]) {
    jdk_differential::check("jar-throwables", PROBE, cases);
}

const PROBE: &str = r#"package demo;

public final class Probe {
    static final class MyRTE extends RuntimeException {
        final int code;
        MyRTE(String message, int code) { super(message); this.code = code; }
    }

    static final class MyCCE extends ClassCastException {
        MyCCE(String message) { super(message); }
    }

    static final class MyNPE extends NullPointerException {
        MyNPE() { super(); }
    }

    static final class Wrapped extends IllegalStateException {
        Wrapped(String message, Throwable cause) { super(message, cause); }
    }

    static class Base extends Exception {
        Base(String message) { super(message); }
    }

    static final class Leaf extends Base {
        Leaf(String message) { super(message); }
    }

    static void deep(int n, RuntimeException e) {
        if (n == 0) throw e;
        deep(n - 1, e);
    }

    static void leaf() throws Base { throw new Leaf("leaf"); }

    static String name(Throwable t) { return t.getClass().getName() + ":" + t.getMessage(); }

    public static String sameFrame() {
        try {
            throw new MyRTE("r", 7);
        } catch (RuntimeException e) {
            return name(e);
        }
    }

    public static String acrossFrames() {
        MyRTE thrown = new MyRTE("deep", 3);
        try {
            deep(3, thrown);
            return "no";
        } catch (MyRTE e) {
            return name(e) + "|" + (e == thrown) + "|" + e.code;
        }
    }

    public static String caughtBySuperclass() {
        StringBuilder out = new StringBuilder();
        try { deep(2, new MyCCE("c")); } catch (ClassCastException e) { out.append(name(e)); }
        try { deep(2, new MyNPE()); } catch (NullPointerException e) { out.append("|").append(name(e)); }
        try { leaf(); } catch (Base e) { out.append("|").append(name(e)).append("|").append(e instanceof Leaf); }
        return out.toString();
    }

    public static String notCaughtByUnrelated() {
        try {
            try {
                deep(1, new MyCCE("c"));
            } catch (IllegalStateException e) {
                return "wrong";
            }
        } catch (RuntimeException e) {
            return name(e);
        }
        return "no";
    }

    public static String cause() {
        MyRTE inner = new MyRTE("inner", 1);
        try {
            deep(2, new Wrapped("outer", inner));
            return "no";
        } catch (IllegalStateException e) {
            return name(e) + "|" + name(e.getCause()) + "|" + (e.getCause() == inner);
        }
    }

    public static String text() {
        return new MyRTE("r", 1) + "|" + new MyNPE() + "|" + new IllegalStateException("c") + "|"
                + new java.util.NoSuchElementException(new IndexOutOfBoundsException("i")).getMessage() + "|"
                + new RuntimeException("x", new IllegalStateException("c")).getCause();
    }

    public static String[] all() {
        return new String[] {"sameFrame", "acrossFrames", "caughtBySuperclass", "notCaughtByUnrelated", "cause", "text"};
    }

    public static void main(String[] args) throws Exception {
        for (String name : all()) {
            System.out.println(name + "\t" + Probe.class.getMethod(name).invoke(null));
        }
    }
}
"#;

#[test]
fn jar_throwables_runs_as_jdk_25_does() {
    check(&[
        ("sameFrame", "demo.Probe$MyRTE:r"),
        ("acrossFrames", "demo.Probe$MyRTE:deep|true|3"),
        (
            "caughtBySuperclass",
            "demo.Probe$MyCCE:c|demo.Probe$MyNPE:null|demo.Probe$Leaf:leaf|true",
        ),
        ("notCaughtByUnrelated", "demo.Probe$MyCCE:c"),
        (
            "cause",
            "demo.Probe$Wrapped:outer|demo.Probe$MyRTE:inner|true",
        ),
        (
            "text",
            "demo.Probe$MyRTE: r|demo.Probe$MyNPE|java.lang.IllegalStateException: c|java.lang.IndexOutOfBoundsException: i|java.lang.IllegalStateException: c",
        ),
    ]);
}
