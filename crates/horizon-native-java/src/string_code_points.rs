//! `String.codePoints()` followed straight by `forEach`, `String.toCharArray()`,
//! `Character.toChars(int)` and the two `identity()` functions, as JDK 25 runs
//! them: the pieces night-config's TOML writer and Forge Config API Port's
//! value reads reach once a Fabric run runs the port's own classes (#124).
//!
//! `Character.toChars` answers one unit for a BMP code point, a surrogate pair
//! for a supplementary one, and throws `IllegalArgumentException("Not a valid
//! Unicode code point: 0x%X")` otherwise. `Function.identity()` and
//! `UnaryOperator.identity()` are each `t -> t`: one non-capturing lambda per
//! method, so each answers the same object every time and the two differ.
//!
//! night-config's TOML writer and parser escape a string with
//! `str.codePoints().forEach(c -> ...)` (`toml/StringWriter`, `StringParser`,
//! `TomlParser`). `codePoints` was counted and answered null, so writing a
//! config threw an NPE on `IntStream.forEach` (every Fabric jar that runs
//! Forge Config API Port's own classes, #124).
//!
//! JDK 25's `String.codePoints()` is a sequential `IntStream` of the string's
//! code points: a surrogate pair is one code point, and an unpaired surrogate
//! is itself. The runtime keeps strings as valid Unicode, so the code points
//! are the string's `char`s. `forEach(IntConsumer)` on that stream runs the
//! consumer once per code point, in order.
//!
//! The runtime's streams are eager (#128 makes them lazy), which only matches
//! the JDK when no stage runs between the source and the terminal operation.
//! So `forEach`/`forEachOrdered` runs here only on a stream `codePoints` has
//! just made; the stream's other operations go through the existing stream
//! model, and a `forEach` after any stage stays counted.

use super::*;

/// The callback markers of `Function.identity()` and `UnaryOperator.identity()`,
/// which `Runtime::invoke_callback` answers with their argument.
pub(crate) const FUNCTION_IDENTITY: &str = "@fn:identity/function";
pub(crate) const UNARY_IDENTITY: &str = "@fn:identity/unary";

/// The class and method a fresh `codePoints` stream's marker carries. A jar
/// frame's own stages name the jar's class, never `java/lang/String`.
const SOURCE: (&str, &str) = ("java/lang/String", "codePoints");

impl<H: Host> Runtime<'_, H> {
    /// `String.codePoints()` and `forEach` on the stream it made; `Ok(None)`
    /// for any other call.
    pub(crate) fn invoke_code_points(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            ("java/lang/String", "codePoints", "()Ljava/util/stream/IntStream;") => {
                let Some(Value::Ref(Some(text))) = receiver else {
                    return Ok(None);
                };
                if self.string_builders.contains_key(text)
                    || crate::jar_text::object_marker_class(text).is_some()
                {
                    return Ok(None);
                }
                let points = text.chars().map(|c| Value::Int(c as i32)).collect();
                Ok(Some(self.new_stream(SOURCE.0, SOURCE.1, points)))
            }
            ("java/util/function/Function", "identity", "()Ljava/util/function/Function;") => {
                Ok(Some(Value::Ref(Some(FUNCTION_IDENTITY.into()))))
            }
            (
                "java/util/function/UnaryOperator",
                "identity",
                "()Ljava/util/function/UnaryOperator;",
            ) => Ok(Some(Value::Ref(Some(UNARY_IDENTITY.into())))),
            ("java/lang/Character", "toChars", "(I)[C") => {
                let point = expect_int(r, StdHost::argument(r, args, 0)?)?;
                let Some(c) = u32::try_from(point).ok().and_then(char::from_u32) else {
                    // A surrogate code point is valid and a single unit.
                    if (0xD800..=0xDFFF).contains(&point) {
                        let marker = self.new_array("java/lang/Character", "toChars");
                        self.arrays.insert(marker.clone(), vec![Value::Int(point)]);
                        self.record_array_component(&marker, "char");
                        return Ok(Some(Value::Ref(Some(marker))));
                    }
                    return Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalArgumentException".into(),
                        message: Some(format!(
                            "Not a valid Unicode code point: 0x{:X}",
                            point as u32
                        )),
                        frame: None,
                    });
                };
                let mut units = [0_u16; 2];
                let units = c
                    .encode_utf16(&mut units)
                    .iter()
                    .map(|unit| Value::Int(i32::from(*unit)))
                    .collect();
                let marker = self.new_array("java/lang/Character", "toChars");
                self.arrays.insert(marker.clone(), units);
                self.record_array_component(&marker, "char");
                Ok(Some(Value::Ref(Some(marker))))
            }
            // `toCharArray()`: a new `char[]` of the string's UTF-16 units,
            // which the writer path below hands to `Writer.write(char[], ..)`.
            ("java/lang/String", "toCharArray", "()[C") => {
                let Some(Value::Ref(Some(text))) = receiver else {
                    return Ok(None);
                };
                if self.string_builders.contains_key(text)
                    || crate::jar_text::object_marker_class(text).is_some()
                {
                    return Ok(None);
                }
                let units = text
                    .encode_utf16()
                    .map(|unit| Value::Int(i32::from(unit)))
                    .collect();
                let marker = self.new_array("java/lang/String", "toCharArray");
                self.arrays.insert(marker.clone(), units);
                self.record_array_component(&marker, "char");
                Ok(Some(Value::Ref(Some(marker))))
            }
            (
                "java/util/stream/IntStream",
                "forEach" | "forEachOrdered",
                "(Ljava/util/function/IntConsumer;)V",
            ) => {
                let Some(marker) = receiver.and_then(expect_ref) else {
                    return Ok(None);
                };
                if !marker.starts_with(&format!("stream/{}/{}/", SOURCE.0, SOURCE.1)) {
                    return Ok(None);
                }
                let Some(points) = self.streams.get(marker).cloned() else {
                    return Ok(None);
                };
                let consumer = StdHost::argument(r, args, 0)?.clone();
                if matches!(consumer, Value::Ref(None)) {
                    // `Objects.requireNonNull(action)`.
                    return Err(NativeError::UncaughtException {
                        class: "java/lang/NullPointerException".into(),
                        message: None,
                        frame: None,
                    });
                }
                let accept = MethodRef {
                    owner: "java/util/function/IntConsumer".into(),
                    name: "accept".into(),
                    descriptor: "(I)V".into(),
                };
                for point in points {
                    if self
                        .invoke_callback(&consumer, std::slice::from_ref(&point))?
                        .is_some()
                    {
                        continue;
                    }
                    match self.dispatch_owner(&accept, 0xb9, Some(&consumer)) {
                        Some((owner, true)) => {
                            self.invoke(
                                &owner,
                                "accept",
                                "(I)V",
                                Some(consumer.clone()),
                                std::slice::from_ref(&point),
                            )?;
                        }
                        _ => {
                            self.host.unknown_operand_call(&accept)?;
                        }
                    }
                }
                Ok(Some(Value::Void))
            }
            _ => Ok(None),
        }
    }
}
