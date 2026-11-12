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
//! * a callback's result is unboxed when the interface method returns a
//!   primitive (`ToIntFunction<Holder>` over `Integer boxed()`), for JDK and
//!   mod-defined functional interfaces alike. Boxing a primitive result for a
//!   reference return (`box_callback_result`) now also covers a mod's own
//!   interfaces.
//!
//! Without these, a boxed marker reached an `int` slot and the frame stopped
//! with a type mismatch (or a record printed `x=integer:7`).

use super::*;

/// `value` converted to the primitive the interface method `call_descriptor`
/// returns, when it returns one and `value` is a box of it; otherwise
/// `value` unchanged.
pub(crate) fn unbox_result(call_descriptor: &str, value: Value) -> Value {
    let returned = return_desc(call_descriptor);
    if returned.len() != 1 || returned == "V" || !matches!(value, Value::Ref(Some(_))) {
        return value;
    }
    let mut slot = [value];
    jdk_model::unbox_for_descriptor(&format!("({returned})V"), &mut slot);
    let [value] = slot;
    value
}
