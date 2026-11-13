//! commons-lang3's `Pair` / `ImmutablePair`, as 3.20.0 (the copy the 26.3 game
//! ships, Apache-2.0) runs them.
//!
//! commons-lang3 is a game library, so a mod calls it without bundling it.
//! Forge Config API Port's `ModConfigSpec.Builder.configure` answers
//! `Pair.of(config, spec)`, and a config class reads `getLeft()`/`getRight()`
//! in its static initializer; unanswered, `Pair.of` was null and the
//! initializer failed with an NPE (cropsloverain, ArmorPoser).
//!
//! From the bytecode (`javap -c -p`):
//!
//! * `Pair.of(l, r)` is `ImmutablePair.of(l, r)`; `ImmutablePair.left(l)` and
//!   `right(r)` are `of(l, null)` and `of(null, r)`;
//! * `ImmutablePair.of(l, r)` is the shared `NULL` instance (`nullPair()`)
//!   when both are null, else `new ImmutablePair(l, r)`;
//! * the object keeps its two values in the public final fields `left` and
//!   `right`, so a jar's `getfield` reads them too;
//! * `getLeft`/`getKey` answer `left`, `getRight`/`getValue` answer `right`;
//! * `toString()` is `"(" + getLeft() + "," + getRight() + ")"`;
//! * `ImmutablePair.setValue` throws `UnsupportedOperationException`.
//!
//! Still counted: `equals`, `hashCode`, `compareTo`, the `Map.Entry`
//! overloads of `of`, `ofNonNull`, `accept`/`apply`, `toString(String)`,
//! `MutablePair`, and a `toString` whose element is a jar object (its own
//! `toString` is bytecode this module does not run).

use super::*;
use crate::runtime::jvm_dispatch::constructed_object_class;

const PAIR: &str = "org/apache/commons/lang3/tuple/Pair";
const IMMUTABLE_PAIR: &str = "org/apache/commons/lang3/tuple/ImmutablePair";
const LEFT: &str = "leftLjava/lang/Object;";
const RIGHT: &str = "rightLjava/lang/Object;";

fn field(marker: &str, name: &str) -> String {
    format!("{marker}|{IMMUTABLE_PAIR}#{name}")
}

impl<H: Host> Runtime<'_, H> {
    /// A new `ImmutablePair` holding `left` and `right`, or the shared `NULL`.
    fn commons_pair(&mut self, left: Value, right: Value) -> Value {
        let null_pair = matches!(left, Value::Ref(None)) && matches!(right, Value::Ref(None));
        let pair = if null_pair {
            Value::Ref(Some(format!("{IMMUTABLE_PAIR}#nullPair")))
        } else {
            self.new_object(IMMUTABLE_PAIR)
        };
        if let Some(marker) = expect_ref(&pair).cloned() {
            self.fields.insert(field(&marker, LEFT), left);
            self.fields.insert(field(&marker, RIGHT), right);
        }
        pair
    }

    /// Answer a commons-lang3 pair call, or `Ok(None)` for any other call.
    pub(crate) fn invoke_commons_pair(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Ref(None));
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            (
                PAIR,
                "of",
                "(Ljava/lang/Object;Ljava/lang/Object;)Lorg/apache/commons/lang3/tuple/Pair;",
            )
            | (
                IMMUTABLE_PAIR,
                "of",
                "(Ljava/lang/Object;Ljava/lang/Object;)Lorg/apache/commons/lang3/tuple/ImmutablePair;",
            ) => return Ok(Some(self.commons_pair(arg(0), arg(1)))),
            (
                IMMUTABLE_PAIR,
                "left",
                "(Ljava/lang/Object;)Lorg/apache/commons/lang3/tuple/Pair;",
            ) => {
                return Ok(Some(self.commons_pair(arg(0), Value::Ref(None))));
            }
            (
                IMMUTABLE_PAIR,
                "right",
                "(Ljava/lang/Object;)Lorg/apache/commons/lang3/tuple/Pair;",
            ) => {
                return Ok(Some(self.commons_pair(Value::Ref(None), arg(0))));
            }
            (IMMUTABLE_PAIR, "<init>", "(Ljava/lang/Object;Ljava/lang/Object;)V") => {
                let Some(marker) = receiver.and_then(expect_ref).cloned() else {
                    return Ok(None);
                };
                self.fields.insert(field(&marker, LEFT), arg(0));
                self.fields.insert(field(&marker, RIGHT), arg(1));
                return Ok(Some(Value::Void));
            }
            _ => {}
        }
        // Instance members of a pair this module built.
        let Some(marker) = receiver.and_then(expect_ref).cloned() else {
            return Ok(None);
        };
        let Some(left) = self.fields.get(&field(&marker, LEFT)).cloned() else {
            return Ok(None);
        };
        let right = self
            .fields
            .get(&field(&marker, RIGHT))
            .cloned()
            .unwrap_or(Value::Ref(None));
        if !matches!(
            r.owner.as_str(),
            PAIR | IMMUTABLE_PAIR | "java/util/Map$Entry" | "java/lang/Object"
        ) {
            return Ok(None);
        }
        Ok(match (r.name.as_str(), r.descriptor.as_str()) {
            ("getLeft" | "getKey", "()Ljava/lang/Object;") => Some(left),
            ("getRight" | "getValue", "()Ljava/lang/Object;") => Some(right),
            ("setValue", "(Ljava/lang/Object;)Ljava/lang/Object;") => {
                return Err(NativeError::UncaughtException {
                    class: "java/lang/UnsupportedOperationException".into(),
                    message: None,
                    frame: None,
                });
            }
            ("toString", "()Ljava/lang/String;") => {
                let mut parts = Vec::with_capacity(2);
                for part in [&left, &right] {
                    if expect_ref(part)
                        .is_some_and(|marker| constructed_object_class(marker).is_some())
                    {
                        return self.host.unknown_operand_call(r).map(Some);
                    }
                    parts.push(match part {
                        Value::Ref(None) => "null".to_owned(),
                        other => self
                            .host
                            .modelled_to_string(other)
                            .unwrap_or_else(|| value_text(other)),
                    });
                }
                Some(Value::Ref(Some(format!("({},{})", parts[0], parts[1]))))
            }
            _ => None,
        })
    }
}
