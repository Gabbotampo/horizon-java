//! `listIterator()` on the host's `ArrayList`s and `LinkedList`s, as JDK 25's
//! `ArrayList.ListItr` and `LinkedList.ListItr` run it.
//!
//! The host's lists had `iterator()` (a snapshot) but no `listIterator`: the
//! call was unmodelled, so a jar walking a list backwards or editing it
//! through the iterator got a neutral answer. A list iterator here works on
//! the host's list itself, so `set`, `add` and `remove` write through and
//! `hasNext` sees the list's current size:
//!
//! * `listIterator(index)` checks `0 <= index <= size`
//!   (`IndexOutOfBoundsException: "Index: i, Size: s"`).
//! * `next`/`previous` past either end throw `NoSuchElementException`;
//!   `remove`/`set` without a current element (none returned yet, or after
//!   `add`/`remove`) throw `IllegalStateException`; `remove` after
//!   `previous` keeps the cursor, after `next` steps it back.
//! * `forEachRemaining` hands each remaining element to the action (the
//!   runtime runs it, since the action is jar code), then leaves the cursor
//!   at the end with the last one current.
//!
//! **Departure:** the JDK detects concurrent modification by `modCount`; the
//! host's lists keep none, so a structural change made around the iterator
//! is detected when it changed the list's size (`add`/`remove` elsewhere),
//! not when an `add` and a `remove` cancel out or a `sort` reorders. `set`
//! is not structural in the JDK either.

use super::*;

/// Prefix of a host list iterator marker.
const PREFIX: &str = "list-iterator/";

/// Which JDK class's iterator: their checks run in different orders.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Array,
    Linked,
}

/// One `ListItr` over a host list.
#[derive(Clone, Debug)]
pub(crate) struct ListIter {
    list: String,
    kind: Kind,
    cursor: usize,
    /// The index `next`/`previous` last returned (`lastRet`), if current.
    last: Option<usize>,
    /// The list's size as this iterator last left it: a different size means
    /// another modification (the `modCount` stand-in).
    expected: usize,
}

fn throw(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.to_owned(),
        message,
        frame: None,
    }
}

fn cme() -> NativeError {
    throw("java/util/ConcurrentModificationException", None)
}

impl StdHost {
    /// `listIterator()`/`listIterator(int)` on a host list, and the members of
    /// the iterator it answers. `None` leaves the call to the rest of the host.
    pub(crate) fn list_iterator_call(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let marker = expect_ref(receiver)?.clone();
        if r.name == "listIterator" && r.descriptor.ends_with(")Ljava/util/ListIterator;") {
            let kind = if marker.starts_with("java/util/ArrayList#") {
                Kind::Array
            } else if marker.starts_with("java/util/LinkedList#") {
                Kind::Linked
            } else {
                return None;
            };
            let size = self.mutable_collections.get(&marker)?.len();
            let index = match args.first() {
                None => 0,
                Some(Value::Int(index)) => *index,
                Some(_) => return None,
            };
            if index < 0 || index as usize > size {
                return Some(Err(throw(
                    "java/lang/IndexOutOfBoundsException",
                    Some(format!("Index: {index}, Size: {size}")),
                )));
            }
            let iterator = format!("{PREFIX}{}", self.next_iterator);
            self.next_iterator = self.next_iterator.saturating_add(1);
            self.list_iterators.insert(
                iterator.clone(),
                ListIter {
                    list: marker,
                    kind,
                    cursor: index as usize,
                    last: None,
                    expected: size,
                },
            );
            return Some(Ok(Value::Ref(Some(iterator))));
        }
        if !marker.starts_with(PREFIX)
            || !matches!(
                r.owner.as_str(),
                "java/util/Iterator" | "java/util/ListIterator"
            )
        {
            return None;
        }
        let mut it = self.list_iterators.get(&marker)?.clone();
        let result = self.list_iterator_member(&mut it, r, args);
        self.list_iterators.insert(marker, it);
        Some(result)
    }

    fn list_iterator_member(
        &mut self,
        it: &mut ListIter,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let arg = || args.first().cloned().unwrap_or(Value::Ref(None));
        let size = self.mutable_collections.get(&it.list).map_or(0, Vec::len);
        let modified = size != it.expected;
        let check = || if modified { Err(cme()) } else { Ok(()) };
        let list = it.list.clone();
        let element = |host: &Self, index: usize| {
            host.mutable_collections
                .get(&list)
                .and_then(|values| values.get(index).cloned())
                .ok_or_else(cme)
        };
        Ok(match (r.name.as_str(), r.descriptor.as_str()) {
            ("hasNext", "()Z") => Value::Int(i32::from(it.cursor != size)),
            ("hasPrevious", "()Z") => Value::Int(i32::from(it.cursor != 0)),
            ("nextIndex", "()I") => Value::Int(it.cursor as i32),
            ("previousIndex", "()I") => Value::Int(it.cursor as i32 - 1),
            ("next", "()Ljava/lang/Object;") => {
                check()?;
                if it.cursor >= size {
                    return Err(throw("java/util/NoSuchElementException", None));
                }
                let value = element(self, it.cursor)?;
                it.last = Some(it.cursor);
                it.cursor += 1;
                value
            }
            ("previous", "()Ljava/lang/Object;") => {
                check()?;
                if it.cursor == 0 {
                    return Err(throw("java/util/NoSuchElementException", None));
                }
                it.cursor -= 1;
                it.last = Some(it.cursor);
                element(self, it.cursor)?
            }
            ("remove", "()V") => {
                // `LinkedList.ListItr` checks for modification first,
                // `ArrayList.ListItr` for a current element first.
                if it.kind == Kind::Linked {
                    check()?;
                }
                let Some(last) = it.last else {
                    return Err(throw("java/lang/IllegalStateException", None));
                };
                check()?;
                if let Some(values) = self.mutable_collections.get_mut(&it.list) {
                    values.remove(last);
                }
                it.cursor = last;
                it.last = None;
                it.expected = size - 1;
                Value::Void
            }
            ("set", "(Ljava/lang/Object;)V") => {
                let Some(last) = it.last else {
                    return Err(throw("java/lang/IllegalStateException", None));
                };
                check()?;
                if let Some(slot) = self
                    .mutable_collections
                    .get_mut(&it.list)
                    .and_then(|values| values.get_mut(last))
                {
                    *slot = arg();
                }
                Value::Void
            }
            ("add", "(Ljava/lang/Object;)V") => {
                check()?;
                if let Some(values) = self.mutable_collections.get_mut(&it.list) {
                    values.insert(it.cursor, arg());
                }
                it.cursor += 1;
                it.last = None;
                it.expected = size + 1;
                Value::Void
            }
            _ => {
                return Err(NativeError::MissingMethod(format!(
                    "{}#{}{}",
                    r.owner, r.name, r.descriptor
                )));
            }
        })
    }
}

impl<H: Host> Runtime<'_, H> {
    /// `forEachRemaining(action)` on a host list iterator: `next()` and
    /// `action.accept` per remaining element. The host keeps the iterator;
    /// the runtime runs the action, which is the jar's.
    pub(crate) fn list_iterator_for_each(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.name != "forEachRemaining"
            || r.descriptor != "(Ljava/util/function/Consumer;)V"
            || !receiver
                .and_then(expect_ref)
                .is_some_and(|marker| marker.starts_with(PREFIX))
        {
            return Ok(None);
        }
        let (Some(it), Some(action)) = (receiver, args.first()) else {
            return Ok(None);
        };
        if *action == Value::Ref(None) {
            return Err(throw("java/lang/NullPointerException", None));
        }
        let member = |name: &str, descriptor: &str| MethodRef {
            owner: "java/util/ListIterator".into(),
            name: name.into(),
            descriptor: descriptor.into(),
        };
        loop {
            let more = self
                .host
                .invoke_instance(&member("hasNext", "()Z"), it, &[])?;
            if !matches!(more, Value::Int(flag) if flag != 0) {
                return Ok(Some(Value::Void));
            }
            let value =
                self.host
                    .invoke_instance(&member("next", "()Ljava/lang/Object;"), it, &[])?;
            if self
                .invoke_callback(action, std::slice::from_ref(&value))?
                .is_none()
            {
                self.host.invoke_instance(
                    &MethodRef {
                        owner: "java/util/function/Consumer".into(),
                        name: "accept".into(),
                        descriptor: "(Ljava/lang/Object;)V".into(),
                    },
                    action,
                    &[value],
                )?;
            }
        }
    }
}
