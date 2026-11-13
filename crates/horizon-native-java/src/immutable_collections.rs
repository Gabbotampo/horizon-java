//! `Set.of`/`Map.of`/`Map.ofEntries` and `IdentityHashMap`, as JDK 25 runs
//! them: what is exact and what no run can reproduce.
//!
//! `Set.of`, `Map.of` and `Map.ofEntries` build `ImmutableCollections`'
//! `Set12`/`SetN`/`Map1`/`MapN`. Their duplicate checks and lookups are
//! deterministic: `Set12` compares `e0.equals(e1)`, and `SetN`/`MapN` place
//! each element by `probe`, linear probing from `floorMod(hashCode, buckets)`
//! over twice as many buckets as elements, calling `equals` on each occupied
//! slot. The host found keys by marker, so `Set.of(new Key(1), new Key(1))`
//! built a two-element set instead of throwing "duplicate element:
//! Key[id=1]". This module runs those bodies and keeps each set's probe
//! layout beside it, so `contains`/`get`/`containsKey`/`containsValue`/
//! `hashCode`/`equals` call the elements' `hashCode` and `equals` as the JDK
//! does.
//!
//! Their **iteration order** is not reproducible: `SetN`/`MapN` iterators
//! start at a slot chosen from `SALT32L`, and `Set12`/`MapN` reverse when
//! `REVERSE`, both random per JVM run. So a call that shows the order of a
//! `Set.of`/`Map.of` with two or more elements (`iterator`, `forEach`,
//! `toString`, `stream`, passing it to a host model) is counted.
//!
//! The varargs factories (`List.of(E...)`, `Set.of(E...)`,
//! `Map.ofEntries(Entry...)`) used to keep the varargs array as one element;
//! their elements are now the array's. `Set.copyOf`/`Map.copyOf` of jar
//! elements (a `HashSet` copy, then `Set.of`) are counted.
//!
//! An `IdentityHashMap` compares keys with `==`. A jar object's identity is
//! its marker, but two equal strings or boxes may or may not be the same
//! object, so a lookup by a string or box whose equal value is a key is
//! counted, as is the map's iteration (identity hashes).

use super::*;
use object_methods::Equality;

const SET_N: &str = "java/util/ImmutableCollections$SetN";
const MAP_N: &str = "java/util/ImmutableCollections$MapN";
const IDENTITY_MAP: &str = "java/util/IdentityHashMap";

/// A `SetN`/`MapN` probe table: each bucket's element (a map's key), by its
/// item marker. `unknown`: an element with an identity hash sits at a slot
/// no run can name.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ProbeTable {
    slots: Vec<Option<String>>,
    unknown: bool,
}

/// Where `probe` ends.
enum Probe {
    Found(usize),
    Empty(usize),
    Unknown,
}

/// The JVM's helpful message for `o.equals(..)` on a null `o`
/// (`Set12.contains`, `Map1.get`/`containsKey`/`containsValue`).
fn null_receiver() -> NativeError {
    thrown(
        "java/lang/NullPointerException",
        Some("Cannot invoke \"Object.equals(Object)\" because \"o\" is null".into()),
    )
}

fn thrown(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message,
        frame: None,
    }
}

impl<H: Host> Runtime<'_, H> {
    /// `probe(pe)`: `floorMod(pe.hashCode(), buckets)`, then `pe.equals(ee)`
    /// slot by slot until an empty slot.
    fn probe(&mut self, table: &ProbeTable, key: &Value) -> Result<Probe, NativeError> {
        let hash = self.java_hash(key)?;
        let buckets = table.slots.len();
        let Some(hash) = hash.filter(|_| !table.unknown) else {
            // Without a position, only an answer no probe order can change:
            // a pure key's membership, or a key that keeps `Object.equals`.
            let pure = self.pure_equality(key);
            let identity = matches!(
                self.equality_of(key),
                Equality::Identity | Equality::Platform
            );
            if !pure && !identity {
                return Ok(Probe::Unknown);
            }
            for (slot, stored) in table.slots.iter().enumerate() {
                let Some(stored) = stored else { continue };
                let stored = parse_collection_value(stored)?;
                if stored == *key || (pure && self.java_equals(key, &stored)?) {
                    return Ok(Probe::Found(slot));
                }
            }
            return Ok(Probe::Unknown);
        };
        if buckets == 0 {
            return Ok(Probe::Empty(0));
        }
        let mut index = i64::from(hash).rem_euclid(buckets as i64) as usize;
        loop {
            let Some(stored) = &table.slots[index] else {
                return Ok(Probe::Empty(index));
            };
            let stored = parse_collection_value(stored)?;
            if self.java_equals(key, &stored)? {
                return Ok(Probe::Found(index));
            }
            index = (index + 1) % buckets;
        }
    }

    /// Lay out `keys` as `SetN(input)`/`MapN(input)` does, throwing on a
    /// duplicate (`duplicate element: e` / `duplicate key: k`).
    fn lay_out(&mut self, keys: &[Value], what: &str) -> Result<ProbeTable, NativeError> {
        let mut table = ProbeTable {
            slots: vec![None; keys.len() * 2],
            unknown: false,
        };
        for key in keys {
            if *key == Value::Ref(None) {
                return Err(thrown("java/lang/NullPointerException", None));
            }
            match self.probe(&table, key)? {
                Probe::Found(_) => {
                    let text = self.value_of_text(key, "Ljava/lang/Object;", "", "of")?;
                    return Err(thrown(
                        "java/lang/IllegalArgumentException",
                        Some(format!("duplicate {what}: {text}")),
                    ));
                }
                Probe::Empty(slot) => table.slots[slot] = Some(collection_item_marker(key)),
                Probe::Unknown => {
                    // An identity-hashed element: kept, at a slot no run can
                    // name.
                    table.unknown = true;
                    if let Some(free) = table.slots.iter().position(Option::is_none) {
                        table.slots[free] = Some(collection_item_marker(key));
                    }
                }
            }
        }
        Ok(table)
    }

    /// The host's unmodifiable `kind` (`List`/`Set`/`Map`) over `values`,
    /// through its fixed-arity factory for any count.
    fn host_unmodifiable(&mut self, kind: &str, values: &[Value]) -> Result<Value, NativeError> {
        let descriptor = format!(
            "({})Ljava/util/{kind};",
            "Ljava/lang/Object;".repeat(values.len())
        );
        self.host.invoke(
            &MethodRef {
                owner: format!("java/util/{kind}"),
                name: "of".into(),
                descriptor,
            },
            values,
        )
    }

    /// `Set.of(elements)`: `Set12` for one or two, `SetN` otherwise.
    pub(crate) fn set_of(&mut self, elements: &[Value]) -> Result<Value, NativeError> {
        match elements {
            [] => {}
            [one] if *one == Value::Ref(None) => {
                return Err(thrown("java/lang/NullPointerException", None));
            }
            [_] => {}
            [first, second] => {
                // `e0.equals(Objects.requireNonNull(e1))`.
                if *second == Value::Ref(None) || *first == Value::Ref(None) {
                    return Err(thrown("java/lang/NullPointerException", None));
                }
                if self.java_equals(first, second)? {
                    let text = self.value_of_text(first, "Ljava/lang/Object;", "", "of")?;
                    return Err(thrown(
                        "java/lang/IllegalArgumentException",
                        Some(format!("duplicate element: {text}")),
                    ));
                }
            }
            _ => {}
        }
        let table = if elements.len() > 2 {
            Some(self.lay_out(elements, "element")?)
        } else {
            None
        };
        let set = self.host_unmodifiable("Set", elements)?;
        if let Some(marker) = expect_ref(&set) {
            self.host
                .store_probe_table(marker, table.unwrap_or_default());
        }
        Ok(set)
    }

    /// `Map.of(k0, v0, ...)`/`Map.ofEntries`: `Map1` for one pair, `MapN`
    /// otherwise (keys and values must not be null).
    fn map_of(&mut self, pairs: &[Value]) -> Result<Value, NativeError> {
        if pairs.contains(&Value::Ref(None)) {
            return Err(thrown("java/lang/NullPointerException", None));
        }
        let keys: Vec<Value> = pairs.iter().step_by(2).cloned().collect();
        let table = if keys.len() > 1 {
            Some(self.lay_out(&keys, "key")?)
        } else {
            None
        };
        let map = self.host_unmodifiable("Map", pairs)?;
        if let Some(marker) = expect_ref(&map) {
            self.host
                .store_probe_table(marker, table.unwrap_or_default());
        }
        Ok(map)
    }

    /// The factories: `List.of`/`Set.of`/`Map.of` (fixed and varargs),
    /// `Map.ofEntries`, and the counted `Set.copyOf`/`Map.copyOf` of jar
    /// elements.
    fn immutable_factory(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let varargs = r.descriptor.starts_with("([");
        let array = |runtime: &Self| -> Option<Vec<Value>> {
            let array = args.first()?;
            if *array == Value::Ref(None) {
                return None;
            }
            runtime.array(array).ok().cloned()
        };
        match (r.owner.as_str(), r.name.as_str()) {
            ("java/util/List", "of") if varargs => {
                let Some(elements) = array(self) else {
                    return Ok(None);
                };
                if elements.contains(&Value::Ref(None)) {
                    return Err(thrown("java/lang/NullPointerException", None));
                }
                self.host_unmodifiable("List", &elements).map(Some)
            }
            ("java/util/Set", "of") => {
                let elements = if varargs {
                    let Some(elements) = array(self) else {
                        return Ok(None);
                    };
                    elements
                } else {
                    args.to_vec()
                };
                self.set_of(&elements).map(Some)
            }
            ("java/util/Map", "of") => self.map_of(args).map(Some),
            ("java/util/Map", "ofEntries") => {
                let Some(entries) = array(self) else {
                    return Ok(None);
                };
                let mut pairs = Vec::with_capacity(entries.len() * 2);
                for entry in &entries {
                    if *entry == Value::Ref(None) {
                        return Err(thrown("java/lang/NullPointerException", None));
                    }
                    let Ok((key, value)) = map_entry_parts(entry) else {
                        return Ok(None);
                    };
                    pairs.extend([key, value]);
                }
                self.map_of(&pairs).map(Some)
            }
            ("java/util/Set" | "java/util/Map", "copyOf") => {
                let Some(source) = args.first() else {
                    return Ok(None);
                };
                let elements = self.collection_elements(source).unwrap_or_default();
                let jar = elements.iter().any(|value| {
                    matches!(
                        self.equality_of(value),
                        Equality::Jar {
                            equals: Some(_),
                            ..
                        }
                    )
                });
                if jar {
                    return self.host.unknown_operand_call(r).map(|_| None);
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Whether `value` is a `Set.of`/`Map.of` of two or more elements, whose
    /// iteration order is random per JVM run.
    pub(crate) fn random_order(&mut self, value: &Value) -> bool {
        let Some(marker) = expect_ref(value).cloned() else {
            return false;
        };
        let Some((map, table)) = self.probed_kind(value) else {
            return false;
        };
        self.host.store_probe_table(&marker, table);
        let size = self
            .collection_elements(value)
            .map_or(0, |entries| entries.len());
        size / if map { 2 } else { 1 } >= 2
    }

    /// The class of a `Set.of`/`Map.of` this module laid out.
    fn probed_kind(&mut self, value: &Value) -> Option<(bool, ProbeTable)> {
        let marker = expect_ref(value)?;
        let map = if marker.starts_with(&format!("{SET_N}#")) {
            false
        } else if marker.starts_with(&format!("{MAP_N}#")) {
            true
        } else {
            return None;
        };
        let table = self.host.take_probe_table(marker)?;
        Some((map, table))
    }

    /// `Set.of`/`Map.of`/`Map.ofEntries`/`List.of`, `IdentityHashMap`, and the
    /// members of a laid-out `Set.of`/`Map.of`. `Ok(None)` leaves the call to
    /// the rest of the dispatch.
    pub(crate) fn invoke_immutable_collection(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if opcode == 0xb8 {
            if let Some(answer) = self.immutable_factory(r, args)? {
                return Ok(Some(answer));
            }
            return self.unordered_argument(r, args);
        }
        let Some(receiver) = receiver.filter(|_| matches!(opcode, 0xb6 | 0xb9)) else {
            return self.unordered_argument(r, args);
        };
        if self.host_collection_class(receiver).as_deref() == Some(IDENTITY_MAP) {
            return self.identity_map_member(r, receiver, args);
        }
        let Some(marker) = expect_ref(receiver).cloned() else {
            return Ok(None);
        };
        let Some((map, table)) = self.probed_kind(receiver) else {
            return self.unordered_argument(r, args);
        };
        let result = self.probed_member(r, receiver, map, &table, args);
        self.host.store_probe_table(&marker, table);
        result
    }

    /// A `Set.of`/`Map.of` of two or more elements passed to a host model
    /// that may show its order: counted.
    fn unordered_argument(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if !self.shows_argument_order(r) {
            return Ok(None);
        }
        for arg in args {
            let Some(marker) = expect_ref(arg).cloned() else {
                continue;
            };
            let _ = &marker;
            if self.random_order(arg) {
                return self.host.unknown_operand_call(r).map(|_| None);
            }
        }
        Ok(None)
    }

    /// The members of a `Set.of`/`Map.of`.
    fn probed_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        map: bool,
        table: &ProbeTable,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let entries = self.collection_elements(receiver)?;
        let size = if map {
            entries.len() / 2
        } else {
            entries.len()
        };
        let boolean = |flag: bool| Some(Value::Int(i32::from(flag)));
        let value_at = |entries: &[Value], key: &Value| -> Value {
            let marker = collection_item_marker(key);
            entries
                .chunks(2)
                .find(|pair| collection_item_marker(&pair[0]) == marker)
                .and_then(|pair| pair.get(1).cloned())
                .unwrap_or(Value::Ref(None))
        };
        // A key's slot: `Set12`/`Map1` compare in order; `SetN`/`MapN` probe.
        let find =
            |runtime: &mut Self, key: &Value| -> Result<Option<Option<Value>>, NativeError> {
                let step = if map { 2 } else { 1 };
                let stored: Vec<Value> = entries.iter().step_by(step).cloned().collect();
                if *key == Value::Ref(None) {
                    // `SetN`/`MapN`: `Objects.requireNonNull(o)`; `Set12`/`Map1`:
                    // `o.equals(..)`.
                    return Err(if table.slots.is_empty() {
                        null_receiver()
                    } else {
                        thrown("java/lang/NullPointerException", None)
                    });
                }
                if !table.slots.is_empty() {
                    return Ok(match runtime.probe(table, key)? {
                        Probe::Found(slot) => match &table.slots[slot] {
                            Some(marker) => Some(Some(parse_collection_value(marker)?)),
                            None => Some(None),
                        },
                        Probe::Empty(_) => Some(None),
                        Probe::Unknown => None,
                    });
                }
                // `Set12.contains`: `o.equals(e0) || e1.equals(o)`; `Map1`:
                // `o.equals(k0)`.
                if let Some(first) = stored.first()
                    && runtime.java_equals(key, first)?
                {
                    return Ok(Some(Some(first.clone())));
                }
                if let Some(second) = stored.get(1)
                    && runtime.java_equals(second, key)?
                {
                    return Ok(Some(Some(second.clone())));
                }
                Ok(Some(None))
            };
        match (map, r.name.as_str(), r.descriptor.as_str(), args) {
            (false, "contains", "(Ljava/lang/Object;)Z", [key])
            | (true, "containsKey", "(Ljava/lang/Object;)Z", [key]) => {
                if size == 0 {
                    if *key == Value::Ref(None) {
                        return Err(thrown("java/lang/NullPointerException", None));
                    }
                    return Ok(boolean(false));
                }
                match find(self, key)? {
                    Some(found) => Ok(boolean(found.is_some())),
                    None => self.host.unknown_operand_call(r).map(|_| None),
                }
            }
            (true, "get", "(Ljava/lang/Object;)Ljava/lang/Object;", [key])
            | (true, "getOrDefault", _, [key, _]) => {
                let default = args.get(1).cloned().unwrap_or(Value::Ref(None));
                if size == 0 {
                    if *key == Value::Ref(None) {
                        return Err(thrown("java/lang/NullPointerException", None));
                    }
                    return Ok(Some(default));
                }
                match find(self, key)? {
                    Some(Some(stored)) => Ok(Some(value_at(&entries, &stored))),
                    Some(None) => Ok(Some(default)),
                    None => self.host.unknown_operand_call(r).map(|_| None),
                }
            }
            (_, "hashCode", "()I", []) => {
                // `Set12`/`Map1`: in element order; `SetN`/`MapN`: slot order.
                let mut keys: Vec<Value> = if table.slots.is_empty() || table.unknown {
                    entries
                        .iter()
                        .step_by(if map { 2 } else { 1 })
                        .cloned()
                        .collect()
                } else {
                    table
                        .slots
                        .iter()
                        .flatten()
                        .map(|marker| parse_collection_value(marker))
                        .collect::<Result<_, _>>()?
                };
                if table.unknown && keys.iter().any(|key| !self.pure_equality(key)) {
                    return self.host.unknown_operand_call(r).map(|_| None);
                }
                let mut hash = 0_i32;
                for key in keys.drain(..) {
                    let Some(part) = self.java_hash(&key)? else {
                        return self.host.unknown_operand_call(r).map(|_| None);
                    };
                    let part = if map {
                        let Some(value) = self.java_hash(&value_at(&entries, &key))? else {
                            return self.host.unknown_operand_call(r).map(|_| None);
                        };
                        part ^ value
                    } else {
                        part
                    };
                    hash = hash.wrapping_add(part);
                }
                Ok(Some(Value::Int(hash)))
            }
            (false, "equals", "(Ljava/lang/Object;)Z", [other]) => {
                // `AbstractImmutableSet.equals`: `contains(e)` for each of the
                // other set's elements, in its order.
                if other == receiver {
                    return Ok(boolean(true));
                }
                if self.host_collection_kind(other) != Some(object_methods::CollectionKind::Set) {
                    return Ok(None);
                }
                let theirs = self.collection_elements(other)?;
                if theirs.len() != size {
                    return Ok(boolean(false));
                }
                for element in &theirs {
                    if *element == Value::Ref(None) {
                        return Ok(boolean(false));
                    }
                    match find(self, element)? {
                        Some(Some(_)) => {}
                        Some(None) => return Ok(boolean(false)),
                        None => return self.host.unknown_operand_call(r).map(|_| None),
                    }
                }
                Ok(boolean(true))
            }
            (false, "containsAll", "(Ljava/util/Collection;)Z", [other]) => {
                // `AbstractCollection.containsAll`: `contains(e)` for each of
                // the other's elements, in its order.
                if self.host_collection_kind(other).is_none() || self.random_order(other) {
                    return Ok(None);
                }
                for element in self.collection_elements(other)? {
                    if size == 0 {
                        if element == Value::Ref(None) {
                            return Err(thrown("java/lang/NullPointerException", None));
                        }
                        return Ok(boolean(false));
                    }
                    match find(self, &element)? {
                        Some(Some(_)) => {}
                        Some(None) => return Ok(boolean(false)),
                        None => return self.host.unknown_operand_call(r).map(|_| None),
                    }
                }
                Ok(boolean(true))
            }
            (true, "containsValue", "(Ljava/lang/Object;)Z", [wanted]) => {
                // `Map1`: `o.equals(v0)`; `MapN`: `requireNonNull(o)`, then
                // `o.equals(v)` in slot order.
                if *wanted == Value::Ref(None) {
                    return Err(if table.slots.is_empty() && size == 1 {
                        null_receiver()
                    } else {
                        thrown("java/lang/NullPointerException", None)
                    });
                }
                let keys: Vec<Value> = if table.slots.is_empty() {
                    entries.iter().step_by(2).cloned().collect()
                } else {
                    if table.unknown && !self.pure_equality(wanted) {
                        return self.host.unknown_operand_call(r).map(|_| None);
                    }
                    table
                        .slots
                        .iter()
                        .flatten()
                        .map(|marker| parse_collection_value(marker))
                        .collect::<Result<_, _>>()?
                };
                for key in &keys {
                    let stored = value_at(&entries, key);
                    if self.java_equals(wanted, &stored)? {
                        return Ok(boolean(true));
                    }
                }
                Ok(boolean(false))
            }
            (_, "size" | "isEmpty", _, []) => Ok(None),
            // Anything else may show the order: random for two or more.
            _ if size >= 2 => self.host.unknown_operand_call(r).map(|_| None),
            _ => Ok(None),
        }
    }

    /// `IdentityHashMap`: a lookup by a string or box whose equal value is a
    /// key, and the iteration of two or more keys, are counted.
    fn identity_map_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let entries = self.collection_elements(receiver)?;
        let keyed = matches!(
            r.name.as_str(),
            "get"
                | "containsKey"
                | "put"
                | "remove"
                | "putIfAbsent"
                | "getOrDefault"
                | "replace"
                | "computeIfAbsent"
                | "computeIfPresent"
                | "compute"
                | "merge"
        );
        if keyed {
            let Some(key) = args.first() else {
                return Ok(None);
            };
            let ambiguous = self.equality_of(key) == Equality::Platform
                && entries.iter().step_by(2).any(|stored| stored == key);
            if ambiguous {
                return self.host.unknown_operand_call(r).map(|_| None);
            }
            return Ok(None);
        }
        if r.name == "containsValue" {
            let Some(value) = args.first() else {
                return Ok(None);
            };
            let ambiguous = self.equality_of(value) == Equality::Platform
                && entries
                    .iter()
                    .skip(1)
                    .step_by(2)
                    .any(|stored| stored == value);
            if ambiguous {
                return self.host.unknown_operand_call(r).map(|_| None);
            }
            return Ok(None);
        }
        if matches!(r.name.as_str(), "size" | "isEmpty" | "clear" | "<init>") || entries.len() < 4 {
            return Ok(None);
        }
        self.host.unknown_operand_call(r).map(|_| None)
    }
}
