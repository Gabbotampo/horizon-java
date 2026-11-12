//! `Double.toString` and `Float.toString` as JDK 19+ (and so JDK 27) print
//! them.
//!
//! Since JDK 19 (Raffaello Giulietti's `DoubleToDecimal`/`FloatToDecimal`)
//! the javadoc pins the digits exactly. With `R` the decimals that round to
//! `v` (round to nearest, ties to even) and `m` the shortest length in `R`:
//! take `T` = the decimals of `R` with length `m`, or, when `m == 1`, with
//! length 1 or 2; pick the one of `T` closest to `v`, and of two equally
//! close the one whose least significant digit is even. Then print it plain
//! for `10^-3 <= |v| < 10^7`, else in computerized scientific notation.
//!
//! Rust's `{:e}` gives a shortest member of `R` (so `m`), but not the javadoc's
//! choice between two equally short candidates (it rounds `-219738.625f` to
//! `...63`, the JDK to `...62`) nor the two-digit rule for `m == 1`
//! (`Double.MIN_VALUE` is `4.9E-324`, not `5.0E-324`). So the choice is made
//! here, exactly: `v`'s full decimal expansion (Rust prints it exactly to any
//! precision) places `v` between the two length-`n` decimals around it and
//! says which is closer or that it is a tie, and Rust's correctly rounded
//! parser says which of the two are in `R`. No floating-point arithmetic
//! decides anything.

/// Enough significant digits for the exact expansion of every `f64` (the
/// longest, a subnormal's, has 767).
const EXACT_DIGITS: usize = 800;

/// `Double.toString(value)`.
pub(crate) fn double_to_string(value: f64) -> String {
    java_text(value, false)
}

/// `Float.toString(value)`: `value` is the `float` widened to `double`
/// (exactly, as every `float` is a `double`).
pub(crate) fn float_to_string(value: f64) -> String {
    java_text(value, true)
}

fn java_text(value: f64, float: bool) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .into();
    }
    let (digits, exponent) = shortest_digits(value.abs(), float);
    let mut text = String::new();
    if value < 0.0 {
        text.push('-');
    }
    text.push_str(&layout(&digits, exponent));
    text
}

/// The javadoc's decimal for a positive finite `value`: its digits (no
/// trailing zeros) and the decimal exponent of the first digit.
fn shortest_digits(value: f64, float: bool) -> (Vec<u8>, i32) {
    // `m`: the length of Rust's shortest round-tripping digits.
    let shortest = if float {
        format!("{:e}", value as f32)
    } else {
        format!("{value:e}")
    };
    let m = shortest
        .split('e')
        .next()
        .unwrap_or_default()
        .bytes()
        .filter(u8::is_ascii_digit)
        .count();
    // A length-1 decimal is a length-2 one ending in 0, so for `m == 1` the
    // candidates of lengths 1 and 2 are the length-2 ones.
    let n = m.max(2);

    // `v` exactly: `d.ddd…eE` with every digit of its expansion.
    let exact = format!("{value:.EXACT_DIGITS$e}");
    let (mantissa, exponent) = exact.split_once('e').expect("`{:e}` writes an exponent");
    let exponent: i32 = exponent.parse().expect("a decimal exponent");
    let all: Vec<u8> = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|digit| digit - b'0')
        .collect();
    let (head, rest) = all.split_at(n);

    // `low` is `v` cut to `n` digits, `high` the next `n`-digit decimal up.
    let low = head.to_vec();
    let (high, high_exponent) = increment(&low, exponent);
    // Where `v` sits between them: below, at or above the midpoint.
    let rest_is_zero = rest.iter().all(|&digit| digit == 0);
    if rest_is_zero {
        return trimmed(low, exponent);
    }
    let first = rest[0];
    let beyond_zero = rest[1..].iter().all(|&digit| digit == 0);
    let closer_high = first > 5 || (first == 5 && !beyond_zero);
    let tie = first == 5 && beyond_zero;

    let low_in = rounds_to(&low, exponent, value, float);
    let high_in = rounds_to(&high, high_exponent, value, float);
    let pick_high = match (low_in, high_in) {
        (true, true) if tie => high.last().is_some_and(|digit| digit % 2 == 0),
        (true, true) => closer_high,
        (false, true) => true,
        _ => false,
    };
    if pick_high {
        trimmed(high, high_exponent)
    } else {
        trimmed(low, exponent)
    }
}

/// `digits` (read as `d.ddd × 10^exponent`) plus one unit in the last place,
/// with the exponent moved up when the carry adds a digit (`99` → `100`).
fn increment(digits: &[u8], exponent: i32) -> (Vec<u8>, i32) {
    let mut out = digits.to_vec();
    for digit in out.iter_mut().rev() {
        if *digit == 9 {
            *digit = 0;
        } else {
            *digit += 1;
            return (out, exponent);
        }
    }
    // Every digit carried: `10…0` one place up, the same length.
    out.insert(0, 1);
    out.pop();
    (out, exponent + 1)
}

/// Whether the decimal `digits × 10^(exponent - len + 1)` rounds to `value`
/// (as a `float` when `float`). Rust's parsers round correctly, ties to
/// even, as Java's "usual round to nearest".
fn rounds_to(digits: &[u8], exponent: i32, value: f64, float: bool) -> bool {
    let mut text: String = digits
        .iter()
        .map(|digit| char::from(b'0' + digit))
        .collect();
    text.push('e');
    text.push_str(&(exponent - digits.len() as i32 + 1).to_string());
    if float {
        text.parse::<f32>()
            .is_ok_and(|parsed| f64::from(parsed) == value)
    } else {
        text.parse::<f64>().is_ok_and(|parsed| parsed == value)
    }
}

/// Drop trailing zeros, keeping one digit.
fn trimmed(mut digits: Vec<u8>, exponent: i32) -> (Vec<u8>, i32) {
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    (digits, exponent)
}

/// Java's layout of `d.ddd × 10^exponent`: plain with at least one fraction
/// digit for `-3 <= exponent < 7`, else `d.ddd` (at least one fraction digit)
/// `E` exponent.
fn layout(digits: &[u8], exponent: i32) -> String {
    let text: String = digits
        .iter()
        .map(|digit| char::from(b'0' + digit))
        .collect();
    if (-3..7).contains(&exponent) {
        if exponent < 0 {
            let zeros = "0".repeat((-exponent - 1) as usize);
            return format!("0.{zeros}{text}");
        }
        let point = exponent as usize + 1;
        if point < text.len() {
            return format!("{}.{}", &text[..point], &text[point..]);
        }
        let zeros = "0".repeat(point - text.len());
        return format!("{text}{zeros}.0");
    }
    let fraction = if text.len() > 1 { &text[1..] } else { "0" };
    format!("{}.{fraction}E{exponent}", &text[..1])
}
