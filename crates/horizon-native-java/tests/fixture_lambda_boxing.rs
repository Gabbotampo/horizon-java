//! The metafactory's boxing and unboxing between a functional interface and
//! a method reference's target (`lambda_boxing.rs`).
//!
//! Before this, the first three cases stopped the frame with a type mismatch
//! (a boxed marker in an `int` slot): `Point::new` through a `BiFunction`,
//! `int plusOne(int)` through a `Function<Integer, Integer>`, and an
//! `Integer`-returning target through `ToIntFunction` or a mod's own
//! `int`-returning interface.
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"package demo;

import java.util.function.BiFunction;
import java.util.function.Function;
import java.util.function.IntFunction;
import java.util.function.Supplier;
import java.util.function.ToIntFunction;

public final class Probe {
    record Point(String name, int x) { }

    static final class Holder {
        Integer boxed() { return 41; }
    }

    /// A mod's own functional interfaces, erased and primitive.
    interface Maker<T> { T make(int value); }
    interface Measure { int measure(String text); }

    static int plusOne(int value) { return value + 1; }
    static long seven() { return 7L; }
    static Integer boxedLength(String text) { return text.length(); }
    static String describe(Object value) { return value.getClass().getSimpleName() + ":" + value; }

    /// `Point::new` through an erased `BiFunction`: the `Integer` argument is
    /// unboxed into the constructor's `int`.
    public static String unboxedArgument() {
        BiFunction<String, Integer, Point> make = Point::new;
        Point point = make.apply("ruby", 7);
        return point.name() + "|" + (point.x() * 2);
    }

    /// `int plusOne(int)` as a `Function<Integer, Integer>`: the result is
    /// boxed, so it compares equal to an `Integer`.
    public static String boxedResult() {
        Function<Integer, Integer> inc = Probe::plusOne;
        Supplier<Long> seven = Probe::seven;
        Object result = inc.apply(4);
        return result.equals(5) + "|" + result + "|" + seven.get().getClass().getSimpleName();
    }

    /// `Integer boxed()` as a `ToIntFunction`: the result is unboxed.
    public static String unboxedResult() {
        ToIntFunction<Holder> value = Holder::boxed;
        Measure measure = Probe::boxedLength;
        return (value.applyAsInt(new Holder()) + 1) + "|" + (measure.measure("abc") * 10);
    }

    /// An `int` passed to an `Object` parameter is boxed to `Integer`.
    public static String boxedArgument() {
        IntFunction<String> plain = Probe::describe;
        Maker<String> maker = Probe::describe;
        return plain.apply(5) + "|" + maker.make(6);
    }
}
"#;

/// Runs `method`, checking that nothing was counted as unknown.
fn exact(method: &str) -> String {
    let fixture = Fixture::build(
        &format!("lambda-boxing-{method}"),
        &[("demo/Probe.java", PROBE)],
    );
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = fixture.text_on(&mut host, method);
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{method}: {:?}",
        host.unknown_host_calls()
    );
    text
}

#[test]
fn an_erased_argument_is_unboxed_into_a_primitive_parameter() {
    assert_eq!(exact("unboxedArgument"), "ruby|14");
}

#[test]
fn a_primitive_result_is_boxed_for_a_reference_return() {
    assert_eq!(exact("boxedResult"), "true|5|Long");
}

/// Through the JDK's `ToIntFunction` and through a mod's own interface.
#[test]
fn a_boxed_result_is_unboxed_for_a_primitive_return() {
    assert_eq!(exact("unboxedResult"), "42|30");
}

/// Already right before this change; pinned so the conversions above never
/// unbox into an `Object` parameter.
#[test]
fn a_primitive_argument_reaches_an_object_parameter_as_its_box() {
    assert_eq!(exact("boxedArgument"), "Integer:5|Integer:6");
}
