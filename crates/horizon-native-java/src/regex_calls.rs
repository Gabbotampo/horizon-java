//! The JDK calls answered by [`crate::java_regex`]: `String`'s regex methods
//! and `Pattern`'s static helpers.
//!
//! Every answer is Java's exact one, including the exceptions a replacement
//! string throws. A pattern, input or group outside the modelled subset is
//! handed to the host, which records it as an unknown host call; a null
//! argument is too (Java's helpful `NullPointerException` text is not
//! reproduced), except for `String.split`, whose null throws were already
//! modelled.

use crate::java_regex::{self, Thrown};
use crate::{Host, MethodRef, NativeError, Runtime, StdHost, Value, expect_int, expect_ref};

/// Split results are caller-visible arrays, so a pattern that matches at
/// every character of a long string must not turn into an unbounded
/// allocation.
const MAX_SPLIT_PARTS: usize = 65_536;

fn thrown(thrown: Thrown) -> NativeError {
    NativeError::UncaughtException {
        class: thrown.class.into(),
        message: Some(thrown.message),
        frame: None,
    }
}

/// Java's `String.split(regex, limit)`, or why it is not answered.
pub(crate) fn split_java_string(
    input: &str,
    pattern: &str,
    limit: i32,
) -> Result<Vec<String>, String> {
    let parts = java_regex::string_split(input, pattern, limit)
        .map_err(|unsupported| format!("pattern {pattern:?}: {}", unsupported.0))?;
    if parts.len() > MAX_SPLIT_PARTS {
        return Err(format!(
            "produced more than {MAX_SPLIT_PARTS} parts in one call"
        ));
    }
    Ok(parts)
}

impl<H: Host> Runtime<'_, H> {
    /// A `CharSequence` argument's text: a `StringBuilder`'s contents, or the
    /// string itself.
    fn char_sequence(&self, value: &Value) -> Option<String> {
        let marker = expect_ref(value)?;
        Some(
            self.string_builders
                .get(marker)
                .cloned()
                .unwrap_or_else(|| marker.clone()),
        )
    }

    /// The regex surface; `Ok(None)` leaves the call to the rest of the
    /// dispatch chain (and so to the host, which counts it).
    pub(crate) fn invoke_regex(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
        class: &str,
        method: &str,
    ) -> Result<Option<Value>, NativeError> {
        let text = |index: usize| args.get(index).and_then(expect_ref).cloned();
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            ("java/lang/String", "split", _) => {
                self.invoke_string_split(r, receiver, args, class, method)
            }
            ("java/lang/String", "matches", "(Ljava/lang/String;)Z") => {
                let (Some(input), Some(pattern)) = (receiver.and_then(expect_ref), text(0)) else {
                    return Ok(None);
                };
                Ok(java_regex::matches(&pattern, input)
                    .ok()
                    .map(|matched| Value::Int(i32::from(matched))))
            }
            (
                "java/util/regex/Pattern",
                "matches",
                "(Ljava/lang/String;Ljava/lang/CharSequence;)Z",
            ) => {
                let (Some(pattern), Some(input)) = (
                    text(0),
                    args.get(1).and_then(|value| self.char_sequence(value)),
                ) else {
                    return Ok(None);
                };
                Ok(java_regex::matches(&pattern, &input)
                    .ok()
                    .map(|matched| Value::Int(i32::from(matched))))
            }
            (
                "java/lang/String",
                "replaceAll" | "replaceFirst",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
            ) => {
                let (Some(input), Some(pattern), Some(replacement)) =
                    (receiver.and_then(expect_ref), text(0), text(1))
                else {
                    return Ok(None);
                };
                match java_regex::replace(input, &pattern, &replacement, r.name == "replaceAll") {
                    Ok(Ok(result)) => Ok(Some(Value::Ref(Some(result)))),
                    Ok(Err(exception)) => Err(thrown(exception)),
                    Err(_) => Ok(None),
                }
            }
            ("java/util/regex/Pattern", "quote", "(Ljava/lang/String;)Ljava/lang/String;") => {
                Ok(text(0).map(|value| Value::Ref(Some(java_regex::quote(&value)))))
            }
            (
                "java/util/regex/Matcher",
                "quoteReplacement",
                "(Ljava/lang/String;)Ljava/lang/String;",
            ) => Ok(text(0).map(|value| Value::Ref(Some(java_regex::quote_replacement(&value))))),
            _ => Ok(None),
        }
    }

    /// `String.split(regex)` / `split(regex, limit)` into a runtime-owned
    /// `String[]`.
    ///
    /// The host layer has no array table, so the result array belongs to this
    /// execution, exactly like a `toArray()` from a stream. A pattern outside
    /// the modelled subset is counted (the host answers it).
    pub(crate) fn invoke_string_split(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
        class: &str,
        method: &str,
    ) -> Result<Option<Value>, NativeError> {
        if r.owner != "java/lang/String" || r.name != "split" {
            return Ok(None);
        }
        let limit = match r.descriptor.as_str() {
            "(Ljava/lang/String;)[Ljava/lang/String;" => 0,
            "(Ljava/lang/String;I)[Ljava/lang/String;" => {
                expect_int(r, StdHost::argument(r, args, 1)?)?
            }
            _ => return Ok(None),
        };
        // A null receiver or pattern is an NPE on the JVM, and config code
        // commonly catches it around an optional value; keep it a real throw
        // so the caller's handler runs instead of turning into a hard stop.
        let receiver =
            receiver
                .and_then(expect_ref)
                .ok_or_else(|| NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: Some("String#split has a null receiver".into()),
                    frame: None,
                })?;
        let pattern = expect_ref(StdHost::argument(r, args, 0)?).ok_or_else(|| {
            NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: Some("String#split has a null pattern".into()),
                frame: None,
            }
        })?;
        let Ok(parts) = split_java_string(receiver.as_str(), pattern, limit) else {
            return Ok(None);
        };
        let marker = self.new_array(class, method);
        self.arrays.insert(
            marker.clone(),
            parts
                .into_iter()
                .map(|part| Value::Ref(Some(part)))
                .collect(),
        );
        Ok(Some(Value::Ref(Some(marker))))
    }
}
