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
//! * `String()` is empty and `String(String original)` copies `original`; a
//!   null `original` throws `NullPointerException: Cannot read field "value"
//!   because "original" is null`.
//! * The `byte[]` constructors decode with the charset they name, or UTF-8
//!   (the default charset since JDK 18) when they name none. The charset is
//!   checked first: a null `Charset` or name throws `NullPointerException`
//!   with no message, an unknown name `UnsupportedEncodingException`. Then a
//!   null array throws `NullPointerException: Cannot read the array length
//!   because "bytes" is null`, and a bad range the same
//!   `StringIndexOutOfBoundsException` as above.
//!
//! The `byte[]` constructors are answered only where the decoding is exact
//! here: ISO-8859-1 (one byte, one char), US-ASCII (a byte over 0x7F is
//! U+FFFD) and well-formed UTF-8. Malformed UTF-8 is counted, because JDK 25
//! does not replace it the way Rust's `from_utf8_lossy` does (`ED A0 80` is
//! one U+FFFD in Java, three in Rust); so is any other charset (UTF-16, a
//! name the host does not model), which also covers the unknown names Java
//! throws on.
//!
//! Every other constructor (`StringBuilder`/`StringBuffer`, the deprecated
//! hibyte `(byte[], int)` pair) is still counted. So is a `char[]`/`int[]`
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

/// The charsets whose `byte[]` decoding is exact here.
#[derive(Clone, Copy)]
enum ExactCharset {
    Utf8,
    UsAscii,
    Latin1,
}

impl ExactCharset {
    fn named(canonical: &str) -> Option<Self> {
        match canonical {
            "UTF-8" => Some(Self::Utf8),
            "US-ASCII" => Some(Self::UsAscii),
            "ISO-8859-1" => Some(Self::Latin1),
            _ => None,
        }
    }

    /// The text, or `None` for malformed UTF-8 (see the module comment).
    fn decode(self, bytes: &[u8]) -> Option<String> {
        match self {
            Self::Utf8 => std::str::from_utf8(bytes).ok().map(str::to_owned),
            Self::UsAscii => Some(
                bytes
                    .iter()
                    .map(|byte| {
                        if byte.is_ascii() {
                            char::from(*byte)
                        } else {
                            '\u{FFFD}'
                        }
                    })
                    .collect(),
            ),
            Self::Latin1 => Some(bytes.iter().map(|byte| char::from(*byte)).collect()),
        }
    }
}

/// The charset argument of a `byte[]` constructor: a `Charset` (`charset/<name>`
/// marker) or a name. `Ok(None)` when it is not one decoded exactly here.
fn byte_charset(value: &Value, by_name: bool) -> Result<Option<ExactCharset>, NativeError> {
    let Value::Ref(Some(marker)) = value else {
        return Err(NativeError::UncaughtException {
            class: "java/lang/NullPointerException".into(),
            message: None,
            frame: None,
        });
    };
    let canonical = if by_name {
        jdk_model::canonical_charset(marker)
    } else {
        marker.strip_prefix("charset/")
    };
    Ok(canonical.and_then(ExactCharset::named))
}

/// The text a `byte[]` constructor builds, after its charset (if any) was
/// resolved: `offset`/`count` index the array when the descriptor has them.
fn decode_bytes<H: Host>(
    runtime: &Runtime<'_, H>,
    r: &MethodRef,
    args: &[Value],
    range: bool,
    charset: ExactCharset,
) -> Result<Option<String>, NativeError> {
    let values = match args.first() {
        Some(array @ Value::Ref(Some(_))) => runtime.array(array)?.clone(),
        _ => {
            return Err(thrown(
                "java/lang/NullPointerException",
                "Cannot read the array length because \"bytes\" is null".into(),
            ));
        }
    };
    // A byte the runtime does not hold as an int is not guessed.
    let Some(bytes) = values
        .iter()
        .map(|value| match value {
            Value::Int(byte) => Some(*byte as u8),
            _ => None,
        })
        .collect::<Option<Vec<u8>>>()
    else {
        return Ok(None);
    };
    let bytes = if range {
        let offset = expect_int(r, StdHost::argument(r, args, 1)?)?;
        let count = expect_int(r, StdHost::argument(r, args, 2)?)?;
        check_range(offset, count, bytes.len())?;
        &bytes[offset as usize..(offset + count) as usize]
    } else {
        &bytes[..]
    };
    Ok(charset.decode(bytes))
}

/// The text a `String` constructor builds; `Ok(None)` for a constructor
/// counted by [`count`] (see the module comment).
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
        "()V" => Some(String::new()),
        "(Ljava/lang/String;)V" => match args.first() {
            Some(Value::Ref(Some(original))) if value_class::is_plain_string(original) => {
                Some(original.clone())
            }
            Some(Value::Ref(None)) | None => {
                return Err(thrown(
                    "java/lang/NullPointerException",
                    "Cannot read field \"value\" because \"original\" is null".into(),
                ));
            }
            Some(_) => None,
        },
        "([B)V" | "([BII)V" => {
            return decode_bytes(
                runtime,
                r,
                args,
                r.descriptor == "([BII)V",
                ExactCharset::Utf8,
            );
        }
        "([BLjava/nio/charset/Charset;)V"
        | "([BLjava/lang/String;)V"
        | "([BIILjava/nio/charset/Charset;)V"
        | "([BIILjava/lang/String;)V" => {
            let range = r.descriptor.starts_with("([BII");
            let by_name = r.descriptor.ends_with("Ljava/lang/String;)V");
            let charset = StdHost::argument(r, args, if range { 3 } else { 1 })?;
            return match byte_charset(charset, by_name)? {
                Some(charset) => decode_bytes(runtime, r, args, range, charset),
                None => Ok(None),
            };
        }
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
