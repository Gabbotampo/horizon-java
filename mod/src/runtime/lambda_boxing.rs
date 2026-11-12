//! The boxing and unboxing `LambdaMetafactory` adds between a functional
//! interface's erased signature and a method reference's target.
//!
//! javac compiles a method reference (`Point::new`, `Holder::boxed`,
//! `Probe::plusOne`) straight to its target. The adaptation between the
//! interface method's descriptor and the target's is the metafactory's job
//! (`LambdaMetafactory` Javadoc, "the linkage-time type adaptations"): a
//! reference argument is unboxed into a primitive parameter, and a result is
//! boxed or unboxed to the interface method's return type. The interpreter
//! runs the target directly, so it applies the same conversions:
//!
//! * the arguments a jar-body target receives are unboxed per its descriptor
//!   (`BiFunction<String, Integer, Point>` → `Point(String, int)`). The
//!   JDK-target path already did this;
//! * a callback's result is unboxed and widened when the interface method
//!   returns a primitive (`ToIntFunction<Holder>` over `Integer boxed()`,
//!   `DoubleSupplier` over `int anInt()`), for JDK and mod-defined
//!   functional interfaces alike; arguments are widened the same way
//!   (`IntFunction` over `takesLong(long)`). Boxing a primitive result for a
//!   reference return (`box_callback_result`) now also covers a mod's own
//!   interfaces.
//!
//! Without these, a boxed marker reached an `int` slot and the frame stopped
//! with a type mismatch (or a record printed `x=integer:7`).

use super::*;

/// `value` converted to the primitive the interface method `call_descriptor`
/// returns, when it returns one: unboxed when it is a box, then widened
/// (`DoubleSupplier` over an `int` target answers `7.0`, `ToLongFunction`
/// over an `Integer` target a `long`); otherwise `value` unchanged.
pub(crate) fn unbox_result(call_descriptor: &str, value: Value) -> Value {
    let returned = return_desc(call_descriptor);
    jdk_model::adapt_to_primitive(&value, returned).unwrap_or(value)
}
