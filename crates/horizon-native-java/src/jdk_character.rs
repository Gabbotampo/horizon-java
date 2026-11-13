//! `java.lang.Character`'s static methods, answered as Java 25 answers them:
//! Minecraft 26.3's launcher manifest pins Java 25 (`java-runtime-epsilon`),
//! so its Unicode 16.0 tables are what a mod's `Character` calls see. JDK 26+
//! moved to Unicode 17.0 and answers 5,486 code points differently (none
//! below U+0100).
//!
//! Every Unicode property comes from [`crate::jdk_character_table`], which
//! `tools/jdk_tables/CharacterTable.java` dumps from Java 25's own
//! `Character` class (Unicode 16.0), one run per stretch of code points with the
//! same answers. The generator checks, for every code point, the identities
//! this module derives from the table instead of storing them:
//!
//! - `isLetter`, `isDigit`, `isLetterOrDigit`, `isTitleCase`, `isSpaceChar`
//!   and `isDefined` are functions of `getType`;
//! - `digit(cp, radix)` is `digit(cp, 36)` when that is below `radix` (and
//!   `radix` is in `[MIN_RADIX, MAX_RADIX]`), else `-1`;
//! - every `char` overload is its `int` overload, the case mappings narrowed
//!   back to `char`.
//!
//! The arithmetic members (`isISOControl`, the surrogate helpers, `forDigit`,
//! `charCount`, `toCodePoint`, `compare`, `reverseBytes`) follow the JDK
//! source's formulas. A code point outside `[0, 0x10FFFF]` gets what Java 25's
//! `CharacterData.of` gives it (`CharacterDataUndefined`): unassigned, no
//! properties, mapped to itself, directionality `-1`.
//!
//! `Character.toString` of a lone surrogate is not answered: a `String` here
//! is Rust text, which cannot hold one, so that call stays a counted unknown.
//! So do `getName`, `codePointOf` and the `char[]`/`CharSequence` members,
//! which need runtime state.

use crate::jdk_character_table::{self as table, ROWS, RUNS, Row};
use crate::{MethodRef, NativeError, Value};

/// Unicode general categories, as `Character.getType` numbers them.
const UNASSIGNED: u8 = 0;
const TITLECASE_LETTER: u8 = 3;
const OTHER_LETTER: u8 = 5;
const DECIMAL_DIGIT_NUMBER: u8 = 9;
const SPACE_SEPARATOR: u8 = 12;
const PARAGRAPH_SEPARATOR: u8 = 14;

/// What Java 25 answers for a code point outside the Unicode range.
const UNDEFINED: Row = (UNASSIGNED, 0, -1, 0, 0, 0, -1, -1);

/// The table row of `code_point`.
fn row(code_point: i32) -> Row {
    let Ok(code_point) = u32::try_from(code_point) else {
        return UNDEFINED;
    };
    if code_point > 0x10FFFF {
        return UNDEFINED;
    }
    // The run that starts at or before `code_point`; the first starts at 0.
    let at = RUNS.partition_point(|&(start, _)| start <= code_point) - 1;
    ROWS[usize::from(RUNS[at].1)]
}

fn general_type(code_point: i32) -> u8 {
    row(code_point).0
}

fn has(code_point: i32, flag: u32) -> bool {
    row(code_point).1 & flag != 0
}

/// `Character.getType(codePoint)`.
pub(crate) fn get_type(code_point: i32) -> i32 {
    i32::from(general_type(code_point))
}

/// `Character.isLetter(codePoint)`: categories `Lu`, `Ll`, `Lt`, `Lm`, `Lo`.
pub(crate) fn is_letter(code_point: i32) -> bool {
    (1..=OTHER_LETTER).contains(&general_type(code_point))
}

/// `Character.digit(codePoint, radix)`.
pub(crate) fn digit(code_point: i32, radix: i32) -> i32 {
    if !(2..=36).contains(&radix) {
        return -1;
    }
    let value = i32::from(row(code_point).7);
    if (0..radix).contains(&value) {
        value
    } else {
        -1
    }
}

/// `Character.toUpperCase(codePoint)`: the simple mapping.
pub(crate) fn to_upper_case(code_point: i32) -> i32 {
    code_point.wrapping_add(row(code_point).3)
}

/// `Character.toLowerCase(codePoint)`: the simple mapping.
pub(crate) fn to_lower_case(code_point: i32) -> i32 {
    code_point.wrapping_add(row(code_point).4)
}

/// `Character.toTitleCase(codePoint)`: the simple mapping.
pub(crate) fn to_title_case(code_point: i32) -> i32 {
    code_point.wrapping_add(row(code_point).5)
}

/// `Character.isISOControl(codePoint)`: `\u0000`–`\u001F` and
/// `\u007F`–`\u009F`.
fn is_iso_control(code_point: i32) -> bool {
    code_point <= 0x9F && (code_point >= 0x7F || (code_point as u32) >> 5 == 0)
}

/// `Character.forDigit(digit, radix)`: `'\0'` for a digit or radix out of
/// range, else `0`–`9` then lower-case `a`–`z`.
fn for_digit(digit: i32, radix: i32) -> i32 {
    if digit >= radix || digit < 0 || !(2..=36).contains(&radix) {
        return 0;
    }
    if digit < 10 {
        i32::from(b'0') + digit
    } else {
        i32::from(b'a') - 10 + digit
    }
}

/// A `char` argument's code unit: the interpreter keeps `char`s as `int`s
/// already narrowed to 16 bits, so this narrows again only to be safe.
fn unit(value: i32) -> i32 {
    value & 0xFFFF
}

fn int_argument(args: &[Value], at: usize) -> Option<i32> {
    match args.get(at) {
        Some(Value::Int(value)) => Some(*value),
        _ => None,
    }
}

/// A `java.lang.Character` static call Java 25 answers from its tables or
/// formulas, or `None` for any other call (and for the ones this module
/// leaves counted: see the module docs).
pub(crate) fn static_call(r: &MethodRef, args: &[Value]) -> Option<Result<Value, NativeError>> {
    if r.owner == "java/lang/Character$UnicodeBlock" {
        return unicode_block_of(r, args);
    }
    if r.owner != "java/lang/Character" {
        return None;
    }
    let first = || int_argument(args, 0);
    let second = || int_argument(args, 1);
    // The `char` overloads take a `C` and narrow it; the `int` ones take the
    // code point as it is.
    let code_point = || {
        let value = first()?;
        Some(if r.descriptor.starts_with("(C") {
            unit(value)
        } else {
            value
        })
    };
    let boolean = |answer: bool| Some(Ok(Value::Int(i32::from(answer))));
    let int = |answer: i32| Some(Ok(Value::Int(answer)));
    // A case mapping's `char` overload casts the `int` answer back to `char`.
    let mapped = |answer: i32| {
        int(if r.descriptor == "(C)C" {
            unit(answer)
        } else {
            answer
        })
    };
    match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "isLetter"
            | "isDigit"
            | "isLetterOrDigit"
            | "isTitleCase"
            | "isSpaceChar"
            | "isDefined"
            | "isLowerCase"
            | "isUpperCase"
            | "isWhitespace"
            | "isJavaIdentifierStart"
            | "isJavaIdentifierPart"
            | "isUnicodeIdentifierStart"
            | "isUnicodeIdentifierPart"
            | "isIdentifierIgnorable"
            | "isMirrored"
            | "isISOControl",
            "(C)Z" | "(I)Z",
        )
        | ("isJavaLetter" | "isJavaLetterOrDigit" | "isSpace", "(C)Z")
        | (
            "isAlphabetic"
            | "isIdeographic"
            | "isEmoji"
            | "isEmojiPresentation"
            | "isEmojiModifier"
            | "isEmojiModifierBase"
            | "isEmojiComponent"
            | "isExtendedPictographic"
            | "isBmpCodePoint"
            | "isValidCodePoint"
            | "isSupplementaryCodePoint",
            "(I)Z",
        ) => {
            let code_point = code_point()?;
            let kind = general_type(code_point);
            let flag = |flag| has(code_point, flag);
            boolean(match r.name.as_str() {
                "isLetter" => is_letter(code_point),
                "isDigit" => kind == DECIMAL_DIGIT_NUMBER,
                "isLetterOrDigit" => is_letter(code_point) || kind == DECIMAL_DIGIT_NUMBER,
                "isTitleCase" => kind == TITLECASE_LETTER,
                "isSpaceChar" => (SPACE_SEPARATOR..=PARAGRAPH_SEPARATOR).contains(&kind),
                "isDefined" => kind != UNASSIGNED,
                "isLowerCase" => flag(table::LOWER),
                "isUpperCase" => flag(table::UPPER),
                "isWhitespace" => flag(table::WHITESPACE),
                "isJavaIdentifierStart" | "isJavaLetter" => flag(table::JAVA_ID_START),
                "isJavaIdentifierPart" | "isJavaLetterOrDigit" => flag(table::JAVA_ID_PART),
                "isUnicodeIdentifierStart" => flag(table::UNICODE_ID_START),
                "isUnicodeIdentifierPart" => flag(table::UNICODE_ID_PART),
                "isIdentifierIgnorable" => flag(table::ID_IGNORABLE),
                "isMirrored" => flag(table::MIRRORED),
                "isAlphabetic" => flag(table::ALPHABETIC),
                "isIdeographic" => flag(table::IDEOGRAPHIC),
                "isEmoji" => flag(table::EMOJI),
                "isEmojiPresentation" => flag(table::EMOJI_PRESENTATION),
                "isEmojiModifier" => flag(table::EMOJI_MODIFIER),
                "isEmojiModifierBase" => flag(table::EMOJI_MODIFIER_BASE),
                "isEmojiComponent" => flag(table::EMOJI_COMPONENT),
                "isExtendedPictographic" => flag(table::EXTENDED_PICTOGRAPHIC),
                "isISOControl" => is_iso_control(code_point),
                // The deprecated `isSpace`: `\t \n \f \r` and space only.
                "isSpace" => matches!(code_point, 0x09 | 0x0A | 0x0C | 0x0D | 0x20),
                "isBmpCodePoint" => (code_point as u32) >> 16 == 0,
                "isValidCodePoint" => (code_point as u32) >> 16 < 0x11,
                _ => (0x10000..0x110000).contains(&code_point),
            })
        }
        ("isHighSurrogate", "(C)Z") => boolean((0xD800..=0xDBFF).contains(&unit(first()?))),
        ("isLowSurrogate", "(C)Z") => boolean((0xDC00..=0xDFFF).contains(&unit(first()?))),
        ("isSurrogate", "(C)Z") => boolean((0xD800..=0xDFFF).contains(&unit(first()?))),
        ("isSurrogatePair", "(CC)Z") => boolean(
            (0xD800..=0xDBFF).contains(&unit(first()?))
                && (0xDC00..=0xDFFF).contains(&unit(second()?)),
        ),
        ("getType", "(C)I" | "(I)I") => int(get_type(code_point()?)),
        ("getDirectionality", "(C)B" | "(I)B") => int(i32::from(row(code_point()?).2)),
        ("getNumericValue", "(C)I" | "(I)I") => int(row(code_point()?).6),
        ("toUpperCase", "(C)C" | "(I)I") => mapped(to_upper_case(code_point()?)),
        ("toLowerCase", "(C)C" | "(I)I") => mapped(to_lower_case(code_point()?)),
        ("toTitleCase", "(C)C" | "(I)I") => mapped(to_title_case(code_point()?)),
        ("digit", "(CI)I" | "(II)I") => int(digit(code_point()?, second()?)),
        ("forDigit", "(II)C") => int(for_digit(first()?, second()?)),
        ("charCount", "(I)I") => int(if first()? >= 0x10000 { 2 } else { 1 }),
        ("toCodePoint", "(CC)I") => {
            let (high, low) = (unit(first()?), unit(second()?));
            int((high << 10) + low + (0x10000 - (0xD800 << 10) - 0xDC00))
        }
        ("highSurrogate", "(I)C") => int(unit(
            ((first()? as u32 >> 10) + (0xD800 - (0x10000 >> 10))) as i32,
        )),
        ("lowSurrogate", "(I)C") => int(unit((first()? & 0x3FF) + 0xDC00)),
        ("compare", "(CC)I") => int(unit(first()?) - unit(second()?)),
        ("hashCode", "(C)I") => int(unit(first()?)),
        ("reverseBytes", "(C)C") => int(i32::from((unit(first()?) as u16).swap_bytes())),
        ("toString", "(C)Ljava/lang/String;") => {
            let text = char::from_u32(unit(first()?) as u32)?;
            Some(Ok(Value::Ref(Some(text.to_string()))))
        }
        ("toString", "(I)Ljava/lang/String;") => {
            let code_point = first()?;
            if !(0..=0x10FFFF).contains(&code_point) {
                return Some(Err(invalid_code_point(code_point)));
            }
            let text = char::from_u32(code_point as u32)?;
            Some(Ok(Value::Ref(Some(text.to_string()))))
        }
        _ => None,
    }
}

/// `Character.toChars`/`UnicodeBlock.of`'s exception for a code point outside
/// `[0, 0x10FFFF]` (`%X` prints a negative one in two's complement, as Rust's
/// `{:X}` does).
fn invalid_code_point(code_point: i32) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(format!("Not a valid Unicode code point: 0x{code_point:X}")),
        frame: None,
    }
}

/// The marker of a `Character.UnicodeBlock` constant. The JDK keeps one
/// instance per block, so two markers are the same block exactly when their
/// texts are equal, which is what `==` compares.
const BLOCK_PREFIX: &str = "unicode-block/";

/// Whether `name` is a block `UnicodeBlock.of` can answer.
fn known_block(name: &str) -> bool {
    !name.is_empty() && table::BLOCK_RUNS.iter().any(|&(_, block)| block == name)
}

/// `Character.UnicodeBlock.of(char)` and `of(int)`: the block, or `null` for a
/// code point in no block.
fn unicode_block_of(r: &MethodRef, args: &[Value]) -> Option<Result<Value, NativeError>> {
    if r.name != "of" {
        return None;
    }
    let code_point = match (r.descriptor.as_str(), args.first()) {
        ("(C)Ljava/lang/Character$UnicodeBlock;", Some(Value::Int(value))) => unit(*value),
        ("(I)Ljava/lang/Character$UnicodeBlock;", Some(Value::Int(value))) => *value,
        _ => return None,
    };
    if !(0..=0x10FFFF).contains(&code_point) {
        return Some(Err(invalid_code_point(code_point)));
    }
    let at = table::BLOCK_RUNS.partition_point(|&(start, _)| start <= code_point as u32) - 1;
    let name = table::BLOCK_RUNS[at].1;
    Some(Ok(Value::Ref(
        (!name.is_empty()).then(|| format!("{BLOCK_PREFIX}{name}")),
    )))
}

/// `getstatic Character$UnicodeBlock.NAME` for a block `of` can answer. The
/// deprecated aliases `of` never returns (`SURROGATES_AREA`) stay unmodelled.
pub(crate) fn unicode_block_static(r: &MethodRef) -> Option<Value> {
    (r.owner == "java/lang/Character$UnicodeBlock"
        && r.descriptor == "Ljava/lang/Character$UnicodeBlock;"
        && known_block(&r.name))
    .then(|| Value::Ref(Some(format!("{BLOCK_PREFIX}{}", r.name))))
}

/// `toString()` of a block: its constant's name.
pub(crate) fn unicode_block_instance(r: &MethodRef, receiver: &Value) -> Option<Value> {
    let Value::Ref(Some(marker)) = receiver else {
        return None;
    };
    let name = marker.strip_prefix(BLOCK_PREFIX)?;
    (r.name == "toString" && r.descriptor == "()Ljava/lang/String;")
        .then(|| Value::Ref(Some(name.to_owned())))
}
