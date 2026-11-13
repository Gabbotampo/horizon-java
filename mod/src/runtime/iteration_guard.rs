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
//! `modCount` (`mod_count.rs`) it started from, of the collection it
//! checks: the collection itself, or the root of a view (a `keySet()` copy's
//! map, a `subList`'s list, an unmodifiable view's list). Then:
//!
//! - **`next()`** throws `ConcurrentModificationException` when that
//!   `modCount` moved, even if the contents came back the same. A member
//!   that may or may not have moved it (`ensureCapacity`) makes the step
//!   counted. As a backstop, a size or key-set change no tracked member
//!   made also throws.
//! - **`hasNext()`** of a cursor iterator reads the live size, as
//!   `ArrayList.Itr` (`cursor != size`), `LinkedList.ListItr` and
//!   `PriorityQueue.Itr` (`cursor < size`) do: removing the second-to-last
//!   element ends an `ArrayList` for-each without an exception, on a JVM too.
//!   A node iterator (hash, tree) keeps the `next` node it found, which the
//!   host's snapshot already answers.
//! - **`ArrayDeque`** has no `modCount`: its iterator reads the live array
//!   and throws only when it meets a slot a removal emptied, which depends
//!   on the array's layout. A step after a change is counted.
//! - **`ArrayList`/`Vector.forEach`** stops after the call that changed the
//!   list and throws; `LinkedList.forEach` (`Iterable.forEach`) stops at the
//!   next `next()`.
//! - **`ArrayList`/`Vector.removeIf`** is JDK 25's: it tests every element
//!   first (survivors up to the first match, then the rest into a bit set),
//!   throws if `modCount` moved, and only then removes.
//!   **`LinkedList.removeIf`** is `Collection.removeIf`: it removes through
//!   the iterator as it goes, so a predicate that throws leaves the earlier
//!   removals done.
//!
//! An iterator's own `remove()` is not modelled by the host (counted).

use super::*;
use mod_count::{ModStamp, ModVerdict};

/// A guarded iterator: what it walks, whose `modCount` it checks, and that
/// count when the iterator was made.
#[derive(Clone, Debug, PartialEq)]
pub struct IteratorGuard {
    source: String,
    /// The collection whose `modCount` the iterator checks.
    root: String,
    /// `ArrayList.Itr`'s `cursor != size` (`Some(true)`), the other cursor
    /// iterators' `cursor < size` (`Some(false)`), or a node iterator.
    cursor: Option<bool>,
    expected: ModStamp,
    structure: Vec<String>,
    /// `ArrayDeque`: no `modCount`.
    deque: bool,
    /// An `ArrayDeque` iterator whose layout is known: its `cursor` slot and
    /// `remaining` count (`array_deque.rs`).
    deque_cursor: Option<(usize, usize)>,
}

/// How a fail-fast collection iterates.
#[derive(Clone, Copy, Debug, PartialEq)]
enum FailFast {
    /// `ArrayList`/`Vector`/`Stack`: `cursor != size`, element sequence.
    ArrayList,
    /// `LinkedList`/`PriorityQueue`: `cursor < size`, element sequence.
    Cursor,
    /// A map: its keys.
    MapKeys,
    /// A hash or tree set: its elements, as a set.
    SetElements,
    /// `ArrayDeque`: no `modCount`.
    Deque,
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
        "java/util/ArrayDeque" => FailFast::Deque,
        _ => return None,
    })
}

fn concurrent_modification() -> NativeError {
    NativeError::UncaughtException {
        class: "java/util/ConcurrentModificationException".into(),
        message: None,
        frame: None,
    }
}

impl<H: Host> Runtime<'_, H> {
    /// The fail-fast collection `value` iterates (a view copy's map, an
    /// unmodifiable view's collection), and how.
    fn fail_fast_source(&self, value: &Value) -> Option<(Value, FailFast)> {
        let marker = expect_ref(value)?;
        let source = self
            .host
            .view_copy_source(marker)
            .or_else(|| self.host.unmodifiable_view_source(marker))
            .map_or_else(|| value.clone(), |source| Value::Ref(Some(source)));
        let class = self.host_collection_class(&source)?;
        Some((source, fail_fast(&class)?))
    }

    /// A collection's structure: its elements in order, or a map's keys.
    fn structure_of(&self, source: &Value, how: FailFast) -> Result<Vec<String>, NativeError> {
        let items = runtime_collection_values(self.host, source)?;
        Ok(match how {
            FailFast::MapKeys => items.into_iter().step_by(2).collect(),
            FailFast::SetElements => {
                let mut items = items;
                items.sort();
                items
            }
            FailFast::ArrayList | FailFast::Cursor | FailFast::Deque => items,
        })
    }

    /// Whether the size or the key set moved since `before`: a change
    /// only a structural modification makes.
    fn resized(
        &self,
        source: &Value,
        how: FailFast,
        before: &[String],
    ) -> Result<bool, NativeError> {
        let now = self.structure_of(source, how)?;
        Ok(match how {
            FailFast::MapKeys => {
                let mut now = now;
                let mut before = before.to_vec();
                now.sort();
                before.sort();
                now != before
            }
            FailFast::SetElements => now != before,
            _ => now.len() != before.len(),
        })
    }

    /// `modCount != expectedModCount` for a guarded loop over `source`.
    fn mod_verdict(
        &self,
        root: &str,
        expected: ModStamp,
        source: &Value,
        how: FailFast,
        structure: &[String],
    ) -> Result<ModVerdict, NativeError> {
        let verdict = self.mod_stamp(root).verdict(expected);
        if verdict == ModVerdict::Same && self.resized(source, how, structure)? {
            return Ok(ModVerdict::Changed);
        }
        Ok(verdict)
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
                let source_marker = expect_ref(&source).cloned().unwrap_or_default();
                let root = self.mod_count_root(&source_marker);
                let structure = self.structure_of(&source, how)?;
                let expected = self.capture_mod_count(&source);
                let deque = !self.has_mod_count(&source);
                // `DeqIterator`: `cursor = head`, `remaining = size()`.
                let deque_cursor = (how == FailFast::Deque)
                    .then(|| self.host.deque_layout(&source_marker))
                    .flatten()
                    .map(|layout| (layout.head(), structure.len()));
                let iterator = self.host.invoke_instance(r, receiver, args)?;
                if let Some(iterator) = expect_ref(&iterator) {
                    self.host.store_iterator_guard(
                        iterator,
                        IteratorGuard {
                            source: source_marker,
                            root,
                            cursor: match how {
                                FailFast::ArrayList => Some(true),
                                FailFast::Cursor => Some(false),
                                _ => None,
                            },
                            expected,
                            structure,
                            deque,
                            deque_cursor,
                        },
                    );
                }
                Ok(Some(iterator))
            }
            ("hasNext", "()Z") => {
                let Some(guard) = self.host.iterator_guard(&marker) else {
                    return Ok(None);
                };
                if let Some((_, remaining)) = guard.deque_cursor {
                    return Ok(Some(Value::Int(i32::from(remaining > 0))));
                }
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
                if guard.deque_cursor.is_some() {
                    return self.deque_next(r, &marker, guard).map(Some);
                }
                let source = Value::Ref(Some(guard.source.clone()));
                let how = match guard.cursor {
                    Some(true) => FailFast::ArrayList,
                    Some(false) => FailFast::Cursor,
                    None => self
                        .fail_fast_source(&source)
                        .map_or(FailFast::MapKeys, |(_, how)| how),
                };
                match self.mod_verdict(
                    &guard.root,
                    guard.expected,
                    &source,
                    how,
                    &guard.structure,
                )? {
                    // A cursor iterator reads the live array (`elementData[
                    // cursor]`): a `set` since the snapshot shows.
                    ModVerdict::Same if guard.cursor.is_some() => {
                        let cursor = self.host.iterator_cursor(&marker);
                        let stepped = self.host.invoke_instance(r, receiver, args)?;
                        let live = self.collection_elements(&source)?;
                        Ok(Some(
                            cursor
                                .and_then(|cursor| live.get(cursor).cloned())
                                .unwrap_or(stepped),
                        ))
                    }
                    ModVerdict::Same => Ok(None),
                    ModVerdict::Changed if !guard.deque => Err(concurrent_modification()),
                    _ => self.host.unknown_operand_call(r).map(|_| None),
                }
            }
            ("forEach", "(Ljava/util/function/Consumer;)V") if self.is_array_deque(receiver) => {
                let action = args.first().cloned().unwrap_or(Value::Ref(None));
                self.deque_for_each(r, receiver, &action)
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
                let callback = args.first().cloned().unwrap_or(Value::Ref(None));
                match how {
                    FailFast::ArrayList => self.array_list_remove_if(&source, &callback).map(Some),
                    FailFast::Cursor
                        if self.host_collection_class(&source).as_deref()
                            == Some("java/util/LinkedList") =>
                    {
                        self.iterator_remove_if(&source, &callback).map(Some)
                    }
                    _ => Ok(None),
                }
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
        let marker = expect_ref(source).cloned().unwrap_or_default();
        let root = self.mod_count_root(&marker);
        let expected = self.mod_stamp(&root);
        let structure = self.structure_of(source, how)?;
        // `ArrayList` reads its size once.
        let size = structure.len();
        let mut index = 0;
        loop {
            let verdict = self.mod_verdict(&root, expected, source, how, &structure)?;
            let live = self.collection_elements(source)?;
            let done = match how {
                FailFast::ArrayList => verdict != ModVerdict::Same || index >= size,
                _ => index >= live.len() || verdict != ModVerdict::Same,
            };
            if done {
                return match verdict {
                    ModVerdict::Same => Ok(Value::Void),
                    ModVerdict::Changed => Err(concurrent_modification()),
                    ModVerdict::Unknown => {
                        let counted = MethodRef {
                            owner: self.host_collection_class(source).unwrap_or_default(),
                            name: "forEach".into(),
                            descriptor: "(Ljava/util/function/Consumer;)V".into(),
                        };
                        self.host
                            .unknown_operand_call(&counted)
                            .map(|_| Value::Void)
                    }
                };
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

    /// Count a list member whose `modCount` check could not be answered.
    fn count_list_member(
        &mut self,
        source: &Value,
        name: &str,
        descriptor: &str,
    ) -> Result<(), NativeError> {
        let counted = MethodRef {
            owner: self.host_collection_class(source).unwrap_or_default(),
            name: name.into(),
            descriptor: descriptor.into(),
        };
        self.host.unknown_operand_call(&counted).map(|_| ())
    }

    /// `ArrayList.removeIf` (and `Vector`'s `bulkRemove`), as JDK 25 runs
    /// them.
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
        let marker = expect_ref(source).cloned().unwrap_or_default();
        let root = self.mod_count_root(&marker);
        let expected = self.mod_stamp(&root);
        let structure = self.structure_of(source, FailFast::ArrayList)?;
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
        // A resize no tracked member made is a change too.
        if self.resized(source, FailFast::ArrayList, &structure)? {
            return Err(concurrent_modification());
        }
        let counted = MethodRef {
            owner: self.host_collection_class(source).unwrap_or_default(),
            name: "removeIf".into(),
            descriptor: "(Ljava/util/function/Predicate;)Z".into(),
        };
        if !self.check_mod_count(&counted, source, expected)? {
            return Ok(Value::Int(0));
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

    /// `Collection.removeIf` on a `LinkedList`: `while (it.hasNext()) if
    /// (filter.test(it.next())) it.remove()`. Each removal happens (and
    /// bumps `modCount`) before the next test, so a predicate that throws
    /// leaves the earlier ones done; `next()` and `remove()` throw after a
    /// change the predicate made.
    fn iterator_remove_if(
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
        let marker = expect_ref(source).cloned().unwrap_or_default();
        let root = self.mod_count_root(&marker);
        let mut expected = self.mod_stamp(&root);
        let mut structure = self.structure_of(source, FailFast::Cursor)?;
        let mut next_index = 0;
        let mut removed = false;
        let check = |this: &mut Self, expected: ModStamp, structure: &[String]| match this
            .mod_verdict(&root, expected, source, FailFast::Cursor, structure)?
        {
            ModVerdict::Same => Ok(true),
            ModVerdict::Changed => Err(concurrent_modification()),
            ModVerdict::Unknown => this
                .count_list_member(source, "removeIf", "(Ljava/util/function/Predicate;)Z")
                .map(|()| false),
        };
        loop {
            let mut live = self.collection_elements(source)?;
            // `hasNext`: `nextIndex < size`.
            if next_index >= live.len() {
                return Ok(Value::Int(i32::from(removed)));
            }
            if !check(self, expected, &structure)? {
                return Ok(Value::Int(i32::from(removed)));
            }
            let element = live[next_index].clone();
            next_index += 1;
            if !self.predicate_holds(predicate, &element)? {
                continue;
            }
            // `it.remove()`: the check, `unlink`, `expectedModCount++`.
            if !check(self, expected, &structure)? {
                return Ok(Value::Int(i32::from(removed)));
            }
            live = self.collection_elements(source)?;
            next_index -= 1;
            live.remove(next_index);
            self.host.replace_mutable_collection(source, live);
            expected = self.own_structural_change(source);
            structure = self.structure_of(source, FailFast::Cursor)?;
            removed = true;
        }
    }

    /// `DeqIterator.next()`: the live array's slot at `cursor`, a
    /// `ConcurrentModificationException` if a removal emptied it, then the
    /// next slot (of the array as it is now).
    fn deque_next(
        &mut self,
        r: &MethodRef,
        marker: &str,
        mut guard: IteratorGuard,
    ) -> Result<Value, NativeError> {
        let Some((cursor, remaining)) = guard.deque_cursor else {
            return Ok(Value::Ref(None));
        };
        if remaining == 0 {
            return Err(NativeError::UncaughtException {
                class: "java/util/NoSuchElementException".into(),
                message: None,
                frame: None,
            });
        }
        let source = Value::Ref(Some(guard.source.clone()));
        let Some(layout) = self.host.deque_layout(&guard.source) else {
            // The layout is no longer known: the host's snapshot, counted.
            self.host.unknown_operand_call(r)?;
            return self
                .host
                .invoke_instance(r, &Value::Ref(Some(marker.to_owned())), &[]);
        };
        let live = self.collection_elements(&source)?;
        let element = layout.slot(cursor, &live);
        if element == Value::Ref(None) {
            return Err(concurrent_modification());
        }
        guard.deque_cursor = Some((layout.next_slot(cursor), remaining - 1));
        self.host.store_iterator_guard(marker, guard);
        Ok(element)
    }

    /// `forEachRemaining(action)` on a guarded iterator, with JDK 25's
    /// `modCount` checks: `ArrayList.Itr`/`Vector.Itr` and
    /// `LinkedList.ListItr` test it in the loop condition and once more
    /// after the loop (so a change made at the last element still throws);
    /// the `Iterator.forEachRemaining` default (hash, tree, `PriorityQueue`)
    /// is `while (hasNext()) action.accept(next())`; `ArrayDeque`'s checks
    /// the first slot and `tail`. `Ok(None)`: not a guarded iterator.
    pub(crate) fn guarded_for_each_remaining(
        &mut self,
        r: &MethodRef,
        iterator: &Value,
        action: &Value,
    ) -> Result<Option<Value>, NativeError> {
        let Some(marker) = expect_ref(iterator).cloned() else {
            return Ok(None);
        };
        let Some(mut guard) = self.host.iterator_guard(&marker) else {
            return Ok(None);
        };
        let source = Value::Ref(Some(guard.source.clone()));
        let class = self.host_collection_class(&source).unwrap_or_default();
        const ACCEPT: (&str, &str, &str) = (
            "java/util/function/Consumer",
            "accept",
            "(Ljava/lang/Object;)V",
        );
        let check = |this: &mut Self, guard: &IteratorGuard| -> Result<(), NativeError> {
            match this.mod_stamp(&guard.root).verdict(guard.expected) {
                ModVerdict::Same => Ok(()),
                ModVerdict::Changed => Err(concurrent_modification()),
                ModVerdict::Unknown => this.host.unknown_operand_call(r).map(|_| ()),
            }
        };
        if let Some((cursor, remaining)) = guard.deque_cursor {
            // `DeqIterator.forEachRemaining`: `remaining = 0` first.
            if remaining == 0 {
                return Ok(Some(Value::Void));
            }
            guard.deque_cursor = Some((cursor, 0));
            self.host.store_iterator_guard(&marker, guard.clone());
            let Some(layout) = self.host.deque_layout(&guard.source) else {
                return self.host.unknown_operand_call(r).map(|_| None);
            };
            let live = self.collection_elements(&source)?;
            let end = layout.tail(live.len());
            let span = (end + layout.capacity_of() - cursor) % layout.capacity_of();
            if layout.slot(cursor, &live) == Value::Ref(None) || span != remaining {
                return Err(concurrent_modification());
            }
            let mut slot = cursor;
            for _ in 0..remaining {
                let live = self.collection_elements(&source)?;
                let Some(now) = self
                    .host
                    .deque_layout(&guard.source)
                    .filter(|now| now.capacity_of() == layout.capacity_of())
                else {
                    return self.host.unknown_operand_call(r).map(|_| None);
                };
                self.run_functional(ACCEPT, action, &[now.slot(slot, &live)])?;
                slot = layout.next_slot(slot);
            }
            let live = self.collection_elements(&source)?.len();
            return match self.host.deque_layout(&guard.source) {
                Some(now) if now.tail(live) != end => Err(concurrent_modification()),
                _ => Ok(Some(Value::Void)),
            };
        }
        let Some(start) = self.host.iterator_cursor(&marker) else {
            return Ok(None);
        };
        match class.as_str() {
            "java/util/ArrayList" | "java/util/Vector" | "java/util/Stack" => {
                // `size` read once; the cursor is written after the loop.
                let size = self.collection_elements(&source)?.len();
                let mut index = start;
                while index < size
                    && self.mod_stamp(&guard.root).verdict(guard.expected) == ModVerdict::Same
                {
                    let element = self
                        .collection_elements(&source)?
                        .get(index)
                        .cloned()
                        .unwrap_or(Value::Ref(None));
                    if let Err(error) = self.run_functional(ACCEPT, action, &[element]) {
                        self.host.set_host_iterator_cursor(&marker, start);
                        return Err(error);
                    }
                    index += 1;
                }
                self.host.set_host_iterator_cursor(&marker, index);
                check(self, &guard)?;
            }
            "java/util/LinkedList" => {
                // `nextIndex < size` reads the live size; it advances after
                // each `accept`.
                let mut index = start;
                while self.mod_stamp(&guard.root).verdict(guard.expected) == ModVerdict::Same {
                    let live = self.collection_elements(&source)?;
                    let Some(element) = live.get(index).cloned() else {
                        break;
                    };
                    self.run_functional(ACCEPT, action, &[element])?;
                    index += 1;
                    self.host.set_host_iterator_cursor(&marker, index);
                }
                check(self, &guard)?;
            }
            _ => {
                // `while (hasNext()) action.accept(next())`.
                let has_next = MethodRef {
                    owner: "java/util/Iterator".into(),
                    name: "hasNext".into(),
                    descriptor: "()Z".into(),
                };
                let next = MethodRef {
                    owner: "java/util/Iterator".into(),
                    name: "next".into(),
                    descriptor: "()Ljava/lang/Object;".into(),
                };
                loop {
                    let more =
                        match self.invoke_iteration_guard(&has_next, 0xb9, Some(iterator), &[])? {
                            Some(answer) => answer,
                            None => self.host.invoke_instance(&has_next, iterator, &[])?,
                        };
                    if more != Value::Int(1) {
                        break;
                    }
                    let element =
                        match self.invoke_iteration_guard(&next, 0xb9, Some(iterator), &[])? {
                            Some(element) => element,
                            None => self.host.host_iterator_next(&marker)?,
                        };
                    self.run_functional(ACCEPT, action, &[element])?;
                }
            }
        }
        Ok(Some(Value::Void))
    }
}
