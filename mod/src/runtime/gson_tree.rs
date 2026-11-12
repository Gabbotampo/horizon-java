//! Gson's JSON tree (`JsonElement` and its four kinds), modelled from
//! gson-2.14.0, the version the 26.3 game, its datagen and Fabric Loader ship.
//!
//! Config code builds its file as a tree (`new JsonObject()`, `addProperty`,
//! `add`), writes it with `Gson#toJson(JsonElement)`, reads it back with
//! `Gson#fromJson(reader, JsonObject.class)` or `JsonParser.parseString`, and
//! walks it with `get(...).getAsInt()`. None of that was modelled: every call
//! was a counted unknown answering `null`, so a config library such as
//! Cupboard built nothing and every mod depending on it read nothing
//! (`docs/in-game-survey.md`, third survey: 280 of the 393 entrypoint unknowns).
//!
//! This module builds, reads, compares and writes trees: `JsonElement.toString`
//! and `Gson.toJson` of a tree (through `gson_model.rs`'s `gson_to_json`).
//! `parse.rs` reads JSON text into a tree (`JsonParser`, `Gson.fromJson` into
//! a tree class).
//!
//! ## Representation
//!
//! The model keeps Gson's own structure, so the runtime's JDK collection model
//! does the bookkeeping:
//!
//! * a `JsonObject` is an object whose `members` field holds a
//!   `com/google/gson/internal/LinkedTreeMap#n` map in the host's collection
//!   table. The host keeps insertion order and replaces an existing key's value
//!   in place, which is exactly `LinkedTreeMap`'s iteration order. `asMap()`
//!   answers that map itself, as Gson does, so a `put` through it is seen by
//!   the object; the two `NullPointerException`s `LinkedTreeMap.put` raises
//!   (`key == null`, `value == null`) are raised here before the host sees it;
//! * a `JsonArray` is an object whose `elements` field holds an
//!   `java/util/ArrayList#n` list;
//! * a `JsonPrimitive` keeps its `value` field plus a kind tag (string,
//!   boolean or number) written by the constructor that built it, because the
//!   untyped runtime cannot tell the string `"boolean:true"` from a boxed
//!   `Boolean` by looking at the value;
//! * `JsonNull.INSTANCE` is one fixed marker;
//! * a `LazilyParsedNumber` (what `getAsNumber()` of a string primitive
//!   answers, and the box `JsonElementTypeAdapter` gives a number read from
//!   text) holds its text, with `intValue`/`longValue`/... following its
//!   parse-then-fall-back chain.
//!
//! Everything here dispatches on the receiver's class (Gson's classes are
//! final, `JsonElement` is the only open one), so a call typed `JsonElement`
//! on a `JsonObject` reaches the object's override as `invokevirtual` would.
//!
//! ## What is counted instead of answered
//!
//! The rule is Gson's exact answer or a counted unknown, never an
//! approximation. Counted (the host records the call and answers neutrally):
//!
//! * `JsonNull.hashCode()` (an identity hash: `JsonNull.class.hashCode()`),
//!   and so the hash of any tree holding a `JsonNull`;
//! * `getAsBigDecimal`/`getAsBigInteger`, `JsonArray.asList()` and any member
//!   not listed in this module;
//! * a boxed number the runtime cannot type exactly: a `Float`/`Double`
//!   value whose `Float.toString` and `Double.toString` differ (`0.1f`), when
//!   its text is needed, and any `Number` the runtime does not model
//!   (`BigDecimal`, `AtomicInteger`...);
//! * a `LazilyParsedNumber` whose text is not a JSON number, when it would
//!   need `BigDecimal`'s parser;
//! * `Gson.toJson` of a tree holding a non-finite number: the `Gson`'s
//!   strictness, which decides between writing `NaN` and throwing, is not
//!   recorded by the builder model.
//!
//! Named departure: a `ClassCastException` from `getAsJsonObject(String)`
//! and its siblings carries `class A cannot be cast to class B` without the
//! JVM's module/loader clause, the same departure `checkcast.rs` names.

use super::*;

mod parse;

pub(crate) const JSON_ELEMENT: &str = "com/google/gson/JsonElement";
pub(crate) const JSON_OBJECT: &str = "com/google/gson/JsonObject";
pub(crate) const JSON_ARRAY: &str = "com/google/gson/JsonArray";
pub(crate) const JSON_PRIMITIVE: &str = "com/google/gson/JsonPrimitive";
pub(crate) const JSON_NULL: &str = "com/google/gson/JsonNull";
const LAZY_NUMBER: &str = "com/google/gson/internal/LazilyParsedNumber";
/// The class `JsonObject.members` (and so `asMap()`) is in Gson.
pub(crate) const TREE_MAP: &str = "com/google/gson/internal/LinkedTreeMap";

/// `JsonNull.INSTANCE`. The serial is past any allocation the runtime's
/// object counter can reach, so a `new JsonNull()` is never this marker.
const NULL_INSTANCE: &str = "com/google/gson/JsonNull#18446744073709551615";

/// The fields Gson declares, keyed the way the interpreter keys `putfield`.
const MEMBERS_FIELD: &str =
    "|com/google/gson/JsonObject#membersLcom/google/gson/internal/LinkedTreeMap;";
const ELEMENTS_FIELD: &str = "|com/google/gson/JsonArray#elementsLjava/util/ArrayList;";
const VALUE_FIELD: &str = "|com/google/gson/JsonPrimitive#valueLjava/lang/Object;";
const LAZY_VALUE_FIELD: &str =
    "|com/google/gson/internal/LazilyParsedNumber#valueLjava/lang/String;";
/// Which `JsonPrimitive` constructor built the primitive: `string`,
/// `boolean` or `number`.
const KIND_FIELD: &str = "|#gsonPrimitiveKind";

/// `NumberLimits`' bounds on the text `BigDecimal` is asked to parse.
const MAX_NUMBER_STRING_LENGTH: usize = 10_000;

/// Whether a `Class` marker (`fromJson`'s target) names a tree class.
fn tree_class(class: &str) -> bool {
    matches!(
        class,
        JSON_ELEMENT | JSON_OBJECT | JSON_ARRAY | JSON_PRIMITIVE | JSON_NULL
    )
}

/// The supertypes of each modelled class (gson-2.14.0's declarations).
const SUPERTYPES: &[(&str, &[&str])] = &[
    (JSON_OBJECT, &[JSON_ELEMENT, "java/lang/Object"]),
    (JSON_PRIMITIVE, &[JSON_ELEMENT, "java/lang/Object"]),
    (JSON_NULL, &[JSON_ELEMENT, "java/lang/Object"]),
    (
        JSON_ARRAY,
        &[JSON_ELEMENT, "java/lang/Iterable", "java/lang/Object"],
    ),
    (
        LAZY_NUMBER,
        &[
            "java/lang/Number",
            "java/io/Serializable",
            "java/lang/Object",
        ],
    ),
    (
        TREE_MAP,
        &[
            "java/util/AbstractMap",
            "java/util/Map",
            "java/io/Serializable",
            "java/lang/Object",
        ],
    ),
];

/// One modelled class, read off an object marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Object,
    Array,
    Primitive,
    Null,
    Lazy,
    Map,
}

fn kind_of(marker: &str) -> Option<Kind> {
    let (class, serial) = marker.split_once('#')?;
    if serial.is_empty() || !serial.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(match class {
        JSON_OBJECT => Kind::Object,
        JSON_ARRAY => Kind::Array,
        JSON_PRIMITIVE => Kind::Primitive,
        JSON_NULL => Kind::Null,
        LAZY_NUMBER => Kind::Lazy,
        TREE_MAP => Kind::Map,
        _ => return None,
    })
}

fn value_kind(value: &Value) -> Option<Kind> {
    expect_ref(value).and_then(|marker| kind_of(marker))
}

/// `JsonNull.INSTANCE` for `getstatic`.
pub(crate) fn static_field(r: &MethodRef) -> Option<Value> {
    (r.owner == JSON_NULL && r.name == "INSTANCE")
        .then(|| Value::Ref(Some(NULL_INSTANCE.to_owned())))
}

/// `marker instanceof wanted` for the modelled classes.
pub(crate) fn instance_of(marker: &str, wanted: &str) -> bool {
    let Some(class) = kind_of(marker).and_then(|_| marker.split_once('#').map(|(c, _)| c)) else {
        return false;
    };
    class == wanted
        || SUPERTYPES
            .iter()
            .any(|(owner, supers)| *owner == class && supers.contains(&wanted))
}

fn thrown(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message,
        frame: None,
    }
}

fn dotted(class: &str) -> String {
    class.replace('/', ".")
}

/// `Objects.checkIndex`'s message, which `ArrayList.get/set/remove` raise.
fn index_error(index: i32, length: usize) -> NativeError {
    thrown(
        "java/lang/IndexOutOfBoundsException",
        Some(format!("Index {index} out of bounds for length {length}")),
    )
}

fn method(owner: &str, name: &str, descriptor: &str) -> MethodRef {
    MethodRef {
        owner: owner.into(),
        name: name.into(),
        descriptor: descriptor.into(),
    }
}

fn simple_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Object => "JsonObject",
        Kind::Array => "JsonArray",
        Kind::Primitive => "JsonPrimitive",
        Kind::Null => "JsonNull",
        Kind::Lazy => "LazilyParsedNumber",
        Kind::Map => "LinkedTreeMap",
    }
}

/// A `JsonPrimitive`'s value, by the constructor that built it.
#[derive(Debug, Clone)]
enum Prim {
    Str(String),
    Bool(bool),
    Num(Value),
}

/// A boxed `Number` as far as the runtime can type it.
#[derive(Debug, Clone)]
enum Num {
    /// `Integer`, `Short` or `Byte`: integral, printed as a decimal integer.
    Int(i32),
    Long(i64),
    Double(f64),
    Float(f64),
    /// A `Float`/`Double` box the runtime keeps as one `f64`: the value is
    /// exactly representable as an `f32`, so either box could hold it.
    Either(f64),
    /// A `LazilyParsedNumber`'s text.
    Lazy(String),
}

impl Num {
    /// `JsonPrimitive.isIntegral`: `BigInteger`, `Long`, `Integer`, `Short`
    /// or `Byte` (a `LazilyParsedNumber` is not).
    fn integral(&self) -> bool {
        matches!(self, Num::Int(_) | Num::Long(_))
    }

    /// `Number.toString()`, or `None` when the box's type decides the text
    /// and the runtime cannot tell which box it is.
    fn text(&self) -> Option<String> {
        Some(match self {
            Num::Int(value) => value.to_string(),
            Num::Long(value) => value.to_string(),
            Num::Double(value) => format_java_float(*value),
            Num::Float(value) => format_java_float32(*value),
            Num::Either(value) => {
                let double = format_java_float(*value);
                if double != format_java_float32(*value) {
                    return None;
                }
                double
            }
            Num::Lazy(text) => text.clone(),
        })
    }

    /// `Number.doubleValue()`; a `LazilyParsedNumber` is
    /// `Double.parseDouble(text)`, answered for JSON number text only.
    fn double(&self) -> Option<f64> {
        Some(match self {
            Num::Int(value) => f64::from(*value),
            Num::Long(value) => *value as f64,
            Num::Double(value) | Num::Float(value) | Num::Either(value) => *value,
            Num::Lazy(text) => {
                json_number(text)?;
                text.parse::<f64>().ok()?
            }
        })
    }

    /// `Number.floatValue()`.
    fn float(&self) -> Option<f64> {
        Some(match self {
            Num::Int(value) => f64::from(*value as f32),
            Num::Long(value) => f64::from(*value as f32),
            Num::Double(value) | Num::Float(value) | Num::Either(value) => f64::from(*value as f32),
            Num::Lazy(text) => {
                json_number(text)?;
                f64::from(text.parse::<f32>().ok()?)
            }
        })
    }

    /// `Number.longValue()`. Java's `d2l` saturates and maps NaN to 0, which
    /// is Rust's `as`. A `LazilyParsedNumber` is `Long.parseLong`, then
    /// `BigDecimal.longValue()`.
    fn long(&self) -> Result<Option<i64>, NativeError> {
        Ok(Some(match self {
            Num::Int(value) => i64::from(*value),
            Num::Long(value) => *value,
            Num::Double(value) | Num::Float(value) | Num::Either(value) => *value as i64,
            Num::Lazy(text) => {
                if let Some(value) = java_long(text) {
                    value
                } else {
                    match big_decimal_low_bits(text)? {
                        Some(bits) => bits as i64,
                        None => return Ok(None),
                    }
                }
            }
        }))
    }

    /// `Number.intValue()`. A `LazilyParsedNumber` is `Integer.parseInt`,
    /// then `(int) Long.parseLong`, then `BigDecimal.intValue()`.
    fn int(&self) -> Result<Option<i32>, NativeError> {
        Ok(Some(match self {
            Num::Int(value) => *value,
            Num::Long(value) => *value as i32,
            Num::Double(value) | Num::Float(value) | Num::Either(value) => *value as i32,
            Num::Lazy(text) => {
                if let Some(value) = java_long(text) {
                    // `parseInt` succeeds exactly when the value fits; the
                    // `parseLong` fallback truncates the same digits.
                    value as i32
                } else {
                    match big_decimal_low_bits(text)? {
                        Some(bits) => bits as i32,
                        None => return Ok(None),
                    }
                }
            }
        }))
    }
}

/// `Long.parseLong(text)` for the texts it accepts, `None` for the rest.
fn java_long(text: &str) -> Option<i64> {
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<i64>().ok()
}

/// The parts of a JSON number token: sign, integer digits, fraction digits,
/// exponent. `None` when `text` is not one.
fn json_number(text: &str) -> Option<(bool, &str, &str, i64)> {
    let bytes = text.as_bytes();
    let mut pos = 0;
    let negative = bytes.first() == Some(&b'-');
    if negative {
        pos += 1;
    }
    let int_start = pos;
    match bytes.get(pos) {
        Some(b'0') => pos += 1,
        Some(b'1'..=b'9') => {
            while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
                pos += 1;
            }
        }
        _ => return None,
    }
    let int_digits = &text[int_start..pos];
    let mut fraction = "";
    if bytes.get(pos) == Some(&b'.') {
        let start = pos + 1;
        pos = start;
        while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
            pos += 1;
        }
        if pos == start {
            return None;
        }
        fraction = &text[start..pos];
    }
    let mut exponent = 0i64;
    if matches!(bytes.get(pos), Some(b'e' | b'E')) {
        pos += 1;
        let exponent_negative = match bytes.get(pos) {
            Some(b'-') => {
                pos += 1;
                true
            }
            Some(b'+') => {
                pos += 1;
                false
            }
            _ => false,
        };
        let start = pos;
        while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
            pos += 1;
        }
        if pos == start {
            return None;
        }
        // An exponent too large for a `BigDecimal` scale is left unanswered.
        let magnitude: i64 = text[start..pos].parse().ok()?;
        exponent = if exponent_negative {
            -magnitude
        } else {
            magnitude
        };
    }
    (pos == bytes.len()).then_some((negative, int_digits, fraction, exponent))
}

/// The low 64 bits of `new BigDecimal(text).toBigInteger()`, which is what
/// `BigDecimal.intValue()`/`longValue()` keep, for a JSON number `text`.
///
/// `NumberLimits.parseBigDecimal` runs first and raises its two
/// `NumberFormatException`s. `Ok(None)` is a text this model does not parse
/// (not a JSON number): the caller counts the call.
fn big_decimal_low_bits(text: &str) -> Result<Option<u64>, NativeError> {
    if text.encode_utf16().count() > MAX_NUMBER_STRING_LENGTH {
        let prefix: String = text.chars().take(30).collect();
        return Err(thrown(
            "java/lang/NumberFormatException",
            Some(format!("Number string too large: {prefix}...")),
        ));
    }
    let Some((negative, int_digits, fraction, exponent)) = json_number(text) else {
        return Ok(None);
    };
    // `BigDecimal`'s scale is the fraction length minus the exponent.
    let scale = fraction.len() as i64 - exponent;
    if scale.abs() >= MAX_NUMBER_STRING_LENGTH as i64 {
        return Err(thrown(
            "java/lang/NumberFormatException",
            Some(format!("Number has unsupported scale: {text}")),
        ));
    }
    let digits = format!("{int_digits}{fraction}");
    // The unscaled digits times 10^-scale, truncated toward zero: drop the
    // last `scale` digits, or append `-scale` zeros. Only the low 64 bits are
    // kept, and those depend only on the low 64 bits of every factor.
    let mut bits: u64 = 0;
    let kept = if scale > 0 {
        digits.len().saturating_sub(scale as usize)
    } else {
        digits.len()
    };
    for digit in digits.bytes().take(kept) {
        bits = bits.wrapping_mul(10).wrapping_add(u64::from(digit - b'0'));
    }
    if scale < 0 {
        for _ in 0..(-scale).min(64) {
            bits = bits.wrapping_mul(10);
        }
    }
    Ok(Some(if negative { bits.wrapping_neg() } else { bits }))
}

/// `Double.doubleToLongBits` folded the way `JsonPrimitive.hashCode` folds it.
fn double_hash(value: f64) -> i32 {
    let bits = if value.is_nan() {
        0x7ff8_0000_0000_0000u64
    } else {
        value.to_bits()
    };
    (bits ^ (bits >> 32)) as i32
}

fn long_hash(value: i64) -> i32 {
    let bits = value as u64;
    (bits ^ (bits >> 32)) as i32
}

/// The writer options one write uses: `JsonElement.toString()` (compact,
/// no HTML escaping, nulls written, lenient) or a `Gson`'s.
pub(crate) struct TreeStyle {
    pub(crate) newline: String,
    pub(crate) indent: String,
    pub(crate) space_after_separators: bool,
    pub(crate) html_safe: bool,
    pub(crate) serialize_nulls: bool,
    /// Whether the writer is known to be `Strictness.LENIENT`, which writes
    /// `NaN`/`Infinity` instead of throwing.
    pub(crate) lenient: bool,
}

impl TreeStyle {
    fn to_string_style() -> Self {
        Self {
            newline: String::new(),
            indent: String::new(),
            space_after_separators: false,
            html_safe: false,
            serialize_nulls: true,
            lenient: true,
        }
    }
}

/// `JsonWriter`'s scope stack entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    EmptyDocument,
    NonemptyDocument,
    EmptyArray,
    NonemptyArray,
    EmptyObject,
    NonemptyObject,
    DanglingName,
}

/// A port of gson-2.14.0's `JsonWriter` for the calls
/// `JsonElementTypeAdapter.write` makes.
struct Writer<'a> {
    out: String,
    stack: Vec<Scope>,
    deferred_name: Option<String>,
    style: &'a TreeStyle,
    comma: &'static str,
    colon: &'static str,
    /// Containers the caller already has open around this value, for the
    /// indentation of a tree written inside another document.
    base_depth: usize,
}

impl<'a> Writer<'a> {
    fn new(style: &'a TreeStyle, base_depth: usize) -> Self {
        // `setFormattingStyle`: a comma gets a space only on one line.
        let comma = if style.space_after_separators && style.newline.is_empty() {
            ", "
        } else {
            ","
        };
        let colon = if style.space_after_separators {
            ": "
        } else {
            ":"
        };
        Self {
            out: String::new(),
            stack: vec![Scope::EmptyDocument],
            deferred_name: None,
            style,
            comma,
            colon,
            base_depth,
        }
    }

    fn top(&mut self) -> &mut Scope {
        self.stack.last_mut().expect("the document scope stays")
    }

    fn newline(&mut self) {
        if self.style.newline.is_empty() && self.style.indent.is_empty() {
            return;
        }
        self.out.push_str(&self.style.newline);
        for _ in 1..self.stack.len() + self.base_depth {
            self.out.push_str(&self.style.indent);
        }
    }

    fn before_value(&mut self) {
        match *self.top() {
            Scope::EmptyDocument => *self.top() = Scope::NonemptyDocument,
            Scope::EmptyArray => {
                *self.top() = Scope::NonemptyArray;
                self.newline();
            }
            Scope::NonemptyArray => {
                self.out.push_str(self.comma);
                self.newline();
            }
            Scope::DanglingName => {
                self.out.push_str(self.colon);
                *self.top() = Scope::NonemptyObject;
            }
            // One value per document, and a name always precedes an
            // object's value: the tree walk never reaches these.
            Scope::NonemptyDocument | Scope::EmptyObject | Scope::NonemptyObject => {}
        }
    }

    fn write_deferred_name(&mut self) {
        if let Some(name) = self.deferred_name.take() {
            if *self.top() == Scope::NonemptyObject {
                self.out.push_str(self.comma);
            }
            self.newline();
            *self.top() = Scope::DanglingName;
            self.string(&name);
        }
    }

    fn open(&mut self, scope: Scope, bracket: char) {
        self.write_deferred_name();
        self.before_value();
        self.stack.push(scope);
        self.out.push(bracket);
    }

    fn close(&mut self, nonempty: Scope, bracket: char) {
        let context = self.stack.pop().expect("an open scope");
        if context == nonempty {
            self.newline();
        }
        self.out.push(bracket);
    }

    fn null_value(&mut self) {
        if self.deferred_name.is_some() {
            if self.style.serialize_nulls {
                self.write_deferred_name();
            } else {
                // The name and its null are both skipped.
                self.deferred_name = None;
                return;
            }
        }
        self.before_value();
        self.out.push_str("null");
    }

    fn raw_value(&mut self, text: &str) {
        self.write_deferred_name();
        self.before_value();
        self.out.push_str(text);
    }

    fn string_value(&mut self, text: &str) {
        self.write_deferred_name();
        self.before_value();
        self.string(text);
    }

    /// `JsonWriter.string`: the JSON escapes, `\u2028`/`\u2029`, and the
    /// HTML-unsafe five while HTML escaping is on.
    fn string(&mut self, text: &str) {
        self.out.push('"');
        for ch in text.chars() {
            match ch {
                '"' => self.out.push_str("\\\""),
                '\\' => self.out.push_str("\\\\"),
                '\t' => self.out.push_str("\\t"),
                '\u{08}' => self.out.push_str("\\b"),
                '\n' => self.out.push_str("\\n"),
                '\r' => self.out.push_str("\\r"),
                '\u{0c}' => self.out.push_str("\\f"),
                '<' | '>' | '&' | '=' | '\'' if self.style.html_safe => {
                    self.out.push_str(&format!("\\u{:04x}", ch as u32));
                }
                '\u{2028}' => self.out.push_str("\\u2028"),
                '\u{2029}' => self.out.push_str("\\u2029"),
                ch if (ch as u32) < 0x20 => self.out.push_str(&format!("\\u{:04x}", ch as u32)),
                ch => self.out.push(ch),
            }
        }
        self.out.push('"');
    }
}

/// `JsonWriter.value(Number)`'s check, by the number's box.
enum NumberCheck {
    Write(String),
    /// The text is needed but cannot be known, or the strictness that
    /// decides a non-finite number is unknown: count the call.
    Count,
    Throw(NativeError),
}

fn check_number(number: &Num, lenient: bool) -> NumberCheck {
    let Some(text) = number.text() else {
        return NumberCheck::Count;
    };
    // `alwaysCreatesValidJsonNumber`: `Integer`, `Long`, `Short`, `Byte`
    // (and the `BigDecimal` family, never modelled here).
    if number.integral() {
        return NumberCheck::Write(text);
    }
    if matches!(text.as_str(), "-Infinity" | "Infinity" | "NaN") {
        // Only a writer that is not `LENIENT` throws; a `Gson`'s strictness
        // is not recorded, so its write is counted.
        return if lenient {
            NumberCheck::Write(text)
        } else {
            NumberCheck::Count
        };
    }
    if matches!(number, Num::Lazy(_)) && json_number(&text).is_none() {
        return NumberCheck::Throw(thrown(
            "java/lang/IllegalArgumentException",
            Some(format!(
                "String created by class {} is not a valid JSON number: {text}",
                dotted(LAZY_NUMBER)
            )),
        ));
    }
    NumberCheck::Write(text)
}

impl<H: Host> Runtime<'_, H> {
    /// The Gson tree surface: `JsonElement`'s four kinds,
    /// `LazilyParsedNumber` and `JsonObject.asMap()`'s map. `Ok(None)` leaves
    /// the call to the rest of the dispatch chain.
    pub(crate) fn invoke_gson_tree(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        // Reading text into a tree (`parse.rs`).
        if parse::is_parser(r) {
            return self.invoke_json_parser(r, receiver, args);
        }
        if r.owner == "com/google/gson/Gson" {
            return self.invoke_gson_fromjson_tree(r, receiver, args);
        }
        let Some(receiver) = receiver else {
            return Ok(None);
        };
        let Some(kind) = value_kind(receiver) else {
            return Ok(None);
        };
        // `getClass` (and the monitor methods) are the receiver's, answered
        // by the runtime's generic object paths.
        if matches!(
            r.name.as_str(),
            "getClass" | "wait" | "notify" | "notifyAll"
        ) {
            return Ok(None);
        }
        let answer = match kind {
            Kind::Map => return self.tree_map_call(r, receiver, args),
            Kind::Lazy => self.lazy_number_call(r, receiver, args)?,
            Kind::Object => self.json_object_call(r, receiver, args)?,
            Kind::Array => self.json_array_call(r, receiver, args)?,
            Kind::Primitive => self.json_primitive_call(r, receiver, args)?,
            Kind::Null => self.json_null_call(r, receiver, args)?,
        };
        let answer = match answer {
            Some(answer) => Some(answer),
            None if kind != Kind::Lazy => self.json_element_call(r, receiver, kind, args)?,
            None => None,
        };
        match answer {
            Some(answer) => Ok(Some(answer)),
            None => self.count_tree_call(r, Some(receiver), args).map(Some),
        }
    }

    /// The host's answer for a call this model does not reproduce: it is
    /// recorded as an unknown host call.
    fn count_tree_call(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        match receiver {
            Some(receiver) => self.host.invoke_instance(r, receiver, args),
            None => self.host.invoke(r, args),
        }
    }

    // ---- building ---------------------------------------------------------

    fn field(&self, marker: &str, field: &str) -> Option<Value> {
        self.fields.get(&format!("{marker}{field}")).cloned()
    }

    fn set_field(&mut self, marker: &str, field: &str, value: Value) {
        self.fields.insert(format!("{marker}{field}"), value);
    }

    /// Give `object` (a `JsonObject` marker) an empty `members` map.
    fn init_json_object(&mut self, object: &str) -> Result<(), NativeError> {
        let members = self.new_object(TREE_MAP);
        self.host.invoke_instance(
            &method("java/util/LinkedHashMap", "<init>", "()V"),
            &members,
            &[],
        )?;
        self.set_field(object, MEMBERS_FIELD, members);
        Ok(())
    }

    fn init_json_array(&mut self, array: &str) -> Result<(), NativeError> {
        let elements = self.new_object("java/util/ArrayList");
        self.host.invoke_instance(
            &method("java/util/ArrayList", "<init>", "()V"),
            &elements,
            &[],
        )?;
        self.set_field(array, ELEMENTS_FIELD, elements);
        Ok(())
    }

    fn new_json_object(&mut self) -> Result<Value, NativeError> {
        let object = self.new_object(JSON_OBJECT);
        self.init_json_object(expect_string(&object))?;
        Ok(object)
    }

    fn new_json_array(&mut self) -> Result<Value, NativeError> {
        let array = self.new_object(JSON_ARRAY);
        self.init_json_array(expect_string(&array))?;
        Ok(array)
    }

    fn new_primitive(&mut self, prim: Prim) -> Value {
        let primitive = self.new_object(JSON_PRIMITIVE);
        let marker = expect_string(&primitive).to_owned();
        self.set_primitive(&marker, prim);
        primitive
    }

    fn set_primitive(&mut self, marker: &str, prim: Prim) {
        let (kind, value) = match prim {
            Prim::Str(text) => ("string", Value::Ref(Some(text))),
            Prim::Bool(value) => ("boolean", boolean_value(value)),
            Prim::Num(value) => ("number", value),
        };
        self.set_field(marker, KIND_FIELD, Value::Ref(Some(kind.to_owned())));
        self.set_field(marker, VALUE_FIELD, value);
    }

    fn new_lazy_number(&mut self, text: String) -> Value {
        let number = self.new_object(LAZY_NUMBER);
        let marker = expect_string(&number).to_owned();
        self.set_field(&marker, LAZY_VALUE_FIELD, Value::Ref(Some(text)));
        number
    }

    fn members_of(&self, object: &str) -> Option<Value> {
        self.field(object, MEMBERS_FIELD)
    }

    fn elements_of(&self, array: &str) -> Option<Value> {
        self.field(array, ELEMENTS_FIELD)
    }

    fn primitive_of(&self, marker: &str) -> Option<Prim> {
        let kind = self.field(marker, KIND_FIELD)?;
        let value = self.field(marker, VALUE_FIELD)?;
        Some(match expect_ref(&kind)?.as_str() {
            "string" => Prim::Str(expect_ref(&value)?.clone()),
            "boolean" => Prim::Bool(boxed_boolean(&value)?),
            "number" => Prim::Num(value),
            _ => return None,
        })
    }

    fn lazy_text(&self, marker: &str) -> Option<String> {
        self.field(marker, LAZY_VALUE_FIELD)
            .and_then(|value| expect_ref(&value).cloned())
    }

    /// A boxed `Number` as the runtime holds it.
    fn number_of(&self, value: &Value) -> Option<Num> {
        Some(match value {
            Value::Int(value) => Num::Int(*value),
            Value::Long(value) => Num::Long(*value),
            Value::Float(value) => {
                if value.is_nan() || float32(*value) == *value {
                    Num::Either(*value)
                } else {
                    Num::Double(*value)
                }
            }
            Value::Ref(Some(marker)) => {
                if value_kind(value) == Some(Kind::Lazy) {
                    return self.lazy_text(marker).map(Num::Lazy);
                }
                let (prefix, rest) = marker.split_once(':')?;
                match prefix {
                    "integer" | "short" | "byte" => Num::Int(rest.parse().ok()?),
                    "long" => Num::Long(rest.parse().ok()?),
                    "double" => Num::Double(rest.parse().ok()?),
                    "float" => Num::Float(rest.parse().ok()?),
                    _ => return None,
                }
            }
            _ => return None,
        })
    }

    /// The members of a `JsonObject` (or of its map), in iteration order.
    fn member_pairs(&self, members: &Value) -> Result<Vec<(String, Value)>, NativeError> {
        let values = runtime_collection_values(self.host, members)?;
        let mut pairs = Vec::with_capacity(values.len() / 2);
        for pair in values.chunks(2) {
            let [key, value] = pair else {
                return Err(NativeError::Metadata(
                    "JsonObject members hold an odd value count".into(),
                ));
            };
            let key = parse_collection_value(key)?;
            let key = expect_ref(&key).cloned().ok_or_else(|| {
                NativeError::Metadata("JsonObject member name is not a string".into())
            })?;
            pairs.push((key, parse_collection_value(value)?));
        }
        Ok(pairs)
    }

    fn element_values(&self, elements: &Value) -> Result<Vec<Value>, NativeError> {
        runtime_collection_values(self.host, elements)?
            .iter()
            .map(|value| parse_collection_value(value))
            .collect()
    }

    fn map_put(&mut self, members: &Value, key: &str, value: Value) -> Result<Value, NativeError> {
        self.host.invoke_instance(
            &method(
                "java/util/LinkedHashMap",
                "put",
                "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            ),
            members,
            &[Value::Ref(Some(key.to_owned())), value],
        )
    }

    fn list_add(&mut self, elements: &Value, value: Value) -> Result<(), NativeError> {
        self.host.invoke_instance(
            &method("java/util/ArrayList", "add", "(Ljava/lang/Object;)Z"),
            elements,
            &[value],
        )?;
        Ok(())
    }

    /// A `JsonPrimitive` from one `addProperty`/`add` overload's argument,
    /// or `JsonNull.INSTANCE` for `null`. `None`: a `Character` the model
    /// cannot hold as text (a lone surrogate).
    fn primitive_argument(&mut self, descriptor: &str, value: &Value) -> Option<Value> {
        if *value == Value::Ref(None) {
            return Some(Value::Ref(Some(NULL_INSTANCE.to_owned())));
        }
        let prim = if descriptor.contains("Ljava/lang/String;)") {
            Prim::Str(expect_ref(value)?.clone())
        } else if descriptor.contains("Ljava/lang/Boolean;)") {
            Prim::Bool(boxed_boolean(value)?)
        } else if descriptor.contains("Ljava/lang/Number;)") {
            Prim::Num(value.clone())
        } else if descriptor.contains("Ljava/lang/Character;)") {
            // `new JsonPrimitive(Character)` stores `c.toString()`.
            let unit = boxed_character(value)?;
            Prim::Str(char::from_u32(unit as u32)?.to_string())
        } else {
            return None;
        };
        Some(self.new_primitive(prim))
    }

    // ---- JsonObject -------------------------------------------------------

    fn json_object_call(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let marker = expect_string(receiver).to_owned();
        if r.name == "<init>" && r.descriptor == "()V" {
            self.init_json_object(&marker)?;
            return Ok(Some(Value::Void));
        }
        let Some(members) = self.members_of(&marker) else {
            return Ok(None);
        };
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Ref(None));
        Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("add", "(Ljava/lang/String;Lcom/google/gson/JsonElement;)V") => {
                let value = match arg(1) {
                    Value::Ref(None) => Value::Ref(Some(NULL_INSTANCE.to_owned())),
                    value => value,
                };
                self.tree_map_put(&members, &arg(0), value)?;
                Value::Void
            }
            ("addProperty", descriptor) if descriptor.starts_with("(Ljava/lang/String;") => {
                let Some(value) = self.primitive_argument(descriptor, &arg(1)) else {
                    return Ok(None);
                };
                self.tree_map_put(&members, &arg(0), value)?;
                Value::Void
            }
            ("remove", "(Ljava/lang/String;)Lcom/google/gson/JsonElement;") => {
                self.host.invoke_instance(
                    &method(
                        "java/util/LinkedHashMap",
                        "remove",
                        "(Ljava/lang/Object;)Ljava/lang/Object;",
                    ),
                    &members,
                    &[arg(0)],
                )?
            }
            ("entrySet", "()Ljava/util/Set;") | ("keySet", "()Ljava/util/Set;") => {
                self.host.invoke_instance(
                    &method("java/util/LinkedHashMap", &r.name, "()Ljava/util/Set;"),
                    &members,
                    &[],
                )?
            }
            ("size", "()I") => Value::Int(self.member_pairs(&members)?.len() as i32),
            ("isEmpty", "()Z") => Value::Int(i32::from(self.member_pairs(&members)?.is_empty())),
            ("has", "(Ljava/lang/String;)Z") => {
                let key = arg(0);
                let found = self
                    .member_pairs(&members)?
                    .iter()
                    .any(|(name, _)| Some(name) == expect_ref(&key));
                Value::Int(i32::from(found))
            }
            ("get", "(Ljava/lang/String;)Lcom/google/gson/JsonElement;") => {
                self.member(&members, &arg(0))?
            }
            ("getAsJsonPrimitive", _) | ("getAsJsonArray", _) | ("getAsJsonObject", _)
                if r.descriptor.starts_with("(Ljava/lang/String;)") =>
            {
                let value = self.member(&members, &arg(0))?;
                let wanted = match r.name.as_str() {
                    "getAsJsonPrimitive" => JSON_PRIMITIVE,
                    "getAsJsonArray" => JSON_ARRAY,
                    _ => JSON_OBJECT,
                };
                if let Some(actual) = expect_ref(&value).and_then(|m| m.split_once('#'))
                    && actual.0 != wanted
                {
                    return Err(thrown(
                        "java/lang/ClassCastException",
                        Some(format!(
                            "class {} cannot be cast to class {}",
                            dotted(actual.0),
                            dotted(wanted)
                        )),
                    ));
                }
                value
            }
            ("asMap", "()Ljava/util/Map;") => members,
            ("deepCopy", "()Lcom/google/gson/JsonObject;" | "()Lcom/google/gson/JsonElement;") => {
                self.deep_copy(receiver)?
            }
            _ => return Ok(None),
        }))
    }

    /// `LinkedTreeMap.get` of one name: `null` when absent.
    fn member(&self, members: &Value, key: &Value) -> Result<Value, NativeError> {
        let Some(key) = expect_ref(key) else {
            return Ok(Value::Ref(None));
        };
        Ok(self
            .member_pairs(members)?
            .into_iter()
            .find(|(name, _)| name == key)
            .map_or(Value::Ref(None), |(_, value)| value))
    }

    /// `LinkedTreeMap.put` with its two null checks.
    fn tree_map_put(
        &mut self,
        members: &Value,
        key: &Value,
        value: Value,
    ) -> Result<Value, NativeError> {
        let Some(key) = expect_ref(key).cloned() else {
            return Err(thrown(
                "java/lang/NullPointerException",
                Some("key == null".into()),
            ));
        };
        if value == Value::Ref(None) {
            return Err(thrown(
                "java/lang/NullPointerException",
                Some("value == null".into()),
            ));
        }
        self.map_put(members, &key, value)
    }

    /// Calls on `asMap()`'s map: `LinkedTreeMap.put`'s null checks, and the
    /// members that print or compare its `JsonElement` values (the host's
    /// map model cannot run them). Everything else is the host's map model.
    fn tree_map_call(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Ref(None));
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("put", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;") => {
                self.tree_map_put(receiver, &arg(0), arg(1)).map(Some)
            }
            ("toString", "()Ljava/lang/String;") => {
                // `AbstractMap.toString`: `{k=v, ...}` with each value's text.
                let mut parts = Vec::new();
                for (key, value) in self.member_pairs(receiver)? {
                    let Some(text) = self.element_text(&value, &TreeStyle::to_string_style(), 0)?
                    else {
                        return self.count_tree_call(r, Some(receiver), args).map(Some);
                    };
                    parts.push(format!("{key}={text}"));
                }
                Ok(Some(Value::Ref(Some(format!("{{{}}}", parts.join(", "))))))
            }
            ("hashCode", "()I") => match self.members_hash(receiver)? {
                Some(hash) => Ok(Some(Value::Int(hash))),
                None => self.count_tree_call(r, Some(receiver), args).map(Some),
            },
            ("equals", "(Ljava/lang/Object;)Z") => {
                let other = arg(0);
                if value_kind(&other) != Some(Kind::Map) {
                    // A comparison with a JDK map needs `JsonElement.equals`
                    // inside the host's model: counted.
                    return self.count_tree_call(r, Some(receiver), args).map(Some);
                }
                match self.members_equal(receiver, &other)? {
                    Some(equal) => Ok(Some(Value::Int(i32::from(equal)))),
                    None => self.count_tree_call(r, Some(receiver), args).map(Some),
                }
            }
            // `putAll`, `putIfAbsent`, `merge`... would need the null checks
            // per entry; the plain reads and removals are the host's.
            (
                "putAll" | "putIfAbsent" | "merge" | "compute" | "computeIfAbsent"
                | "computeIfPresent" | "replace" | "replaceAll",
                _,
            ) => self.count_tree_call(r, Some(receiver), args).map(Some),
            _ => Ok(None),
        }
    }

    // ---- JsonArray --------------------------------------------------------

    fn json_array_call(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let marker = expect_string(receiver).to_owned();
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("<init>", "()V") => {
                self.init_json_array(&marker)?;
                return Ok(Some(Value::Void));
            }
            ("<init>", "(I)V") => {
                let capacity = expect_int(r, args.first().unwrap_or(&Value::Int(0)))?;
                if capacity < 0 {
                    return Err(thrown(
                        "java/lang/IllegalArgumentException",
                        Some(format!("Illegal Capacity: {capacity}")),
                    ));
                }
                self.init_json_array(&marker)?;
                return Ok(Some(Value::Void));
            }
            _ => {}
        }
        let Some(elements) = self.elements_of(&marker) else {
            return Ok(None);
        };
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Ref(None));
        let index_arg = |index: usize| -> Result<i32, NativeError> {
            expect_int(r, args.get(index).unwrap_or(&Value::Int(0)))
        };
        Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("add", "(Lcom/google/gson/JsonElement;)V") => {
                let value = match arg(0) {
                    Value::Ref(None) => Value::Ref(Some(NULL_INSTANCE.to_owned())),
                    value => value,
                };
                self.list_add(&elements, value)?;
                Value::Void
            }
            ("add", descriptor) => {
                let Some(value) = self.primitive_argument(descriptor, &arg(0)) else {
                    return Ok(None);
                };
                self.list_add(&elements, value)?;
                Value::Void
            }
            ("addAll", "(Lcom/google/gson/JsonArray;)V") => {
                let other = arg(0);
                let Some(other_elements) = expect_ref(&other).and_then(|m| self.elements_of(m))
                else {
                    return Ok(None);
                };
                // `ArrayList.addAll` copies the source first, so `a.addAll(a)`
                // doubles `a`.
                for value in self.element_values(&other_elements)? {
                    self.list_add(&elements, value)?;
                }
                Value::Void
            }
            ("set", "(ILcom/google/gson/JsonElement;)Lcom/google/gson/JsonElement;") => {
                let index = index_arg(0)?;
                let length = self.element_values(&elements)?.len();
                if index < 0 || index as usize >= length {
                    return Err(index_error(index, length));
                }
                let value = match arg(1) {
                    Value::Ref(None) => Value::Ref(Some(NULL_INSTANCE.to_owned())),
                    value => value,
                };
                self.host.invoke_instance(
                    &method(
                        "java/util/ArrayList",
                        "set",
                        "(ILjava/lang/Object;)Ljava/lang/Object;",
                    ),
                    &elements,
                    &[Value::Int(index), value],
                )?
            }
            ("remove", "(I)Lcom/google/gson/JsonElement;") => {
                let index = index_arg(0)?;
                let length = self.element_values(&elements)?.len();
                if index < 0 || index as usize >= length {
                    return Err(index_error(index, length));
                }
                self.host.invoke_instance(
                    &method("java/util/ArrayList", "remove", "(I)Ljava/lang/Object;"),
                    &elements,
                    &[Value::Int(index)],
                )?
            }
            ("remove", "(Lcom/google/gson/JsonElement;)Z")
            | ("contains", "(Lcom/google/gson/JsonElement;)Z") => {
                // `ArrayList.indexOf`: the first element `equals` the argument.
                let needle = arg(0);
                let mut found = None;
                for (index, value) in self.element_values(&elements)?.iter().enumerate() {
                    let Some(equal) = self.element_equals(&needle, value)? else {
                        return Ok(None);
                    };
                    if equal {
                        found = Some(index);
                        break;
                    }
                }
                if r.name == "remove"
                    && let Some(index) = found
                {
                    self.host.invoke_instance(
                        &method("java/util/ArrayList", "remove", "(I)Ljava/lang/Object;"),
                        &elements,
                        &[Value::Int(index as i32)],
                    )?;
                }
                Value::Int(i32::from(found.is_some()))
            }
            ("size", "()I") => Value::Int(self.element_values(&elements)?.len() as i32),
            ("isEmpty", "()Z") => Value::Int(i32::from(self.element_values(&elements)?.is_empty())),
            ("iterator", "()Ljava/util/Iterator;") => self.host.invoke_instance(
                &method("java/util/ArrayList", "iterator", "()Ljava/util/Iterator;"),
                &elements,
                &[],
            )?,
            ("get", "(I)Lcom/google/gson/JsonElement;") => {
                let index = index_arg(0)?;
                let values = self.element_values(&elements)?;
                if index < 0 || index as usize >= values.len() {
                    return Err(index_error(index, values.len()));
                }
                values[index as usize].clone()
            }
            ("forEach", "(Ljava/util/function/Consumer;)V") => {
                let for_each = method(
                    "java/util/ArrayList",
                    "forEach",
                    "(Ljava/util/function/Consumer;)V",
                );
                let Some(value) =
                    self.invoke_collection_callback(&for_each, Some(&elements), args)?
                else {
                    return Ok(None);
                };
                value
            }
            ("deepCopy", "()Lcom/google/gson/JsonArray;" | "()Lcom/google/gson/JsonElement;") => {
                self.deep_copy(receiver)?
            }
            (name, descriptor)
                if name.starts_with("getAs")
                    && descriptor.starts_with("()")
                    && !matches!(
                        name,
                        "getAsJsonObject"
                            | "getAsJsonArray"
                            | "getAsJsonPrimitive"
                            | "getAsJsonNull"
                    ) =>
            {
                // `getAsSingleElement()`, then the element's own getter.
                let values = self.element_values(&elements)?;
                if values.len() != 1 {
                    return Err(thrown(
                        "java/lang/IllegalStateException",
                        Some(format!(
                            "Array must have size 1, but has size {}",
                            values.len()
                        )),
                    ));
                }
                let element = values[0].clone();
                let typed = method(JSON_ELEMENT, name, descriptor);
                return self.invoke_gson_tree(&typed, Some(&element), &[]);
            }
            _ => return Ok(None),
        }))
    }

    // ---- JsonPrimitive ----------------------------------------------------

    fn json_primitive_call(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let marker = expect_string(receiver).to_owned();
        if r.name == "<init>" {
            let value = args.first().cloned().unwrap_or(Value::Ref(None));
            if value == Value::Ref(None) {
                // `Objects.requireNonNull(value)`.
                return Err(thrown("java/lang/NullPointerException", None));
            }
            let Some(primitive) = self.primitive_argument(&r.descriptor, &value) else {
                return Ok(None);
            };
            // The constructor fills this receiver: move what
            // `primitive_argument` built onto it.
            let Some(prim) = expect_ref(&primitive).and_then(|m| self.primitive_of(m)) else {
                return Ok(None);
            };
            self.set_primitive(&marker, prim);
            return Ok(Some(Value::Void));
        }
        let Some(prim) = self.primitive_of(&marker) else {
            return Ok(None);
        };
        let number = match &prim {
            Prim::Num(value) => match self.number_of(value) {
                Some(number) => Some(number),
                None => return Ok(None),
            },
            _ => None,
        };
        let string_text = |prim: &Prim| -> Option<String> {
            match prim {
                Prim::Str(text) => Some(text.clone()),
                Prim::Bool(value) => Some(value.to_string()),
                Prim::Num(_) => number.as_ref().and_then(Num::text),
            }
        };
        Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("isBoolean", "()Z") => Value::Int(i32::from(matches!(prim, Prim::Bool(_)))),
            ("isNumber", "()Z") => Value::Int(i32::from(matches!(prim, Prim::Num(_)))),
            ("isString", "()Z") => Value::Int(i32::from(matches!(prim, Prim::Str(_)))),
            ("getAsBoolean", "()Z") => match &prim {
                Prim::Bool(value) => Value::Int(i32::from(*value)),
                // `Boolean.parseBoolean(getAsString())`.
                other => {
                    let Some(text) = string_text(other) else {
                        return Ok(None);
                    };
                    Value::Int(i32::from(text.eq_ignore_ascii_case("true")))
                }
            },
            ("getAsString", "()Ljava/lang/String;") => match string_text(&prim) {
                Some(text) => Value::Ref(Some(text)),
                None => return Ok(None),
            },
            ("getAsNumber", "()Ljava/lang/Number;") => match &prim {
                Prim::Num(value) => value.clone(),
                Prim::Str(text) => self.new_lazy_number(text.clone()),
                Prim::Bool(_) => {
                    return Err(thrown(
                        "java/lang/UnsupportedOperationException",
                        Some("Primitive is neither a number nor a string".into()),
                    ));
                }
            },
            ("getAsCharacter", "()C") => {
                let Some(text) = string_text(&prim) else {
                    return Ok(None);
                };
                let Some(first) = text.encode_utf16().next() else {
                    return Err(thrown(
                        "java/lang/UnsupportedOperationException",
                        Some("String value is empty".into()),
                    ));
                };
                Value::Int(i32::from(first))
            }
            (
                "getAsInt" | "getAsLong" | "getAsShort" | "getAsByte" | "getAsDouble"
                | "getAsFloat",
                _,
            ) => match (&prim, &number) {
                (_, Some(number)) => match self.number_conversion(&r.name, number)? {
                    Some(value) => value,
                    None => return Ok(None),
                },
                (other, None) => {
                    // `Integer.parseInt(getAsString())` and its siblings.
                    let Some(text) = string_text(other) else {
                        return Ok(None);
                    };
                    match self.parse_string_number(&r.name, &text)? {
                        Some(value) => value,
                        None => return Ok(None),
                    }
                }
            },
            // `deepCopy()` answers the primitive itself.
            ("deepCopy", _) => receiver.clone(),
            _ => return Ok(None),
        }))
    }

    /// `getAsInt`/`getAsLong`/... of a number: the box's own conversion.
    fn number_conversion(
        &mut self,
        name: &str,
        number: &Num,
    ) -> Result<Option<Value>, NativeError> {
        Ok(match name {
            "getAsInt" => number.int()?.map(Value::Int),
            "getAsShort" => number
                .int()?
                .map(|value| Value::Int(i32::from(value as i16))),
            "getAsByte" => number
                .int()?
                .map(|value| Value::Int(i32::from(value as i8))),
            "getAsLong" => number.long()?.map(Value::Long),
            "getAsDouble" => number.double().map(Value::Float),
            "getAsFloat" => number.float().map(Value::Float),
            _ => None,
        })
    }

    /// `Integer.parseInt`, `Long.parseLong`, `Short.parseShort`,
    /// `Byte.parseByte`, `Double.parseDouble` or `Float.parseFloat` of a
    /// string primitive's text, with the JDK's exceptions. A floating-point
    /// text outside plain decimal notation (`0x1p3`, `1d`, padding) is
    /// counted: the runtime's float parser does not cover Java's grammar.
    fn parse_string_number(
        &mut self,
        name: &str,
        text: &str,
    ) -> Result<Option<Value>, NativeError> {
        let (owner, parser, descriptor) = match name {
            "getAsInt" => ("java/lang/Integer", "parseInt", "(Ljava/lang/String;)I"),
            "getAsLong" => ("java/lang/Long", "parseLong", "(Ljava/lang/String;)J"),
            "getAsShort" => ("java/lang/Short", "parseShort", "(Ljava/lang/String;)S"),
            "getAsByte" => ("java/lang/Byte", "parseByte", "(Ljava/lang/String;)B"),
            "getAsDouble" | "getAsFloat" => {
                if !plain_decimal(text) {
                    return Ok(None);
                }
                let Ok(value) = text.parse::<f64>() else {
                    return Ok(None);
                };
                return Ok(Some(Value::Float(if name == "getAsFloat" {
                    f64::from(text.parse::<f32>().unwrap_or(value as f32))
                } else {
                    value
                })));
            }
            _ => return Ok(None),
        };
        let call = method(owner, parser, descriptor);
        match crate::runtime::java_numbers::static_call(&call, &[Value::Ref(Some(text.to_owned()))]) {
            Some(result) => result.map(Some),
            None => Ok(None),
        }
    }

    // ---- JsonNull ---------------------------------------------------------

    fn json_null_call(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let _ = (receiver, args);
        Ok(match (r.name.as_str(), r.descriptor.as_str()) {
            ("<init>", "()V") => Some(Value::Void),
            ("deepCopy", _) => Some(Value::Ref(Some(NULL_INSTANCE.to_owned()))),
            _ => None,
        })
    }

    // ---- JsonElement (the inherited members) ------------------------------

    fn json_element_call(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        kind: Kind,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let is = |wanted: Kind| Value::Int(i32::from(kind == wanted));
        Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("isJsonObject", "()Z") => is(Kind::Object),
            ("isJsonArray", "()Z") => is(Kind::Array),
            ("isJsonPrimitive", "()Z") => is(Kind::Primitive),
            ("isJsonNull", "()Z") => is(Kind::Null),
            ("getAsJsonObject", "()Lcom/google/gson/JsonObject;")
            | ("getAsJsonArray", "()Lcom/google/gson/JsonArray;")
            | ("getAsJsonPrimitive", "()Lcom/google/gson/JsonPrimitive;")
            | ("getAsJsonNull", "()Lcom/google/gson/JsonNull;") => {
                let (wanted, label) = match r.name.as_str() {
                    "getAsJsonObject" => (Kind::Object, "Object"),
                    "getAsJsonArray" => (Kind::Array, "Array"),
                    "getAsJsonPrimitive" => (Kind::Primitive, "Primitive"),
                    _ => (Kind::Null, "Null"),
                };
                if kind == wanted {
                    receiver.clone()
                } else {
                    // `"Not a JSON Object: " + this`.
                    let Some(text) =
                        self.element_text(receiver, &TreeStyle::to_string_style(), 0)?
                    else {
                        return Ok(None);
                    };
                    return Err(thrown(
                        "java/lang/IllegalStateException",
                        Some(format!("Not a JSON {label}: {text}")),
                    ));
                }
            }
            (
                "getAsBoolean" | "getAsNumber" | "getAsString" | "getAsDouble" | "getAsFloat"
                | "getAsLong" | "getAsInt" | "getAsByte" | "getAsCharacter" | "getAsShort",
                _,
            ) => {
                // `JsonElement`'s own body: the receiver's simple class name.
                return Err(thrown(
                    "java/lang/UnsupportedOperationException",
                    Some(simple_name(kind).into()),
                ));
            }
            ("toString", "()Ljava/lang/String;") => {
                match self.element_text(receiver, &TreeStyle::to_string_style(), 0)? {
                    Some(text) => Value::Ref(Some(text)),
                    None => return Ok(None),
                }
            }
            ("equals", "(Ljava/lang/Object;)Z") => {
                let other = args.first().cloned().unwrap_or(Value::Ref(None));
                match self.element_equals(receiver, &other)? {
                    Some(equal) => Value::Int(i32::from(equal)),
                    None => return Ok(None),
                }
            }
            ("hashCode", "()I") => match self.element_hash(receiver)? {
                Some(hash) => Value::Int(hash),
                None => return Ok(None),
            },
            _ => return Ok(None),
        }))
    }

    // ---- LazilyParsedNumber -----------------------------------------------

    fn lazy_number_call(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let marker = expect_string(receiver).to_owned();
        if r.name == "<init>" && r.descriptor == "(Ljava/lang/String;)V" {
            let text = args.first().and_then(expect_ref).cloned();
            self.set_field(
                &marker,
                LAZY_VALUE_FIELD,
                text.map_or(Value::Ref(None), |text| Value::Ref(Some(text))),
            );
            return Ok(Some(Value::Void));
        }
        let Some(text) = self.lazy_text(&marker) else {
            return Ok(None);
        };
        let number = Num::Lazy(text.clone());
        Ok(match (r.name.as_str(), r.descriptor.as_str()) {
            ("intValue", "()I") => number.int()?.map(Value::Int),
            // `Number.shortValue()`/`byteValue()` narrow `intValue()`.
            ("shortValue", "()S") => number
                .int()?
                .map(|value| Value::Int(i32::from(value as i16))),
            ("byteValue", "()B") => number
                .int()?
                .map(|value| Value::Int(i32::from(value as i8))),
            ("longValue", "()J") => number.long()?.map(Value::Long),
            ("doubleValue", "()D") => number.double().map(Value::Float),
            ("floatValue", "()F") => number.float().map(Value::Float),
            ("toString", "()Ljava/lang/String;") => Some(Value::Ref(Some(text))),
            ("hashCode", "()I") => Some(Value::Int(jdk_model::java_string_hash(&text))),
            ("equals", "(Ljava/lang/Object;)Z") => {
                let other = args.first().cloned().unwrap_or(Value::Ref(None));
                let equal = value_kind(&other) == Some(Kind::Lazy)
                    && expect_ref(&other)
                        .and_then(|other| self.lazy_text(other))
                        .is_some_and(|other| other == text);
                Some(Value::Int(i32::from(equal)))
            }
            _ => None,
        })
    }

    // ---- tree walks -------------------------------------------------------

    /// `JsonElement.equals(other)` for a tree element; `None` when a number
    /// the comparison needs cannot be typed.
    fn element_equals(&mut self, a: &Value, b: &Value) -> Result<Option<bool>, NativeError> {
        if a == b && *a != Value::Ref(None) {
            return Ok(Some(true));
        }
        let (Some(kind), other_kind) = (value_kind(a), value_kind(b)) else {
            // A Java `null` receiver never reaches here; a non-tree value
            // equals no tree element.
            return Ok(Some(false));
        };
        if other_kind != Some(kind) {
            return Ok(Some(false));
        }
        let (a_marker, b_marker) = (expect_string(a).to_owned(), expect_string(b).to_owned());
        match kind {
            Kind::Null => Ok(Some(true)),
            Kind::Object => {
                let (Some(left), Some(right)) =
                    (self.members_of(&a_marker), self.members_of(&b_marker))
                else {
                    return Ok(None);
                };
                self.members_equal(&left, &right)
            }
            Kind::Array => {
                let (Some(left), Some(right)) =
                    (self.elements_of(&a_marker), self.elements_of(&b_marker))
                else {
                    return Ok(None);
                };
                let (left, right) = (self.element_values(&left)?, self.element_values(&right)?);
                if left.len() != right.len() {
                    return Ok(Some(false));
                }
                for (a, b) in left.iter().zip(&right) {
                    match self.element_equals(a, b)? {
                        Some(true) => {}
                        other => return Ok(other),
                    }
                }
                Ok(Some(true))
            }
            Kind::Primitive => {
                let (Some(left), Some(right)) =
                    (self.primitive_of(&a_marker), self.primitive_of(&b_marker))
                else {
                    return Ok(None);
                };
                Ok(match (left, right) {
                    (Prim::Num(left), Prim::Num(right)) => {
                        let (Some(left), Some(right)) =
                            (self.number_of(&left), self.number_of(&right))
                        else {
                            return Ok(None);
                        };
                        if left.integral() && right.integral() {
                            match (left.long()?, right.long()?) {
                                (Some(left), Some(right)) => Some(left == right),
                                _ => None,
                            }
                        } else {
                            match (left.double(), right.double()) {
                                (Some(left), Some(right)) => {
                                    Some(left == right || (left.is_nan() && right.is_nan()))
                                }
                                _ => None,
                            }
                        }
                    }
                    (Prim::Str(left), Prim::Str(right)) => Some(left == right),
                    (Prim::Bool(left), Prim::Bool(right)) => Some(left == right),
                    // `value.equals(other.value)` across types is false.
                    _ => Some(false),
                })
            }
            Kind::Lazy | Kind::Map => Ok(None),
        }
    }

    /// `AbstractMap.equals` between two members maps.
    fn members_equal(&mut self, left: &Value, right: &Value) -> Result<Option<bool>, NativeError> {
        let (left, right) = (self.member_pairs(left)?, self.member_pairs(right)?);
        if left.len() != right.len() {
            return Ok(Some(false));
        }
        for (key, value) in &left {
            let Some((_, other)) = right.iter().find(|(name, _)| name == key) else {
                return Ok(Some(false));
            };
            match self.element_equals(value, other)? {
                Some(true) => {}
                other => return Ok(other),
            }
        }
        Ok(Some(true))
    }

    /// `JsonElement.hashCode()`; `None` for a `JsonNull` anywhere (an
    /// identity hash) or an untypeable number.
    fn element_hash(&mut self, value: &Value) -> Result<Option<i32>, NativeError> {
        let Some(kind) = value_kind(value) else {
            return Ok(None);
        };
        let marker = expect_string(value).to_owned();
        match kind {
            Kind::Object => match self.members_of(&marker) {
                Some(members) => self.members_hash(&members),
                None => Ok(None),
            },
            Kind::Array => {
                let Some(elements) = self.elements_of(&marker) else {
                    return Ok(None);
                };
                let mut hash: i32 = 1;
                for element in self.element_values(&elements)? {
                    let Some(element) = self.element_hash(&element)? else {
                        return Ok(None);
                    };
                    hash = hash.wrapping_mul(31).wrapping_add(element);
                }
                Ok(Some(hash))
            }
            Kind::Primitive => {
                let Some(prim) = self.primitive_of(&marker) else {
                    return Ok(None);
                };
                Ok(match prim {
                    Prim::Str(text) => Some(jdk_model::java_string_hash(&text)),
                    // `Boolean.hashCode`.
                    Prim::Bool(value) => Some(if value { 1231 } else { 1237 }),
                    Prim::Num(value) => {
                        let Some(number) = self.number_of(&value) else {
                            return Ok(None);
                        };
                        if number.integral() {
                            number.long()?.map(long_hash)
                        } else {
                            number.double().map(double_hash)
                        }
                    }
                })
            }
            Kind::Null | Kind::Lazy | Kind::Map => Ok(None),
        }
    }

    /// `AbstractMap.hashCode`: the sum of `key.hashCode() ^ value.hashCode()`.
    fn members_hash(&mut self, members: &Value) -> Result<Option<i32>, NativeError> {
        let mut hash: i32 = 0;
        for (key, value) in self.member_pairs(members)? {
            let Some(value) = self.element_hash(&value)? else {
                return Ok(None);
            };
            hash = hash.wrapping_add(jdk_model::java_string_hash(&key) ^ value);
        }
        Ok(Some(hash))
    }

    /// `deepCopy()`: new objects and arrays all the way down, primitives
    /// shared (they are immutable), `JsonNull.INSTANCE` for any null.
    fn deep_copy(&mut self, value: &Value) -> Result<Value, NativeError> {
        let marker = expect_string(value).to_owned();
        match value_kind(value) {
            Some(Kind::Object) => {
                let copy = self.new_json_object()?;
                let (Some(source), Some(target)) = (
                    self.members_of(&marker),
                    self.members_of(expect_string(&copy)),
                ) else {
                    return Ok(copy);
                };
                for (key, member) in self.member_pairs(&source)? {
                    let member = self.deep_copy(&member)?;
                    self.map_put(&target, &key, member)?;
                }
                Ok(copy)
            }
            Some(Kind::Array) => {
                let copy = self.new_json_array()?;
                let (Some(source), Some(target)) = (
                    self.elements_of(&marker),
                    self.elements_of(expect_string(&copy)),
                ) else {
                    return Ok(copy);
                };
                for element in self.element_values(&source)? {
                    let element = self.deep_copy(&element)?;
                    self.list_add(&target, element)?;
                }
                Ok(copy)
            }
            Some(Kind::Null) => Ok(Value::Ref(Some(NULL_INSTANCE.to_owned()))),
            _ => Ok(value.clone()),
        }
    }

    /// The text `JsonElementTypeAdapter.write` produces for `value` through a
    /// writer with `style`, `depth` containers deep in an enclosing document.
    /// `Ok(None)`: the text is not known exactly (count the call).
    pub(crate) fn element_text(
        &mut self,
        value: &Value,
        style: &TreeStyle,
        depth: usize,
    ) -> Result<Option<String>, NativeError> {
        let mut writer = Writer::new(style, depth);
        if !self.write_element(&mut writer, value)? {
            return Ok(None);
        }
        Ok(Some(writer.out))
    }

    fn write_element(
        &mut self,
        writer: &mut Writer<'_>,
        value: &Value,
    ) -> Result<bool, NativeError> {
        let marker = match value {
            Value::Ref(None) => {
                writer.null_value();
                return Ok(true);
            }
            Value::Ref(Some(marker)) => marker.clone(),
            _ => return Ok(false),
        };
        match kind_of(&marker) {
            Some(Kind::Null) => writer.null_value(),
            Some(Kind::Primitive) => {
                let Some(prim) = self.primitive_of(&marker) else {
                    return Ok(false);
                };
                match prim {
                    Prim::Str(text) => writer.string_value(&text),
                    Prim::Bool(value) => writer.raw_value(if value { "true" } else { "false" }),
                    Prim::Num(value) => {
                        let Some(number) = self.number_of(&value) else {
                            return Ok(false);
                        };
                        match check_number(&number, writer.style.lenient) {
                            NumberCheck::Write(text) => writer.raw_value(&text),
                            NumberCheck::Count => return Ok(false),
                            NumberCheck::Throw(error) => return Err(error),
                        }
                    }
                }
            }
            Some(Kind::Array) => {
                let Some(elements) = self.elements_of(&marker) else {
                    return Ok(false);
                };
                writer.open(Scope::EmptyArray, '[');
                for element in self.element_values(&elements)? {
                    if !self.write_element(writer, &element)? {
                        return Ok(false);
                    }
                }
                writer.close(Scope::NonemptyArray, ']');
            }
            Some(Kind::Object) => {
                let Some(members) = self.members_of(&marker) else {
                    return Ok(false);
                };
                writer.open(Scope::EmptyObject, '{');
                for (key, member) in self.member_pairs(&members)? {
                    writer.deferred_name = Some(key);
                    if !self.write_element(writer, &member)? {
                        return Ok(false);
                    }
                }
                writer.close(Scope::NonemptyObject, '}');
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// `Gson.toJson` text for a tree element: `None` when `value` is not one,
    /// `Some(None)` when the text cannot be known exactly.
    pub(crate) fn gson_tree_json(
        &mut self,
        value: &Value,
        style: &TreeStyle,
        depth: usize,
    ) -> Result<Option<Option<String>>, NativeError> {
        let Some(kind) = value_kind(value) else {
            return Ok(None);
        };
        if matches!(kind, Kind::Lazy | Kind::Map) {
            return Ok(None);
        }
        // Inside a document the enclosing writer already wrote the member's
        // name; a `JsonNull` there would drop that name when nulls are not
        // serialized, which the enclosing writer cannot undo.
        if depth > 0 && kind == Kind::Null && !style.serialize_nulls {
            return Ok(Some(None));
        }
        self.element_text(value, style, depth).map(Some)
    }
}

/// Whether `text` is a decimal floating-point literal that Rust's parser and
/// Java's `Double.parseDouble` read the same way.
fn plain_decimal(text: &str) -> bool {
    let body = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(at) => (&body[..at], Some(&body[at + 1..])),
        None => (body, None),
    };
    let digits = mantissa.replacen('.', "", 1);
    let mantissa_ok = !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && mantissa.matches('.').count() <= 1;
    let exponent_ok = exponent.is_none_or(|exponent| {
        let exponent = exponent.strip_prefix(['-', '+']).unwrap_or(exponent);
        !exponent.is_empty() && exponent.bytes().all(|byte| byte.is_ascii_digit())
    });
    mantissa_ok && exponent_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_decimal_truncates_toward_zero_and_keeps_the_low_bits() {
        // `Long.toUnsignedString(new BigDecimal(s).longValue())` on OpenJDK 27:
        // 1.50 1 | -2.99 18446744073709551614 | 1e3 1000 | 2.5E-3 0 |
        // 123456789012345678901234567890 14083847773837265618 | 1e70 0
        assert_eq!(big_decimal_low_bits("1.50").unwrap(), Some(1));
        assert_eq!(
            big_decimal_low_bits("-2.99").unwrap(),
            Some(18_446_744_073_709_551_614)
        );
        assert_eq!(big_decimal_low_bits("1e3").unwrap(), Some(1000));
        assert_eq!(big_decimal_low_bits("2.5E-3").unwrap(), Some(0));
        assert_eq!(
            big_decimal_low_bits("123456789012345678901234567890").unwrap(),
            Some(14_083_847_773_837_265_618)
        );
        assert_eq!(big_decimal_low_bits("1e70").unwrap(), Some(0));
        assert_eq!(big_decimal_low_bits("abc").unwrap(), None);
    }
}
