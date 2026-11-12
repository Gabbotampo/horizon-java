//! JDK 21's `SequencedCollection` members on the lists the host models:
//! `getFirst`, `getLast`, `removeFirst`, `removeLast`, `addFirst`, `addLast`
//! and `reversed`.
//!
//! These used to be answered only for a queue owner, so `List#getFirst` fell
//! through to the neutral `null` (collective's `ArrayBackedEvent#rebuildInvoker`
//! read a field of that null). What JDK 27 does, measured on OpenJDK 27 for
//! `ArrayList`, `LinkedList`, `List.of` and `Collections.unmodifiableList`:
//!
//! - `getFirst`/`getLast` on an empty list throw `NoSuchElementException` with
//!   no message, on every kind;
//! - `removeFirst`/`removeLast` on a `List.of` list throw
//!   `UnsupportedOperationException` even when it is empty (the immutable
//!   list overrides them). On an unmodifiable *view* the default method runs:
//!   empty is `NoSuchElementException`, otherwise `remove(0)` is
//!   `UnsupportedOperationException`;
//! - `addFirst`/`addLast` are `add(0, e)`/`add(e)`, so unmodifiable lists
//!   throw `UnsupportedOperationException`;
//! - `reversed()` is a live view (`ReverseOrderListView$Rand`, or
//!   `LinkedList$ReverseOrderLinkedListView` for a `LinkedList`). It reads its
//!   list backwards, writes through to it when the list is modifiable
//!   (`view.add(e)` is `list.addFirst(e)`), throws
//!   `UnsupportedOperationException` for every mutator when it is not, and
//!   `view.reversed()` is the list itself.
//!
//! A view mutator this module does not map (`add(int, e)`, `set`, `sort`,
//! `removeIf`, ...) is counted, not guessed. So is `reversed()` of a list kind
//! whose view class is not listed above (`CopyOnWriteArrayList`'s own view, a
//! fastutil list).

use super::*;

const LINKED_VIEW: &str = "java/util/LinkedList$ReverseOrderLinkedListView";
const RANDOM_ACCESS_VIEW: &str = "java/util/ReverseOrderListView$Rand";

fn no_such_element() -> NativeError {
    NativeError::UncaughtException {
        class: "java/util/NoSuchElementException".into(),
        message: None,
        frame: None,
    }
}

fn unsupported() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/UnsupportedOperationException".into(),
        message: None,
        frame: None,
    }
}

/// The supertypes `instanceof` sees for a reversed view.
pub(crate) fn supertypes(class: &str) -> &'static [&'static str] {
    match class {
        RANDOM_ACCESS_VIEW => &[
            "java/util/List",
            "java/util/SequencedCollection",
            "java/util/Collection",
            "java/lang/Iterable",
            "java/util/RandomAccess",
        ],
        LINKED_VIEW => &[
            "java/util/LinkedList",
            "java/util/AbstractSequentialList",
            "java/util/AbstractList",
            "java/util/AbstractCollection",
            "java/util/List",
            "java/util/Deque",
            "java/util/Queue",
            "java/util/SequencedCollection",
            "java/util/Collection",
            "java/lang/Iterable",
            "java/lang/Cloneable",
            "java/io/Serializable",
        ],
        _ => &[],
    }
}

impl StdHost {
    /// The elements a reversed view reads: its list's current ones, backwards.
    pub(crate) fn reversed_view_values(&self, marker: &str) -> Option<Vec<String>> {
        let (base, _) = self.reversed_views.get(marker)?;
        let mut values = host_collection_values(self, &Value::Ref(Some(base.clone()))).ok()?;
        values.reverse();
        Some(values)
    }

    /// A sequenced member on a list, or `None` for anything else.
    pub(crate) fn sequenced_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        marker: &str,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        if !is_list_owner(&r.owner) && r.owner != "java/util/SequencedCollection" {
            return None;
        }
        if let Some((base, modifiable)) = self.reversed_views.get(marker).cloned()
            && let Some(answer) = self.reversed_member(r, &base, modifiable, args)
        {
            return Some(answer);
        }
        let name = r.name.as_str();
        match (name, r.descriptor.as_str()) {
            ("getFirst" | "getLast", "()Ljava/lang/Object;") => {
                Some(self.end_value(receiver, name == "getFirst"))
            }
            ("removeFirst" | "removeLast", "()Ljava/lang/Object;") => {
                Some(self.remove_end(receiver, marker, name == "removeFirst"))
            }
            ("addFirst" | "addLast", "(Ljava/lang/Object;)V") => {
                let value = args.first().cloned().unwrap_or(Value::Ref(None));
                Some(
                    self.add_end(receiver, marker, value, name == "addFirst")
                        .map(|()| Value::Void),
                )
            }
            ("reversed", descriptor) if descriptor.starts_with("()L") => {
                self.reversed(marker).map(Ok)
            }
            _ => None,
        }
    }

    fn end_value(&self, receiver: &Value, first: bool) -> Result<Value, NativeError> {
        let values = host_collection_values(self, receiver)?;
        let end = if first { values.first() } else { values.last() };
        parse_collection_value(end.ok_or_else(no_such_element)?)
    }

    /// The mutable table behind `marker`, seeded from its current elements
    /// (a content-addressed `list/...` result is an `ArrayList`).
    fn mutable_table(
        &mut self,
        receiver: &Value,
        marker: &str,
    ) -> Result<&mut Vec<Value>, NativeError> {
        if !self.mutable_collections.contains_key(marker) {
            let values = host_collection_values(self, receiver)?
                .iter()
                .map(|value| parse_collection_value(value))
                .collect::<Result<Vec<_>, _>>()?;
            self.mutable_collections.insert(marker.to_owned(), values);
        }
        Ok(self
            .mutable_collections
            .entry(marker.to_owned())
            .or_default())
    }

    fn remove_end(
        &mut self,
        receiver: &Value,
        marker: &str,
        first: bool,
    ) -> Result<Value, NativeError> {
        match self.unmodifiable_collections.get(marker) {
            // `List.of`: the immutable list's own override.
            Some(None) => return Err(unsupported()),
            // A view: the default method's emptiness check, then `remove`.
            Some(Some(_)) => {
                return Err(if host_collection_values(self, receiver)?.is_empty() {
                    no_such_element()
                } else {
                    unsupported()
                });
            }
            None => {}
        }
        let values = self.mutable_table(receiver, marker)?;
        if values.is_empty() {
            return Err(no_such_element());
        }
        Ok(if first {
            values.remove(0)
        } else {
            values.pop().unwrap_or(Value::Ref(None))
        })
    }

    fn add_end(
        &mut self,
        receiver: &Value,
        marker: &str,
        value: Value,
        first: bool,
    ) -> Result<(), NativeError> {
        if self.unmodifiable_collections.contains_key(marker) {
            return Err(unsupported());
        }
        let values = self.mutable_table(receiver, marker)?;
        if first {
            values.insert(0, value);
        } else {
            values.push(value);
        }
        Ok(())
    }

    /// `list.reversed()`: a view over `marker`, or `None` for a list whose
    /// view class this module does not model.
    fn reversed(&mut self, marker: &str) -> Option<Value> {
        let class = marker.split_once('#').map_or(marker, |(class, _)| class);
        let view = match class {
            "java/util/LinkedList" => LINKED_VIEW,
            "java/util/ArrayList" | "java/util/ImmutableCollections$ListN" => RANDOM_ACCESS_VIEW,
            _ if marker == "list" || marker.starts_with("list/") => RANDOM_ACCESS_VIEW,
            _ => return None,
        };
        let modifiable = !self.unmodifiable_collections.contains_key(marker);
        let view_marker = format!("{view}#reversed{}", self.reversed_views.len());
        self.reversed_views
            .insert(view_marker.clone(), (marker.to_owned(), modifiable));
        Some(Value::Ref(Some(view_marker)))
    }

    /// A member of a reversed view: a mutator maps onto its list, and
    /// `reversed()` is the list itself. Reads answer `None` and fall through
    /// to the list arms, which read the view backwards.
    fn reversed_member(
        &mut self,
        r: &MethodRef,
        base: &str,
        modifiable: bool,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let name = r.name.as_str();
        if name == "reversed" && r.descriptor.starts_with("()L") {
            return Some(Ok(Value::Ref(Some(base.to_owned()))));
        }
        if !java_immutable::is_mutator(name) {
            return None;
        }
        if !modifiable {
            return Some(Err(unsupported()));
        }
        let list = Value::Ref(Some(base.to_owned()));
        let value = || args.first().cloned().unwrap_or(Value::Ref(None));
        Some(match (name, r.descriptor.as_str()) {
            ("add", "(Ljava/lang/Object;)Z") => self
                .add_end(&list, base, value(), true)
                .map(|()| Value::Int(1)),
            ("addFirst", "(Ljava/lang/Object;)V") => self
                .add_end(&list, base, value(), false)
                .map(|()| Value::Void),
            ("addLast", "(Ljava/lang/Object;)V") => self
                .add_end(&list, base, value(), true)
                .map(|()| Value::Void),
            ("removeFirst", "()Ljava/lang/Object;") => self.remove_end(&list, base, false),
            ("removeLast", "()Ljava/lang/Object;") => self.remove_end(&list, base, true),
            ("clear", "()V") => self
                .mutable_table(&list, base)
                .map(|values| values.clear())
                .map(|()| Value::Void),
            _ => self.unknown_host_call(r),
        })
    }
}
