//! `Optional`, `OptionalInt`, `OptionalLong` and `OptionalDouble` as host
//! handles (#156 P0): their keys, and their `toString`/`hashCode`/`equals`
//! rows in the kind table (`host_ref.rs`), written from the JDK 25 sources.
//!
//! `Optional.empty()` is an object in Java: `isPresent()`, `orElse` and
//! `ifPresent` are ordinary calls on it, and only a real `null` receiver
//! throws `NullPointerException`. Every optional is a handle: key `""` when
//! empty (the kind's singleton), the held value otherwise. No `String` is a
//! handle, so no jar string can spell an optional, and an optional of an
//! optional is its own payload.
//!
//! Keys (equal keys are equal optionals):
//!
//! * `Optional`: `HostRef::of_values` of the held value, so a string with
//!   separators, escapes or any Unicode reads back as it went in;
//! * `OptionalInt`/`OptionalLong`: the number in decimal;
//! * `OptionalDouble`: `Double.doubleToLongBits(value)` in decimal, which is
//!   what `OptionalDouble.equals` compares (`Double.compare`): every `NaN`
//!   is one key, `-0.0` and `0.0` are two. Named departure: `getAsDouble()`
//!   of an optional made from a `NaN` with a non-canonical payload answers
//!   the canonical `NaN` (`Double.doubleToRawLongBits` alone can tell).

use crate::host_ref::{HostKind, HostRef, Methods, Nested, instance_of_class};
use crate::{NativeError, Value, format_java_float, java_double_bits, object_methods, return_desc};

/// `Optional.empty()`.
pub(crate) fn empty_optional() -> Value {
    Value::Host(HostRef::new(HostKind::Optional, ""))
}

/// Whether `value` is an `Optional` (empty or present).
pub(crate) fn is_optional(value: &Value) -> bool {
    matches!(value, Value::Host(host) if host.kind() == HostKind::Optional)
}

/// The empty value for a method whose declared result is one of the JDK's
/// optional types, so a host answer of `null` for it becomes the empty
/// optional Java would have returned (no JDK or library API returns a
/// `null` Optional).
pub(crate) fn empty_optional_for(descriptor: &str) -> Option<Value> {
    let class = return_desc(descriptor)
        .strip_prefix('L')?
        .strip_suffix(';')?;
    let kind = HostKind::of_class(class)?;
    matches!(
        kind,
        HostKind::Optional
            | HostKind::OptionalInt
            | HostKind::OptionalLong
            | HostKind::OptionalDouble
    )
    .then(|| Value::Host(HostRef::new(kind, "")))
}

/// The present key of a handle of `kind`, `None` when it is empty or no such
/// handle.
fn present_key(value: &Value, kind: HostKind) -> Option<&str> {
    match value {
        Value::Host(host) if host.kind() == kind && !host.key().is_empty() => Some(host.key()),
        _ => None,
    }
}

/// The value an `Optional` handle holds.
fn held(handle: &HostRef) -> Option<Value> {
    match handle.values()?.as_slice() {
        [held] => Some(held.clone()),
        _ => None,
    }
}

/// The value a present optional holds, `None` when it is empty or no
/// optional at all.
pub(crate) fn optional_present(value: &Value) -> Option<Value> {
    match value {
        Value::Host(host) if host.kind() == HostKind::Optional => held(host),
        _ => None,
    }
}

/// [`optional_present`].
pub(crate) fn optional_payload(value: &Value) -> Option<Value> {
    optional_present(value)
}

/// `Optional.ofNullable(value)`: the empty optional for `null`, otherwise an
/// optional holding the value as it is (a `String`, a box, another
/// optional), never read as an optional itself.
pub(crate) fn optional_wrap(value: Value) -> Value {
    match value {
        Value::Ref(None) | Value::Void => empty_optional(),
        value => Value::Host(HostRef::of_values(HostKind::Optional, &[value])),
    }
}

/// `OptionalInt.of(value)`.
pub(crate) fn optional_int(value: i32) -> Value {
    Value::Host(HostRef::new(HostKind::OptionalInt, value.to_string()))
}

/// `OptionalLong.of(value)`.
pub(crate) fn optional_long(value: i64) -> Value {
    Value::Host(HostRef::new(HostKind::OptionalLong, value.to_string()))
}

/// `OptionalDouble.of(value)`, keyed by `Double.doubleToLongBits`.
pub(crate) fn optional_double(value: f64) -> Value {
    Value::Host(HostRef::new(
        HostKind::OptionalDouble,
        java_double_bits(value).to_string(),
    ))
}

/// The empty optional of `kind`.
pub(crate) fn empty_of(kind: HostKind) -> Value {
    Value::Host(HostRef::new(kind, ""))
}

pub(crate) fn optional_int_value(value: &Value) -> Option<i32> {
    present_key(value, HostKind::OptionalInt)?.parse().ok()
}

pub(crate) fn optional_long_value(value: &Value) -> Option<i64> {
    present_key(value, HostKind::OptionalLong)?.parse().ok()
}

pub(crate) fn optional_double_value(value: &Value) -> Option<f64> {
    present_key(value, HostKind::OptionalDouble)?
        .parse()
        .ok()
        .map(f64::from_bits)
}

/// `"Optional[" + value + "]"` / `"Optional.empty"` (JDK 25 `Optional.toString`).
fn optional_to_string(
    handle: &HostRef,
    nested: &mut dyn Nested,
) -> Result<Option<String>, NativeError> {
    Ok(match held(handle) {
        Some(value) => nested.text(&value)?.map(|text| format!("Optional[{text}]")),
        None => Some("Optional.empty".into()),
    })
}

/// `Objects.hashCode(value)`.
fn optional_hash_code(
    handle: &HostRef,
    nested: &mut dyn Nested,
) -> Result<Option<i32>, NativeError> {
    match held(handle) {
        Some(value) => nested.hash(&value),
        None => Ok(Some(0)),
    }
}

/// `this == obj || obj instanceof Optional<?> other && Objects.equals(value,
/// other.value)`.
fn optional_equals(
    handle: &HostRef,
    other: &Value,
    nested: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    let Value::Host(other) = other else {
        return Ok(Some(false));
    };
    if other.kind() != HostKind::Optional {
        return Ok(Some(false));
    }
    match (held(handle), held(other)) {
        (Some(left), Some(right)) => nested.equals(&left, &right),
        (None, None) => Ok(Some(true)),
        _ => Ok(Some(false)),
    }
}

pub(crate) const OPTIONAL: Methods = Methods {
    to_string: optional_to_string,
    hash_code: optional_hash_code,
    equals: optional_equals,
    instance_of: instance_of_class,
    // Empty, or holding a value that crosses.
    crosses_jars: |handle, crosses| held(handle).is_none_or(|value| crosses(&value)),
    compare_to: None,
};

/// The primitive optionals' text: `OptionalInt[5]`, `OptionalInt.empty`.
fn primitive_to_string(handle: &HostRef) -> String {
    let value = Value::Host(handle.clone());
    let (name, held) = match handle.kind() {
        HostKind::OptionalInt => (
            "OptionalInt",
            optional_int_value(&value).map(|v| v.to_string()),
        ),
        HostKind::OptionalLong => (
            "OptionalLong",
            optional_long_value(&value).map(|v| v.to_string()),
        ),
        _ => (
            "OptionalDouble",
            optional_double_value(&value).map(format_java_float),
        ),
    };
    match held {
        Some(held) => format!("{name}[{held}]"),
        None => format!("{name}.empty"),
    }
}

/// `Integer`/`Long`/`Double.hashCode(value)`, `0` when empty.
fn primitive_hash(handle: &HostRef) -> i32 {
    let value = Value::Host(handle.clone());
    match handle.kind() {
        HostKind::OptionalInt => optional_int_value(&value).unwrap_or(0),
        HostKind::OptionalLong => optional_long_value(&value).map_or(0, object_methods::long_hash),
        _ => optional_double_value(&value).map_or(0, object_methods::double_hash),
    }
}

/// `obj instanceof OptionalX other && (isPresent && other.isPresent ? value
/// == other.value : isPresent == other.isPresent)`, the `double` compared with
/// `Double.compare`. The key is that comparison's canonical form.
fn primitive_equals(handle: &HostRef, other: &Value) -> bool {
    matches!(other, Value::Host(other) if other == handle)
}

pub(crate) const OPTIONAL_INT: Methods = Methods {
    to_string: |handle, _| Ok(Some(primitive_to_string(handle))),
    hash_code: |handle, _| Ok(Some(primitive_hash(handle))),
    equals: |handle, other, _| Ok(Some(primitive_equals(handle, other))),
    instance_of: instance_of_class,
    // A number always crosses.
    crosses_jars: |_, _| true,
    compare_to: None,
};

pub(crate) const OPTIONAL_LONG: Methods = OPTIONAL_INT;

pub(crate) const OPTIONAL_DOUBLE: Methods = OPTIONAL_INT;
