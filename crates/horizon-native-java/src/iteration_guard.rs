//! Fail-fast iteration: `ConcurrentModificationException` where JDK 25's
//! `modCount` checks throw it.
//!
//! The host hands out iterators over a snapshot of a collection, so a loop
//! that changed the collection kept going over the old contents: an
//! `ArrayList` for-each that added inside the loop printed every element
//! instead of throwing, a `HashMap` key loop that removed a later key still
//! visited it. Java's fail-fast iterators (`ArrayList`, `LinkedList`,
//! `Vector`, `HashMap`/`HashSet`/`LinkedHash*`, `TreeMap`/`TreeSet`,
//! `PriorityQueue`, `IdentityHashMap`) throw on the next `next()` after any
//! structural change they did not make themselves.
//!
//! This module remembers, for each iterator a host collection hands out, the
//! collection's structure when it was made: the element sequence of a list
//! or queue, the keys of a map or set. Then:
//!
//! - **`next()`** throws `ConcurrentModificationException` when the structure
//!   has changed: a different size or a different key set can only come from
//!   a structural change, so `modCount` moved. A list of the same size with
//!   different elements could be a `set` (no `modCount`) or a remove and an
//!   add: that one is counted.
//! - **`hasNext()`** of a cursor iterator reads the live size, as
//!   `ArrayList.Itr` (`cursor != size`), `LinkedList.ListItr` and
//!   `PriorityQueue.Itr` (`cursor < size`) do: removing the second-to-last
//!   element ends an `ArrayList` for-each without an exception, on a JVM too.
//!   A node iterator (hash, tree) keeps the `next` node it found, which the
//!   host's snapshot already answers.
//! - **`ArrayList`/`Vector.forEach`** stops after the call that changed the
//!   list and throws; `LinkedList.forEach` (`Iterable.forEach`) stops at the
//!   next `next()`.
//! - **`ArrayList.removeIf`** is JDK 25's: it tests every element first
//!   (survivors up to the first match, then the rest into a bit set), throws
//!   if the predicate changed the list (which then keeps what the predicate
//!   added), and only then removes.
//!
//! An iterator's own `remove()` is not modelled by the host (counted), so it
//! never needs to move the remembered structure.

use super::*;

/// A guarded iterator: its collection and that collection's structure when
/// the iterator was made.
#[derive(Clone, Debug, PartialEq)]
pub struct IteratorGuard {
    source: String,
    /// `ArrayList.Itr`'s `cursor != size` (`Some(true)`), the other cursor
    /// iterators' `cursor < size` (`Some(false)`), or a node iterator.
    cursor: Option<bool>,
    structure: Vec<String>,
}

/// How a fail-fast collection iterates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FailFast {
    /// `ArrayList`/`Vector`/`Stack`: `cursor != size`, element sequence.
    ArrayList,
    /// `LinkedList`/`PriorityQueue`: `cursor < size`, element sequence.
    Cursor,
    /// A map: its keys.
    MapKeys,
    /// A hash or tree set: its elements, as a set.
    SetElements,
}

fn fail_fast(class: &str) -> Option<FailFast> {
    Some(match class {
        "java/util/ArrayList" | "java/util/Vector" | "java/util/Stack" => FailFast::ArrayList,
        "java/util/LinkedList" | "java/util/PriorityQueue" => FailFast::Cursor,
        "java/util/HashMap"
        | "java/util/LinkedHashMap"
        | "java/util/TreeMap"
        | "java/util/IdentityHashMap"
        | "java/util/WeakHashMap" => FailFast::MapKeys,
        "java/util/HashSet" | "java/util/LinkedHashSet" | "java/util/TreeSet" => {
            FailFast::SetElements
        }
        _ => return None,
    })
}

pub(crate) fn concurrent_modification() -> NativeError {
    NativeError::UncaughtException {
        class: "java/util/ConcurrentModificationException".into(),
        message: None,
        frame: None,
    }
}

impl<H: Host> Runtime<'_, H> {
    /// The fail-fast collection `value` iterates (a view copy's map), and
    /// how.
    pub(crate) fn fail_fast_source(&self, value: &Value) -> Option<(Value, FailFast)> {
        let marker = expect_ref(value)?;
        let source = self
            .host
            .view_copy_source(marker)
            .map_or_else(|| value.clone(), |source| Value::Ref(Some(source)));
        let class = self.host_collection_class(&source)?;
        Some((source, fail_fast(&class)?))
    }

    /// A collection's structure: its elements in order, or a map's keys.
    pub(crate) fn structure_of(
        &self,
        source: &Value,
        how: FailFast,
    ) -> Result<Vec<String>, NativeError> {
        let items = runtime_collection_values(self.host, source)?;
        Ok(match how {
            FailFast::MapKeys => items.into_iter().step_by(2).collect(),
            FailFast::SetElements => {
                let mut items = items;
                items.sort();
                items
            }
            FailFast::ArrayList | FailFast::Cursor => items,
        })
    }

    /// Whether the structure moved since `before`: `Some(true)` for a
    /// change only a structural modification makes, `Some(false)` for none,
    /// `None` when a list's elements changed at the same size (a `set`, or a
    /// remove and an add).
    pub(crate) fn structure_changed(
        &self,
        source: &Value,
        how: FailFast,
        before: &[String],
    ) -> Result<Option<bool>, NativeError> {
        let now = self.structure_of(source, how)?;
        if now == before {
            return Ok(Some(false));
        }
        if now.len() != before.len() {
            return Ok(Some(true));
        }
        Ok(match how {
            FailFast::ArrayList | FailFast::Cursor => None,
            FailFast::MapKeys => {
                let mut now = now;
                let mut before = before.to_vec();
                now.sort();
                before.sort();
                Some(now != before)
            }
            FailFast::SetElements => Some(true),
        })
    }

    /// `iterator()`, `hasNext()`/`next()` of a guarded iterator, and the
    /// fail-fast `forEach`/`removeIf` of the lists. `Ok(None)` leaves the
    /// call to the rest of the dispatch.
    pub(crate) fn invoke_iteration_guard(
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
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("iterator", "()Ljava/util/Iterator;") => {
                let Some((source, how)) = self.fail_fast_source(receiver) else {
                    return Ok(None);
                };
                let structure = self.structure_of(&source, how)?;
                let iterator = self.host.invoke_instance(r, receiver, args)?;
                if let Some(iterator) = expect_ref(&iterator) {
                    self.host.store_iterator_guard(
                        iterator,
                        IteratorGuard {
                            source: expect_ref(&source).cloned().unwrap_or_default(),
                            cursor: match how {
                                FailFast::ArrayList => Some(true),
                                FailFast::Cursor => Some(false),
                                _ => None,
                            },
                            structure,
                        },
                    );
                }
                Ok(Some(iterator))
            }
            ("hasNext", "()Z") => {
                let Some(guard) = self.host.iterator_guard(&marker) else {
                    return Ok(None);
                };
                let Some(not_equal) = guard.cursor else {
                    return Ok(None);
                };
                let Some(cursor) = self.host.iterator_cursor(&marker) else {
                    return Ok(None);
                };
                let source = Value::Ref(Some(guard.source.clone()));
                let size = runtime_collection_values(self.host, &source)?.len();
                let more = if not_equal {
                    cursor != size
                } else {
                    cursor < size
                };
                Ok(Some(Value::Int(i32::from(more))))
            }
            ("next", "()Ljava/lang/Object;") => {
                let Some(guard) = self.host.iterator_guard(&marker) else {
                    return Ok(None);
                };
                let source = Value::Ref(Some(guard.source.clone()));
                let how = match guard.cursor {
                    Some(true) => FailFast::ArrayList,
                    Some(false) => FailFast::Cursor,
                    None => self
                        .fail_fast_source(&source)
                        .map_or(FailFast::MapKeys, |(_, how)| how),
                };
                match self.structure_changed(&source, how, &guard.structure)? {
                    Some(false) => Ok(None),
                    Some(true) => Err(concurrent_modification()),
                    None => self.host.unknown_operand_call(r).map(|_| None),
                }
            }
            ("forEach", "(Ljava/util/function/Consumer;)V") => {
                let Some((source, how)) = self.fail_fast_source(receiver) else {
                    return Ok(None);
                };
                if !matches!(how, FailFast::ArrayList | FailFast::Cursor)
                    || self.host_collection_class(&source).as_deref()
                        == Some("java/util/PriorityQueue")
                {
                    return Ok(None);
                }
                let callback = args.first().cloned().unwrap_or(Value::Ref(None));
                self.list_for_each(&source, how, &callback).map(Some)
            }
            ("removeIf", "(Ljava/util/function/Predicate;)Z") => {
                let Some((source, how)) = self.fail_fast_source(receiver) else {
                    return Ok(None);
                };
                if how != FailFast::ArrayList {
                    return Ok(None);
                }
                let callback = args.first().cloned().unwrap_or(Value::Ref(None));
                self.array_list_remove_if(&source, &callback).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// `ArrayList.forEach`: `for (i = 0; modCount == expectedModCount && i
    /// < size; i++) accept(es[i])`, then the check; `LinkedList` through its
    /// iterator (`nextIndex < size`, then `next()`'s check).
    fn list_for_each(
        &mut self,
        source: &Value,
        how: FailFast,
        callback: &Value,
    ) -> Result<Value, NativeError> {
        let before = self.structure_of(source, how)?;
        let mut index = 0;
        loop {
            let live = self.collection_elements(source)?;
            let changed = self.structure_changed(source, how, &before)?;
            if how == FailFast::ArrayList {
                // The loop condition reads `modCount` first, then the size
                // `forEach` captured.
                if changed != Some(false) || index >= before.len() {
                    return match changed {
                        Some(false) => Ok(Value::Void),
                        Some(true) => Err(concurrent_modification()),
                        None => {
                            let counted = MethodRef {
                                owner: "java/util/ArrayList".into(),
                                name: "forEach".into(),
                                descriptor: "(Ljava/util/function/Consumer;)V".into(),
                            };
                            self.host
                                .unknown_operand_call(&counted)
                                .map(|_| Value::Void)
                        }
                    };
                }
            } else {
                if index >= live.len() {
                    return Ok(Value::Void);
                }
                match changed {
                    Some(false) => {}
                    Some(true) => return Err(concurrent_modification()),
                    None => {
                        let counted = MethodRef {
                            owner: "java/util/LinkedList".into(),
                            name: "forEach".into(),
                            descriptor: "(Ljava/util/function/Consumer;)V".into(),
                        };
                        return self
                            .host
                            .unknown_operand_call(&counted)
                            .map(|_| Value::Void);
                    }
                }
            }
            let Some(element) = live.get(index).cloned() else {
                return Ok(Value::Void);
            };
            self.run_functional(
                (
                    "java/util/function/Consumer",
                    "accept",
                    "(Ljava/lang/Object;)V",
                ),
                callback,
                &[element],
            )?;
            index += 1;
        }
    }

    /// `ArrayList.removeIf`, as JDK 25 runs it.
    fn array_list_remove_if(
        &mut self,
        source: &Value,
        predicate: &Value,
    ) -> Result<Value, NativeError> {
        if *predicate == Value::Ref(None) {
            return Err(NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: None,
                frame: None,
            });
        }
        let before = self.structure_of(source, FailFast::ArrayList)?;
        // `es = elementData` and `end = size` are read once.
        let es = self.collection_elements(source)?;
        let end = es.len();
        let mut doomed = vec![false; end];
        let mut first = end;
        for (index, element) in es.iter().enumerate() {
            if self.predicate_holds(predicate, element)? {
                doomed[index] = true;
                first = index;
                break;
            }
        }
        for index in first.saturating_add(1)..end {
            if self.predicate_holds(predicate, &es[index])? {
                doomed[index] = true;
            }
        }
        match self.structure_changed(source, FailFast::ArrayList, &before)? {
            Some(false) => {}
            Some(true) => return Err(concurrent_modification()),
            // A `set` (no `modCount`) or a remove and an add: unknown.
            None => {
                let counted = MethodRef {
                    owner: "java/util/ArrayList".into(),
                    name: "removeIf".into(),
                    descriptor: "(Ljava/util/function/Predicate;)Z".into(),
                };
                return self
                    .host
                    .unknown_operand_call(&counted)
                    .map(|_| Value::Int(0));
            }
        }
        if first >= end {
            return Ok(Value::Int(0));
        }
        let kept: Vec<Value> = es
            .into_iter()
            .zip(doomed)
            .filter(|(_, doomed)| !doomed)
            .map(|(element, _)| element)
            .collect();
        self.host.replace_mutable_collection(source, kept);
        Ok(Value::Int(1))
    }
}
