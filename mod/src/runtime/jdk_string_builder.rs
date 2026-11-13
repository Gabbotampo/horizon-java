//! `StringBuilder` and `StringBuffer`, as Java 25's `AbstractStringBuilder`
//! behaves, over a builder this invocation constructed.
//!
//! A builder's text lives in [`Runtime::string_builders`] (keyed by the
//! object's marker), so `append` mutates the receiver and a caller that drops
//! the return value still sees every append. This module answers the whole
//! public surface the two classes share — `append` and `insert` in every
//! overload, `delete`, `deleteCharAt`, `replace`, `reverse`, `setCharAt`,
//! `setLength`, `charAt`, the code point members, `indexOf`/`lastIndexOf`,
//! `substring`, `getChars`, `repeat`, `compareTo` — and the same members
//! called through `CharSequence` or `Appendable` on such a builder.
//!
//! Every check and message is the JDK's, read from the bytecode
//! (`javap -c -p java.lang.AbstractStringBuilder`) and confirmed on Java 25,
//! the runtime Minecraft 26.3 ships. JDK 27's bytecode is the same for every
//! member this models (25 only adds the package-private concat helpers `mix`
//! and `prepend`), and both answer the differential identically.
//! `checkIndex` throws `StringIndexOutOfBoundsException: Index i out of
//! bounds for length n`; `checkFromToIndex` `Range [a, b) out of bounds for
//! length n`, as a `StringIndexOutOfBoundsException` or a plain
//! `IndexOutOfBoundsException` depending on the formatter the method passes
//! (the `CharSequence` ranges and `codePointCount` use the plain one); an
//! `insert` offset is checked as the range `[offset, count)`; `delete` and
//! `replace` clamp `end` to the length before checking; null arguments raise
//! the helpful `NullPointerException` text Java 25 prints (its classes keep
//! parameter names, so the messages name `str`, `seq`, `dst`, `tgtStr`).
//! `float`/`double` text is `Float.toString`/`Double.toString`
//! ([`crate::runtime::jdk_float_text`]).
//!
//! Left to the host, so counted as unknown, never approximated:
//! - a result holding a lone surrogate: a `String` here is Rust text, which
//!   cannot hold one (`setCharAt` of one half of a pair, an `append(char)` of
//!   a single surrogate, a `substring` that splits a pair);
//! - `capacity()`: the JDK's growth policy is not tracked;
//! - a capacity or length past [`MAX_UNITS`]: whether the JDK allocates it or
//!   throws `OutOfMemoryError` depends on the heap it runs with;
//! - an argument whose text is runtime state this module cannot read (a jar
//!   object implementing `CharSequence`), and `insert` of a builder into
//!   itself (the JDK reads it after shifting it);
//! - `chars`, `codePoints`, `offsetByCodePoints`.
//!
//! A counted call that may have changed the text (every one above but
//! `capacity` and the reads) also forgets the builder, and a builder that
//! reaches the host without this model is counted there: the standard host no
//! longer answers `append`/`toString`/`length` on a `StringBuilder#n` it never
//! saw. So nothing is answered from text the JDK's builder no longer holds.

use super::*;
use crate::runtime::jdk_float_text::{double_to_string, float_to_string};

/// Longest text, capacity or length a call may ask for before it is left to
/// the host: past it, the JDK's answer (`OutOfMemoryError` or not) depends on
/// the heap the JVM was given.
const MAX_UNITS: i64 = 1 << 24;

/// One argument, resolved from its runtime value by the parameter type.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Arg {
    /// `int`, `char`, `boolean`, `short`, `byte`.
    Int(i32),
    Long(i64),
    /// A `float`, widened to `double` exactly.
    Float(f64),
    Double(f64),
    /// A `String`, `CharSequence` or `Object` argument's text; `None` is null.
    Text(Option<String>),
    /// A `char[]` argument's code units; `None` is null.
    Chars(Option<Vec<u16>>),
}

/// What a call returns.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Answer {
    /// The receiver (`append`, `insert`, …).
    This,
    Void,
    Int(i32),
    Text(String),
    /// `getChars`: write these units into the destination array at `at`.
    WriteChars {
        at: usize,
        units: Vec<u16>,
    },
}

/// The effect of one call on a builder's text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Outcome {
    /// The call returned; `text` is the builder's new contents when it changed.
    Done {
        text: Option<String>,
        answer: Answer,
    },
    /// The call threw `class` (internal form) with `message`.
    Throw {
        class: &'static str,
        message: String,
    },
    /// Not answered: the caller counts the call.
    Unmodelled,
}

const SIOOBE: &str = "java/lang/StringIndexOutOfBoundsException";
const IOOBE: &str = "java/lang/IndexOutOfBoundsException";
const NPE: &str = "java/lang/NullPointerException";
const IAE: &str = "java/lang/IllegalArgumentException";

fn throw(class: &'static str, message: String) -> Outcome {
    Outcome::Throw { class, message }
}

/// `Preconditions.checkIndex`: `0 <= index < length`.
fn check_index(index: i32, length: i32) -> Result<(), Outcome> {
    if index < 0 || index >= length {
        return Err(throw(
            SIOOBE,
            format!("Index {index} out of bounds for length {length}"),
        ));
    }
    Ok(())
}

/// `Preconditions.checkFromToIndex`: `0 <= from <= to <= length`, thrown as
/// `class`.
fn check_range(class: &'static str, from: i32, to: i32, length: i32) -> Result<(), Outcome> {
    if from < 0 || from > to || to > length {
        return Err(throw(
            class,
            format!("Range [{from}, {to}) out of bounds for length {length}"),
        ));
    }
    Ok(())
}

/// `String.checkOffset`: `checkFromToIndex(offset, length, length)`.
fn check_offset(offset: i32, length: i32) -> Result<(), Outcome> {
    check_range(SIOOBE, offset, length, length)
}

fn units_of(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn len(units: &[u16]) -> i32 {
    units.len() as i32
}

fn is_high(unit: u16) -> bool {
    (0xD800..=0xDBFF).contains(&unit)
}

fn is_low(unit: u16) -> bool {
    (0xDC00..=0xDFFF).contains(&unit)
}

fn pair(high: u16, low: u16) -> i32 {
    ((i32::from(high) - 0xD800) << 10) + (i32::from(low) - 0xDC00) + 0x10000
}

/// `String.valueOf` of a primitive or text argument, as `append`/`insert`
/// print it; `None` for an argument kind the descriptor does not print.
fn printed(param: &str, arg: &Arg) -> Option<Vec<u16>> {
    let text = match (param, arg) {
        ("Z", Arg::Int(value)) => (if *value != 0 { "true" } else { "false" }).to_owned(),
        ("C", Arg::Int(value)) => return Some(vec![*value as u16]),
        ("I" | "S" | "B", Arg::Int(value)) => value.to_string(),
        ("J", Arg::Long(value)) => value.to_string(),
        ("F", Arg::Float(value)) => float_to_string(*value),
        ("D", Arg::Double(value)) => double_to_string(*value),
        (_, Arg::Text(text)) => text.clone().unwrap_or_else(|| "null".into()),
        _ => return None,
    };
    Some(units_of(&text))
}

/// The parameter types of a method descriptor, one entry per argument
/// (`(ILjava/lang/String;)V` → `["I", "Ljava/lang/String;"]`).
pub(crate) fn parameters(descriptor: &str) -> Vec<&str> {
    let inner = descriptor
        .strip_prefix('(')
        .and_then(|rest| rest.split_once(')'))
        .map_or("", |(inner, _)| inner);
    let mut out = Vec::new();
    let mut at = 0;
    let bytes = inner.as_bytes();
    while at < bytes.len() {
        let start = at;
        while bytes[at] == b'[' {
            at += 1;
        }
        if bytes[at] == b'L' {
            at += inner[at..]
                .find(';')
                .map_or(inner.len() - at, |end| end + 1);
        } else {
            at += 1;
        }
        out.push(&inner[start..at]);
    }
    out
}

/// One `AbstractStringBuilder` member on a builder holding `text`, by name and
/// parameter list; `args` are already resolved by [`parameters`].
pub(crate) fn call(name: &str, descriptor: &str, text: &str, args: &[Arg]) -> Outcome {
    match run(name, descriptor, text, args) {
        Ok(outcome) | Err(outcome) => outcome,
    }
}

/// The builder's new units as its text, or the call left to the host when
/// they hold a lone surrogate.
fn store(units: Vec<u16>, answer: Answer) -> Result<Outcome, Outcome> {
    match String::from_utf16(&units) {
        Ok(text) => Ok(Outcome::Done {
            text: Some(text),
            answer,
        }),
        Err(_) => Err(Outcome::Unmodelled),
    }
}

fn answer(answer: Answer) -> Result<Outcome, Outcome> {
    Ok(Outcome::Done { text: None, answer })
}

fn int(at: &[Arg], index: usize) -> Result<i32, Outcome> {
    match at.get(index) {
        Some(Arg::Int(value)) => Ok(*value),
        _ => Err(Outcome::Unmodelled),
    }
}

fn chars(at: &[Arg], index: usize) -> Result<Option<&Vec<u16>>, Outcome> {
    match at.get(index) {
        Some(Arg::Chars(chars)) => Ok(chars.as_ref()),
        _ => Err(Outcome::Unmodelled),
    }
}

fn text_arg(at: &[Arg], index: usize) -> Result<Option<&str>, Outcome> {
    match at.get(index) {
        Some(Arg::Text(text)) => Ok(text.as_deref()),
        _ => Err(Outcome::Unmodelled),
    }
}

/// The helpful message Java 25 prints for `str.length` on a null `char[]`.
fn null_array(name: &str) -> Outcome {
    throw(
        NPE,
        format!("Cannot read the array length because \"{name}\" is null"),
    )
}

/// `units` with `added` spliced in at `at`.
fn spliced(units: &[u16], at: usize, removed: usize, added: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(units.len() + added.len());
    out.extend_from_slice(&units[..at]);
    out.extend_from_slice(added);
    out.extend_from_slice(&units[at + removed..]);
    out
}

fn too_long(length: i64) -> Result<(), Outcome> {
    if length > MAX_UNITS {
        return Err(Outcome::Unmodelled);
    }
    Ok(())
}

fn run(name: &str, descriptor: &str, text: &str, args: &[Arg]) -> Result<Outcome, Outcome> {
    let params = parameters(descriptor);
    let units = units_of(text);
    let count = len(&units);
    let params: Vec<&str> = params.into_iter().collect();
    match (name, params.as_slice()) {
        ("<init>", []) => store(Vec::new(), Answer::Void),
        ("<init>", ["I"]) => {
            let capacity = int(args, 0)?;
            if capacity < 0 {
                return Err(throw(
                    "java/lang/NegativeArraySizeException",
                    capacity.to_string(),
                ));
            }
            too_long(i64::from(capacity))?;
            store(Vec::new(), Answer::Void)
        }
        ("<init>", ["Ljava/lang/String;"]) => match text_arg(args, 0)? {
            Some(initial) => store(units_of(initial), Answer::Void),
            None => Err(throw(
                NPE,
                "Cannot invoke \"String.length()\" because \"str\" is null".into(),
            )),
        },
        ("<init>", ["Ljava/lang/CharSequence;"]) => match text_arg(args, 0)? {
            Some(initial) => store(units_of(initial), Answer::Void),
            None => Err(throw(
                NPE,
                "Cannot invoke \"java.lang.CharSequence.length()\" because \"seq\" is null".into(),
            )),
        },
        ("length", []) => answer(Answer::Int(count)),
        ("isEmpty", []) => answer(Answer::Int(i32::from(count == 0))),
        ("toString", []) => answer(Answer::Text(text.to_owned())),
        ("capacity", []) => Err(Outcome::Unmodelled),
        ("ensureCapacity", ["I"]) => {
            too_long(i64::from(int(args, 0)?))?;
            answer(Answer::Void)
        }
        ("trimToSize", []) => answer(Answer::Void),
        ("charAt", ["I"]) => {
            let index = int(args, 0)?;
            check_index(index, count)?;
            answer(Answer::Int(i32::from(units[index as usize])))
        }
        ("codePointAt", ["I"]) => {
            let index = int(args, 0)?;
            check_index(index, count)?;
            let at = index as usize;
            let high = units[at];
            Ok(match units.get(at + 1) {
                Some(&low) if is_high(high) && is_low(low) => Outcome::Done {
                    text: None,
                    answer: Answer::Int(pair(high, low)),
                },
                _ => Outcome::Done {
                    text: None,
                    answer: Answer::Int(i32::from(high)),
                },
            })
        }
        ("codePointBefore", ["I"]) => {
            let index = int(args, 0)?;
            check_index(index.wrapping_sub(1), count)?;
            let at = (index - 1) as usize;
            let low = units[at];
            let value = match at.checked_sub(1).map(|before| units[before]) {
                Some(high) if is_low(low) && is_high(high) => pair(high, low),
                _ => i32::from(low),
            };
            answer(Answer::Int(value))
        }
        ("codePointCount", ["I", "I"]) => {
            let (begin, end) = (int(args, 0)?, int(args, 1)?);
            check_range(IOOBE, begin, end, count)?;
            let (begin, end) = (begin as usize, end as usize);
            let mut points = (end - begin) as i32;
            let mut at = begin;
            while at < end {
                let unit = units[at];
                at += 1;
                if is_high(unit) && at < end && is_low(units[at]) {
                    points -= 1;
                    at += 1;
                }
            }
            answer(Answer::Int(points))
        }
        ("setCharAt", ["I", "C"]) => {
            let (index, unit) = (int(args, 0)?, int(args, 1)?);
            check_index(index, count)?;
            let mut out = units;
            out[index as usize] = unit as u16;
            store(out, Answer::Void)
        }
        ("setLength", ["I"]) => {
            let length = int(args, 0)?;
            if length < 0 {
                return Err(throw(
                    SIOOBE,
                    format!("String index out of range: {length}"),
                ));
            }
            too_long(i64::from(length))?;
            let mut out = units;
            out.resize(length as usize, 0);
            store(out, Answer::Void)
        }
        ("deleteCharAt", ["I"]) => {
            let index = int(args, 0)?;
            check_index(index, count)?;
            store(spliced(&units, index as usize, 1, &[]), Answer::This)
        }
        ("delete", ["I", "I"]) => {
            let start = int(args, 0)?;
            let end = int(args, 1)?.min(count);
            check_range(SIOOBE, start, end, count)?;
            let (start, end) = (start as usize, end as usize);
            store(spliced(&units, start, end - start, &[]), Answer::This)
        }
        ("replace", ["I", "I", "Ljava/lang/String;"]) => {
            let start = int(args, 0)?;
            let end = int(args, 1)?.min(count);
            check_range(SIOOBE, start, end, count)?;
            let Some(with) = text_arg(args, 2)? else {
                return Err(throw(
                    NPE,
                    "Cannot invoke \"String.length()\" because \"str\" is null".into(),
                ));
            };
            let (start, end) = (start as usize, end as usize);
            store(
                spliced(&units, start, end - start, &units_of(with)),
                Answer::This,
            )
        }
        ("reverse", []) => {
            // The JDK swaps code units, then swaps every reversed valid
            // surrogate pair back; on text with no lone surrogate that is
            // reversing the code points.
            store(
                units_of(&text.chars().rev().collect::<String>()),
                Answer::This,
            )
        }
        ("substring", ["I"]) => substring(&units, int(args, 0)?, count),
        ("substring" | "subSequence", ["I", "I"]) => {
            substring(&units, int(args, 0)?, int(args, 1)?)
        }
        ("indexOf", ["Ljava/lang/String;"]) => index_of(&units, text_arg(args, 0)?, 0),
        ("indexOf", ["Ljava/lang/String;", "I"]) => {
            index_of(&units, text_arg(args, 0)?, int(args, 1)?)
        }
        ("lastIndexOf", ["Ljava/lang/String;"]) => last_index_of(&units, text_arg(args, 0)?, count),
        ("lastIndexOf", ["Ljava/lang/String;", "I"]) => {
            last_index_of(&units, text_arg(args, 0)?, int(args, 1)?)
        }
        ("compareTo", [_]) => {
            let Some(other) = text_arg(args, 0)? else {
                return Err(throw(
                    NPE,
                    "Cannot read field \"value\" because \"another\" is null".into(),
                ));
            };
            let other = units_of(other);
            let differing = units.iter().zip(&other).find(|(a, b)| a != b);
            answer(Answer::Int(match differing {
                Some((a, b)) => i32::from(*a) - i32::from(*b),
                None => count - len(&other),
            }))
        }
        ("getChars", ["I", "I", "[C", "I"]) => {
            let (begin, end) = (int(args, 0)?, int(args, 1)?);
            check_range(SIOOBE, begin, end, count)?;
            let Some(destination) = chars(args, 2)? else {
                return Err(null_array("dst"));
            };
            let at = int(args, 3)?;
            let n = end - begin;
            check_range(IOOBE, at, at.wrapping_add(n), len(destination))?;
            answer(Answer::WriteChars {
                at: at as usize,
                units: units[begin as usize..end as usize].to_vec(),
            })
        }
        ("append", [param]) => {
            let added = match (*param, args.first()) {
                ("[C", Some(Arg::Chars(None))) => return Err(null_array("str")),
                ("[C", Some(Arg::Chars(Some(chars)))) => chars.clone(),
                (param, Some(arg)) => printed(param, arg).ok_or(Outcome::Unmodelled)?,
                _ => return Err(Outcome::Unmodelled),
            };
            too_long(i64::from(count) + added.len() as i64)?;
            let mut out = units;
            out.extend_from_slice(&added);
            store(out, Answer::This)
        }
        ("append", ["Ljava/lang/CharSequence;", "I", "I"]) => {
            let source = units_of(text_arg(args, 0)?.unwrap_or("null"));
            let (start, end) = (int(args, 1)?, int(args, 2)?);
            check_range(IOOBE, start, end, len(&source))?;
            let mut out = units;
            out.extend_from_slice(&source[start as usize..end as usize]);
            store(out, Answer::This)
        }
        ("append", ["[C", "I", "I"]) => {
            let Some(source) = chars(args, 0)? else {
                return Err(null_array("str"));
            };
            let (offset, length) = (int(args, 1)?, int(args, 2)?);
            let end = offset.wrapping_add(length);
            check_range(IOOBE, offset, end, len(source))?;
            let mut out = units;
            out.extend_from_slice(&source[offset as usize..end as usize]);
            store(out, Answer::This)
        }
        ("appendCodePoint", ["I"]) => {
            let added = code_point_units(int(args, 0)?)?;
            let mut out = units;
            out.extend_from_slice(&added);
            store(out, Answer::This)
        }
        ("insert", ["I", "[C"]) => {
            let offset = int(args, 0)?;
            check_offset(offset, count)?;
            let Some(source) = chars(args, 1)? else {
                return Err(null_array("str"));
            };
            store(spliced(&units, offset as usize, 0, source), Answer::This)
        }
        ("insert", ["I", "[C", "I", "I"]) => {
            let index = int(args, 0)?;
            check_offset(index, count)?;
            let Some(source) = chars(args, 1)? else {
                return Err(null_array("str"));
            };
            let (offset, length) = (int(args, 2)?, int(args, 3)?);
            let end = offset.wrapping_add(length);
            check_range(SIOOBE, offset, end, len(source))?;
            store(
                spliced(
                    &units,
                    index as usize,
                    0,
                    &source[offset as usize..end as usize],
                ),
                Answer::This,
            )
        }
        ("insert", ["I", "Ljava/lang/CharSequence;", "I", "I"]) => {
            let offset = int(args, 0)?;
            let source = units_of(text_arg(args, 1)?.unwrap_or("null"));
            check_offset(offset, count)?;
            let (start, end) = (int(args, 2)?, int(args, 3)?);
            check_range(IOOBE, start, end, len(&source))?;
            store(
                spliced(
                    &units,
                    offset as usize,
                    0,
                    &source[start as usize..end as usize],
                ),
                Answer::This,
            )
        }
        ("insert", ["I", param]) => {
            let offset = int(args, 0)?;
            check_offset(offset, count)?;
            let added = args
                .get(1)
                .and_then(|arg| printed(param, arg))
                .ok_or(Outcome::Unmodelled)?;
            too_long(i64::from(count) + added.len() as i64)?;
            store(spliced(&units, offset as usize, 0, &added), Answer::This)
        }
        ("repeat", ["I", "I"]) => {
            let (code_point, times) = (int(args, 0)?, int(args, 1)?);
            if times < 0 {
                return Err(throw(IAE, format!("count is negative: {times}")));
            }
            if times == 0 {
                return answer(Answer::This);
            }
            let once = code_point_units(code_point)?;
            repeated(units, &once, times)
        }
        ("repeat", ["Ljava/lang/CharSequence;", "I"]) => {
            let times = int(args, 1)?;
            if times < 0 {
                return Err(throw(IAE, format!("count is negative: {times}")));
            }
            if times == 0 {
                return answer(Answer::This);
            }
            let once = units_of(text_arg(args, 0)?.unwrap_or("null"));
            repeated(units, &once, times)
        }
        _ => Err(Outcome::Unmodelled),
    }
}

/// `units` followed by `times` copies of `once`.
fn repeated(units: Vec<u16>, once: &[u16], times: i32) -> Result<Outcome, Outcome> {
    too_long(units.len() as i64 + once.len() as i64 * i64::from(times))?;
    let mut out = units;
    for _ in 0..times {
        out.extend_from_slice(once);
    }
    store(out, Answer::This)
}

/// The code units of `code_point`, or the JDK's `IllegalArgumentException`
/// (`Character.toChars`) for an invalid one.
fn code_point_units(code_point: i32) -> Result<Vec<u16>, Outcome> {
    if (code_point as u32) >> 16 == 0 {
        return Ok(vec![code_point as u16]);
    }
    if !(0x10000..0x110000).contains(&code_point) {
        return Err(throw(
            IAE,
            format!("Not a valid Unicode code point: 0x{code_point:X}"),
        ));
    }
    let offset = code_point - 0x10000;
    Ok(vec![
        (0xD800 + (offset >> 10)) as u16,
        (0xDC00 + (offset & 0x3FF)) as u16,
    ])
}

fn substring(units: &[u16], start: i32, end: i32) -> Result<Outcome, Outcome> {
    check_range(SIOOBE, start, end, len(units))?;
    match String::from_utf16(&units[start as usize..end as usize]) {
        Ok(text) => answer(Answer::Text(text)),
        Err(_) => Err(Outcome::Unmodelled),
    }
}

/// `String.indexOf(byte[], coder, count, tgtStr, fromIndex)`: `fromIndex`
/// clamped to `[0, count]`, an empty target found at it.
fn index_of(units: &[u16], target: Option<&str>, from: i32) -> Result<Outcome, Outcome> {
    let count = len(units);
    let from = from.clamp(0, count);
    let Some(target) = target else {
        return Err(throw(
            NPE,
            "Cannot invoke \"String.length()\" because \"tgtStr\" is null".into(),
        ));
    };
    let target = units_of(target);
    let found = if len(&target) > count - from {
        -1
    } else if target.is_empty() {
        from
    } else {
        (from as usize..=units.len() - target.len())
            .find(|&at| units[at..at + target.len()] == target[..])
            .map_or(-1, |at| at as i32)
    };
    answer(Answer::Int(found))
}

/// `String.lastIndexOf(byte[], coder, count, tgtStr, fromIndex)`:
/// `fromIndex` capped at `count - target length`, negative is not found.
fn last_index_of(units: &[u16], target: Option<&str>, from: i32) -> Result<Outcome, Outcome> {
    let Some(target) = target else {
        return Err(throw(
            NPE,
            "Cannot read field \"value\" because \"tgtStr\" is null".into(),
        ));
    };
    let target = units_of(target);
    let from = from.min(len(units) - len(&target));
    let found = if from < 0 {
        -1
    } else if target.is_empty() {
        from
    } else {
        (0..=from as usize)
            .rev()
            .find(|&at| units[at..at + target.len()] == target[..])
            .map_or(-1, |at| at as i32)
    };
    answer(Answer::Int(found))
}

/// The builder classes and the interfaces a builder is called through.
fn builder_class(owner: &str) -> bool {
    matches!(owner, "java/lang/StringBuilder" | "java/lang/StringBuffer")
}

impl<H: Host> Runtime<'_, H> {
    /// Answer a `StringBuilder`/`StringBuffer` member (or a `CharSequence`/
    /// `Appendable` one) on a builder this invocation constructed, or `None`
    /// to leave the call to the host, which counts it.
    pub(crate) fn invoke_string_builder(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(marker) = receiver.and_then(expect_ref).cloned() else {
            return Ok(None);
        };
        let constructing = r.name == "<init>";
        let typed_as_builder = builder_class(&r.owner);
        let through_interface = matches!(
            r.owner.as_str(),
            "java/lang/CharSequence" | "java/lang/Appendable" | "java/lang/AbstractStringBuilder"
        );
        if !(typed_as_builder || through_interface && !constructing) {
            return Ok(None);
        }
        let text = if constructing {
            if !typed_as_builder {
                return Ok(None);
            }
            String::new()
        } else {
            match self.string_builders.get(&marker) {
                Some(text) => text.clone(),
                None => return Ok(None),
            }
        };
        let resolved = self.builder_arguments(r, &marker, args)?;
        let Some(resolved) = resolved else {
            self.forget_builder_after(r, &marker);
            return Ok(None);
        };
        match call(&r.name, &r.descriptor, &text, &resolved) {
            Outcome::Unmodelled => {
                self.forget_builder_after(r, &marker);
                Ok(None)
            }
            Outcome::Throw { class, message } => Err(NativeError::UncaughtException {
                class: class.into(),
                message: Some(message),
                frame: None,
            }),
            Outcome::Done { text, answer } => {
                if let Some(text) = text {
                    self.string_builders.insert(marker.clone(), text);
                }
                Ok(Some(match answer {
                    Answer::This => Value::Ref(Some(marker)),
                    Answer::Void => Value::Void,
                    Answer::Int(value) => Value::Int(value),
                    Answer::Text(text) => Value::Ref(Some(text)),
                    Answer::WriteChars { at, units } => {
                        let Some(array) = args.get(2).and_then(expect_ref) else {
                            return Ok(None);
                        };
                        let Some(elements) = self.arrays.get_mut(array) else {
                            return Ok(None);
                        };
                        for (offset, unit) in units.into_iter().enumerate() {
                            elements[at + offset] = Value::Int(i32::from(unit));
                        }
                        Value::Void
                    }
                }))
            }
        }
    }

    /// Forget a builder whose text a counted call may have changed, so every
    /// later read of it is counted too rather than answered from the text it
    /// held before (a lone surrogate the JDK now holds, an argument this
    /// module could not read). A call that only reads keeps it.
    fn forget_builder_after(&mut self, r: &MethodRef, marker: &str) {
        let mutates = matches!(
            r.name.as_str(),
            "<init>"
                | "append"
                | "appendCodePoint"
                | "insert"
                | "delete"
                | "deleteCharAt"
                | "replace"
                | "reverse"
                | "setCharAt"
                | "setLength"
                | "repeat"
        );
        if mutates {
            self.string_builders.remove(marker);
        }
    }

    /// Each argument of a builder call, resolved by its parameter type, or
    /// `None` when one holds state this module cannot read (the call is then
    /// counted).
    fn builder_arguments(
        &mut self,
        r: &MethodRef,
        receiver: &str,
        args: &[Value],
    ) -> Result<Option<Vec<Arg>>, NativeError> {
        let mut resolved = Vec::with_capacity(args.len());
        for (param, value) in parameters(&r.descriptor).into_iter().zip(args) {
            let arg = match (param, value) {
                ("I" | "C" | "Z" | "S" | "B", Value::Int(value)) => Arg::Int(*value),
                ("J", Value::Long(value)) => Arg::Long(*value),
                ("F", Value::Float(value)) => Arg::Float(*value),
                ("D", Value::Float(value)) => Arg::Double(*value),
                ("[C", Value::Ref(None)) => Arg::Chars(None),
                ("[C", Value::Ref(Some(array))) => {
                    let Some(elements) = self.arrays.get(array) else {
                        return Ok(None);
                    };
                    let mut units = Vec::with_capacity(elements.len());
                    for element in elements {
                        let Value::Int(unit) = element else {
                            return Ok(None);
                        };
                        units.push(*unit as u16);
                    }
                    Arg::Chars(Some(units))
                }
                (_, Value::Ref(None)) if param.starts_with('L') => Arg::Text(None),
                // `append(Object)` of a box is `String.valueOf`: its value.
                ("Ljava/lang/Object;", Value::Boxed(boxed)) => Arg::Text(Some(boxed.text())),
                // An `Integer`/`Long`/`Double` the runtime holds as its value
                // (one read back from a collection, a lambda's box).
                ("Ljava/lang/Object;", Value::Int(_) | Value::Long(_) | Value::Float(_)) => {
                    Arg::Text(Some(crate::runtime::value_text(value)))
                }
                (_, Value::Ref(Some(marker))) if param.starts_with('L') => {
                    // `insert` reads its argument after shifting the
                    // receiver, so a builder inserted into itself is not
                    // the text it held.
                    if marker == receiver && r.name == "insert" {
                        return Ok(None);
                    }
                    match self.builder_argument_text(param, marker) {
                        Some(text) => Arg::Text(Some(text)),
                        None => return Ok(None),
                    }
                }
                _ => return Ok(None),
            };
            resolved.push(arg);
        }
        Ok(Some(resolved))
    }

    /// The text a reference argument prints or reads as: a tracked
    /// builder's contents, a string's own text, and for an `Object`
    /// parameter what `String.valueOf` gives a box or a host-modelled object.
    /// `None` for an object whose text only its own class knows.
    fn builder_argument_text(&mut self, param: &str, marker: &str) -> Option<String> {
        if let Some(text) = self.string_builders.get(marker) {
            return Some(text.clone());
        }
        match param {
            "Ljava/lang/String;" => Some(marker.to_owned()),
            "Ljava/lang/Object;" => {
                // `append(Object)` of a jar object was rewritten to the
                // `String` overload by `jar_text.rs` (#70). What reaches here
                // is a string, a box or a host object; a jar object passed to
                // `insert(int, Object)` prints its own `toString`, which this
                // module cannot run, so that call is counted.
                if let Some(text) = crate::runtime::java_tostring::modelled_marker(self.host, marker) {
                    return Some(text);
                }
                crate::runtime::jar_text::object_marker_class(marker)
                    .is_none()
                    .then(|| marker.to_owned())
            }
            // A `CharSequence`/`StringBuilder`/`StringBuffer` parameter: an
            // object this run built from a jar class (or one the host
            // models, a `CharBuffer`) has text only its `charAt` knows.
            _ => crate::runtime::jar_text::object_marker_class(marker)
                .is_none()
                .then(|| marker.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lone_surrogate_result_is_left_to_the_host() {
        // `"😀"` is one code point, two units: replacing either half, or
        // appending a single half, would leave a lone surrogate.
        let set = call("setCharAt", "(IC)V", "😀", &[Arg::Int(0), Arg::Int(0x41)]);
        assert_eq!(set, Outcome::Unmodelled);
        let append = call(
            "append",
            "(C)Ljava/lang/StringBuilder;",
            "a",
            &[Arg::Int(0xD83D)],
        );
        assert_eq!(append, Outcome::Unmodelled);
        let split = call(
            "substring",
            "(II)Ljava/lang/String;",
            "😀",
            &[Arg::Int(0), Arg::Int(1)],
        );
        assert_eq!(split, Outcome::Unmodelled);
    }

    #[test]
    fn descriptors_split_into_parameters() {
        assert_eq!(
            parameters("(I[CII)Ljava/lang/StringBuilder;"),
            ["I", "[C", "I", "I"]
        );
        assert_eq!(
            parameters("(ILjava/lang/CharSequence;II)Ljava/lang/StringBuilder;"),
            ["I", "Ljava/lang/CharSequence;", "I", "I"]
        );
        assert!(parameters("()V").is_empty());
    }
}
