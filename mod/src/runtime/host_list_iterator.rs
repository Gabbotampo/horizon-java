//! `listIterator()` on the host's `ArrayList`s, `Vector`s and `LinkedList`s,
//! as JDK 25's `ArrayList.ListItr` and `LinkedList.ListItr` run it.
//!
//! The host's lists had `iterator()` but no `listIterator`: the call was
//! unmodelled, so a jar walking a list backwards or editing it through the
//! iterator got a neutral answer. A list iterator here works on the host's
//! list itself, so `set`, `add` and `remove` write through and `hasNext` sees
//! the list's current size:
//!
//! * `listIterator(index)` checks `0 <= index <= size`
//!   (`IndexOutOfBoundsException: "Index: i, Size: s"`).
//! * `next`/`previous` past either end throw `NoSuchElementException`;
//!   `remove`/`set` without a current element (none returned yet, or after
//!   `add`/`remove`) throw `IllegalStateException`, `LinkedList.ListItr`
//!   checking for modification before that and `ArrayList.ListItr` after;
//!   `remove` after `previous` keeps the cursor, after `next` steps it back.
//! * `forEachRemaining` hands each remaining element to the action, then
//!   checks for modification once more.
//!
//! Concurrent modification is the fail-fast iterators' (`iteration_guard.rs`):
//! the iterator remembers the list's element sequence, and a later
//! `next`/`previous`/`remove`/`set`/`add` throws
//! `ConcurrentModificationException` when the size changed behind it. The
//! same size with different elements (a `set`, or a remove and an add) is
//! counted, not answered. The iterator's own `set`/`add`/`remove` move the
//! remembered sequence, as they move `expectedModCount`.

use super::*;
use iteration_guard::FailFast;

/// Prefix of a host list iterator marker.
pub(crate) const PREFIX: &str = "list-iterator/";

/// One `ListItr` over a host list.
#[derive(Clone, Debug, PartialEq)]
pub struct ListIter {
    source: Value,
    /// `LinkedList.ListItr` (its checks run in a different order).
    linked: bool,
    how: FailFast,
    cursor: usize,
    /// The index `next`/`previous` last returned (`lastRet`), if current.
    last: Option<usize>,
    /// The list's elements when this iterator last checked or changed it.
    structure: Vec<String>,
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
        let Some((source, how)) = self.fail_fast_source(receiver) else {
            return Ok(None);
        };
        let class = self.host_collection_class(&source).unwrap_or_default();
        let linked = match class.as_str() {
            "java/util/ArrayList" | "java/util/Vector" | "java/util/Stack" => false,
            "java/util/LinkedList" => true,
            _ => return Ok(None),
        };
        let size = self.collection_elements(&source)?.len();
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
        let structure = self.structure_of(&source, how)?;
        let state = ListIter {
            source,
            linked,
            how,
            cursor: index as usize,
            last: None,
            structure,
        };
        Ok(self
            .host
            .new_list_iterator(state)
            .map(|marker| Value::Ref(Some(marker))))
    }

    /// `checkForComodification()`: the size changed behind the iterator
    /// throws; the same size with different elements is counted.
    fn list_iterator_check(&mut self, it: &ListIter, r: &MethodRef) -> Result<(), NativeError> {
        match self.structure_changed(&it.source, it.how, &it.structure)? {
            Some(false) => Ok(()),
            Some(true) => Err(iteration_guard::concurrent_modification()),
            None => self.host.unknown_operand_call(r).map(drop),
        }
    }

    /// Write `elements` back and remember them as this iterator's own change.
    fn list_iterator_write(
        &mut self,
        it: &mut ListIter,
        elements: Vec<Value>,
    ) -> Result<(), NativeError> {
        self.host.replace_mutable_collection(&it.source, elements);
        it.structure = self.structure_of(&it.source, it.how)?;
        Ok(())
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
            ("hasNext", "()Z") => Value::Int(i32::from(it.cursor < size)),
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
                it.cursor -= 1;
                it.last = Some(it.cursor);
                elements
                    .get(it.cursor)
                    .cloned()
                    .ok_or_else(no_such_element)?
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
                if last < elements.len() {
                    elements.remove(last);
                }
                self.list_iterator_write(it, elements)?;
                it.cursor = last;
                it.last = None;
                Value::Void
            }
            ("set", "(Ljava/lang/Object;)V") => {
                let Some(last) = it.last else {
                    return Err(illegal_state());
                };
                self.list_iterator_check(it, r)?;
                if let Some(slot) = elements.get_mut(last) {
                    *slot = arg();
                }
                self.list_iterator_write(it, elements)?;
                Value::Void
            }
            ("add", "(Ljava/lang/Object;)V") => {
                self.list_iterator_check(it, r)?;
                elements.insert(it.cursor.min(elements.len()), arg());
                self.list_iterator_write(it, elements)?;
                it.cursor += 1;
                it.last = None;
                Value::Void
            }
            ("forEachRemaining", "(Ljava/util/function/Consumer;)V") => {
                let action = arg();
                if action == Value::Ref(None) {
                    return Err(throw("java/lang/NullPointerException", None));
                }
                // Both loops stop at a modification and check once more.
                while it.cursor < size {
                    if self.structure_changed(&it.source, it.how, &it.structure)? != Some(false) {
                        break;
                    }
                    let value = elements[it.cursor].clone();
                    it.last = Some(it.cursor);
                    it.cursor += 1;
                    self.run_functional(
                        (
                            "java/util/function/Consumer",
                            "accept",
                            "(Ljava/lang/Object;)V",
                        ),
                        &action,
                        &[value],
                    )?;
                }
                self.list_iterator_check(it, r)?;
                Value::Void
            }
            _ => return Ok(None),
        }))
    }
}
