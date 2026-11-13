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
//! [`build`] now builds the text for the `char[]` and code-point constructors,
//! as JDK 25 does, and the interpreter rewrites the allocation marker to that
//! text wherever the frame holds it (the `dup`ed copy on the operand stack, and
//! the locals), so the string is a string from then on. night-config's TOML
//! parser builds every key with `new String(char[], int, int)`; with the
//! marker in its place a key read `java/lang/String#477`, and every Forge
//! Config API Port user on Fabric failed to parse its config (#124).
//!
//! From JDK 25 (`java.lang.String`, Temurin 25.0.5+8 `src.zip`, checked on a
//! JVM):
//!
//! * `String(char[] value)` and `String(char[] value, int offset, int count)`
//!   copy the units (a later write to the array does not change the string).
//!   A null array throws `NullPointerException: Cannot read the array length
//!   because "value" is null`; a bad range throws
//!   `StringIndexOutOfBoundsException: Range [offset, offset + count) out of
//!   bounds for length N`.
//! * `String(int[] codePoints, int offset, int count)` checks the range the
//!   same way (`"codePoints"` in the NPE), then each code point: an invalid one
//!   throws `IllegalArgumentException` with the code point in decimal.
//!
//! Every other constructor (`String()`, `String(String)`, the `byte[]` and
//! charset ones, `StringBuilder`/`StringBuffer`) is still counted: the `byte[]`
//! and charset constructors are Gabbotampo's #6. So is a `char[]`/`int[]`
//! constructor whose text has an unpaired surrogate, which the runtime's
//! strings cannot hold. A `String` constructor reached through a
//! `MethodHandle` (`String::new`) is answered or counted the same way.

use super::*;

fn thrown(class: &str, message: String) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message: Some(message),
        frame: None,
    }
}

/// `Objects.checkFromIndexSize(offset, count, length)` as `String` reports it.
fn check_range(offset: i32, count: i32, length: usize) -> Result<(), NativeError> {
    let length_i = length as i64;
    let (offset_i, count_i) = (i64::from(offset), i64::from(count));
    if offset_i < 0 || count_i < 0 || offset_i + count_i > length_i {
        return Err(thrown(
            "java/lang/StringIndexOutOfBoundsException",
            format!("Range [{offset}, {offset} + {count}) out of bounds for length {length}"),
        ));
    }
    Ok(())
}

/// The text a `char[]`/`int[]` `String` constructor builds; `Ok(None)` for any
/// other constructor (counted by [`count`]) and for text with an unpaired
/// surrogate.
pub(crate) fn build<H: Host>(
    runtime: &mut Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
) -> Result<Option<String>, NativeError> {
    if r.owner != "java/lang/String" || r.name != "<init>" {
        return Ok(None);
    }
    let codes = |runtime: &Runtime<'_, H>, parameter: &str| -> Result<Vec<i32>, NativeError> {
        match args.first() {
            Some(array @ Value::Ref(Some(_))) => Ok(runtime
                .array(array)?
                .iter()
                .map(|value| match value {
                    Value::Int(code) => *code,
                    _ => 0,
                })
                .collect()),
            _ => Err(thrown(
                "java/lang/NullPointerException",
                format!("Cannot read the array length because \"{parameter}\" is null"),
            )),
        }
    };
    let int = |index: usize| -> Result<i32, NativeError> {
        expect_int(r, StdHost::argument(r, args, index)?)
    };
    let text = match r.descriptor.as_str() {
        "([C)V" => {
            let units = codes(runtime, "value")?;
            String::from_utf16(&units.iter().map(|unit| *unit as u16).collect::<Vec<_>>()).ok()
        }
        "([CII)V" => {
            let units = codes(runtime, "value")?;
            let (offset, count) = (int(1)?, int(2)?);
            check_range(offset, count, units.len())?;
            let slice = &units[offset as usize..(offset + count) as usize];
            String::from_utf16(&slice.iter().map(|unit| *unit as u16).collect::<Vec<_>>()).ok()
        }
        "([III)V" => {
            let points = codes(runtime, "codePoints")?;
            let (offset, count) = (int(1)?, int(2)?);
            check_range(offset, count, points.len())?;
            let mut text = String::new();
            for point in &points[offset as usize..(offset + count) as usize] {
                if !(0..=0x10FFFF).contains(point) {
                    return Err(thrown(
                        "java/lang/IllegalArgumentException",
                        point.to_string(),
                    ));
                }
                match char::from_u32(*point as u32) {
                    Some(c) => text.push(c),
                    // A surrogate code point is valid but unpaired here.
                    None => return Ok(None),
                }
            }
            Some(text)
        }
        _ => None,
    };
    Ok(text)
}

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
