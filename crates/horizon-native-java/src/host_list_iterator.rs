//! `listIterator()` on the host's `ArrayList`s, `Vector`s and `LinkedList`s,
//! as JDK 25's `ArrayList.ListItr`, `Vector.ListItr` and `LinkedList.ListItr`
//! run it.
//!
//! The host's lists had `iterator()` but no `listIterator`: the call was
//! unmodelled, so a jar walking a list backwards or editing it through the
//! iterator got a neutral answer. A list iterator here works on the host's
//! list itself, so `set`, `add` and `remove` write through:
//!
//! * `listIterator(index)` checks `0 <= index <= size`
//!   (`IndexOutOfBoundsException: "Index: i, Size: s"`).
//! * `hasNext` is `cursor != size` for the array lists and
//!   `nextIndex < size` for `LinkedList`, read live: after an outside
//!   removal an array list's iterator still says `true`, and its `next()`
//!   then throws.
//! * `next`/`previous` past either end throw `NoSuchElementException`;
//!   `remove`/`set` without a current element (none returned yet, or after
//!   `add`/`remove`) throw `IllegalStateException`, `LinkedList.ListItr`
//!   checking for modification before that and the array lists after;
//!   `remove` after `previous` keeps the cursor, after `next` steps it back.
//! * `forEachRemaining`: the array lists' `Itr` loops while the size and
//!   `modCount` hold and writes `cursor`/`lastRet` once, after the loop (an
//!   action that throws leaves them where they were); `LinkedList.ListItr`
//!   advances after each `accept` returns. Both check `modCount` at the end.
//!
//! Concurrent modification is the host's real `modCount` (`mod_count.rs`):
//! the iterator keeps `expectedModCount`, every member that checks it throws
//! `ConcurrentModificationException` when it moved (a restoring add and
//! remove included), and the iterator's own `add`/`remove` move both.

use super::*;
use mod_count::ModStamp;

/// Prefix of a host list iterator marker.
pub(crate) const PREFIX: &str = "list-iterator/";

/// One `ListItr` over a host list.
#[derive(Clone, Debug, PartialEq)]
pub struct ListIter {
    source: Value,
    /// `LinkedList.ListItr` rather than an array list's.
    linked: bool,
    cursor: usize,
    /// The index `next`/`previous` last returned (`lastRet`), if current.
    last: Option<usize>,
    expected: ModStamp,
}

fn throw(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.to_owned(),
        message,
        frame: None,
    }
}

fn no_such_element() -> NativeError {
    throw("java/util/NoSuchElementException", None)
}

fn illegal_state() -> NativeError {
    throw("java/lang/IllegalStateException", None)
}

/// The array lists' `catch (IndexOutOfBoundsException) { throw new
/// ConcurrentModificationException(); }` around their own writes.
fn iteration_guard_cme() -> NativeError {
    throw("java/util/ConcurrentModificationException", None)
}

/// `Consumer.accept(Object)`.
const ACCEPT: (&str, &str, &str) = (
    "java/util/function/Consumer",
    "accept",
    "(Ljava/lang/Object;)V",
);

impl<H: Host> Runtime<'_, H> {
    /// `listIterator()`/`listIterator(int)` on a host list, and the members of
    /// the iterator it answers. `Ok(None)` leaves the call to the rest of the
    /// dispatch (a list this does not model is counted there).
    pub(crate) fn invoke_list_iterator(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(receiver) = receiver.filter(|_| matches!(opcode, 0xb6 | 0xb9)) else {
            return Ok(None);
        };
        let Some(marker) = expect_ref(receiver).cloned() else {
            return Ok(None);
        };
        if marker.starts_with(PREFIX) {
            let Some(mut it) = self.host.list_iterator(&marker) else {
                return Ok(None);
            };
            let result = self.list_iterator_member(&mut it, r, args);
            self.host.store_list_iterator(&marker, it);
            return result;
        }
        if r.name != "listIterator" || !r.descriptor.ends_with(")Ljava/util/ListIterator;") {
            return Ok(None);
        }
        let class = self.host_collection_class(receiver).unwrap_or_default();
        let linked = match class.as_str() {
            "java/util/ArrayList" | "java/util/Vector" | "java/util/Stack" => false,
            "java/util/LinkedList" => true,
            _ => return Ok(None),
        };
        if !self.has_mod_count(receiver) {
            return Ok(None);
        }
        let size = self.collection_elements(receiver)?.len();
        let index = match args.first() {
            None => 0,
            Some(Value::Int(index)) => *index,
            Some(_) => return Ok(None),
        };
        if index < 0 || index as usize > size {
            return Err(throw(
                "java/lang/IndexOutOfBoundsException",
                Some(format!("Index: {index}, Size: {size}")),
            ));
        }
        let state = ListIter {
            source: receiver.clone(),
            linked,
            cursor: index as usize,
            last: None,
            expected: self.capture_mod_count(receiver),
        };
        Ok(self
            .host
            .new_list_iterator(state)
            .map(|marker| Value::Ref(Some(marker))))
    }

    /// `checkForComodification()`.
    fn list_iterator_check(&mut self, it: &ListIter, r: &MethodRef) -> Result<(), NativeError> {
        self.check_mod_count(r, &it.source, it.expected).map(drop)
    }

    /// The iterator's own structural change, written to the host's row:
    /// `modCount++` and `expectedModCount = modCount`.
    fn list_iterator_write(&mut self, it: &mut ListIter, elements: Vec<Value>) {
        self.host.replace_mutable_collection(&it.source, elements);
        it.expected = self.own_structural_change(&it.source);
    }

    fn list_iterator_member(
        &mut self,
        it: &mut ListIter,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let arg = || args.first().cloned().unwrap_or(Value::Ref(None));
        let mut elements = self.collection_elements(&it.source)?;
        let size = elements.len();
        Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("hasNext", "()Z") => Value::Int(i32::from(if it.linked {
                it.cursor < size
            } else {
                it.cursor != size
            })),
            ("hasPrevious", "()Z") => Value::Int(i32::from(it.cursor != 0)),
            ("nextIndex", "()I") => Value::Int(it.cursor as i32),
            ("previousIndex", "()I") => Value::Int(it.cursor as i32 - 1),
            ("next", "()Ljava/lang/Object;") => {
                self.list_iterator_check(it, r)?;
                let Some(value) = elements.get(it.cursor).cloned() else {
                    return Err(no_such_element());
                };
                it.last = Some(it.cursor);
                it.cursor += 1;
                value
            }
            ("previous", "()Ljava/lang/Object;") => {
                self.list_iterator_check(it, r)?;
                if it.cursor == 0 {
                    return Err(no_such_element());
                }
                let Some(value) = elements.get(it.cursor - 1).cloned() else {
                    return Err(no_such_element());
                };
                it.cursor -= 1;
                it.last = Some(it.cursor);
                value
            }
            ("remove", "()V") => {
                if it.linked {
                    self.list_iterator_check(it, r)?;
                }
                let Some(last) = it.last else {
                    return Err(illegal_state());
                };
                if !it.linked {
                    self.list_iterator_check(it, r)?;
                }
                if last >= elements.len() {
                    return Err(iteration_guard_cme());
                }
                elements.remove(last);
                self.list_iterator_write(it, elements);
                it.cursor = last;
                it.last = None;
                Value::Void
            }
            ("set", "(Ljava/lang/Object;)V") => {
                let Some(last) = it.last else {
                    return Err(illegal_state());
                };
                self.list_iterator_check(it, r)?;
                let Some(slot) = elements.get_mut(last) else {
                    return Err(iteration_guard_cme());
                };
                *slot = arg();
                // `set` is not structural: no `modCount++`.
                self.host.replace_mutable_collection(&it.source, elements);
                Value::Void
            }
            ("add", "(Ljava/lang/Object;)V") => {
                self.list_iterator_check(it, r)?;
                if it.cursor > elements.len() {
                    return Err(iteration_guard_cme());
                }
                elements.insert(it.cursor, arg());
                self.list_iterator_write(it, elements);
                it.cursor += 1;
                it.last = None;
                Value::Void
            }
            ("forEachRemaining", "(Ljava/util/function/Consumer;)V") => {
                let action = arg();
                if action == Value::Ref(None) {
                    return Err(throw("java/lang/NullPointerException", None));
                }
                if it.linked {
                    self.linked_for_each_remaining(it, r, &action)?;
                } else {
                    self.array_for_each_remaining(it, r, &action, size)?;
                }
                Value::Void
            }
            _ => return Ok(None),
        }))
    }

    /// `ArrayList.Itr`/`Vector.Itr.forEachRemaining`: `size` read once,
    /// `for (; i < size && modCount == expectedModCount; i++)
    /// accept(es[i])`, then `cursor = i; lastRet = i - 1` and the check. A
    /// throwing action leaves `cursor`/`lastRet` untouched.
    fn array_for_each_remaining(
        &mut self,
        it: &mut ListIter,
        r: &MethodRef,
        action: &Value,
        size: usize,
    ) -> Result<(), NativeError> {
        let mut i = it.cursor;
        if i >= size {
            return Ok(());
        }
        while i < size && self.capture_mod_count(&it.source) == it.expected {
            let element = self
                .collection_elements(&it.source)?
                .get(i)
                .cloned()
                .unwrap_or(Value::Ref(None));
            if !self.run_functional(ACCEPT, action, &[element])? {
                // An action the runtime cannot run (a host marker).
                self.host.unknown_operand_call(r)?;
            }
            i += 1;
        }
        it.cursor = i;
        it.last = i.checked_sub(1);
        self.list_iterator_check(it, r)
    }

    /// `LinkedList.ListItr.forEachRemaining`: `while (modCount ==
    /// expectedModCount && nextIndex < size)`, advancing after each `accept`
    /// returns, then the check.
    fn linked_for_each_remaining(
        &mut self,
        it: &mut ListIter,
        r: &MethodRef,
        action: &Value,
    ) -> Result<(), NativeError> {
        loop {
            let elements = self.collection_elements(&it.source)?;
            if self.capture_mod_count(&it.source) != it.expected || it.cursor >= elements.len() {
                break;
            }
            let element = elements[it.cursor].clone();
            if !self.run_functional(ACCEPT, action, &[element])? {
                // An action the runtime cannot run (a host marker).
                self.host.unknown_operand_call(r)?;
            }
            it.last = Some(it.cursor);
            it.cursor += 1;
        }
        self.list_iterator_check(it, r)
    }
}
