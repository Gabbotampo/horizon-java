//! `HashMap`, `HashSet`, `LinkedHashMap` and `LinkedHashSet` as JDK 25 runs
//! them: their iteration order and their lookups.
//!
//! The host keeps a hash collection's entries in a list, in insertion order,
//! and finds a key by its marker. Two answers were silently wrong:
//!
//! - **Order.** A `HashMap`/`HashSet` iterates its bucket table, not its
//!   insertion order: `HashSet` of `"pear","b","apple","a"` iterates
//!   `apple,a,pear,b`, `HashMap` keys `300,17,5` iterate `17,5,300`.
//! - **Lookups with jar keys.** A record or any jar class with its own
//!   `equals`/`hashCode` is found by value on a JVM, but only through its
//!   hash: `HashMap.getNode` calls `key.hashCode()` once and `key.equals(k)`
//!   only for the nodes of its bin whose stored hash equals the key's. A
//!   class that overrides `equals` but not `hashCode` is therefore usually
//!   *not* found by an equal key, an `equals` that answers `true` for
//!   everything does not merge keys of different hashes, and an `equals`
//!   that throws only throws for a same-hash key.
//!
//! [`hash_table`](crate::hash_table) keeps the table the JDK would have for
//! each collection. Before any call that has one as its receiver or an
//! argument, [`Runtime::resync_hash_table`] reconciles it with the host's
//! list (entries some other path added or removed) and puts the list in the
//! table's order. The members that look a key up are answered here, with
//! the jar's `hashCode` and `equals` run exactly as `HashMap` runs them:
//! `get`/`getOrDefault`/`containsKey`/`put`/`putIfAbsent`/`remove`/
//! `replace`/`containsValue`/`putAll`/`clear`, a set's `add`/`contains`/
//! `remove`/`addAll`, `hashCode`, and the constructors (which size the
//! table). `compute*`/`merge`, the bulk set members, `equals` and the
//! streams' hash collections are [`hash_bulk`](crate::hash_bulk)'s.
//!
//! What no run can reproduce is counted instead of answered:
//!
//! - a key whose hash is an identity hash (`Object.hashCode`, an enum
//!   constant) has no known bin, so the collection's **iteration order** is
//!   unknown, and a call that shows it (`iterator`, `forEach`, `keySet`,
//!   `toString`, `stream`, passing it to another collection...) is counted;
//! - a lookup by a key whose class has its own `equals` but keeps
//!   `Object.hashCode` is counted (whether `equals` runs depends on the
//!   identity hashes);
//! - a bin that would become a red-black tree, and a `LinkedHashMap` in
//!   access order, make the order unknown too.

use super::*;
use hash_table::HashTable;
use mod_count::{ModStamp, ModVerdict};
use object_methods::Equality;

/// What a hash collection is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TableKind {
    pub(crate) map: bool,
    pub(crate) linked: bool,
}

/// The JDK class a hash collection is, by the class name.
fn table_kind_of(class: &str) -> Option<TableKind> {
    Some(match class {
        "java/util/HashMap" => TableKind {
            map: true,
            linked: false,
        },
        "java/util/LinkedHashMap" => TableKind {
            map: true,
            linked: true,
        },
        "java/util/HashSet" => TableKind {
            map: false,
            linked: false,
        },
        "java/util/LinkedHashSet" => TableKind {
            map: false,
            linked: true,
        },
        _ => return None,
    })
}

/// Where a key is, as `HashMap.getNode` would find it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Lookup {
    /// The node of this stored key.
    Found(Value),
    Absent,
    /// The answer depends on identity hashes or a tree bin.
    Unknown,
}

/// Members whose answer does not depend on iteration order (with pure
/// values: the modelled ones count themselves when an impure value would
/// show the order).
fn order_insensitive(name: &str) -> bool {
    matches!(
        name,
        "<init>"
            | "size"
            | "isEmpty"
            | "get"
            | "getOrDefault"
            | "containsKey"
            | "contains"
            | "put"
            | "add"
            | "remove"
            | "putIfAbsent"
            | "replace"
            | "clear"
            | "containsValue"
            | "equals"
            | "hashCode"
            | "computeIfAbsent"
            | "computeIfPresent"
            | "compute"
            | "merge"
            | "putAll"
            | "addAll"
            | "containsAll"
            | "removeAll"
            | "retainAll"
    )
}

impl<H: Host> Runtime<'_, H> {
    /// The hash collection `value` is: a host row of one of the four classes,
    /// or of a jar class extending one.
    pub(crate) fn hash_table_kind(&self, value: &Value) -> Option<TableKind> {
        let marker = expect_ref(value)?;
        let (class, _) = marker.split_once('#')?;
        let mut class = class.to_owned();
        let mut kind = table_kind_of(&class);
        if kind.is_none() {
            jvm_dispatch::constructed_object_class(marker)?;
            for _ in 0..MAX_SUPERCLASS_WALK {
                match self.class(&class) {
                    Some(file) => class = file.super_name.clone(),
                    None => break,
                }
            }
            kind = table_kind_of(&class);
        }
        let kind = kind?;
        self.host.has_mutable_collection(marker).then_some(kind)
    }

    /// The table of `marker`, made from the host's list when this runtime
    /// has not seen the collection built (its entries put in list order).
    pub(crate) fn take_table(&mut self, marker: &str, kind: TableKind) -> HashTable {
        self.host.take_hash_table(marker).unwrap_or_else(|| {
            let mut table = HashTable::new(None, 0.75);
            table.linked = kind.linked;
            table
        })
    }

    /// Reconcile the table of `value` with the host's list (entries another
    /// path added or removed), and put the list in the table's order.
    pub(crate) fn resync_hash_table(&mut self, value: &Value) -> Result<(), NativeError> {
        let Some(kind) = self.hash_table_kind(value) else {
            return Ok(());
        };
        let Some(marker) = expect_ref(value).cloned() else {
            return Ok(());
        };
        let mut table = self.take_table(&marker, kind);
        let result = self.resync_with(value, kind, &mut table);
        self.host.store_hash_table(&marker, table);
        result
    }

    fn resync_with(
        &mut self,
        value: &Value,
        kind: TableKind,
        table: &mut HashTable,
    ) -> Result<(), NativeError> {
        let entries = self.collection_elements(value)?;
        let step = if kind.map { 2 } else { 1 };
        let keys: Vec<String> = entries
            .iter()
            .step_by(step)
            .map(collection_item_marker)
            .collect();
        let present: std::collections::BTreeSet<&String> = keys.iter().collect();
        for stale in table.order() {
            if !present.contains(&stale) {
                table.remove(&stale);
            }
        }
        for (index, key) in keys.iter().enumerate() {
            if table.contains(key) {
                continue;
            }
            // Added by a path that did not run its `hashCode`: a pure key's
            // hash is computed now (unobservably), any other key's position
            // is unknown.
            let entry = &entries[index * step];
            let hash = if self.pure_equality(entry) {
                self.java_hash(entry)?
            } else {
                None
            };
            table.insert(key.clone(), hash);
        }
        if table.linked || table.order_unknown() {
            return Ok(());
        }
        let order = table.order();
        if order == keys {
            return Ok(());
        }
        let position: BTreeMap<&String, usize> =
            keys.iter().enumerate().map(|(i, key)| (key, i)).collect();
        let mut sorted = Vec::with_capacity(entries.len());
        for key in &order {
            let Some(&index) = position.get(key) else {
                continue;
            };
            sorted.extend_from_slice(&entries[index * step..index * step + step]);
        }
        self.host.replace_mutable_collection(value, sorted);
        Ok(())
    }

    /// `HashMap.getNode`'s search for `key`, whose `hashCode()` is `hash`,
    /// calling `key.equals(k)` for each same-hash node of its bin in order.
    pub(crate) fn table_lookup(
        &mut self,
        table: &HashTable,
        key: &Value,
        hash: Option<i32>,
    ) -> Result<Lookup, NativeError> {
        let marker = collection_item_marker(key);
        let Some(hash) = hash else {
            // An identity-hashed (or unhashable) key. With `Object.equals`
            // (or a box's), only the same value matches, wherever it is.
            // Its own `equals` runs only for a node of equal hash: unknown,
            // unless there is no node at all.
            if table.len() > 0
                && matches!(
                    self.equality_of(key),
                    Equality::Jar {
                        equals: Some(_),
                        ..
                    }
                )
            {
                return Ok(Lookup::Unknown);
            }
            return Ok(if table.contains(&marker) {
                Lookup::Found(key.clone())
            } else {
                Lookup::Absent
            });
        };
        if table.lookups_unknown() && !self.pure_equality(key) {
            return Ok(Lookup::Unknown);
        }
        for candidate in table.candidates(hash) {
            let stored = parse_collection_value(&candidate)?;
            if stored == *key && !matches!(stored, Value::Float(_)) {
                return Ok(Lookup::Found(stored));
            }
            if *key != Value::Ref(None) && self.java_equals(key, &stored)? {
                return Ok(Lookup::Found(stored));
            }
        }
        // The same object under another stored hash (a key mutated after it
        // was put): the JDK misses it, the host's list would find it.
        if table.contains(&marker) {
            return Ok(Lookup::Unknown);
        }
        Ok(Lookup::Absent)
    }

    /// The value stored for `key` in a map's host list.
    pub(crate) fn stored_value(entries: &[Value], key: &Value) -> Value {
        let marker = collection_item_marker(key);
        entries
            .chunks(2)
            .find(|pair| collection_item_marker(&pair[0]) == marker)
            .map_or(Value::Ref(None), |pair| {
                pair.get(1).cloned().unwrap_or(Value::Ref(None))
            })
    }

    /// Count `r` as unknown, keeping the host's answer.
    pub(crate) fn count_and_continue(
        &mut self,
        r: &MethodRef,
    ) -> Result<Option<Value>, NativeError> {
        self.host.unknown_operand_call(r)?;
        Ok(None)
    }

    /// Hash collections: resync every one the call touches, count a call
    /// that would show an unknown order, and answer the members that look a
    /// key up. `args[0]` may be replaced by the stored key it `equals`
    /// (`compute*`/`merge`), for the rest of the dispatch to find.
    pub(crate) fn invoke_hash_collection(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &mut [Value],
    ) -> Result<Option<Value>, NativeError> {
        if let Some(answer) = self.map_view_copy(r, opcode, receiver, args)? {
            return Ok(answer);
        }
        let receiver_kind = receiver
            .filter(|_| matches!(opcode, 0xb6 | 0xb7 | 0xb9))
            .and_then(|value| {
                // A constructor runs before the host has a row for it.
                if r.name == "<init>" {
                    table_kind_of(&r.owner)
                } else {
                    self.hash_table_kind(value)
                }
            });
        // A hash collection passed to anything: in order first, and counted
        // when that order is unknown (its order reaches the callee), unless
        // the receiver is a hash collection taking it in (`putAll`/`addAll`/
        // a copy constructor, which then inherits the unknown order).
        let mut tainted_argument = false;
        for arg in args.iter() {
            // A `Set.of`/`Map.of` of two or more iterates in a random order.
            if self.random_order(arg) {
                tainted_argument = true;
                continue;
            }
            if self.hash_table_kind(arg).is_none() {
                continue;
            }
            self.resync_hash_table(arg)?;
            if let Some(marker) = expect_ref(arg)
                && let Some(table) = self.host.take_hash_table(marker)
            {
                tainted_argument |= table.order_unknown();
                self.host.store_hash_table(marker, table);
            }
        }
        let Some(kind) = receiver_kind else {
            if self.other_map_replace_all(r, opcode, receiver) {
                // `hash_bulk.rs`: no other map's `replaceAll` is modelled.
                return self.count_and_continue(r);
            }
            if tainted_argument && self.shows_argument_order(r) {
                return self.count_and_continue(r);
            }
            return self.snapshot_set_contains(r, opcode, receiver, args);
        };
        let Some(receiver) = receiver else {
            return Ok(None);
        };
        let Some(marker) = expect_ref(receiver).cloned() else {
            return Ok(None);
        };
        if r.name == "<init>" {
            if table_kind_of(&r.owner).is_none() {
                return Ok(None);
            }
            return self.construct_hash_collection(r, receiver, &marker, kind, args);
        }
        self.resync_hash_table(receiver)?;
        if kind.map
            && matches!(
                r.name.as_str(),
                "computeIfAbsent" | "computeIfPresent" | "compute" | "merge"
            )
        {
            // `hash_bulk.rs`.
            return self.hash_compute(r, receiver, &marker, kind, args);
        }
        match (r.name.as_str(), r.descriptor.as_str(), &*args) {
            ("removeIf", "(Ljava/util/function/Predicate;)Z", [predicate]) if !kind.map => {
                let predicate = predicate.clone();
                if !self
                    .host
                    .take_hash_table(&marker)
                    .map(|table| {
                        let known = !table.order_unknown() && !table.lookups_unknown();
                        self.host.store_hash_table(&marker, table);
                        known
                    })
                    .unwrap_or(false)
                {
                    return self.count_and_continue(r);
                }
                return self
                    .hash_remove_if(receiver, &marker, kind, &predicate)
                    .map(Some);
            }
            ("replaceAll", "(Ljava/util/function/BiFunction;)V", [function]) if kind.map => {
                let function = function.clone();
                let unknown = self.host.take_hash_table(&marker).map(|table| {
                    let unknown = table.order_unknown();
                    self.host.store_hash_table(&marker, table);
                    unknown
                });
                if unknown == Some(true) {
                    return self.count_and_continue(r);
                }
                // `hash_bulk.rs`.
                return self
                    .hash_replace_all(r, receiver, &marker, &function)
                    .map(Some);
            }
            ("forEach", "(Ljava/util/function/Consumer;)V", [callback])
            | ("forEach", "(Ljava/util/function/BiConsumer;)V", [callback]) => {
                let callback = callback.clone();
                let unknown = self.host.take_hash_table(&marker).map(|table| {
                    let unknown = table.order_unknown();
                    self.host.store_hash_table(&marker, table);
                    unknown
                });
                if unknown == Some(true) {
                    return self.count_and_continue(r);
                }
                return self.hash_for_each(r, receiver, kind, &callback).map(Some);
            }
            _ => {}
        }
        let mut table = self.take_table(&marker, kind);
        if tainted_argument && matches!(r.name.as_str(), "putAll" | "addAll") {
            table.tainted = true;
        }
        let result = self.hash_member(r, receiver, kind, &mut table, args);
        self.host.store_hash_table(&marker, table);
        match result? {
            Some(answer) => Ok(Some(answer)),
            None => Ok(None),
        }
    }

    /// A map's `keySet()`/`values()`/`entrySet()`, which the host answers
    /// with a copy rather than a view: the copy is remembered, and a mutator
    /// called on it (which on a JVM writes through to the map) is counted.
    /// `Some(answer)` answers the call; `Some(None)` leaves a counted call
    /// to the host.
    fn map_view_copy(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Option<Value>>, NativeError> {
        let Some(receiver) = receiver.filter(|_| matches!(opcode, 0xb6 | 0xb9)) else {
            return Ok(None);
        };
        let Some(marker) = expect_ref(receiver).cloned() else {
            return Ok(None);
        };
        if self.host.is_view_copy(&marker) && java_immutable::is_mutator(&r.name) {
            return self.count_and_continue(r).map(Some);
        }
        // `equals` of a copy holding jar keys: the host compares markers.
        if self.host.is_view_copy(&marker)
            && r.name == "equals"
            && r.descriptor == "(Ljava/lang/Object;)Z"
        {
            let mut values = self.collection_elements(receiver)?;
            values.extend(
                args.first()
                    .and_then(|other| self.collection_elements(other).ok())
                    .unwrap_or_default(),
            );
            if values
                .iter()
                .any(|value| matches!(self.equality_of(value), Equality::Jar { .. }))
            {
                return self.count_and_continue(r).map(Some);
            }
        }
        if !matches!(r.name.as_str(), "keySet" | "values" | "entrySet")
            || !args.is_empty()
            || !is_map_owner(&r.owner)
            || self.host_collection_kind(receiver) != Some(object_methods::CollectionKind::Map)
        {
            return Ok(None);
        }
        if let Some(kind) = self.hash_table_kind(receiver) {
            self.resync_hash_table(receiver)?;
            let table = self.take_table(&marker, kind);
            let unknown = table.order_unknown();
            self.host.store_hash_table(&marker, table);
            if unknown {
                self.host.unknown_operand_call(r)?;
            }
        }
        let copy = self.host.invoke_instance(r, receiver, args)?;
        if let Some(copy) = expect_ref(&copy) {
            self.host.note_view_copy(copy, &marker);
        }
        Ok(Some(Some(copy)))
    }

    /// Whether a call that is handed a collection of unknown order can show
    /// that order: a host model that copies, prints or iterates it. Jar code
    /// (whose own calls on the collection are checked one by one) and the
    /// helpers that only pass the reference on or compare it cannot.
    pub(crate) fn shows_argument_order(&self, r: &MethodRef) -> bool {
        if self
            .member_owner(&r.owner, &r.name, &r.descriptor)
            .is_some_and(|(_, jar)| jar)
        {
            return false;
        }
        !matches!(
            (r.owner.as_str(), r.name.as_str()),
            (
                "java/util/Objects",
                "requireNonNull" | "requireNonNullElse" | "isNull" | "nonNull" | "equals"
            ) | ("java/util/Optional", "of" | "ofNullable")
        ) && !matches!(
            r.name.as_str(),
            "equals" | "containsAll" | "contains" | "containsKey" | "containsValue"
        )
    }

    /// The constructors: the table's initial capacity and load factor, and a
    /// copy constructor's entries.
    fn construct_hash_collection(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        marker: &str,
        kind: TableKind,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let illegal = |message: String| NativeError::UncaughtException {
            class: "java/lang/IllegalArgumentException".into(),
            message: Some(message),
            frame: None,
        };
        let (capacity, load_factor, access_order, source) = match (r.descriptor.as_str(), args) {
            ("()V", []) => (kind.linked.then_some(16), 0.75_f32, false, None),
            ("(I)V", [Value::Int(cap)]) => (Some(*cap), 0.75, false, None),
            ("(IF)V", [Value::Int(cap), Value::Float(load)]) => {
                (Some(*cap), *load as f32, false, None)
            }
            ("(IFZ)V", [Value::Int(cap), Value::Float(load), Value::Int(order)]) if kind.map => {
                (Some(*cap), *load as f32, *order != 0, None)
            }
            ("(Ljava/util/Map;)V", [source]) if kind.map => (None, 0.75, false, Some(source)),
            ("(Ljava/util/Collection;)V", [source]) if !kind.map => {
                (None, 0.75, false, Some(source))
            }
            _ => return Ok(None),
        };
        if let Some(cap) = capacity
            && cap < 0
        {
            return Err(illegal(format!("Illegal initial capacity: {cap}")));
        }
        if load_factor <= 0.0 || load_factor.is_nan() {
            return Err(illegal(format!(
                "Illegal load factor: {}",
                format_java_float32(f64::from(load_factor))
            )));
        }
        // The host's empty row.
        let empty = MethodRef {
            owner: r.owner.clone(),
            name: "<init>".into(),
            descriptor: "()V".into(),
        };
        self.host.invoke_instance(&empty, receiver, &[])?;
        let source = match source {
            None => None,
            Some(Value::Ref(None)) => {
                return Err(NativeError::UncaughtException {
                    class: "java/lang/NullPointerException".into(),
                    message: None,
                    frame: None,
                });
            }
            Some(source) => {
                if self.host_collection_kind(source).is_none() {
                    // A source the host holds no elements for.
                    self.host
                        .store_hash_table(marker, HashTable::new(None, 0.75));
                    return self.count_and_continue(r).map(|_| Some(Value::Void));
                }
                Some(source.clone())
            }
        };
        let capacity = match (&source, kind.map) {
            // `HashSet(c)`/`LinkedHashSet(c)`: `HashMap.newHashMap(max(n, 12))`.
            (Some(source), false) => {
                let size = self.collection_elements(source)?.len();
                Some(hash_table::capacity_for(size.max(12)).min(i64::from(i32::MAX)) as i32)
            }
            _ => capacity,
        };
        let mut table = HashTable::new(capacity, load_factor);
        table.linked = kind.linked;
        table.access_order = access_order;
        let result = match source {
            Some(source) => {
                let tainted = self
                    .hash_table_kind(&source)
                    .and_then(|_| expect_ref(&source).cloned())
                    .and_then(|marker| {
                        let table = self.host.take_hash_table(&marker)?;
                        let unknown = table.order_unknown();
                        self.host.store_hash_table(&marker, table);
                        Some(unknown)
                    })
                    .unwrap_or(false);
                table.tainted |= tainted || self.random_order(&source);
                if kind.map {
                    self.put_all(receiver, &mut table, &source)
                } else {
                    self.add_all(receiver, &mut table, &source).map(|_| ())
                }
            }
            None => Ok(()),
        };
        self.host.store_hash_table(marker, table);
        result.map(|()| Some(Value::Void))
    }

    /// `putVal`: put `value` under `key` (`only_if_absent`: keep a non-null
    /// existing value), answering the previous value. The key's `hashCode`
    /// runs first, always.
    pub(crate) fn put_value(
        &mut self,
        receiver: &Value,
        table: &mut HashTable,
        key: &Value,
        value: Value,
        only_if_absent: bool,
    ) -> Result<Option<Value>, NativeError> {
        let hash = self.java_hash(key)?;
        let found = self.table_lookup(table, key, hash)?;
        let mut entries = self.collection_elements(receiver)?;
        match found {
            Lookup::Unknown => Ok(None),
            Lookup::Found(stored) => {
                let marker = collection_item_marker(&stored);
                let Some(index) = entries
                    .chunks(2)
                    .position(|pair| collection_item_marker(&pair[0]) == marker)
                else {
                    return Ok(None);
                };
                let old = entries[index * 2 + 1].clone();
                if !only_if_absent || old == Value::Ref(None) {
                    entries[index * 2 + 1] = value;
                    self.host.replace_mutable_collection(receiver, entries);
                }
                Ok(Some(old))
            }
            Lookup::Absent => {
                entries.extend([key.clone(), value]);
                self.host.replace_mutable_collection(receiver, entries);
                table.insert(collection_item_marker(key), hash);
                Ok(Some(Value::Ref(None)))
            }
        }
    }

    /// A set's `add`: `map.put(e, PRESENT) == null`. `None` when the lookup
    /// cannot be answered.
    pub(crate) fn set_add(
        &mut self,
        receiver: &Value,
        table: &mut HashTable,
        element: &Value,
    ) -> Result<Option<bool>, NativeError> {
        let hash = self.java_hash(element)?;
        match self.table_lookup(table, element, hash)? {
            Lookup::Unknown => Ok(None),
            Lookup::Found(_) => Ok(Some(false)),
            Lookup::Absent => {
                let mut entries = self.collection_elements(receiver)?;
                entries.push(element.clone());
                self.host.replace_mutable_collection(receiver, entries);
                table.insert(collection_item_marker(element), hash);
                Ok(Some(true))
            }
        }
    }

    /// `getNode(key)`: no hash at all while no table is allocated.
    pub(crate) fn get_node(
        &mut self,
        table: &HashTable,
        key: &Value,
    ) -> Result<Lookup, NativeError> {
        if !table.allocated() {
            return Ok(Lookup::Absent);
        }
        let hash = self.java_hash(key)?;
        self.table_lookup(table, key, hash)
    }

    /// `removeNode(hash(key), ...)`: the hash runs even on an empty map.
    /// `matching`: `remove(key, value)`'s `value.equals(v)` test. Answers
    /// the removed key's value, or `None` when nothing matched.
    pub(crate) fn remove_node(
        &mut self,
        receiver: &Value,
        kind: TableKind,
        table: &mut HashTable,
        key: &Value,
        matching: Option<&Value>,
    ) -> Result<Option<Option<Value>>, NativeError> {
        let hash = self.java_hash(key)?;
        let found = if table.allocated() {
            self.table_lookup(table, key, hash)?
        } else {
            Lookup::Absent
        };
        let stored = match found {
            Lookup::Unknown => return Ok(None),
            Lookup::Absent => return Ok(Some(None)),
            Lookup::Found(stored) => stored,
        };
        let mut entries = self.collection_elements(receiver)?;
        let step = if kind.map { 2 } else { 1 };
        let marker = collection_item_marker(&stored);
        let Some(index) = entries
            .chunks(step)
            .position(|chunk| collection_item_marker(&chunk[0]) == marker)
        else {
            return Ok(Some(None));
        };
        let value = if kind.map {
            entries[index * 2 + 1].clone()
        } else {
            Value::Int(1)
        };
        if let Some(wanted) = matching {
            let matches = value == *wanted
                || (*wanted != Value::Ref(None) && self.java_equals(wanted, &value)?);
            if !matches {
                return Ok(Some(None));
            }
        }
        entries.drain(index * step..index * step + step);
        self.host.replace_mutable_collection(receiver, entries);
        table.remove(&marker);
        Ok(Some(Some(value)))
    }

    /// `putMapEntries(source)`: pre-size, then `putVal` each entry in the
    /// source's order.
    fn put_all(
        &mut self,
        receiver: &Value,
        table: &mut HashTable,
        source: &Value,
    ) -> Result<(), NativeError> {
        let entries = self.collection_elements(source)?;
        table.presize(entries.len() / 2);
        for pair in entries.chunks(2) {
            let [key, value] = pair else { continue };
            if self
                .put_value(receiver, table, key, value.clone(), false)?
                .is_none()
            {
                table.tainted = true;
            }
        }
        Ok(())
    }

    /// `AbstractCollection.addAll(source)`: `add` each element in order.
    fn add_all(
        &mut self,
        receiver: &Value,
        table: &mut HashTable,
        source: &Value,
    ) -> Result<bool, NativeError> {
        let elements = self.collection_elements(source)?;
        let mut changed = false;
        for element in &elements {
            match self.set_add(receiver, table, element)? {
                Some(added) => changed |= added,
                None => table.tainted = true,
            }
        }
        Ok(changed)
    }

    /// One member call on a hash collection. `Ok(None)` leaves it to the rest
    /// of the dispatch.
    fn hash_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        kind: TableKind,
        table: &mut HashTable,
        args: &mut [Value],
    ) -> Result<Option<Value>, NativeError> {
        let unknown = table.order_unknown();
        if unknown && !order_insensitive(&r.name) {
            return self.count_and_continue(r);
        }
        let boolean = |flag: bool| Some(Value::Int(i32::from(flag)));
        let desc = r.descriptor.as_str();
        match (kind.map, r.name.as_str(), &*args) {
            (true, "get", [key]) if desc == "(Ljava/lang/Object;)Ljava/lang/Object;" => {
                match self.get_node(table, key)? {
                    Lookup::Found(stored) => {
                        let entries = self.collection_elements(receiver)?;
                        Ok(Some(Self::stored_value(&entries, &stored)))
                    }
                    Lookup::Absent => Ok(Some(Value::Ref(None))),
                    Lookup::Unknown => self.count_and_continue(r),
                }
            }
            (true, "getOrDefault", [key, default]) => {
                let default = default.clone();
                match self.get_node(table, key)? {
                    Lookup::Found(stored) => {
                        let entries = self.collection_elements(receiver)?;
                        Ok(Some(Self::stored_value(&entries, &stored)))
                    }
                    Lookup::Absent => Ok(Some(default)),
                    Lookup::Unknown => self.count_and_continue(r),
                }
            }
            (true, "containsKey", [key]) | (false, "contains", [key])
                if desc == "(Ljava/lang/Object;)Z" =>
            {
                match self.get_node(table, key)? {
                    Lookup::Found(_) => Ok(boolean(true)),
                    Lookup::Absent => Ok(boolean(false)),
                    Lookup::Unknown => self.count_and_continue(r),
                }
            }
            (true, name @ ("put" | "putIfAbsent"), [key, value])
                if desc == "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;" =>
            {
                let (key, value) = (key.clone(), value.clone());
                match self.put_value(receiver, table, &key, value, name == "putIfAbsent")? {
                    Some(old) => Ok(Some(old)),
                    None => self.count_and_continue(r),
                }
            }
            (false, "add", [element]) if desc == "(Ljava/lang/Object;)Z" => {
                let element = element.clone();
                match self.set_add(receiver, table, &element)? {
                    Some(added) => Ok(boolean(added)),
                    None => self.count_and_continue(r),
                }
            }
            (_, "remove", [key])
                if desc == "(Ljava/lang/Object;)Ljava/lang/Object;"
                    || desc == "(Ljava/lang/Object;)Z" =>
            {
                let key = key.clone();
                match self.remove_node(receiver, kind, table, &key, None)? {
                    None => self.count_and_continue(r),
                    Some(removed) if kind.map => Ok(Some(removed.unwrap_or(Value::Ref(None)))),
                    Some(removed) => Ok(boolean(removed.is_some())),
                }
            }
            (true, "remove", [key, value]) if desc == "(Ljava/lang/Object;Ljava/lang/Object;)Z" => {
                let (key, value) = (key.clone(), value.clone());
                match self.remove_node(receiver, kind, table, &key, Some(&value))? {
                    None => self.count_and_continue(r),
                    Some(removed) => Ok(boolean(removed.is_some())),
                }
            }
            (true, "replace", [key, value])
                if desc == "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;" =>
            {
                let (key, value) = (key.clone(), value.clone());
                match self.get_node(table, &key)? {
                    Lookup::Absent => Ok(Some(Value::Ref(None))),
                    Lookup::Unknown => self.count_and_continue(r),
                    Lookup::Found(stored) => {
                        let old = self.set_stored_value(receiver, &stored, value)?;
                        Ok(Some(old))
                    }
                }
            }
            (true, "replace", [key, expected, value]) => {
                let (key, expected, value) = (key.clone(), expected.clone(), value.clone());
                match self.get_node(table, &key)? {
                    Lookup::Absent => Ok(boolean(false)),
                    Lookup::Unknown => self.count_and_continue(r),
                    Lookup::Found(stored) => {
                        let entries = self.collection_elements(receiver)?;
                        let current = Self::stored_value(&entries, &stored);
                        // `(v = e.value) == oldValue || (v != null && v.equals(oldValue))`.
                        let matches = current == expected
                            || (current != Value::Ref(None)
                                && self.java_equals(&current, &expected)?);
                        if matches {
                            self.set_stored_value(receiver, &stored, value)?;
                        }
                        Ok(boolean(matches))
                    }
                }
            }
            (true, "containsValue", [wanted]) => {
                let wanted = wanted.clone();
                if unknown && !self.pure_equality(&wanted) {
                    return self.count_and_continue(r);
                }
                let entries = self.collection_elements(receiver)?;
                for pair in entries.chunks(2) {
                    let Some(stored) = pair.get(1) else { continue };
                    if *stored == wanted
                        || (wanted != Value::Ref(None) && self.java_equals(&wanted, stored)?)
                    {
                        return Ok(boolean(true));
                    }
                }
                Ok(boolean(false))
            }
            (_, "clear", []) => {
                self.host.replace_mutable_collection(receiver, Vec::new());
                table.clear();
                Ok(Some(Value::Void))
            }
            (true, "putAll", [source]) => {
                let source = source.clone();
                if self.host_collection_kind(&source).is_none() {
                    return Ok(None);
                }
                self.put_all(receiver, table, &source)?;
                Ok(Some(Value::Void))
            }
            (false, "addAll", [source]) => {
                let source = source.clone();
                if self.host_collection_kind(&source).is_none() {
                    return Ok(None);
                }
                Ok(boolean(self.add_all(receiver, table, &source)?))
            }
            (_, "hashCode", []) => {
                if unknown {
                    return self.count_and_continue(r);
                }
                match self.java_hash(receiver)? {
                    Some(hash) => Ok(Some(Value::Int(hash))),
                    None => self.count_and_continue(r),
                }
            }
            // `containsAll`/`removeAll`/`retainAll`/`equals` (`hash_bulk.rs`).
            _ => self.hash_bulk_member(r, receiver, kind, table, args),
        }
    }

    /// Replace the value stored under the stored key `stored`, answering the
    /// previous one.
    pub(crate) fn set_stored_value(
        &mut self,
        receiver: &Value,
        stored: &Value,
        value: Value,
    ) -> Result<Value, NativeError> {
        let mut entries = self.collection_elements(receiver)?;
        let marker = collection_item_marker(stored);
        let Some(index) = entries
            .chunks(2)
            .position(|pair| collection_item_marker(&pair[0]) == marker)
        else {
            return Ok(Value::Ref(None));
        };
        let old = std::mem::replace(&mut entries[index * 2 + 1], value);
        self.host.replace_mutable_collection(receiver, entries);
        Ok(old)
    }
}

impl<H: Host> Runtime<'_, H> {
    /// The keys of a hash collection, sorted: what a structural change moves.
    fn key_signature(&self, value: &Value, kind: TableKind) -> Result<Vec<String>, NativeError> {
        let step = if kind.map { 2 } else { 1 };
        let mut keys: Vec<String> = runtime_collection_values(self.host, value)?
            .into_iter()
            .step_by(step)
            .collect();
        keys.sort();
        Ok(keys)
    }

    /// `modCount != expectedModCount` for a hash collection (`mod_count.rs`);
    /// a key set that moved without it (a path the counter does not see)
    /// is a change too.
    fn hash_verdict(
        &self,
        receiver: &Value,
        marker: &str,
        kind: TableKind,
        expected: (ModStamp, &[String]),
    ) -> Result<ModVerdict, NativeError> {
        let verdict = self.mod_stamp(marker).verdict(expected.0);
        if verdict == ModVerdict::Same && self.key_signature(receiver, kind)? != expected.1 {
            return Ok(ModVerdict::Changed);
        }
        Ok(verdict)
    }

    /// `Collection.removeIf` on a `HashSet`/`LinkedHashSet` (neither
    /// overrides it): the iterator walks the set, and `it.remove()` runs
    /// `removeNode(hash, key)`, which searches the bin again, calling
    /// `key.equals(k)` on every earlier node of equal hash. A predicate that
    /// changes the set makes the next `next()` (or `remove()`) throw
    /// `ConcurrentModificationException`.
    fn hash_remove_if(
        &mut self,
        receiver: &Value,
        marker: &str,
        kind: TableKind,
        predicate: &Value,
    ) -> Result<Value, NativeError> {
        if *predicate == Value::Ref(None) {
            return Err(NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: None,
                frame: None,
            });
        }
        let elements = self.collection_elements(receiver)?;
        let mut expected = self.mod_stamp(marker);
        let mut signature = self.key_signature(receiver, kind)?;
        let mut removed = false;
        let counted = MethodRef {
            owner: "java/util/Collection".into(),
            name: "removeIf".into(),
            descriptor: "(Ljava/util/function/Predicate;)Z".into(),
        };
        for (index, element) in elements.iter().enumerate() {
            // `each.next()` (after the first) checks `modCount`.
            if index > 0 {
                match self.hash_verdict(receiver, marker, kind, (expected, &signature))? {
                    ModVerdict::Same => {}
                    ModVerdict::Changed => return Err(concurrent_modification()),
                    ModVerdict::Unknown => {
                        self.host.unknown_operand_call(&counted)?;
                        return Ok(Value::Int(i32::from(removed)));
                    }
                }
            }
            if !self.predicate_holds(predicate, element)? {
                continue;
            }
            // `each.remove()`: the check, then the re-walk.
            self.resync_hash_table(receiver)?;
            match self.hash_verdict(receiver, marker, kind, (expected, &signature))? {
                ModVerdict::Same => {}
                ModVerdict::Changed => return Err(concurrent_modification()),
                ModVerdict::Unknown => {
                    self.host.unknown_operand_call(&counted)?;
                    return Ok(Value::Int(i32::from(removed)));
                }
            }
            let mut table = self.take_table(marker, kind);
            let item = collection_item_marker(element);
            let walked = match table.stored_hash(&item) {
                Some(stored) => {
                    self.table_lookup(&table, element, Some(hash_table::spread(stored)))
                }
                None => Ok(Lookup::Found(element.clone())),
            };
            let node = match walked {
                Ok(Lookup::Found(node)) => collection_item_marker(&node),
                Ok(_) => item.clone(),
                Err(error) => {
                    self.host.store_hash_table(marker, table);
                    return Err(error);
                }
            };
            table.remove(&node);
            self.host.store_hash_table(marker, table);
            let kept: Vec<Value> = self
                .collection_elements(receiver)?
                .into_iter()
                .filter(|value| collection_item_marker(value) != node)
                .collect();
            self.host.replace_mutable_collection(receiver, kept);
            // `removeNode` bumps `modCount`; the iterator takes it.
            self.bump_mod_count(marker, false);
            expected = self.mod_stamp(marker);
            signature = self.key_signature(receiver, kind)?;
            removed = true;
        }
        Ok(Value::Int(i32::from(removed)))
    }

    /// `HashMap.forEach`/`HashSet.forEach` (and the linked ones): every
    /// entry in order, then `ConcurrentModificationException` if a callback
    /// changed the collection. The JDK keeps walking the live table after a
    /// change, which this does not model: a change is counted.
    fn hash_for_each(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        kind: TableKind,
        callback: &Value,
    ) -> Result<Value, NativeError> {
        let entries = self.collection_elements(receiver)?;
        let marker = expect_ref(receiver).cloned().unwrap_or_default();
        let before = (self.mod_stamp(&marker), self.key_signature(receiver, kind)?);
        let step = if kind.map { 2 } else { 1 };
        let mut counted = false;
        for chunk in entries.chunks(step) {
            if kind.map {
                let [key, value] = chunk else { continue };
                self.run_functional(
                    (
                        "java/util/function/BiConsumer",
                        "accept",
                        "(Ljava/lang/Object;Ljava/lang/Object;)V",
                    ),
                    callback,
                    &[key.clone(), value.clone()],
                )?;
            } else {
                self.run_functional(
                    (
                        "java/util/function/Consumer",
                        "accept",
                        "(Ljava/lang/Object;)V",
                    ),
                    callback,
                    &[chunk[0].clone()],
                )?;
            }
            // The JDK walks the live table (or linked list) after a change,
            // even after the last entry (a key added behind it is visited):
            // not modelled, so any change is counted.
            if !counted
                && self.hash_verdict(receiver, &marker, kind, (before.0, &before.1))?
                    != ModVerdict::Same
            {
                self.host.unknown_operand_call(r)?;
                counted = true;
            }
        }
        match self.hash_verdict(receiver, &marker, kind, (before.0, &before.1))? {
            ModVerdict::Same => Ok(Value::Void),
            ModVerdict::Changed => Err(concurrent_modification()),
            // Counted above.
            ModVerdict::Unknown => Ok(Value::Void),
        }
    }
}

fn concurrent_modification() -> NativeError {
    NativeError::UncaughtException {
        class: "java/util/ConcurrentModificationException".into(),
        message: None,
        frame: None,
    }
}
