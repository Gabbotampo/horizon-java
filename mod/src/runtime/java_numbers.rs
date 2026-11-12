//! `Integer`/`Long`/`Short`/`Byte` string parsing, as the JDK specifies it.
//!
//! The old paths trimmed (`parseInt(" 5")` was 5; the JDK throws), took only
//! ASCII digits, raised interpreter errors instead of `NumberFormatException`
//! for `Short`/`Byte`, and handed a radix above 36 to `from_str_radix`, which
//! panics and takes the process down. Here a parse walks the UTF-16 code
//! units with `Character.digit`, so `"٣٤"` is 34 as on a JVM, and
//! every failure is the JDK's `NumberFormatException` with its message.
//!
//! `Character.digit` (and every other `Character` query) answers from the
//! table generated from Java 25, the game's runtime (`jdk_character.rs`).

use crate::runtime::{MethodRef, NativeError, Value, expect_ref};

fn number_format(message: String) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NumberFormatException".into(),
        message: Some(message),
        frame: None,
    }
}

/// `NumberFormatException.forInputString`.
fn for_input(text: &str, radix: i32) -> NativeError {
    let under = if radix == 10 {
        String::new()
    } else {
        format!(" under radix {radix}")
    };
    number_format(format!("For input string: \"{text}\"{under}"))
}

/// `Character.digit(codePoint, radix)`.
fn digit(code_point: u32, radix: i32) -> i32 {
    crate::runtime::jdk_character::digit(code_point as i32, radix)
}

/// `Long.parseLong(text, radix)` checked against `[min, max]`: the JDK's
/// algorithm (one optional sign, then `Character.digit` of each UTF-16 code
/// unit, no trimming) and its messages.
fn parse(text: Option<&str>, radix: i32, min: i64, max: i64) -> Result<i64, NativeError> {
    let Some(text) = text else {
        return Err(number_format("Cannot parse null string".into()));
    };
    if radix < 2 {
        return Err(number_format(format!(
            "radix {radix} less than Character.MIN_RADIX"
        )));
    }
    if radix > 36 {
        return Err(number_format(format!(
            "radix {radix} greater than Character.MAX_RADIX"
        )));
    }
    let units = text.encode_utf16().collect::<Vec<_>>();
    let (negative, digits) = match units.first() {
        Some(&sign) if sign == u16::from(b'-') => (true, &units[1..]),
        Some(&sign) if sign == u16::from(b'+') => (false, &units[1..]),
        _ => (false, &units[..]),
    };
    if digits.is_empty() {
        return Err(for_input(text, radix));
    }
    let mut value: i128 = 0;
    for &unit in digits {
        let digit = digit(u32::from(unit), radix);
        if digit < 0 {
            return Err(for_input(text, radix));
        }
        value = value * i128::from(radix) + i128::from(digit);
        if value > i128::from(max) + 1 {
            return Err(for_input(text, radix));
        }
    }
    let value = if negative { -value } else { value };
    if value < i128::from(min) || value > i128::from(max) {
        return Err(for_input(text, radix));
    }
    Ok(value as i64)
}

/// `Short.parseShort`/`Byte.parseByte`: an `int` parse, then the range check
/// with its own message.
fn parse_narrow(text: Option<&str>, radix: i32, min: i64, max: i64) -> Result<i64, NativeError> {
    let value = parse(text, radix, i64::from(i32::MIN), i64::from(i32::MAX))?;
    if value < min || value > max {
        return Err(number_format(format!(
            "Value out of range. Value:\"{}\" Radix:{radix}",
            text.unwrap_or_default()
        )));
    }
    Ok(value)
}

/// The string parsers, or `None` for any other call.
pub(crate) fn static_call(r: &MethodRef, args: &[Value]) -> Option<Result<Value, NativeError>> {
    let text = args.first().and_then(expect_ref).map(String::as_str);
    let radix = match args.get(1) {
        Some(Value::Int(radix)) => *radix,
        _ => 10,
    };
    let (owner, name) = (r.owner.as_str(), r.name.as_str());
    let parsed = match (owner, name) {
        _ if !r.descriptor.starts_with("(Ljava/lang/String;") => return None,
        ("java/lang/Integer", "parseInt" | "valueOf") => {
            parse(text, radix, i32::MIN.into(), i32::MAX.into())
        }
        ("java/lang/Long", "parseLong" | "valueOf") => parse(text, radix, i64::MIN, i64::MAX),
        ("java/lang/Short", "parseShort" | "valueOf") => {
            parse_narrow(text, radix, i16::MIN.into(), i16::MAX.into())
        }
        ("java/lang/Byte", "parseByte" | "valueOf") => {
            parse_narrow(text, radix, i8::MIN.into(), i8::MAX.into())
        }
        _ => return None,
    };
    // A primitive result is the value; a `valueOf` box uses the host's markers.
    Some(parsed.map(|value| match (owner, name) {
        (_, "parseInt" | "parseShort" | "parseByte") => Value::Int(value as i32),
        ("java/lang/Long", _) => Value::Long(value),
        ("java/lang/Integer", _) => Value::Ref(Some(format!("integer:{value}"))),
        ("java/lang/Short", _) => Value::Ref(Some(format!("short:{value}"))),
        _ => Value::Ref(Some(format!("byte:{value}"))),
    }))
}
