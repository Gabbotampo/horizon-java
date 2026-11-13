//! `Charset.newDecoder()` for UTF-8 and the decoder's error-action builders, as
//! JDK 25 defines them.
//!
//! night-config's `ConfigParser.parse(Path, ..)` reads a config file through
//! `new InputStreamReader(in, charset.newDecoder()
//! .onMalformedInput(CodingErrorAction.REPORT)
//! .onUnmappableCharacter(CodingErrorAction.REPORT))`. Unanswered, `newDecoder`
//! was null and the parse failed with an NPE on `onMalformedInput` (every
//! Fabric jar that runs Forge Config API Port's own config classes, #124).
//!
//! From JDK 25's `CharsetDecoder` and `sun.nio.cs.UTF_8.Decoder` (Temurin
//! 25.0.5+8 `src.zip`):
//!
//! * a new decoder's actions are both `REPORT`, its replacement `"�"`, and
//!   UTF-8's `averageCharsPerByte`/`maxCharsPerByte` are both `1.0`;
//! * `onMalformedInput`/`onUnmappableCharacter` set the action and return
//!   `this`; a null action throws `IllegalArgumentException("Null action")`;
//! * `replaceWith(s)` returns `this`; a null, empty, or longer than
//!   `maxCharsPerByte` replacement throws `IllegalArgumentException("Null
//!   replacement")`, `("Empty replacement")`, `("Replacement too long")`;
//! * `CodingErrorAction.IGNORE`/`REPLACE`/`REPORT` print their names.
//!
//! The decoding itself is the host's `InputStreamReader` model, which reads the
//! stream's text. Named departure: that model decodes invalid UTF-8 with
//! replacement characters, where a `REPORT` decoder makes the JDK's read throw
//! `MalformedInputException`. The files the model writes, and the jar
//! resources a config ships, are valid UTF-8, where the two agree. A decoder
//! of any other charset, and the decoder's `decode` methods, stay counted.

use super::*;

const DECODER: &str = "java/nio/charset/CharsetDecoder";
const ACTION: &str = "java/nio/charset/CodingErrorAction";
const ACTION_PREFIX: &str = "coding-error-action/";
const DECODER_PREFIX: &str = "charset-decoder/UTF-8/";

/// One decoder's settable state: (malformed action, unmappable action,
/// replacement).
#[derive(Clone, Debug)]
pub(crate) struct DecoderState {
    malformed: String,
    unmappable: String,
    replacement: String,
}

fn iae(message: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

fn action(name: &str) -> Value {
    Value::Ref(Some(format!("{ACTION_PREFIX}{name}")))
}

impl StdHost {
    /// `CodingErrorAction`'s three constants.
    pub(crate) fn coding_error_action_static(r: &MethodRef) -> Option<Value> {
        (r.owner == ACTION && matches!(r.name.as_str(), "IGNORE" | "REPLACE" | "REPORT"))
            .then(|| action(&r.name))
    }

    /// `Charset.newDecoder`, a decoder's members and `CodingErrorAction`'s
    /// `toString`; `None` for any other call.
    pub(crate) fn charset_decoder_call(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let marker = expect_ref(receiver)?.clone();
        if r.owner == "java/nio/charset/Charset"
            && r.name == "newDecoder"
            && r.descriptor == "()Ljava/nio/charset/CharsetDecoder;"
        {
            if marker != "charset/UTF-8" {
                return None;
            }
            let decoder = format!("{DECODER_PREFIX}{}", self.charset_decoders.len());
            self.charset_decoders.insert(
                decoder.clone(),
                DecoderState {
                    malformed: "REPORT".into(),
                    unmappable: "REPORT".into(),
                    replacement: "\u{FFFD}".into(),
                },
            );
            return Some(Ok(Value::Ref(Some(decoder))));
        }
        if r.owner == ACTION
            && r.name == "toString"
            && let Some(name) = marker.strip_prefix(ACTION_PREFIX)
        {
            return Some(Ok(Value::Ref(Some(name.to_owned()))));
        }
        if r.owner != DECODER {
            return None;
        }
        let state = self.charset_decoders.get(&marker)?.clone();
        let argument = || args.first().cloned().unwrap_or(Value::Ref(None));
        Some(match (r.name.as_str(), r.descriptor.as_str()) {
            (
                "onMalformedInput" | "onUnmappableCharacter",
                "(Ljava/nio/charset/CodingErrorAction;)Ljava/nio/charset/CharsetDecoder;",
            ) => {
                let Some(name) = expect_ref(&argument())
                    .and_then(|value| value.strip_prefix(ACTION_PREFIX))
                    .map(str::to_owned)
                else {
                    return Some(match argument() {
                        Value::Ref(None) => Err(iae("Null action")),
                        _ => self.unknown_host_call(r),
                    });
                };
                let entry = self.charset_decoders.get_mut(&marker)?;
                if r.name == "onMalformedInput" {
                    entry.malformed = name;
                } else {
                    entry.unmappable = name;
                }
                Ok(receiver.clone())
            }
            ("replaceWith", "(Ljava/lang/String;)Ljava/nio/charset/CharsetDecoder;") => {
                match argument() {
                    Value::Ref(None) => Err(iae("Null replacement")),
                    Value::Ref(Some(text)) if text.is_empty() => Err(iae("Empty replacement")),
                    // `maxCharsPerByte` is 1.0: one UTF-16 unit at most.
                    Value::Ref(Some(text)) if text.encode_utf16().count() > 1 => {
                        Err(iae("Replacement too long"))
                    }
                    Value::Ref(Some(text)) => {
                        self.charset_decoders.get_mut(&marker)?.replacement = text;
                        Ok(receiver.clone())
                    }
                    _ => self.unknown_host_call(r),
                }
            }
            ("malformedInputAction", "()Ljava/nio/charset/CodingErrorAction;") => {
                Ok(action(&state.malformed))
            }
            ("unmappableCharacterAction", "()Ljava/nio/charset/CodingErrorAction;") => {
                Ok(action(&state.unmappable))
            }
            ("replacement", "()Ljava/lang/String;") => Ok(Value::Ref(Some(state.replacement))),
            ("charset", "()Ljava/nio/charset/Charset;") => {
                Ok(Value::Ref(Some("charset/UTF-8".into())))
            }
            ("averageCharsPerByte" | "maxCharsPerByte", "()F") => Ok(Value::Float(1.0)),
            _ => self.unknown_host_call(r),
        })
    }
}
