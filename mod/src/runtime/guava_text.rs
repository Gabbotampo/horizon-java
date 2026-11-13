//! Guava's `Splitter` and `Joiner` over a fixed separator, and
//! `Lists.newArrayList(Iterable)`, as Guava 33.6 (the copy the 26.3 game
//! ships) answers them.
//!
//! NeoForge's `ModConfigSpec` splits and joins config paths with
//! `Splitter.on(".")`, `Joiner.on(".")` and `Lists.newArrayList`. A jar that
//! runs its own copy of that class (Forge Config API Port on Fabric, under
//! the 2026-11-13 precedence ruling) reaches these from its `<clinit>`.
//! Unanswered, `Splitter.on` was null and every config path failed with an
//! NPE. `Lists.newArrayList(Iterable)` alone is counted in 9 census jars.
//!
//! The behaviour, checked against Guava 33.6.0 on JDK 25 (the expected values
//! of `tests/fixture_guava_text.rs`):
//!
//! * `Splitter.on(String)` / `on(char)`: an empty separator throws
//!   `IllegalArgumentException("The separator may not be the empty
//!   string.")`. `split(sequence)` keeps every part, empty ones included
//!   (`"a.b..c."` gives `[a, b, , c, ]`); a null sequence throws an NPE with
//!   no message. `splitToList` is the same parts as a list.
//! * `Joiner.on(String)` / `on(char)`: `join(Iterable)` and `join(Iterator)`
//!   write each part's `toString` with the separator between them; a null
//!   part throws an NPE with no message. `join(Object[])` stays counted: an
//!   array belongs to the interpreter, not this host.
//! * `Lists.newArrayList(Iterable)`: a new mutable list of the elements, in
//!   iteration order; null throws an NPE with no message.
//!
//! Departure: `split` answers a host list where Guava answers a lazy
//! `Iterable` that is not a `Collection`. Its elements, iteration order and
//! `toString` are Guava's; `instanceof Collection` on it is true here and
//! false in Guava. A part that is a jar object (whose `toString` is the jar's
//! bytecode) is counted rather than printed. Any other `Splitter` or `Joiner`
//! configuration (`trimResults`, `omitEmptyStrings`, `limit`, `skipNulls`,
//! `useForNull`, a pattern or a `CharMatcher` separator) is counted.

use super::*;
use crate::runtime::jvm_dispatch::constructed_object_class;

const SPLITTER: &str = "com/google/common/base/Splitter";
const JOINER: &str = "com/google/common/base/Joiner";
const LISTS: &str = "com/google/common/collect/Lists";
const SPLITTER_PREFIX: &str = "guava-splitter/";
const JOINER_PREFIX: &str = "guava-joiner/";

fn npe() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    }
}

/// The separator a `Splitter.on`/`Joiner.on` argument names: a string, or a
/// `char` passed as its code.
fn separator(r: &MethodRef, value: &Value) -> Result<String, NativeError> {
    match value {
        Value::Ref(None) => Err(npe()),
        Value::Int(code) if r.descriptor.starts_with("(C)") => {
            Ok(char::from_u32(*code as u32).map_or_else(String::new, String::from))
        }
        Value::Ref(Some(text)) => Ok(text.clone()),
        _ => Err(NativeError::Metadata(format!(
            "{}#{} separator is not text",
            r.owner, r.name
        ))),
    }
}

impl StdHost {
    /// Answer a Guava text call, or `None` for any other call.
    pub(crate) fn guava_text(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let marker = receiver.and_then(expect_ref).map(String::as_str);
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            (
                SPLITTER,
                "on",
                "(Ljava/lang/String;)Lcom/google/common/base/Splitter;"
                | "(C)Lcom/google/common/base/Splitter;",
            ) => Some(Self::argument(r, args, 0).and_then(|value| {
                let separator = separator(r, value)?;
                if separator.is_empty() {
                    return Err(NativeError::UncaughtException {
                        class: "java/lang/IllegalArgumentException".into(),
                        message: Some("The separator may not be the empty string.".into()),
                        frame: None,
                    });
                }
                Ok(Value::Ref(Some(format!("{SPLITTER_PREFIX}{separator}"))))
            })),
            (
                SPLITTER,
                "split" | "splitToList",
                "(Ljava/lang/CharSequence;)Ljava/lang/Iterable;"
                | "(Ljava/lang/CharSequence;)Ljava/util/List;",
            ) => {
                let separator = marker?.strip_prefix(SPLITTER_PREFIX)?.to_owned();
                Some(Self::argument(r, args, 0).and_then(|value| {
                    let Value::Ref(Some(text)) = value else {
                        return Err(npe());
                    };
                    let parts = text
                        .split(separator.as_str())
                        .map(|part| Value::Ref(Some(part.to_owned())))
                        .collect::<Vec<_>>();
                    Ok(self.new_list(parts))
                }))
            }
            (
                JOINER,
                "on",
                "(Ljava/lang/String;)Lcom/google/common/base/Joiner;"
                | "(C)Lcom/google/common/base/Joiner;",
            ) => Some(Self::argument(r, args, 0).and_then(|value| {
                Ok(Value::Ref(Some(format!(
                    "{JOINER_PREFIX}{}",
                    separator(r, value)?
                ))))
            })),
            (
                JOINER,
                "join",
                "(Ljava/lang/Iterable;)Ljava/lang/String;"
                | "(Ljava/util/Iterator;)Ljava/lang/String;",
            ) => {
                let separator = marker?.strip_prefix(JOINER_PREFIX)?.to_owned();
                let parts = Self::argument(r, args, 0).and_then(|value| self.guava_parts(value));
                Some(match parts {
                    Err(error) => Err(error),
                    Ok(None) => self.unknown_host_call(r),
                    Ok(Some(parts)) => self.join_parts(r, &separator, &parts),
                })
            }
            (LISTS, "newArrayList", "(Ljava/lang/Iterable;)Ljava/util/ArrayList;") => {
                let parts = Self::argument(r, args, 0).and_then(|value| self.guava_parts(value));
                Some(match parts {
                    Err(error) => Err(error),
                    Ok(None) => self.unknown_host_call(r),
                    Ok(Some(parts)) => Ok(self.new_list(parts)),
                })
            }
            _ => None,
        }
    }

    /// The elements of an `Iterable` or `Iterator` argument the host holds, in iteration order; `Ok(None)` when it holds none (the
    /// call is then counted), an NPE for null.
    fn guava_parts(&mut self, value: &Value) -> Result<Option<Vec<Value>>, NativeError> {
        let Value::Ref(Some(marker)) = value else {
            return Err(npe());
        };
        if let Some((values, index)) = self.iterators.get(marker) {
            return Ok(Some(values[(*index).min(values.len())..].to_vec()));
        }
        match runtime_collection_values(self, value) {
            Ok(values) => values
                .iter()
                .map(|value| parse_collection_value(value))
                .collect::<Result<Vec<_>, _>>()
                .map(Some),
            Err(_) => Ok(None),
        }
    }

    /// `Joiner.join` over parts the host holds.
    fn join_parts(
        &mut self,
        r: &MethodRef,
        separator: &str,
        parts: &[Value],
    ) -> Result<Value, NativeError> {
        let mut text = Vec::with_capacity(parts.len());
        for part in parts {
            if matches!(part, Value::Ref(None)) {
                return Err(npe());
            }
            // A jar object's `toString` is its own bytecode, which this host
            // can't run.
            if expect_ref(part).is_some_and(|marker| constructed_object_class(marker).is_some()) {
                return self.unknown_host_call(r);
            }
            text.push(
                self.modelled_to_string(part)
                    .unwrap_or_else(|| value_text(part)),
            );
        }
        Ok(Value::Ref(Some(text.join(separator))))
    }

    /// A new mutable host list holding `values`.
    fn new_list(&mut self, values: Vec<Value>) -> Value {
        let list = self.empty_collection("list");
        if let Some(marker) = expect_ref(&list) {
            self.mutable_collections.insert(marker.to_owned(), values);
        }
        list
    }
}
