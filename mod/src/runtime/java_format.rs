//! `java.util.Formatter`, as `String.format`, `String.formatted` and
//! `PrintStream.printf` run it, answered exactly or not at all.
//!
//! The earlier subset answered some specifiers confidently and wrongly: `%b`
//! printed `true` for a boxed `false`, and `%f` rounded the binary value
//! half-to-even (`"%f".formatted(5e-7)` was `0.000000`, the JDK prints
//! `0.000001`). This module answers a specifier only when it reproduces the
//! JDK's output for that exact argument; anything else (a conversion or flag
//! combination it does not model, an argument whose class the runtime cannot
//! tell, or a call on which the JDK throws) makes [`format`] return `None`,
//! and the caller leaves the whole call to the host as an unknown, counted
//! call instead of guessing.
//!
//! What the JDK does, as the tests in this file pin it from a real JVM:
//! - A specifier is `%[index$][flags][width][.precision]conversion`. The
//!   ordinary index counts on its own; an explicit `n$` and the `<` flag
//!   (the previous argument) do not move it.
//! - `%s`/`%b`/`%h`/`%c` take the `-` flag only; a precision truncates
//!   `%s`/`%b`/`%h` in UTF-16 units, and the width counts UTF-16 units too.
//! - `%d` writes the sign (`-`, `(`, `+` or space), the digits (grouped in
//!   threes by `,`), then zero padding between the sign and the digits up to
//!   the width (minus one for the closing `)`).
//! - `%o`/`%x` print a negative `byte`/`short`/`int`/`long` as the two's
//!   complement of the argument's own width; `#` adds `0`/`0x`.
//! - `%e`/`%f`/`%g` round the double's shortest decimal digits (the digits
//!   `Double.toString` prints) `HALF_UP`, never the binary value: `%.2f` of
//!   `2.675` is `2.68`. `-0.0` counts as negative (`-0.000000`). A `Float`
//!   is widened to `double` first, so `%.10f` of `0.1f` shows its binary
//!   tail. `NaN` has no sign and neither it nor the infinities is zero-padded.
//! - A `null` argument prints `null` (`false` for `%b`) under every
//!   conversion, padded with spaces.
//!
//! Only the default `en`/root symbols are modelled (`.` decimal point, `,`
//! grouping, ASCII digits); the caller refuses other locales. Upper-case
//! conversions (`%S`, `%X`, `%E`, ...) are answered only for ASCII text,
//! where `toUpperCase` cannot depend on the locale or the Unicode version.

/// One `Object` argument, as far as the runtime can tell its class.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Arg {
    Null,
    /// A `String`, or another object whose `toString` the runtime answers
    /// (a `StringBuilder`, a modelled library object).
    Str(String),
    Bool(bool),
    /// The runtime's bare `true`/`false`: a boxed `Boolean` or a `String` of
    /// that text, which only `%s` prints alike.
    BoolOrString(bool),
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    /// A `Character`: one UTF-16 code unit.
    Char(u16),
    Double(f64),
    Float(f32),
    /// The runtime keeps a boxed `Float` and a boxed `Double` as the same
    /// value (a `float` is stored widened), so their `toString` and
    /// `hashCode` differ while every numeric conversion agrees.
    FloatOrDouble(f64),
    /// A non-null object the runtime cannot print or hash.
    Opaque,
}

/// `String.format(Locale.US, format, args)`, or `None` when the JDK's answer
/// is not reproduced exactly (including every call on which it throws).
///
/// `args` is `None` for a `null` `Object[]`, which Java formats as if every
/// argument were `null`.
pub(crate) fn format(format: &str, args: Option<&[Arg]>) -> Option<String> {
    let mut output = String::with_capacity(format.len());
    let mut rest = format;
    // The JDK's `last` (the argument `<` reuses) and `lasto` (the ordinary
    // index), both zero-based; `None` before the first argument.
    let mut last: Option<usize> = None;
    let mut ordinary: Option<usize> = None;
    while let Some(percent) = rest.find('%') {
        output.push_str(&rest[..percent]);
        let (spec, after) = Spec::parse(&rest[percent + 1..])?;
        rest = after;
        if matches!(spec.conversion, '%' | 'n') {
            output.push_str(&spec.print_text()?);
            continue;
        }
        let index = match spec.index {
            Index::Ordinary => {
                let next = ordinary.map_or(0, |index| index + 1);
                ordinary = Some(next);
                next
            }
            Index::Explicit(index) => index,
            Index::Previous => last?,
        };
        last = Some(index);
        let null = Arg::Null;
        let arg = match args {
            Some(args) => args.get(index)?,
            None => &null,
        };
        output.push_str(&spec.print(arg)?);
    }
    output.push_str(rest);
    Some(output)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Index {
    Ordinary,
    /// Zero-based (`%1$s` is 0).
    Explicit(usize),
    /// The `<` flag.
    Previous,
}

/// One parsed specifier.
#[derive(Debug, Clone, Copy)]
struct Spec {
    index: Index,
    left: bool,
    alternate: bool,
    plus: bool,
    space: bool,
    zero: bool,
    group: bool,
    parentheses: bool,
    width: Option<usize>,
    precision: Option<usize>,
    /// The conversion, lower-cased; `upper` keeps its case.
    conversion: char,
    upper: bool,
}

/// Widths and precisions above this are left to the host: the JDK takes any
/// `int`, but such a call is never one a mod relies on, and the cap keeps a
/// hostile format from allocating gigabytes here.
const MAX_FIELD: usize = 4096;

impl Spec {
    /// Read one specifier from `text` (just past its `%`), following the
    /// JDK's `%(\d+\$)?([-#+ 0,(\<]*)?(\d+)?(\.\d+)?([tT])?([a-zA-Z%])`.
    /// `None` for anything the JDK refuses or this module does not model.
    fn parse(text: &str) -> Option<(Self, &str)> {
        let bytes = text.as_bytes();
        let mut at = 0;
        let digits = |at: &mut usize| -> Option<Option<usize>> {
            let start = *at;
            while bytes.get(*at).is_some_and(u8::is_ascii_digit) {
                *at += 1;
            }
            if *at == start {
                return Some(None);
            }
            // Leading zeros are part of the number (`%05d` never gets here:
            // its `0` is read as a flag first).
            let value = text[start..*at].parse::<usize>().ok()?;
            (value <= MAX_FIELD).then_some(Some(value))
        };
        let mut spec = Spec {
            index: Index::Ordinary,
            left: false,
            alternate: false,
            plus: false,
            space: false,
            zero: false,
            group: false,
            parentheses: false,
            width: None,
            precision: None,
            conversion: '%',
            upper: false,
        };
        // An explicit index is digits followed by `$`; otherwise the digits
        // are read again below, as flags (`0`) and a width.
        let mut probe = 0;
        while bytes.get(probe).is_some_and(u8::is_ascii_digit) {
            probe += 1;
        }
        if probe > 0 && bytes.get(probe) == Some(&b'$') {
            let index = digits(&mut at)??;
            // `%0$s` throws (`IllegalFormatArgumentIndexException`).
            spec.index = Index::Explicit(index.checked_sub(1)?);
            at += 1;
        }
        while let Some(flag) = bytes.get(at) {
            let seen = match flag {
                b'-' => &mut spec.left,
                b'#' => &mut spec.alternate,
                b'+' => &mut spec.plus,
                b' ' => &mut spec.space,
                b'0' => &mut spec.zero,
                b',' => &mut spec.group,
                b'(' => &mut spec.parentheses,
                b'<' => {
                    // `<` beside an explicit index is not modelled.
                    if spec.index != Index::Ordinary {
                        return None;
                    }
                    spec.index = Index::Previous;
                    at += 1;
                    continue;
                }
                _ => break,
            };
            // A repeated flag throws `DuplicateFormatFlagsException`.
            if *seen {
                return None;
            }
            *seen = true;
            at += 1;
        }
        spec.width = digits(&mut at)?;
        if bytes.get(at) == Some(&b'.') {
            at += 1;
            spec.precision = Some(digits(&mut at)??);
        }
        let conversion = *bytes.get(at)? as char;
        at += 1;
        if !matches!(
            conversion,
            'b' | 'B'
                | 'h'
                | 'H'
                | 's'
                | 'S'
                | 'c'
                | 'C'
                | 'd'
                | 'o'
                | 'x'
                | 'X'
                | 'e'
                | 'E'
                | 'f'
                | 'g'
                | 'G'
                | '%'
                | 'n'
        ) {
            // `%a`, the date/time `%t`, and every unknown conversion.
            return None;
        }
        spec.upper = conversion.is_ascii_uppercase();
        spec.conversion = conversion.to_ascii_lowercase();
        spec.check()?;
        Some((spec, &text[at..]))
    }

    /// The flag, width and precision combinations each conversion takes; the
    /// JDK throws on every other one.
    fn check(&self) -> Option<()> {
        let others = self.alternate
            || self.plus
            || self.space
            || self.zero
            || self.group
            || self.parentheses;
        // `-` and `0` need a width to pad to.
        if (self.left || self.zero) && self.width.is_none() {
            return None;
        }
        match self.conversion {
            '%' => {
                // `%-5%` pads; no index, other flag or precision.
                (self.index == Index::Ordinary && !others && self.precision.is_none()).then_some(())
            }
            'n' => (self.index == Index::Ordinary
                && !self.left
                && !others
                && self.width.is_none()
                && self.precision.is_none())
            .then_some(()),
            // `#` on `%s` is only for a `Formattable`, which no modelled
            // argument is.
            'b' | 'h' | 's' => (!others).then_some(()),
            'c' => (!others && self.precision.is_none()).then_some(()),
            'd' | 'o' | 'x' | 'e' | 'f' | 'g' => {
                if (self.left && self.zero) || (self.plus && self.space) {
                    return None;
                }
                match self.conversion {
                    'd' => (!self.alternate && self.precision.is_none()).then_some(()),
                    // The sign and grouping flags are only for `BigInteger`.
                    'o' | 'x' => (!self.plus
                        && !self.space
                        && !self.group
                        && !self.parentheses
                        && self.precision.is_none())
                    .then_some(()),
                    'e' => (!self.group).then_some(()),
                    'g' => (!self.alternate).then_some(()),
                    _ => Some(()),
                }
            }
            _ => None,
        }
    }

    /// `%%` and `%n`, which take no argument.
    fn print_text(&self) -> Option<String> {
        match self.conversion {
            '%' => Some(self.justify("%".to_owned())),
            // `System.lineSeparator()`, which the host answers `\n`.
            _ => Some("\n".to_owned()),
        }
    }

    fn print(&self, arg: &Arg) -> Option<String> {
        if *arg == Arg::Null && self.conversion != 'b' {
            // `print(fmt, "null", l)`: truncated, cased and space-padded like
            // any string, whatever the conversion.
            return self.print_string("null".to_owned());
        }
        match self.conversion {
            'b' => self.print_string(
                match arg {
                    Arg::Null | Arg::Bool(false) => "false",
                    // `Boolean.FALSE` prints `false`, the string `"false"`
                    // prints `true`.
                    Arg::BoolOrString(false) => return None,
                    _ => "true",
                }
                .to_owned(),
            ),
            'h' => self.print_string(format!("{:x}", java_hash(arg)? as u32)),
            's' => self.print_string(java_to_string(arg)?),
            'c' => self.print_string(java_char(arg)?),
            'd' => {
                let value = integral(arg)?.0;
                let mut text = self.leading_sign(value < 0);
                let digits = value.unsigned_abs().to_string();
                let width = self.magnitude_width(self.width, value < 0);
                self.magnitude(&mut text, &digits, width);
                self.trailing_sign(&mut text, value < 0);
                Some(self.justify(text))
            }
            'o' | 'x' => {
                let (value, bits) = integral(arg)?;
                // Two's complement of the argument's own width.
                let unsigned = if bits == 64 {
                    value as u64
                } else {
                    (value as u64) & ((1u64 << bits) - 1)
                };
                let (digits, prefix) = if self.conversion == 'o' {
                    (format!("{unsigned:o}"), "0")
                } else if self.upper {
                    (format!("{unsigned:X}"), "0X")
                } else {
                    (format!("{unsigned:x}"), "0x")
                };
                let prefix = if self.alternate { prefix } else { "" };
                let mut text = prefix.to_owned();
                if self.zero
                    && let Some(width) = self.width
                {
                    let length = digits.len() + prefix.len();
                    text.extend(std::iter::repeat_n('0', width.saturating_sub(length)));
                }
                text.push_str(&digits);
                Some(self.justify(text))
            }
            'e' | 'f' | 'g' => {
                let value = floating(arg)?;
                Some(self.justify(self.print_floating(value)?))
            }
            _ => None,
        }
    }

    /// The general conversions' `print(String)`: precision, case, width.
    fn print_string(&self, text: String) -> Option<String> {
        let text = match self.precision {
            Some(precision) => truncate_utf16(text, precision)?,
            None => text,
        };
        let text = if self.upper {
            // Locale- and Unicode-version-free only for ASCII.
            if !text.is_ascii() {
                return None;
            }
            text.to_ascii_uppercase()
        } else {
            text
        };
        Some(self.justify(text))
    }

    fn print_floating(&self, value: f64) -> Option<String> {
        if value.is_nan() {
            return Some(if self.upper { "NAN" } else { "NaN" }.to_owned());
        }
        // `Double.compare(value, 0.0) == -1`: `-0.0` is negative too.
        let negative = value.is_sign_negative();
        let mut text = self.leading_sign(negative);
        if value.is_infinite() {
            text.push_str(if self.upper { "INFINITY" } else { "Infinity" });
        } else {
            let digits = Digits::of(value.abs())?;
            match self.conversion {
                'e' => self.scientific(&mut text, digits, self.precision.unwrap_or(6), negative),
                'f' => {
                    let precision = self.precision.unwrap_or(6);
                    let mantissa = self.decimal(digits.round_fraction(precision), precision);
                    let width = self.magnitude_width(self.width, negative);
                    self.magnitude(&mut text, &mantissa, width);
                }
                _ => {
                    // `%g`: `precision` significant digits (0 means 1); the
                    // rounded value picks decimal for 1e-4 <= m < 10^precision.
                    let precision = match self.precision {
                        None => 6,
                        Some(0) => 1,
                        Some(precision) => precision,
                    };
                    let rounded = digits.round_significant(precision);
                    let exponent = rounded.exponent();
                    if digits.is_zero() || (-4..precision as i64).contains(&exponent) {
                        let fraction = if digits.is_zero() {
                            precision - 1
                        } else {
                            (precision as i64 - 1 - exponent) as usize
                        };
                        let mantissa = self.decimal(rounded.round_fraction(fraction), fraction);
                        let width = self.magnitude_width(self.width, negative);
                        self.magnitude(&mut text, &mantissa, width);
                    } else {
                        self.scientific(&mut text, digits, precision - 1, negative);
                    }
                }
            }
        }
        self.trailing_sign(&mut text, negative);
        Some(text)
    }

    /// `d.ddde+xx` with `precision` fraction digits.
    fn scientific(&self, text: &mut String, digits: Digits, precision: usize, negative: bool) {
        let rounded = digits.round_significant(precision + 1);
        let exponent = if digits.is_zero() {
            0
        } else {
            rounded.exponent()
        };
        let mut significant = rounded.digits;
        significant.resize(precision + 1, b'0');
        let mut mantissa = String::new();
        mantissa.push(significant[0] as char);
        if precision > 0 || self.alternate {
            mantissa.push('.');
        }
        mantissa.extend(significant[1..].iter().map(|digit| *digit as char));
        // The exponent: a sign and at least two digits.
        let exponent_text = format!(
            "{}{:02}",
            if exponent < 0 { '-' } else { '+' },
            exponent.unsigned_abs()
        );
        // `width - exp.length - 1`: the sign and digits, then the `e`.
        let width = self
            .width
            .map(|width| width.saturating_sub(exponent_text.len() + 1));
        let width = self.magnitude_width(width, negative);
        self.magnitude(text, &mantissa, width);
        text.push(if self.upper { 'E' } else { 'e' });
        text.push_str(&exponent_text);
    }

    /// `int.frac` from a decimal already rounded to `precision` fraction
    /// digits; `#` keeps the point when there are none.
    fn decimal(&self, rounded: Decimal, precision: usize) -> String {
        let (integer, fraction) = rounded.split(precision);
        let mut text = integer;
        if precision > 0 || self.alternate {
            text.push('.');
        }
        text.push_str(&fraction);
        text
    }

    /// `(`, `-`, `+`, space or nothing.
    fn leading_sign(&self, negative: bool) -> String {
        match (negative, self.parentheses, self.plus, self.space) {
            (true, true, _, _) => "(",
            (true, false, _, _) => "-",
            (false, _, true, _) => "+",
            (false, _, _, true) => " ",
            _ => "",
        }
        .to_owned()
    }

    fn trailing_sign(&self, text: &mut String, negative: bool) {
        if negative && self.parentheses {
            text.push(')');
        }
    }

    /// The JDK's `adjustWidth`: a closing `)` takes one column of the width
    /// the zero padding fills.
    fn magnitude_width(&self, width: Option<usize>, negative: bool) -> Option<usize> {
        width.map(|width| {
            if negative && self.parentheses {
                width.saturating_sub(1)
            } else {
                width
            }
        })
    }

    /// Append `digits` (`123456.78`), grouping the integer part when `,` is
    /// set, then zero-pad after whatever `text` already holds (the sign) up
    /// to `width` when `0` is set.
    fn magnitude(&self, text: &mut String, digits: &str, width: Option<usize>) {
        let begin = text.len();
        let (integer, fraction) = match digits.find('.') {
            Some(point) => digits.split_at(point),
            None => (digits, ""),
        };
        if self.group {
            for (index, digit) in integer.chars().enumerate() {
                if index != 0 && (integer.len() - index).is_multiple_of(3) {
                    text.push(',');
                }
                text.push(digit);
            }
        } else {
            text.push_str(integer);
        }
        text.push_str(fraction);
        if self.zero
            && let Some(width) = width
        {
            // Every character so far is ASCII, so bytes are columns.
            let fill = width.saturating_sub(text.len());
            text.insert_str(begin, &"0".repeat(fill));
        }
    }

    /// Space-pad to the width in UTF-16 units, left-justified with `-`.
    fn justify(&self, text: String) -> String {
        let Some(width) = self.width else {
            return text;
        };
        let length = text.encode_utf16().count();
        if length >= width {
            return text;
        }
        let fill = " ".repeat(width - length);
        if self.left {
            text + &fill
        } else {
            fill + &text
        }
    }
}

/// `s.substring(0, precision)` when that is shorter; `None` when the cut
/// would split a surrogate pair (a lone surrogate is not a Rust string).
fn truncate_utf16(text: String, precision: usize) -> Option<String> {
    let mut units = 0;
    for (offset, ch) in text.char_indices() {
        if units == precision {
            return Some(text[..offset].to_owned());
        }
        units += ch.len_utf16();
        if units > precision {
            return None;
        }
    }
    Some(text)
}

/// An integral argument's value and its type's width in bits.
fn integral(arg: &Arg) -> Option<(i64, u32)> {
    match *arg {
        Arg::Byte(value) => Some((i64::from(value), 8)),
        Arg::Short(value) => Some((i64::from(value), 16)),
        Arg::Int(value) => Some((i64::from(value), 32)),
        Arg::Long(value) => Some((value, 64)),
        // `BigInteger` and the rest are not modelled or throw.
        _ => None,
    }
}

/// A floating-point argument as the `double` the JDK formats (a `Float` is
/// widened first, which the runtime has already done).
fn floating(arg: &Arg) -> Option<f64> {
    match *arg {
        Arg::Double(value) | Arg::FloatOrDouble(value) => Some(value),
        Arg::Float(value) => Some(f64::from(value)),
        // `BigDecimal` and the rest are not modelled or throw.
        _ => None,
    }
}

/// `String.valueOf(arg)`.
fn java_to_string(arg: &Arg) -> Option<String> {
    Some(match arg {
        Arg::Null => "null".to_owned(),
        Arg::Str(text) => text.clone(),
        Arg::Bool(value) | Arg::BoolOrString(value) => value.to_string(),
        Arg::Byte(value) => value.to_string(),
        Arg::Short(value) => value.to_string(),
        Arg::Int(value) => value.to_string(),
        Arg::Long(value) => value.to_string(),
        Arg::Char(unit) => String::from(char::from_u32(u32::from(*unit))?),
        // A subnormal's `toString` may weigh two-digit decimals too (see
        // [`Digits::of`]), which the runtime's `toString` does not model.
        Arg::Double(value) if !shortest_is_plain(*value, false) => return None,
        Arg::Float(value) if !shortest_is_plain(f64::from(*value), true) => return None,
        Arg::FloatOrDouble(value)
            if !shortest_is_plain(*value, false) || !shortest_is_plain(*value, true) =>
        {
            return None;
        }
        Arg::Double(value) => super::format_java_float(*value),
        Arg::Float(value) => super::format_java_float32(f64::from(*value)),
        Arg::FloatOrDouble(value) => {
            // A value an `f32` cannot hold was a `Double`; otherwise the two
            // readings must print alike.
            let double = super::format_java_float(*value);
            if f64::from(*value as f32).to_bits() == value.to_bits() || value.is_nan() {
                let float = super::format_java_float32(*value);
                if float != double {
                    return None;
                }
            }
            double
        }
        Arg::Opaque => return None,
    })
}

/// Whether Rust's shortest digits for `value` (as an `f32` when `single`)
/// are certainly the ones the JDK picks.
///
/// Both pick the shortest decimal that rounds back to the value, and the
/// closest of several. They part in two cases this refuses: an exact tie
/// between two shortest candidates (the JDK takes the even last digit:
/// `Float.toString(-219738.625f)` is `-219738.62`, Rust's `-219738.63`), and
/// a subnormal whose shortest form is one digit, where the JDK also weighs
/// two-digit decimals (`Double.MIN_VALUE` is `4.9E-324`, not `5E-324`).
fn shortest_is_plain(value: f64, single: bool) -> bool {
    if !value.is_finite() || value == 0.0 {
        return true;
    }
    let shortest = if single {
        format!("{:e}", (value as f32).abs())
    } else {
        format!("{:e}", value.abs())
    };
    let subnormal = if single {
        (value as f32).is_subnormal()
    } else {
        value.is_subnormal()
    };
    let (digits, exponent) = scientific_parts(&shortest);
    if subnormal && digits.len() == 1 {
        return false;
    }
    // Every double is a finite decimal of at most 767 significant digits,
    // so this is its exact value.
    let exact = format!("{:.800e}", value.abs());
    let (exact_digits, exact_exponent) = scientific_parts(&exact);
    // The exact digits at or above the shortest form's last place.
    let Ok(kept) = usize::try_from(exact_exponent - exponent + digits.len() as i64) else {
        return true;
    };
    let Some(rest) = exact_digits.get(kept..) else {
        return true;
    };
    // A tie is the exact value sitting half a unit past a candidate.
    !(rest.first() == Some(&b'5') && rest[1..].iter().all(|digit| *digit == b'0'))
}

/// The digits and exponent of a Rust `{:e}` string.
fn scientific_parts(scientific: &str) -> (Vec<u8>, i64) {
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("`{:e}` of a finite value has an exponent");
    (
        mantissa.bytes().filter(u8::is_ascii_digit).collect(),
        exponent.parse().expect("`{:e}` writes a decimal exponent"),
    )
}

/// `%c`: a `Character`, or a `Byte`/`Short`/`Integer` code point.
fn java_char(arg: &Arg) -> Option<String> {
    let code_point = match *arg {
        Arg::Char(unit) => u32::from(unit),
        // A negative value is not a valid code point (the JDK throws).
        Arg::Byte(value) => u32::try_from(value).ok()?,
        Arg::Short(value) => u32::try_from(value).ok()?,
        Arg::Int(value) => u32::try_from(value).ok()?,
        _ => return None,
    };
    // Past U+10FFFF the JDK throws; a lone surrogate is not a Rust char.
    char::from_u32(code_point).map(String::from)
}

/// `arg.hashCode()` for the classes whose hash is specified.
fn java_hash(arg: &Arg) -> Option<i32> {
    Some(match *arg {
        Arg::Str(ref text) => text.encode_utf16().fold(0i32, |hash, unit| {
            hash.wrapping_mul(31).wrapping_add(i32::from(unit))
        }),
        Arg::Bool(value) => {
            if value {
                1231
            } else {
                1237
            }
        }
        Arg::Byte(value) => i32::from(value),
        Arg::Short(value) => i32::from(value),
        Arg::Int(value) => value,
        Arg::Long(value) => (value ^ ((value as u64) >> 32) as i64) as i32,
        Arg::Char(unit) => i32::from(unit),
        Arg::Double(value) => {
            // `doubleToLongBits`: every NaN hashes as the canonical one.
            let bits = if value.is_nan() {
                0x7ff8_0000_0000_0000
            } else {
                value.to_bits()
            };
            (bits ^ (bits >> 32)) as i32
        }
        Arg::Float(value) => {
            if value.is_nan() {
                0x7fc0_0000
            } else {
                value.to_bits() as i32
            }
        }
        Arg::Null | Arg::BoolOrString(_) | Arg::FloatOrDouble(_) | Arg::Opaque => return None,
    })
}

/// A non-negative finite double as decimal digits: `0.d1d2d3... × 10^point`
/// (`point` digits sit before the decimal point).
#[derive(Debug, Clone)]
struct Digits {
    digits: Vec<u8>,
    point: i64,
}

/// A decimal rounded to a number of fraction digits.
#[derive(Debug, Clone)]
struct Decimal(Digits);

impl Digits {
    /// The shortest digits that round-trip to `value` (Rust's `{:e}`), the
    /// digits the JDK's `Formatter` rounds.
    ///
    /// `None` where the JDK may pick other digits ([`shortest_is_plain`]).
    fn of(value: f64) -> Option<Self> {
        let (digits, exponent) = scientific_parts(&format!("{value:e}"));
        if digits.iter().all(|digit| *digit == b'0') {
            return Some(Self {
                digits: vec![b'0'],
                point: 1,
            });
        }
        if !shortest_is_plain(value, false) {
            return None;
        }
        Some(Self {
            digits,
            point: exponent + 1,
        })
    }

    fn is_zero(&self) -> bool {
        self.digits == b"0"
    }

    /// The exponent of the leading digit (`d.ddd × 10^exponent`).
    fn exponent(&self) -> i64 {
        self.point - 1
    }

    /// Keep the first `keep` digits, rounding `HALF_UP`; a carry out of the
    /// leading digit moves the point. `keep <= 0` rounds to zero or one unit
    /// of the position before the first digit.
    fn round_to(&self, keep: i64) -> Self {
        if keep >= self.digits.len() as i64 || self.is_zero() {
            return self.clone();
        }
        let round_up = keep >= 0 && self.digits[keep as usize] >= b'5';
        if keep <= 0 {
            // Every digit drops; only `keep == 0` with a leading 5-9 rounds
            // up, to one unit at the position just before the first digit.
            return if round_up {
                Self {
                    digits: vec![b'1'],
                    point: self.point + 1,
                }
            } else {
                Self {
                    digits: vec![b'0'],
                    point: 1,
                }
            };
        }
        let mut digits = self.digits[..keep as usize].to_vec();
        let mut point = self.point;
        if round_up {
            let mut index = digits.len();
            loop {
                if index == 0 {
                    // All nines: 999 -> 1000.
                    digits.insert(0, b'1');
                    digits.pop();
                    point += 1;
                    break;
                }
                index -= 1;
                if digits[index] == b'9' {
                    digits[index] = b'0';
                } else {
                    digits[index] += 1;
                    break;
                }
            }
        }
        Self { digits, point }
    }

    fn round_significant(&self, significant: usize) -> Self {
        self.round_to(significant as i64)
    }

    fn round_fraction(&self, fraction: usize) -> Decimal {
        Decimal(self.round_to(self.point + fraction as i64))
    }
}

impl Decimal {
    /// The integer digits (at least `0`) and exactly `fraction` fraction
    /// digits.
    fn split(&self, fraction: usize) -> (String, String) {
        let Digits { digits, point } = &self.0;
        let digit = |index: i64| -> char {
            usize::try_from(index)
                .ok()
                .and_then(|index| digits.get(index))
                .map_or('0', |digit| *digit as char)
        };
        let integer: String = if *point <= 0 {
            "0".to_owned()
        } else {
            (0..*point).map(digit).collect()
        };
        let fraction: String = (0..fraction as i64).map(|at| digit(point + at)).collect();
        (integer, fraction)
    }
}

#[cfg(test)]
mod tests {
    use super::{Arg, format};

    fn run(pattern: &str, args: &[Arg]) -> Option<String> {
        format(pattern, Some(args))
    }

    /// Every expected string here is what `String.format(Locale.US, ...)`
    /// printed on OpenJDK 27 and OpenJDK 17 for the same pattern and argument
    /// (a differential run, not this module's output).
    #[test]
    fn answers_match_the_jdk() {
        use Arg::*;
        let cases: &[(&str, &[Arg], &str)] = &[
            ("%b", &[Bool(false)], "false"),
            ("%b", &[Bool(true)], "true"),
            ("%b", &[Null], "false"),
            ("%b", &[Str("false".into())], "true"),
            ("%b", &[Int(0)], "true"),
            ("%b", &[BoolOrString(true)], "true"),
            ("%s", &[BoolOrString(false)], "false"),
            ("%.2b", &[Bool(false)], "fa"),
            ("%-7B|", &[Bool(true)], "TRUE   |"),
            ("%f", &[Double(5e-7)], "0.000001"),
            ("%f", &[Double(4.9e-7)], "0.000000"),
            ("%f", &[Double(-5e-7)], "-0.000001"),
            ("%f", &[Double(-0.0)], "-0.000000"),
            ("%(f", &[Double(-0.0)], "(0.000000)"),
            ("%.20f", &[Double(0.1)], "0.10000000000000000000"),
            // The vectors the earlier `%.<p>f` helper was pinned to.
            ("%.2f", &[Double(0.125)], "0.13"),
            ("%.2f", &[Double(-0.125)], "-0.13"),
            ("%.3f", &[Double(1.0 / 3.0)], "0.333"),
            ("%.2f", &[Double(1.005)], "1.01"),
            ("%.2f", &[Double(0.045)], "0.05"),
            ("%.2f", &[Double(123456.785)], "123456.79"),
            ("%.3f", &[Double(1e-7)], "0.000"),
            ("%f", &[Double(1.5)], "1.500000"),
            ("%f", &[Double(f64::NAN)], "NaN"),
            ("%f", &[Double(f64::NEG_INFINITY)], "-Infinity"),
            ("%.2f", &[Double(2.675)], "2.68"),
            ("%.2f", &[Double(9.995)], "10.00"),
            ("%.0f", &[Double(0.5)], "1"),
            ("%.0f", &[Double(2.5)], "3"),
            ("%#.0f", &[Double(2.5)], "3."),
            ("%f", &[Double(1e23)], "100000000000000000000000.000000"),
            ("%,.3f", &[Double(-1234567.0625)], "-1,234,567.063"),
            ("%(,012.1f", &[Double(-1234.56)], "(0001,234.6)"),
            ("%+010.2f", &[Double(4.56789)], "+000004.57"),
            ("% .1f", &[Double(2.0)], " 2.0"),
            ("%08.2f", &[Double(f64::INFINITY)], "Infinity"),
            ("%010f", &[Double(f64::NEG_INFINITY)], " -Infinity"),
            ("%+10f", &[Double(f64::NAN)], "       NaN"),
            ("%E", &[Double(f64::NAN)], "NAN"),
            ("%(f", &[Double(f64::NEG_INFINITY)], "(Infinity)"),
            ("%.10f", &[Float(0.1)], "0.1000000015"),
            ("%e", &[Double(0.0)], "0.000000e+00"),
            ("%e", &[Double(123456.789)], "1.234568e+05"),
            ("%.2e", &[Double(9.995)], "1.00e+01"),
            ("%E", &[Double(-1e-300)], "-1.000000E-300"),
            ("%.0e", &[Double(15.0)], "2e+01"),
            ("%#.0e", &[Double(15.0)], "2.e+01"),
            ("%012.3e", &[Double(-2.5)], "-002.500e+00"),
            ("%g", &[Double(0.0)], "0.00000"),
            ("%g", &[Double(1e-4)], "0.000100000"),
            ("%g", &[Double(9.99999e-5)], "9.99999e-05"),
            ("%g", &[Double(123456.0)], "123456"),
            ("%g", &[Double(999999.5)], "1.00000e+06"),
            ("%.3g", &[Double(0.00012345)], "0.000123"),
            ("%.0g", &[Double(25.0)], "3e+01"),
            ("%,g", &[Double(12345.678)], "12,345.7"),
            ("%G", &[Double(1e10)], "1.00000E+10"),
            ("%,010d", &[Int(1234567)], "01,234,567"),
            ("%(d", &[Int(-42)], "(42)"),
            ("%(08d", &[Int(-42)], "(000042)"),
            ("%+d %+d", &[Int(5), Int(-5)], "+5 -5"),
            ("% 5d", &[Int(42)], "   42"),
            ("%-6d|", &[Long(-7)], "-7    |"),
            ("%d", &[Long(i64::MIN)], "-9223372036854775808"),
            ("%,d", &[Long(i64::MIN)], "-9,223,372,036,854,775,808"),
            ("%d", &[Short(-300)], "-300"),
            ("%x", &[Int(-1)], "ffffffff"),
            ("%x", &[Byte(-1)], "ff"),
            ("%X", &[Short(-2)], "FFFE"),
            ("%x", &[Long(-1)], "ffffffffffffffff"),
            ("%o", &[Int(-8)], "37777777770"),
            ("%#x", &[Int(255)], "0xff"),
            ("%#X", &[Int(255)], "0XFF"),
            ("%#o", &[Int(8)], "010"),
            ("%#010x", &[Int(255)], "0x000000ff"),
            ("%08X", &[Int(-1)], "FFFFFFFF"),
            ("%X", &[Null], "NULL"),
            ("%010d", &[Null], "      null"),
            ("%.2s", &[Null], "nu"),
            ("%5.1f", &[Null], "    n"),
            ("%c", &[Char(65)], "A"),
            ("%c", &[Int(0x1F600)], "\u{1F600}"),
            ("%-3c|", &[Byte(97)], "a  |"),
            ("%C", &[Char(98)], "B"),
            ("%s", &[Str("hi".into())], "hi"),
            ("%S", &[Str("hi".into())], "HI"),
            ("%.1s", &[Str("hello".into())], "h"),
            ("%-6.3s|", &[Str("hello".into())], "hel   |"),
            ("%3s", &[Str("\u{1F600}".into())], " \u{1F600}"),
            ("%s", &[Long(-3)], "-3"),
            ("%s", &[Char(0x41)], "A"),
            ("%s", &[Bool(false)], "false"),
            ("%s", &[Double(1e7)], "1.0E7"),
            ("%s", &[Float(0.1)], "0.1"),
            ("%s", &[FloatOrDouble(0.5)], "0.5"),
            ("%h", &[Str("hello".into())], "5e918d2"),
            ("%h", &[Long(-1)], "0"),
            ("%h", &[Bool(true)], "4cf"),
            ("%H", &[Double(1.5)], "3FF80000"),
            ("%h", &[Short(-1)], "ffffffff"),
            ("%h", &[Null], "null"),
            ("%%|%5%|%-5%|", &[], "%|    %|%    |"),
            ("a%nb", &[], "a\nb"),
            ("%2$s %1$s %s %<s %s", &[Int(1), Int(2)], "2 1 1 1 2"),
            ("%s %<s %1$s %<s", &[Str("a".into())], "a a a a"),
        ];
        for (pattern, args, expected) in cases {
            assert_eq!(
                run(pattern, args).as_deref(),
                Some(*expected),
                "{pattern} {args:?}"
            );
        }
    }

    /// Calls the JDK throws on, or whose answer depends on what the runtime
    /// cannot tell, are left to the host rather than approximated.
    #[test]
    fn refuses_what_it_cannot_answer_exactly() {
        use Arg::*;
        let cases: &[(&str, &[Arg])] = &[
            // The JDK throws on each of these.
            ("%d", &[]),
            ("%2$s", &[Int(1)]),
            ("%<s", &[Int(1)]),
            ("%0$s", &[Int(1)]),
            ("%d", &[Double(1.0)]),
            ("%d", &[Char(65)]),
            ("%f", &[Int(1)]),
            ("%c", &[Int(-1)]),
            ("%c", &[Long(65)]),
            ("%c", &[Int(0x110000)]),
            ("%.2c", &[Char(65)]),
            ("%.2d", &[Int(1)]),
            ("%-d", &[Int(1)]),
            ("%-05d", &[Int(1)]),
            ("%+ d", &[Int(1)]),
            ("%#d", &[Int(1)]),
            ("%,x", &[Int(1)]),
            ("%+x", &[Int(1)]),
            ("%,e", &[Double(1.0)]),
            ("%#g", &[Double(1.0)]),
            ("%+s", &[Str("a".into())]),
            ("%#s", &[Str("a".into())]),
            ("%05s", &[Str("a".into())]),
            ("%--5s", &[Str("a".into())]),
            ("%5n", &[]),
            ("%.2%", &[]),
            ("%q", &[]),
            ("abc%", &[]),
            ("%.s", &[Str("a".into())]),
            // Not modelled.
            ("%a", &[Double(1.0)]),
            ("%tY", &[Long(0)]),
            ("%d", &[Opaque]),
            ("%s", &[Opaque]),
            ("%h", &[Opaque]),
            ("%S", &[Str("\u{e9}".into())]),
            ("%.1s", &[Str("\u{1F600}".into())]),
            ("%c", &[Int(0xD800)]),
            ("%s", &[Char(0xD800)]),
            // A boxed `Float` and `Double` print differently here.
            ("%s", &[FloatOrDouble(f64::from(0.1f32))]),
            ("%h", &[FloatOrDouble(1.5)]),
            // `Boolean.FALSE` and the string `"false"` differ here.
            ("%b", &[BoolOrString(false)]),
            ("%h", &[BoolOrString(true)]),
            ("%1$<s", &[Int(1)]),
            ("%99999s", &[Int(1)]),
        ];
        for (pattern, args) in cases {
            assert_eq!(run(pattern, args), None, "{pattern} {args:?}");
        }
    }

    #[test]
    fn a_null_argument_array_formats_every_argument_as_null() {
        // `String.format("%s %b %2$d", (Object[]) null)` on the JDK.
        assert_eq!(
            format("%s %b %2$d", None).as_deref(),
            Some("null false null")
        );
    }
}
