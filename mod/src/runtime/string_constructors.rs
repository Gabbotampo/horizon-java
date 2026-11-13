//! `new String(...)`: counted, never answered with the allocation marker.
//!
//! `new java/lang/String` pushes an object marker (`java/lang/String#n`), and
//! no model turns that marker into the text the constructor builds. Every
//! overload therefore left the marker in place of the string: on `main`,
//! `new String(new char[] {'a', 'b'})` answered an 18-character string whose
//! `equals("ab")` was `false`, and `new String("xy")`, `new String()`,
//! `new String(byte[], UTF_8)` and `new String(StringBuilder)` answered the
//! same marker, all without a count, so the run still read clean.
//!
//! Until a model builds the text (and rewrites the marker wherever the frame
//! holds it), each of the JDK's fifteen public `String` constructors is
//! counted as an unknown host call. A `String` constructor reached through a
//! `MethodHandle` (`String::new`) is counted the same way.

use super::*;

/// Count `r` when it is a `java.lang.String` constructor; `Some(Void)` then
/// stands for the constructor's (void) answer.
pub(crate) fn count<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
) -> Result<Option<Value>, NativeError> {
    if r.owner != "java/lang/String" || r.name != "<init>" {
        return Ok(None);
    }
    runtime.host.unknown_operand_call(r)?;
    Ok(Some(Value::Void))
}
