//! The rest of the hash collections (`hash_collections.rs`): the members
//! that run a callback or another collection's lookups, as JDK 25 runs them.
//!
//! - **`computeIfAbsent`/`computeIfPresent`/`compute`/`merge`** resize first
//!   when the table is over its threshold (`computeIfPresent` does not),
//!   call `hashCode` once, search the bin, run the function with the table
//!   back in place (one that adds or removes a key throws
//!   `ConcurrentModificationException`, as the JDK's `modCount` check does),
//!   then insert a new key at its bin's **head**, or remove the key by
//!   searching the bin again (`computeIfPresent` hashes the key once more
//!   first). The host's own `merge` could not run a function over a jar
//!   value.
//! - **`containsAll`/`removeAll`/`retainAll`** of a set: `AbstractSet.removeAll`
//!   iterates the smaller side; the iterating paths remove through the
//!   iterator, whose `removeNode` searches the bin again, so `e.equals(k)`
//!   runs for every earlier same-hash node.
//! - **`equals`**: `AbstractMap.equals` (`other.get(key)` for each entry, in
//!   this map's order) and `AbstractSet.equals` (`this.contains(e)` for each
//!   of the other's elements); both answer `false` for a
//!   `ClassCastException` or `NullPointerException`.
//! - **A collection argument's `contains`** for a list's `removeAll`/
//!   `retainAll` (`list_equality.rs`) uses the hash table.
//! - **`contains` on another set the host holds** (a map's `keySet()` or
//!   `entrySet()`, which the host hands out as a copy): a scan while every
//!   value is pure, counted otherwise.
//! - **Streams**: `distinct()` keeps `DistinctOps`' `HashSet`,
//!   `Collectors.toSet()` is a `HashSet` (so it iterates in bucket order),
//!   and `Collectors.toMap` is a `HashMap` filled with `putIfAbsent`
//!   (throwing "Duplicate key k (attempted merging values u and v)") or
//!   `merge`.

use super::*;
use hash_collections::{Lookup, TableKind};
use hash_table::HashTable;
use object_methods::Equality;

/// Whether `AbstractMap.equals`/`AbstractSet.equals` catch `error`
/// (`ClassCastException` and `NullPointerException` answer `false`).
fn caught_by_abstract_equals(error: &NativeError) -> bool {
    matches!(
        error,
        NativeError::UncaughtException { class, .. }
            if class == "java/lang/ClassCastException"
                || class == "java/lang/NullPointerException"
    )
}

impl<H: Host> Runtime<'_, H> {
    /// The set members that take another collection, and `equals`, on a
    /// hash collection (`hash_collections.rs` hands them over). `Ok(None)`
    /// leaves the call to the rest of the dispatch.
    pub(crate) fn hash_bulk_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        kind: TableKind,
        table: &mut HashTable,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let boolean = |flag: bool| Some(Value::Int(i32::from(flag)));
        match (kind.map, r.name.as_str(), args) {
            (false, "containsAll", [source]) => {
                let source = source.clone();
                if self.host_collection_kind(&source).is_none() {
                    return Ok(None);
                }
                for element in self.collection_elements(&source)? {
                    match self.get_node(table, &element)? {
                        Lookup::Found(_) => {}
                        Lookup::Absent => return Ok(boolean(false)),
                        Lookup::Unknown => return self.count_and_continue(r),
                    }
                }
                Ok(boolean(true))
            }
            (false, name @ ("removeAll" | "retainAll"), [source]) => {
                let source = source.clone();
                self.set_remove_all(r, receiver, kind, table, &source, name == "retainAll")
            }
            (_, "equals", [other]) => {
                let other = other.clone();
                self.hash_collection_equals(r, receiver, kind, table, &other)
            }
            _ => Ok(None),
        }
    }

    /// `contains` with a jar key on any other set the host holds (a map's
    /// `keySet()`/`entrySet()`, which the host hands out as a copy, or an
    /// unmodifiable set): its own lookup can only differ from a scan by the
    /// order of the `equals` calls, so a scan answers while every value is
    /// pure, and the call is counted otherwise.
    pub(crate) fn snapshot_set_contains(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let (Some(receiver), [needle]) = (receiver, args) else {
            return Ok(None);
        };
        if !matches!(opcode, 0xb6 | 0xb9)
            || r.name != "contains"
            || r.descriptor != "(Ljava/lang/Object;)Z"
            || self.host_collection_kind(receiver) != Some(object_methods::CollectionKind::Set)
        {
            return Ok(None);
        }
        let jar_involved = |runtime: &Self, value: &Value| match runtime.equality_of(value) {
            Equality::Jar { .. } => true,
            Equality::Host => object_methods::is_map_entry(value),
            _ => false,
        };
        if !jar_involved(self, needle) {
            return Ok(None);
        }
        // Only a set that looks elements up by `equals`: a copy the host
        // handed out, an unmodifiable set, or `Set.of`. A sorted set (or a
        // view of one) finds by `compareTo`, which a scan cannot stand for.
        let Some(marker) = expect_ref(receiver).cloned() else {
            return Ok(None);
        };
        let copy_like = self.host.is_view_copy(&marker)
            || marker.starts_with("set/")
            || marker.starts_with("set#")
            || marker.contains("#immutable");
        let sorted_source = self
            .host
            .unmodifiable_view_source(&marker)
            .map(|source| Value::Ref(Some(source)))
            .and_then(|source| self.host_collection_class(&source))
            .is_some_and(|class| {
                class != "java/util/HashSet" && class != "java/util/LinkedHashSet"
            });
        if !copy_like || sorted_source {
            return Ok(None);
        }
        let elements = self.collection_elements(receiver)?;
        if !self.pure_equality(needle) || !elements.iter().all(|e| self.pure_equality(e)) {
            return self.count_and_continue(r);
        }
        for element in &elements {
            if self.objects_equals(needle, element)? {
                return Ok(Some(Value::Int(1)));
            }
        }
        Ok(Some(Value::Int(0)))
    }

    /// `c.contains(e)` for a collection argument: a hash collection by its
    /// table, a list by its own rules (`list_equality.rs`), any other set
    /// only while every value is pure. `None` when it cannot be answered.
    pub(crate) fn hash_aware_contains(
        &mut self,
        collection: &Value,
        element: &Value,
    ) -> Result<Option<bool>, NativeError> {
        let Some(kind) = self.hash_table_kind(collection) else {
            return self.argument_contains(collection, element);
        };
        let Some(marker) = expect_ref(collection).cloned() else {
            return Ok(None);
        };
        let table = self.take_table(&marker, kind);
        let found = if kind.map {
            Ok(Lookup::Unknown)
        } else {
            self.get_node(&table, element)
        };
        self.host.store_hash_table(&marker, table);
        Ok(match found? {
            Lookup::Found(_) => Some(true),
            Lookup::Absent => Some(false),
            Lookup::Unknown => None,
        })
    }

    /// `m.get(key)` on another map, for `AbstractMap.equals`.
    fn other_map_get(
        &mut self,
        map: &Value,
        key: &Value,
    ) -> Result<Option<(bool, Value)>, NativeError> {
        let entries = self.collection_elements(map)?;
        if let Some(kind) = self.hash_table_kind(map)
            && let Some(marker) = expect_ref(map).cloned()
        {
            let table = self.take_table(&marker, kind);
            let found = self.get_node(&table, key);
            self.host.store_hash_table(&marker, table);
            return Ok(match found? {
                Lookup::Found(stored) => Some((true, Self::stored_value(&entries, &stored))),
                Lookup::Absent => Some((false, Value::Ref(None))),
                Lookup::Unknown => None,
            });
        }
        if !self.pure_equality(key) || !entries.iter().step_by(2).all(|k| self.pure_equality(k)) {
            return Ok(None);
        }
        for pair in entries.chunks(2) {
            if self.objects_equals(key, &pair[0])? {
                return Ok(Some((
                    true,
                    pair.get(1).cloned().unwrap_or(Value::Ref(None)),
                )));
            }
        }
        Ok(Some((false, Value::Ref(None))))
    }

    /// `computeIfAbsent`/`computeIfPresent`/`compute`/`merge`, as `HashMap`
    /// runs them: the resize check first (not `computeIfPresent`), one
    /// `hashCode`, the bin search, the function with the table back in place
    /// (a function that adds or removes a key throws
    /// `ConcurrentModificationException`, as the JDK's `modCount` check
    /// does), then a new key at the bin's head, or a removal that searches
    /// the bin again.
    pub(crate) fn hash_compute(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        marker: &str,
        kind: TableKind,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let null_pointer = || NativeError::UncaughtException {
            class: "java/lang/NullPointerException".into(),
            message: None,
            frame: None,
        };
        let name = r.name.as_str();
        let (key, function, merged) = match (name, args) {
            ("merge", [key, value, function]) => (key, function, Some(value)),
            (_, [key, function]) if name != "merge" => (key, function, None),
            _ => return Ok(None),
        };
        if *function == Value::Ref(None) || merged.is_some_and(|value| *value == Value::Ref(None)) {
            return Err(null_pointer());
        }
        let mut table = self.take_table(marker, kind);
        // The search, with the table held; put back before any return.
        let searched = (|| -> Result<(Option<i32>, Lookup), NativeError> {
            if name == "computeIfPresent" {
                let found = self.get_node(&table, key)?;
                return Ok((None, found));
            }
            table.begin_compute();
            let hash = self.java_hash(key)?;
            let found = self.table_lookup(&table, key, hash)?;
            Ok((hash, found))
        })();
        let (hash, found) = match searched {
            Ok(searched) => searched,
            Err(error) => {
                self.host.store_hash_table(marker, table);
                return Err(error);
            }
        };
        if found == Lookup::Unknown {
            self.host.store_hash_table(marker, table);
            return self.count_and_continue(r);
        }
        let stored = match &found {
            Lookup::Found(stored) => Some(stored.clone()),
            _ => None,
        };
        let entries = self.collection_elements(receiver)?;
        let old = stored.as_ref().map_or(Value::Ref(None), |stored| {
            Self::stored_value(&entries, stored)
        });
        // Which call the function gets, if any.
        let call: Option<Vec<Value>> = match name {
            "computeIfAbsent" if old != Value::Ref(None) => {
                self.host.store_hash_table(marker, table);
                return Ok(Some(old));
            }
            "computeIfAbsent" => Some(vec![key.clone()]),
            "computeIfPresent" if old == Value::Ref(None) => {
                self.host.store_hash_table(marker, table);
                return Ok(Some(Value::Ref(None)));
            }
            "computeIfPresent" | "compute" => Some(vec![key.clone(), old.clone()]),
            // `merge`: the function only for a present, non-null value.
            _ if old != Value::Ref(None) => Some(vec![
                old.clone(),
                merged.cloned().unwrap_or(Value::Ref(None)),
            ]),
            _ => None,
        };
        let value = match call {
            Some(call) => {
                let keys = table.order();
                // `int mc = modCount;`, then `if (mc != modCount) throw`.
                let mc = self.mod_stamp(marker);
                self.host.store_hash_table(marker, table);
                let value = self.apply_callback(function, &call)?;
                self.resync_hash_table(receiver)?;
                table = self.take_table(marker, kind);
                let verdict = self.mod_stamp(marker).verdict(mc);
                if verdict == mod_count::ModVerdict::Unknown {
                    self.host.unknown_operand_call(r)?;
                }
                if verdict == mod_count::ModVerdict::Changed || table.order() != keys {
                    self.host.store_hash_table(marker, table);
                    return Err(NativeError::UncaughtException {
                        class: "java/util/ConcurrentModificationException".into(),
                        message: None,
                        frame: None,
                    });
                }
                value
            }
            None => merged.cloned().unwrap_or(Value::Ref(None)),
        };
        let result =
            self.finish_compute(receiver, kind, &mut table, key, hash, stored, &value, name);
        self.host.store_hash_table(marker, table);
        result.map(|()| Some(value))
    }

    /// `HashMap.replaceAll`/`LinkedHashMap.replaceAll`: `e.value =
    /// function.apply(e.key, e.value)` for every entry in iteration order,
    /// each reading the value as it is then, and a
    /// `ConcurrentModificationException` at the end if the function changed
    /// the map's structure. The JDK keeps walking its table after such a
    /// change, which is not modelled: counted.
    pub(crate) fn hash_replace_all(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        marker: &str,
        function: &Value,
    ) -> Result<Value, NativeError> {
        if *function == Value::Ref(None) {
            return Err(NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: None,
                frame: None,
            });
        }
        let keys: Vec<Value> = self
            .collection_elements(receiver)?
            .into_iter()
            .step_by(2)
            .collect();
        let mc = self.mod_stamp(marker);
        let mut counted = false;
        for key in keys {
            if !counted && self.mod_stamp(marker) != mc {
                self.host.unknown_operand_call(r)?;
                counted = true;
            }
            let entries = self.collection_elements(receiver)?;
            let item = collection_item_marker(&key);
            if !entries
                .chunks(2)
                .any(|pair| collection_item_marker(&pair[0]) == item)
            {
                continue;
            }
            let value = Self::stored_value(&entries, &key);
            let replaced = self.apply_callback(function, &[key.clone(), value])?;
            self.set_stored_value(receiver, &key, replaced)?;
        }
        match self.mod_stamp(marker).verdict(mc) {
            mod_count::ModVerdict::Same => Ok(Value::Void),
            mod_count::ModVerdict::Changed => Err(NativeError::UncaughtException {
                class: "java/util/ConcurrentModificationException".into(),
                message: None,
                frame: None,
            }),
            mod_count::ModVerdict::Unknown => {
                if !counted {
                    self.host.unknown_operand_call(r)?;
                }
                Ok(Value::Void)
            }
        }
    }

    /// `replaceAll` with a function on a host map this runtime does not
    /// model (the host's answer skips a jar function): counted.
    pub(crate) fn other_map_replace_all(
        &self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
    ) -> bool {
        r.name == "replaceAll"
            && r.descriptor == "(Ljava/util/function/BiFunction;)V"
            && matches!(opcode, 0xb6 | 0xb9)
            && receiver.is_some_and(|receiver| {
                self.host_collection_kind(receiver) == Some(object_methods::CollectionKind::Map)
                    && self
                        .host_collection_class(receiver)
                        .is_some_and(|class| !class.starts_with("java/util/ImmutableCollections"))
            })
    }

    /// The end of a compute: store `value` under the found key, remove it
    /// (`value == null`), or insert the key at its bin's head.
    #[allow(clippy::too_many_arguments)]
    fn finish_compute(
        &mut self,
        receiver: &Value,
        kind: TableKind,
        table: &mut HashTable,
        key: &Value,
        hash: Option<i32>,
        stored: Option<Value>,
        value: &Value,
        name: &str,
    ) -> Result<(), NativeError> {
        match stored {
            Some(stored) if *value != Value::Ref(None) => {
                self.set_stored_value(receiver, &stored, value.clone())?;
            }
            Some(_) => {
                // `removeNode(hash, key, ...)` searches the bin again;
                // `computeIfPresent` first computes `hash(key)` once more.
                let hash = if name == "computeIfPresent" {
                    self.java_hash(key)?
                } else {
                    hash
                };
                if let Lookup::Found(node) = self.table_lookup(table, key, hash)? {
                    let mut entries = self.collection_elements(receiver)?;
                    let marker = collection_item_marker(&node);
                    if let Some(index) = entries
                        .chunks(2)
                        .position(|pair| collection_item_marker(&pair[0]) == marker)
                    {
                        entries.drain(index * 2..index * 2 + 2);
                        self.host.replace_mutable_collection(receiver, entries);
                    }
                    table.remove(&marker);
                }
                let _ = kind;
            }
            None if *value != Value::Ref(None) && name != "computeIfPresent" => {
                let mut entries = self.collection_elements(receiver)?;
                entries.extend([key.clone(), value.clone()]);
                self.host.replace_mutable_collection(receiver, entries);
                table.insert_at_head(collection_item_marker(key), hash);
            }
            None => {}
        }
        Ok(())
    }

    /// `AbstractSet.removeAll` (iterate the smaller side) and
    /// `AbstractCollection.retainAll`.
    fn set_remove_all(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        kind: TableKind,
        table: &mut HashTable,
        source: &Value,
        retain: bool,
    ) -> Result<Option<Value>, NativeError> {
        if *source == Value::Ref(None) {
            return Err(NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: None,
                frame: None,
            });
        }
        if self.host_collection_kind(source).is_none() {
            return Ok(None);
        }
        let theirs = self.collection_elements(source)?;
        let mine = self.collection_elements(receiver)?;
        let mut changed = false;
        if !retain && mine.len() > theirs.len() {
            // `for (Object e : c) modified |= remove(e);`
            for element in &theirs {
                match self.remove_node(receiver, kind, table, element, None)? {
                    None => return self.count_and_continue(r),
                    Some(removed) => changed |= removed.is_some(),
                }
            }
            return Ok(Some(Value::Int(i32::from(changed))));
        }
        let mut kept = Vec::with_capacity(mine.len());
        let itself = source == receiver;
        for (index, element) in mine.iter().enumerate() {
            let lookup = if itself {
                // `c.contains(e)` on this very set (its table is held here).
                self.get_node(table, element).map(|found| match found {
                    Lookup::Found(_) => Some(true),
                    Lookup::Absent => Some(false),
                    Lookup::Unknown => None,
                })
            } else {
                self.hash_aware_contains(source, element)
            };
            let contained = match lookup {
                Ok(Some(contained)) => contained,
                Ok(None) => return self.count_and_continue(r),
                Err(error) => {
                    // The iterator already removed what it removed.
                    if changed {
                        kept.extend_from_slice(&mine[index..]);
                        self.host.replace_mutable_collection(receiver, kept);
                    }
                    return Err(error);
                }
            };
            if contained == retain {
                kept.push(element.clone());
                continue;
            }
            // `it.remove()`: `removeNode(p.hash, p.key, ...)` walks the bin
            // again, so `e.equals(k)` runs for each earlier same-hash node.
            let marker = collection_item_marker(element);
            let walked = match table.stored_hash(&marker) {
                Some(stored) => self.table_lookup(table, element, Some(hash_table::spread(stored))),
                None => Ok(Lookup::Found(element.clone())),
            };
            match walked {
                Ok(Lookup::Found(node)) => {
                    let node = collection_item_marker(&node);
                    table.remove(&node);
                    if node != marker {
                        // An `equals` that matched an earlier node removed
                        // that one instead.
                        kept.retain(|kept| collection_item_marker(kept) != node);
                        kept.push(element.clone());
                    }
                }
                Ok(_) => kept.push(element.clone()),
                Err(error) => {
                    kept.extend_from_slice(&mine[index..]);
                    self.host.replace_mutable_collection(receiver, kept);
                    return Err(error);
                }
            }
            changed = true;
        }
        if changed {
            self.host.replace_mutable_collection(receiver, kept);
        }
        Ok(Some(Value::Int(i32::from(changed))))
    }

    /// `AbstractMap.equals`/`AbstractSet.equals`.
    fn hash_collection_equals(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        kind: TableKind,
        table: &HashTable,
        other: &Value,
    ) -> Result<Option<Value>, NativeError> {
        if receiver == other {
            return Ok(Some(Value::Int(1)));
        }
        let wanted = if kind.map {
            object_methods::CollectionKind::Map
        } else {
            object_methods::CollectionKind::Set
        };
        let Some(other_kind) = self.host_collection_kind(other) else {
            // Not a collection the host holds: a jar `Map`/`Set` (counted
            // earlier in the dispatch) or not a map at all.
            return Ok(None);
        };
        if other_kind != wanted {
            return Ok(Some(Value::Int(0)));
        }
        let mine = self.collection_elements(receiver)?;
        let theirs = self.collection_elements(other)?;
        if mine.len() != theirs.len() {
            return Ok(Some(Value::Int(0)));
        }
        if kind.map {
            for pair in mine.chunks(2) {
                let [key, value] = pair else { continue };
                let Some((present, found)) = self.other_map_get(other, key)? else {
                    return self.count_and_continue(r);
                };
                let equal = if *value == Value::Ref(None) {
                    found == Value::Ref(None) && present
                } else {
                    // `catch (ClassCastException | NullPointerException)`:
                    // `false`.
                    match self.java_equals(value, &found) {
                        Ok(equal) => equal,
                        Err(error) if caught_by_abstract_equals(&error) => false,
                        Err(error) => return Err(error),
                    }
                };
                if !equal {
                    return Ok(Some(Value::Int(0)));
                }
            }
            return Ok(Some(Value::Int(1)));
        }
        // `containsAll(o)`: this set's `contains` for each of `o`'s elements.
        let mut answer = Some(Value::Int(1));
        for element in &theirs {
            match self.get_node(table, element) {
                Err(error) if caught_by_abstract_equals(&error) => {
                    answer = Some(Value::Int(0));
                    break;
                }
                Ok(Lookup::Found(_)) => {}
                Ok(Lookup::Absent) => {
                    answer = Some(Value::Int(0));
                    break;
                }
                Ok(Lookup::Unknown) => {
                    answer = None;
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        match answer {
            Some(answer) => Ok(Some(answer)),
            None => self.count_and_continue(r),
        }
    }
}

/// The member a stream stage counts when a lookup in its hash collection
/// cannot be answered.
fn stream_member(name: &str, descriptor: &str) -> MethodRef {
    MethodRef {
        owner: "java/util/stream/Stream".into(),
        name: name.into(),
        descriptor: descriptor.into(),
    }
}

impl<H: Host> Runtime<'_, H> {
    /// A new, empty host `HashMap`/`HashSet` with its table: what
    /// `HashMap::new`/`HashSet::new` hand a collector.
    fn new_hash_collection(&mut self, map: bool) -> Result<(Value, HashTable), NativeError> {
        let class = if map {
            "java/util/HashMap"
        } else {
            "java/util/HashSet"
        };
        let value = self.new_object(class);
        let empty = MethodRef {
            owner: class.into(),
            name: "<init>".into(),
            descriptor: "()V".into(),
        };
        self.host.invoke_instance(&empty, &value, &[])?;
        Ok((value, HashTable::new(None, 0.75)))
    }

    /// Put a collector's table back beside its collection.
    fn keep_table(&mut self, collection: &Value, table: HashTable) {
        if let Some(marker) = expect_ref(collection) {
            self.host.store_hash_table(marker, table);
        }
    }

    /// `Stream.distinct()` on a sequential stream: `DistinctOps` keeps a
    /// `HashSet` of what it has passed on, and passes `t` on when
    /// `seen.add(t)`.
    pub(crate) fn hash_distinct(&mut self, values: Vec<Value>) -> Result<Vec<Value>, NativeError> {
        let (seen, mut table) = self.new_hash_collection(false)?;
        let mut kept = Vec::with_capacity(values.len());
        let mut result = Ok(());
        for value in values {
            match self.set_add(&seen, &mut table, &value) {
                Ok(Some(true)) => kept.push(value),
                Ok(Some(false)) => {}
                Ok(None) => {
                    // Identity-hash membership: counted, kept by marker.
                    self.host.unknown_operand_call(&stream_member(
                        "distinct",
                        "()Ljava/util/stream/Stream;",
                    ))?;
                    if !kept.contains(&value) {
                        kept.push(value);
                    }
                }
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        self.keep_table(&seen, table);
        result.map(|()| kept)
    }

    /// `Collectors.toSet()`: `HashSet::new` and `Set::add`, so the result
    /// iterates in the set's bucket order.
    pub(crate) fn hash_set_collect(&mut self, values: &[Value]) -> Result<Value, NativeError> {
        let (set, mut table) = self.new_hash_collection(false)?;
        let mut result = Ok(());
        for value in values {
            match self.set_add(&set, &mut table, value) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    self.host.unknown_operand_call(&stream_member(
                        "collect",
                        "(Ljava/util/stream/Collector;)Ljava/lang/Object;",
                    ))?;
                    table.tainted = true;
                }
                Err(error) => {
                    result = Err(error);
                    break;
                }
            }
        }
        self.keep_table(&set, table);
        result.map(|()| set)
    }

    /// `Collectors.toMap(key, value[, merge])`: `HashMap::new`, then per
    /// element `putIfAbsent` (a repeated key throws "Duplicate key k
    /// (attempted merging values u and v)") or `merge`.
    pub(crate) fn hash_map_collect(
        &mut self,
        key_fn: &Value,
        value_fn: &Value,
        merge_fn: Option<&Value>,
        values: &[Value],
    ) -> Result<Value, NativeError> {
        let (map, mut table) = self.new_hash_collection(true)?;
        let result =
            self.hash_map_collect_into(&map, &mut table, key_fn, value_fn, merge_fn, values);
        self.keep_table(&map, table);
        result.map(|()| map)
    }

    fn hash_map_collect_into(
        &mut self,
        map: &Value,
        table: &mut HashTable,
        key_fn: &Value,
        value_fn: &Value,
        merge_fn: Option<&Value>,
        values: &[Value],
    ) -> Result<(), NativeError> {
        let null_pointer = || NativeError::UncaughtException {
            class: "java/lang/NullPointerException".into(),
            message: None,
            frame: None,
        };
        let counted = stream_member(
            "collect",
            "(Ljava/util/stream/Collector;)Ljava/lang/Object;",
        );
        for element in values {
            let key = self.apply_callback(key_fn, std::slice::from_ref(element))?;
            let value = self.apply_callback(value_fn, std::slice::from_ref(element))?;
            if value == Value::Ref(None) {
                return Err(null_pointer());
            }
            let Some(merge) = merge_fn else {
                match self.put_value(map, table, &key, value.clone(), true)? {
                    Some(Value::Ref(None)) => {}
                    Some(previous) => {
                        let key = self.value_of_text(&key, "Ljava/lang/Object;", "", "toMap")?;
                        let previous =
                            self.value_of_text(&previous, "Ljava/lang/Object;", "", "toMap")?;
                        let value =
                            self.value_of_text(&value, "Ljava/lang/Object;", "", "toMap")?;
                        return Err(NativeError::UncaughtException {
                            class: "java/lang/IllegalStateException".into(),
                            message: Some(format!(
                                "Duplicate key {key} (attempted merging values {previous} and {value})"
                            )),
                            frame: None,
                        });
                    }
                    None => {
                        self.host.unknown_operand_call(&counted)?;
                        table.tainted = true;
                    }
                }
                continue;
            };
            // `HashMap.merge(key, value, merge)`.
            table.begin_compute();
            let hash = self.java_hash(&key)?;
            match self.table_lookup(table, &key, hash)? {
                Lookup::Unknown => {
                    self.host.unknown_operand_call(&counted)?;
                    table.tainted = true;
                }
                Lookup::Absent => {
                    let mut entries = self.collection_elements(map)?;
                    entries.extend([key.clone(), value]);
                    self.host.replace_mutable_collection(map, entries);
                    table.insert_at_head(collection_item_marker(&key), hash);
                }
                Lookup::Found(stored) => {
                    let entries = self.collection_elements(map)?;
                    let old = Self::stored_value(&entries, &stored);
                    let merged = if old == Value::Ref(None) {
                        value
                    } else {
                        self.apply_callback(merge, &[old, value])?
                    };
                    if merged == Value::Ref(None) {
                        let kind = TableKind {
                            map: true,
                            linked: false,
                        };
                        self.remove_node(map, kind, table, &stored, None)?;
                    } else {
                        self.set_stored_value(map, &stored, merged)?;
                    }
                }
            }
        }
        Ok(())
    }
}
