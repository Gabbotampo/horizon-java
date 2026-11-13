//! The metafactory's boxing and unboxing between a functional interface and
//! a method reference's target (`lambda_boxing.rs`).
//!
//! Before this, the first three cases stopped the frame with a type mismatch
//! (a boxed marker in an `int` slot): `Point::new` through a `BiFunction`,
//! `int plusOne(int)` through a `Function<Integer, Integer>`, and an
//! `Integer`-returning target through `ToIntFunction` or a mod's own
//! `int`-returning interface.
//!
//! The widening cases (an independent review's programs) answered `7`
//! where Java prints `7.0`, uncounted, or stopped with a type mismatch.
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
import java.util.function.DoubleSupplier;
import java.util.function.Function;
import java.util.function.IntFunction;
import java.util.function.LongFunction;
import java.util.function.Supplier;
import java.util.function.ToDoubleFunction;
import java.util.function.ToIntFunction;
import java.util.function.ToLongFunction;

public final class Probe {
    record Point(String name, int x) { }

    static final class Holder {
        Integer boxed() { return 41; }
    }

    /// A mod's own functional interfaces, erased and primitive.
    interface Maker<T> { T make(int value); }
    interface Measure { int measure(String text); }
    interface DblMaker { double make(); }

    static int plusOne(int value) { return value + 1; }
    static long seven() { return 7L; }
    static Integer boxedLength(String text) { return text.length(); }
    static int anInt() { return 7; }
    static String takesLong(long value) { return "L" + (value << 33); }
    static String takesDouble(double value) { return "D" + value; }
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

    /// An `int` result widened to the interface's `double`.
    public static String widenedResult() {
        DoubleSupplier supplier = Probe::anInt;
        DblMaker maker = Probe::anInt;
        return supplier.getAsDouble() + "|" + maker.make();
    }

    /// An `Integer` result unboxed, then widened to `long` and `double`.
    public static String unboxedThenWidenedResult() {
        ToLongFunction<Holder> asLong = Holder::boxed;
        ToDoubleFunction<Holder> asDouble = Holder::boxed;
        return (asLong.applyAsLong(new Holder()) << 33) + "|" + asDouble.applyAsDouble(new Holder());
    }

    /// Arguments widened (and unboxed first) into the target's parameters.
    public static String widenedArguments() {
        IntFunction<String> fromInt = Probe::takesLong;
        LongFunction<String> fromLong = Probe::takesDouble;
        Function<Integer, String> fromBox = Probe::takesLong;
        return fromInt.apply(3) + "|" + fromLong.apply(5L) + "|" + fromBox.apply(2);
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

/// An `int` target behind `DoubleSupplier` and a mod's `double make()`.
#[test]
fn a_primitive_result_is_widened_to_the_interface_return() {
    assert_eq!(exact("widenedResult"), "7.0|7.0");
}

/// `Integer boxed()` behind `ToLongFunction` and `ToDoubleFunction`.
#[test]
fn a_boxed_result_is_unboxed_then_widened() {
    assert_eq!(exact("unboxedThenWidenedResult"), "352187318272|41.0");
}

/// `int` → `long`, `long` → `double`, and `Integer` → `int` → `long`.
#[test]
fn arguments_are_unboxed_then_widened_to_the_target_parameters() {
    assert_eq!(exact("widenedArguments"), "L25769803776|D5.0|L17179869184");
}
