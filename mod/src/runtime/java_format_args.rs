//! The runtime side of [`crate::runtime::java_format`]: a `String.format`/`printf`
//! call's locale, format and `Object[]` read into the formatter's arguments.
//!
//! The interpreter keeps values loosely typed, so each argument is classed the
//! way the rest of the runtime classes it (`Object.getClass`, `instanceof
//! String`), and every value whose class or text it cannot vouch for becomes
//! [`Arg::Opaque`] or an ambiguous variant the formatter refuses where the
//! readings differ. A refused call goes to the host as an unknown call.

use super::*;
use crate::runtime::java_format::{self, Arg};

impl<H: Host> Runtime<'_, H> {
    /// `String.format(locale, format, args)`'s text, or `None` when the
    /// formatter cannot answer it exactly. `locale` is `None` for the
    /// overloads without one (the default locale, which the bounded host
    /// treats as `en_US`).
    pub(crate) fn java_format(
        &mut self,
        locale: Option<&Value>,
        format: &Value,
        args: &Value,
    ) -> Option<String> {
        if let Some(locale) = locale
            && !formats_like_us(locale)
        {
            return None;
        }
        // The parameter is a `String`, so any reference is its text; a null
        // format throws.
        let Value::Ref(Some(format)) = format else {
            return None;
        };
        let args = match args {
            // `String.format(f, (Object[]) null)` formats every argument as
            // null.
            Value::Ref(None) => None,
            array => Some(self.array(array).ok()?.clone()),
        };
        let args = args.map(|values| {
            values
                .iter()
                .map(|value| self.format_arg(value))
                .collect::<Vec<_>>()
        });
        java_format::format(format, args.as_deref())
    }

    /// One element of the `Object[]`, classed as `Object.getClass` would
    /// class it (`class_of_value`).
    fn format_arg(&mut self, value: &Value) -> Arg {
        let marker = match value {
            Value::Ref(None) => return Arg::Null,
            Value::Int(value) => return Arg::Int(*value),
            Value::Long(value) => return Arg::Long(*value),
            // A boxed `Float` and a boxed `Double` are the same value here.
            Value::Float(value) => return Arg::FloatOrDouble(*value),
            Value::Void => return Arg::Opaque,
            Value::Ref(Some(marker)) => marker,
        };
        if let Some(arg) = boxed_arg(marker) {
            return arg;
        }
        if let Some(text) = self.string_builders.get(marker) {
            return Arg::Str(text.clone());
        }
        if value_class::is_plain_string(marker) {
            return Arg::Str(marker.clone());
        }
        // A jar object prints its own `toString` (`jar_text.rs`); one that
        // throws, or that only the JVM can print (an identity hash), is
        // refused here and counted by the host.
        if jar_text::object_marker_class(marker).is_some() {
            return match self.runtime_value_of(value, "java/util/Formatter", "format") {
                Ok(Some(Value::Ref(Some(text)))) => Arg::Str(text),
                Ok(Some(Value::Ref(None))) => Arg::Null,
                _ => Arg::Opaque,
            };
        }
        // A model object whose `toString` the host answers; anything else
        // (a marker no model prints) cannot be printed or hashed exactly.
        match self.host.modelled_to_string(value) {
            Some(text) => Arg::Str(text),
            None => Arg::Opaque,
        }
    }
}

/// A boxed primitive's marker as its argument.
fn boxed_arg(marker: &str) -> Option<Arg> {
    // The bare `true`/`false` are boxed booleans to the runtime
    // (`class_of_value`), but are also what a `String` of that text looks
    // like. Both print alike under `%s`; under `%b` and `%h` they differ.
    if let Some(value) = match marker {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    } {
        return Some(Arg::BoolOrString(value));
    }
    let (kind, text) = marker.split_once(':')?;
    Some(match kind {
        "boolean" => Arg::Bool(text.parse().ok()?),
        "integer" => Arg::Int(text.parse().ok()?),
        "long" => Arg::Long(text.parse().ok()?),
        "short" => Arg::Short(text.parse().ok()?),
        "byte" => Arg::Byte(text.parse().ok()?),
        "character" => Arg::Char(text.parse().ok()?),
        "double" => Arg::Double(text.parse().ok()?),
        "float" => Arg::Float(text.parse().ok()?),
        _ => return None,
    })
}

/// Whether a `Locale` argument formats with the symbols the formatter
/// models: `.` decimal point, `,` grouping in threes, ASCII digits. A null
/// locale applies no localization, which is the same.
fn formats_like_us(locale: &Value) -> bool {
    match locale {
        Value::Ref(None) => true,
        Value::Ref(Some(marker)) => matches!(
            marker.as_str(),
            "locale/US" | "locale/ROOT" | "locale/ENGLISH" | "locale/en_US" | "locale/en_"
        ),
        _ => false,
    }
}
