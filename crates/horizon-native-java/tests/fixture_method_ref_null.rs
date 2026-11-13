//! Method references applied to `null`, a `null` bound receiver, and array
//! constructor references with a negative size, as JDK 25 throws them.
//!
//! Each expected line is OpenJDK 25.0.5's output for the same probe method
//! (`javac --release 21`, `java -cp out demo.Probe`), pasted verbatim; a case
//! must also count no unknown host call. A method reference's own null check
//! runs in the JDK's generated code, so its `NullPointerException` has no
//! helpful message (`null`), unlike a lambda body's.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

const PROBE: &str = r#"package demo;

import java.util.*;
import java.util.function.*;
import java.util.stream.*;

public final class Probe {
    static String err(Throwable e) { return e.getClass().getName() + ":" + e.getMessage(); }

    interface Run { String run() throws Throwable; }

    static String attempt(Run r) {
        try { return "ok:" + r.run(); } catch (Throwable e) { return err(e); }
    }

    static final class Item {
        final String name;
        Item(String name) { this.name = name; }
        String name() { return name; }
        static String label(Item i) { return "L" + i.name; }
    }

    public static String unboundJdk() {
        Function<String, Integer> f = String::length;
        return attempt(() -> String.valueOf(f.apply(null)));
    }

    public static String unboundJdkInStream() {
        return attempt(() -> String.valueOf(Stream.of("a").map(x -> (String) null).map(String::length).toList()));
    }

    public static String unboundJdkTwoArgs() {
        BiFunction<String, String, Boolean> f = String::startsWith;
        return attempt(() -> String.valueOf(f.apply(null, "a")));
    }

    public static String unboundJar() {
        Function<Item, String> f = Item::name;
        return attempt(() -> f.apply(null));
    }

    public static String staticJarNullArg() {
        Function<Item, String> f = Item::label;
        return attempt(() -> f.apply(null));
    }

    public static String boundNull() {
        String s = null;
        return attempt(() -> { Supplier<Integer> f = s::length; return "made"; });
    }

    public static String boundJarNull() {
        Item i = null;
        return attempt(() -> { Supplier<String> f = i::name; return "made"; });
    }

    public static String boundOk() {
        String s = "abc";
        Supplier<Integer> f = s::length;
        return attempt(() -> String.valueOf(f.get()));
    }

    public static String arrayCtorNegative() {
        IntFunction<int[]> f = int[]::new;
        IntFunction<String[]> g = String[]::new;
        return attempt(() -> String.valueOf(f.apply(-1).length)) + "|" + attempt(() -> String.valueOf(g.apply(-2).length)) + "|" + attempt(() -> String.valueOf(g.apply(3).length));
    }

    public static String ctorRef() {
        Function<String, Item> f = Item::new;
        return attempt(() -> f.apply(null).name());
    }

    public static String unboxingRef() {
        ToIntFunction<Integer> f = Integer::intValue;
        Function<Integer, String> g = Integer::toHexString;
        return attempt(() -> String.valueOf(f.applyAsInt(null))) + "|" + attempt(() -> g.apply(null));
    }

    public static String lambdaNull() {
        Function<String, Integer> f = x -> x.length();
        return attempt(() -> String.valueOf(f.apply(null)));
    }


    public static String negativeArrays() {
        int n = -1, m = -3, z = 0;
        return attempt(() -> String.valueOf(new int[n].length)) + "|" + attempt(() -> String.valueOf(new String[n].length))
            + "|" + attempt(() -> String.valueOf(new int[2][m].length)) + "|" + attempt(() -> String.valueOf(new int[n][m].length))
            + "|" + attempt(() -> String.valueOf(new int[z][m].length)) + "|" + attempt(() -> String.valueOf(java.lang.reflect.Array.getLength(java.lang.reflect.Array.newInstance(String.class, n))))
            + "|" + attempt(() -> { try { int[] a = new int[n]; return "no"; } catch (NegativeArraySizeException e) { return "caught:" + e.getMessage(); } });
    }

    public static String nullPrimitiveArgs() {
        Function<Integer, Integer> abs = Math::abs;
        BiFunction<String, Integer, Character> at = String::charAt;
        return attempt(() -> String.valueOf(abs.apply(null))) + "|" + attempt(() -> String.valueOf(at.apply("ab", null)));
    }

}
"#;

#[test]
fn method_references_throw_as_jdk_25_does() {
    let fixture = javac_fixture::Fixture::build("method-ref-null", &[("demo/Probe.java", PROBE)]);
    let cases: &[(&str, &str)] = &[
        (
            "arrayCtorNegative",
            "java.lang.NegativeArraySizeException:-1|java.lang.NegativeArraySizeException:-2|ok:3",
        ),
        ("boundJarNull", "java.lang.NullPointerException:null"),
        ("boundNull", "java.lang.NullPointerException:null"),
        ("boundOk", "ok:3"),
        ("ctorRef", "ok:null"),
        (
            "negativeArrays",
            "java.lang.NegativeArraySizeException:-1|java.lang.NegativeArraySizeException:-1|java.lang.NegativeArraySizeException:-3|java.lang.NegativeArraySizeException:-1|java.lang.NegativeArraySizeException:-3|java.lang.NegativeArraySizeException:-1|ok:caught:-1",
        ),
        (
            "nullPrimitiveArgs",
            "java.lang.NullPointerException:null|java.lang.NullPointerException:null",
        ),
        ("unboundJar", "java.lang.NullPointerException:null"),
        ("unboundJdk", "java.lang.NullPointerException:null"),
        ("unboundJdkInStream", "java.lang.NullPointerException:null"),
        ("unboundJdkTwoArgs", "java.lang.NullPointerException:null"),
        (
            "unboxingRef",
            "java.lang.NullPointerException:null|java.lang.NullPointerException:null",
        ),
    ];
    let mut failures = Vec::new();
    for (method, expected) in cases {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let answer = fixture.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        let unknown = host.unknown_host_calls();
        let text = match &answer {
            Ok(Value::Ref(Some(text))) => text.clone(),
            other => format!("{other:?}"),
        };
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
