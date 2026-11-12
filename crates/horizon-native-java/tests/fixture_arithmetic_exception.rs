//! Integer division by zero and `Math`'s exact operations throw a catchable
//! `java.lang.ArithmeticException`, with the JDK's messages, rather than
//! ending the run with an interpreter error.

/// Each `Probe` method's text under `java` (JDK 25), pasted verbatim.
const EXPECTED: &[(&str, &str)] = &[
    ("intDivide", "AE:/ by zero"),
    ("intRemainder", "AE:/ by zero"),
    ("longDivide", "AE:/ by zero"),
    ("longRemainder", "AE:/ by zero"),
    ("floorDivInt", "AE:/ by zero"),
    ("floorModInt", "AE:/ by zero"),
    ("floorDivLong", "AE:/ by zero"),
    ("floorModLong", "AE:/ by zero"),
    ("addExactInt", "AE:integer overflow"),
    ("addExactLong", "AE:long overflow"),
    (
        "caughtAsRuntime",
        "RuntimeException:java.lang.ArithmeticException",
    ),
];

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{NativeError, StdHost};

const PROBE: &str = r#"package demo;

public final class Probe {
    static int zero = 0;
    static long zeroLong = 0L;

    public static String intDivide() {
        try { return "" + (7 / zero); } catch (ArithmeticException e) { return "AE:" + e.getMessage(); }
    }

    public static String intRemainder() {
        try { return "" + (7 % zero); } catch (ArithmeticException e) { return "AE:" + e.getMessage(); }
    }

    public static String longDivide() {
        try { return "" + (7L / zeroLong); } catch (ArithmeticException e) { return "AE:" + e.getMessage(); }
    }

    public static String longRemainder() {
        try { return "" + (7L % zeroLong); } catch (ArithmeticException e) { return "AE:" + e.getMessage(); }
    }

    public static String floorDivInt() {
        try { return "" + Math.floorDiv(7, zero); } catch (ArithmeticException e) { return "AE:" + e.getMessage(); }
    }

    public static String floorModInt() {
        try { return "" + Math.floorMod(7, zero); } catch (ArithmeticException e) { return "AE:" + e.getMessage(); }
    }

    public static String floorDivLong() {
        try { return "" + Math.floorDiv(7L, zeroLong); } catch (ArithmeticException e) { return "AE:" + e.getMessage(); }
    }

    public static String floorModLong() {
        try { return "" + Math.floorMod(7L, zeroLong); } catch (ArithmeticException e) { return "AE:" + e.getMessage(); }
    }

    public static String addExactInt() {
        try { return "" + Math.addExact(Integer.MAX_VALUE, 1); } catch (ArithmeticException e) { return "AE:" + e.getMessage(); }
    }

    public static String addExactLong() {
        try { return "" + Math.addExact(Long.MAX_VALUE, 1L); } catch (ArithmeticException e) { return "AE:" + e.getMessage(); }
    }

    public static String caughtAsRuntime() {
        try { return "" + (1 / zero); } catch (RuntimeException e) { return "RuntimeException:" + e.getClass().getName(); }
    }

    public static String uncaught() {
        return "" + (1 / zero);
    }
}
"#;

#[test]
fn a_caught_arithmetic_exception_reaches_the_handler_with_the_jdk_message() {
    let fixture = javac_fixture::Fixture::build("arithmetic", &[("demo/Probe.java", PROBE)]);
    for (method, expected) in EXPECTED {
        assert_eq!(fixture.text(method), *expected, "{method}");
    }
}

#[test]
fn an_uncaught_division_by_zero_surfaces_as_the_java_exception() {
    let fixture =
        javac_fixture::Fixture::build("arithmetic-uncaught", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let result = fixture.jar.invoke_method_with_args(
        "demo/Probe",
        "uncaught",
        "()Ljava/lang/String;",
        None,
        &[],
        &mut host,
    );
    assert!(
        matches!(
            &result,
            Err(NativeError::UncaughtException { class, message, .. })
                if class == "java/lang/ArithmeticException"
                    && message.as_deref() == Some("/ by zero")
        ),
        "{result:?}"
    );
}
