//! `AbstractMap` (with its `keySet()`/`values()` views),
//! `AbstractMap.SimpleEntry`/`SimpleImmutableEntry` and the `Map` defaults,
//! as JDK 25 runs them.
//!
//! An `AbstractMap` subclass keeps its mappings behind `entrySet()`; every
//! inherited member walks that set's iterator and asks each entry for its
//! key and value, in the order below. The views are the JDK's private
//! classes (`AbstractMap$1` for `keySet()`, `AbstractMap$2` for `values()`,
//! with `KeyIterator`/`ValueIterator`), cached in the map's `keySet`/`values`
//! fields as the JDK caches them. `SimpleEntry`'s key and value are its own
//! fields, so a jar subclass that extends it (Guava's `ImmutableMapEntry`
//! did, through `SimpleImmutableEntry`) keeps them here.

use super::*;

/// `java.util.AbstractMap`.
pub(crate) const ABSTRACT_MAP: &str = "java/util/AbstractMap";
/// `java.util.AbstractMap.SimpleEntry`.
pub(crate) const SIMPLE_ENTRY: &str = "java/util/AbstractMap$SimpleEntry";
/// `java.util.AbstractMap.SimpleImmutableEntry`.
pub(crate) const IMMUTABLE_ENTRY: &str = "java/util/AbstractMap$SimpleImmutableEntry";
/// `keySet()`'s view class.
pub(super) const KEY_SET: &str = "java/util/AbstractMap$1";
/// `values()`'s view class.
pub(super) const VALUES: &str = "java/util/AbstractMap$2";
/// The key set's iterator.
pub(super) const KEY_ITERATOR: &str = "java/util/AbstractMap$KeyIterator";
/// The values' iterator.
pub(super) const VALUE_ITERATOR: &str = "java/util/AbstractMap$ValueIterator";
/// `java.util.Map`.
const MAP: &str = "java/util/Map";
/// `java.util.Map.Entry`.
const ENTRY: &str = "java/util/Map$Entry";

/// The members this file implements on the skeletons and `Map`.
pub(super) const BODIES: &[(&str, &str, &str)] = &[
    (ABSTRACT_MAP, "size", "()I"),
    (ABSTRACT_MAP, "isEmpty", "()Z"),
    (ABSTRACT_MAP, "containsValue", "(Ljava/lang/Object;)Z"),
    (ABSTRACT_MAP, "containsKey", "(Ljava/lang/Object;)Z"),
    (
        ABSTRACT_MAP,
        "get",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
    ),
    (
        ABSTRACT_MAP,
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
    ),
    (
        ABSTRACT_MAP,
        "remove",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
    ),
    (ABSTRACT_MAP, "putAll", "(Ljava/util/Map;)V"),
    (ABSTRACT_MAP, "clear", "()V"),
    (ABSTRACT_MAP, "keySet", "()Ljava/util/Set;"),
    (ABSTRACT_MAP, "values", "()Ljava/util/Collection;"),
    (ABSTRACT_MAP, "equals", "(Ljava/lang/Object;)Z"),
    (ABSTRACT_MAP, "hashCode", "()I"),
    (ABSTRACT_MAP, "toString", "()Ljava/lang/String;"),
    (SIMPLE_ENTRY, "getKey", "()Ljava/lang/Object;"),
    (SIMPLE_ENTRY, "getValue", "()Ljava/lang/Object;"),
    (
        SIMPLE_ENTRY,
        "setValue",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
    ),
    (SIMPLE_ENTRY, "equals", "(Ljava/lang/Object;)Z"),
    (SIMPLE_ENTRY, "hashCode", "()I"),
    (SIMPLE_ENTRY, "toString", "()Ljava/lang/String;"),
    (IMMUTABLE_ENTRY, "getKey", "()Ljava/lang/Object;"),
    (IMMUTABLE_ENTRY, "getValue", "()Ljava/lang/Object;"),
    (
        IMMUTABLE_ENTRY,
        "setValue",
        "(Ljava/lang/Object;)Ljava/lang/Object;",
    ),
    (IMMUTABLE_ENTRY, "equals", "(Ljava/lang/Object;)Z"),
    (IMMUTABLE_ENTRY, "hashCode", "()I"),
    (IMMUTABLE_ENTRY, "toString", "()Ljava/lang/String;"),
    (
        MAP,
        "getOrDefault",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
    ),
    (MAP, "forEach", "(Ljava/util/function/BiConsumer;)V"),
    (MAP, "replaceAll", "(Ljava/util/function/BiFunction;)V"),
    (
        MAP,
        "putIfAbsent",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
    ),
    (MAP, "remove", "(Ljava/lang/Object;Ljava/lang/Object;)Z"),
    (
        MAP,
        "replace",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
    ),
    (
        MAP,
        "replace",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
    ),
    (
        MAP,
        "computeIfAbsent",
        "(Ljava/lang/Object;Ljava/util/function/Function;)Ljava/lang/Object;",
    ),
    (
        MAP,
        "computeIfPresent",
        "(Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
    ),
    (
        MAP,
        "compute",
        "(Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
    ),
    (
        MAP,
        "merge",
        "(Ljava/lang/Object;Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
    ),
];

/// The members the view classes declare themselves.
pub(super) const OWN_MEMBERS: &[(&str, &str, &str)] = &[
    (KEY_SET, "iterator", "()Ljava/util/Iterator;"),
    (KEY_SET, "size", "()I"),
    (KEY_SET, "isEmpty", "()Z"),
    (KEY_SET, "clear", "()V"),
    (KEY_SET, "contains", "(Ljava/lang/Object;)Z"),
    (VALUES, "iterator", "()Ljava/util/Iterator;"),
    (VALUES, "size", "()I"),
    (VALUES, "isEmpty", "()Z"),
    (VALUES, "clear", "()V"),
    (VALUES, "contains", "(Ljava/lang/Object;)Z"),
    (KEY_ITERATOR, "hasNext", "()Z"),
    (KEY_ITERATOR, "next", "()Ljava/lang/Object;"),
    (KEY_ITERATOR, "remove", "()V"),
    (VALUE_ITERATOR, "hasNext", "()Z"),
    (VALUE_ITERATOR, "next", "()Ljava/lang/Object;"),
    (VALUE_ITERATOR, "remove", "()V"),
];

/// Whether `owner descriptor` is an entry constructor (`(K, V)` or
/// `(Map.Entry)`).
pub(super) fn is_entry_constructor(owner: &str, descriptor: &str) -> bool {
    [SIMPLE_ENTRY, IMMUTABLE_ENTRY].contains(&owner)
        && matches!(
            descriptor,
            "(Ljava/lang/Object;Ljava/lang/Object;)V" | "(Ljava/util/Map$Entry;)V"
        )
}

/// A view's or view iterator's `this$0`: the map it reads.
fn outer_field(class: &str) -> (&str, &str, &str) {
    (class, "this$0", "Ljava/util/AbstractMap;")
}

/// The field `name` of `owner` holding an object reference.
fn object_field<'a>(owner: &'a str, name: &'a str) -> (&'a str, &'a str, &'a str) {
    (owner, name, "Ljava/lang/Object;")
}

impl<H: Host> Runtime<'_, H> {
    /// `this.entrySet().iterator()`, both calls as the JDK makes them.
    fn entry_iterator(&mut self, map: &Value) -> Result<Value, NativeError> {
        let entries = self.call_virtual(
            Site::Virtual,
            ABSTRACT_MAP,
            "entrySet",
            "()Ljava/util/Set;",
            map,
            &[],
        )?;
        self.call_virtual(
            Site::Interface,
            "java/util/Set",
            "iterator",
            "()Ljava/util/Iterator;",
            &entries,
            &[],
        )
    }

    /// `entry.getKey()` / `entry.getValue()` (`invokeinterface Map.Entry`).
    fn entry_part(&mut self, entry: &Value, name: &str) -> Result<Value, NativeError> {
        self.call_virtual(
            Site::Interface,
            ENTRY,
            name,
            "()Ljava/lang/Object;",
            entry,
            &[],
        )
    }

    /// `Objects.equals(a, b)` with the JDK's `eq` shape: `a == null ? b ==
    /// null : a.equals(b)` (identity short-circuits inside `equals`).
    fn objects_equal(&mut self, a: &Value, b: &Value) -> Result<bool, NativeError> {
        if a == b {
            return Ok(true);
        }
        if *a == Value::Ref(None) {
            return Ok(false);
        }
        self.java_equals(a, b)
    }

    /// `AbstractMap.SimpleEntry.eq(a, b)`: no identity short-circuit.
    fn entry_eq(&mut self, a: &Value, b: &Value) -> Result<bool, NativeError> {
        if *a == Value::Ref(None) {
            return Ok(*b == Value::Ref(None));
        }
        self.java_equals(a, b)
    }

    /// Run a SimpleEntry constructor on a jar subclass object.
    pub(super) fn entry_init(
        &mut self,
        owner: &str,
        this: &Value,
        args: &[Value],
    ) -> Result<(), NativeError> {
        let (key, value) = if args.len() == 2 {
            (args[0].clone(), args[1].clone())
        } else {
            let entry = args.first().cloned().unwrap_or(Value::Ref(None));
            let key = self.entry_part(&entry, "getKey")?;
            (key, self.entry_part(&entry, "getValue")?)
        };
        self.put_jdk_field(this, object_field(owner, "key"), key);
        self.put_jdk_field(this, object_field(owner, "value"), value);
        Ok(())
    }

    /// An `AbstractMap` or entry member, or a `Map` default.
    pub(super) fn map_body(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Ref(None));
        let own = Site::Virtual;
        match (owner, name) {
            (ABSTRACT_MAP, "size") => {
                let entries = self.call_virtual(
                    own,
                    ABSTRACT_MAP,
                    "entrySet",
                    "()Ljava/util/Set;",
                    this,
                    &[],
                )?;
                self.call_virtual(
                    Site::Interface,
                    "java/util/Set",
                    "size",
                    "()I",
                    &entries,
                    &[],
                )
            }
            (ABSTRACT_MAP, "isEmpty") => Ok(boolean(self.size_of(own, ABSTRACT_MAP, this)? == 0)),
            (ABSTRACT_MAP, "containsValue" | "containsKey" | "get") => {
                let wanted = arg(0);
                let part = if name == "containsValue" {
                    "getValue"
                } else {
                    "getKey"
                };
                let i = self.entry_iterator(this)?;
                while self.has_next(&i)? {
                    let e = self.next(&i)?;
                    let candidate = self.entry_part(&e, part)?;
                    let found = if wanted == Value::Ref(None) {
                        candidate == Value::Ref(None)
                    } else {
                        self.java_equals(&wanted, &candidate)?
                    };
                    if found {
                        return if name == "get" {
                            self.entry_part(&e, "getValue")
                        } else {
                            Ok(boolean(true))
                        };
                    }
                }
                Ok(if name == "get" {
                    Value::Ref(None)
                } else {
                    boolean(false)
                })
            }
            (ABSTRACT_MAP, "put") => Err(throw("java/lang/UnsupportedOperationException", None)),
            (ABSTRACT_MAP, "remove") => {
                let key = arg(0);
                let i = self.entry_iterator(this)?;
                let mut correct = None;
                while correct.is_none() && self.has_next(&i)? {
                    let e = self.next(&i)?;
                    let k = self.entry_part(&e, "getKey")?;
                    let found = if key == Value::Ref(None) {
                        k == Value::Ref(None)
                    } else {
                        self.java_equals(&key, &k)?
                    };
                    if found {
                        correct = Some(e);
                    }
                }
                match correct {
                    Some(entry) => {
                        let old = self.entry_part(&entry, "getValue")?;
                        self.remove_current(&i)?;
                        Ok(old)
                    }
                    None => Ok(Value::Ref(None)),
                }
            }
            (ABSTRACT_MAP, "putAll") => {
                let m = arg(0);
                let entries = self.call_virtual(
                    Site::Interface,
                    MAP,
                    "entrySet",
                    "()Ljava/util/Set;",
                    &m,
                    &[],
                )?;
                let i = self.call_virtual(
                    Site::Interface,
                    "java/util/Set",
                    "iterator",
                    "()Ljava/util/Iterator;",
                    &entries,
                    &[],
                )?;
                while self.has_next(&i)? {
                    let e = self.next(&i)?;
                    let key = self.entry_part(&e, "getKey")?;
                    let value = self.entry_part(&e, "getValue")?;
                    self.call_virtual(own, ABSTRACT_MAP, "put", BODIES[5].2, this, &[key, value])?;
                }
                Ok(Value::Void)
            }
            (ABSTRACT_MAP, "clear") => {
                let entries = self.call_virtual(
                    own,
                    ABSTRACT_MAP,
                    "entrySet",
                    "()Ljava/util/Set;",
                    this,
                    &[],
                )?;
                self.call_virtual(
                    Site::Interface,
                    "java/util/Set",
                    "clear",
                    "()V",
                    &entries,
                    &[],
                )?;
                Ok(Value::Void)
            }
            (ABSTRACT_MAP, "keySet" | "values") => {
                let (field, class) = if name == "keySet" {
                    ((ABSTRACT_MAP, "keySet", "Ljava/util/Set;"), KEY_SET)
                } else {
                    ((ABSTRACT_MAP, "values", "Ljava/util/Collection;"), VALUES)
                };
                let cached = self.jdk_field(this, field);
                if cached != Value::Ref(None) {
                    return Ok(cached);
                }
                let view = self.new_object(class);
                self.put_jdk_field(
                    &view,
                    (class, "this$0", "Ljava/util/AbstractMap;"),
                    this.clone(),
                );
                self.put_jdk_field(this, field, view.clone());
                Ok(view)
            }
            (ABSTRACT_MAP, "equals") => self.map_equals(this, &arg(0)),
            (ABSTRACT_MAP, "hashCode") => {
                let i = self.entry_iterator(this)?;
                let mut h = 0i32;
                while self.has_next(&i)? {
                    let e = self.next(&i)?;
                    h = h.wrapping_add(self.java_hash(&e)?);
                }
                Ok(Value::Int(h))
            }
            (ABSTRACT_MAP, "toString") => {
                let i = self.entry_iterator(this)?;
                if !self.has_next(&i)? {
                    return Ok(Value::Ref(Some("{}".into())));
                }
                let mut text = String::from("{");
                loop {
                    let e = self.next(&i)?;
                    let key = self.entry_part(&e, "getKey")?;
                    let value = self.entry_part(&e, "getValue")?;
                    for (index, part) in [key, value].iter().enumerate() {
                        if index == 1 {
                            text.push('=');
                        }
                        if part == this {
                            text.push_str("(this Map)");
                        } else {
                            text.push_str(&self.java_text(part)?);
                        }
                    }
                    if !self.has_next(&i)? {
                        text.push('}');
                        return Ok(Value::Ref(Some(text)));
                    }
                    text.push_str(", ");
                }
            }
            (SIMPLE_ENTRY | IMMUTABLE_ENTRY, _) => self.entry_body(owner, name, this, &arg(0)),
            (MAP, _) => self.map_default(name, descriptor, this, args),
            _ => Err(NativeError::Metadata(format!(
                "no JDK body for {owner}#{name}{descriptor}"
            ))),
        }
    }

    /// `AbstractMap.equals`.
    fn map_equals(&mut self, this: &Value, o: &Value) -> Result<Value, NativeError> {
        if o == this {
            return Ok(boolean(true));
        }
        if !self.java_instance_of(o, MAP)? {
            return Ok(boolean(false));
        }
        let other_size = int_of(&self.call_virtual(Site::Interface, MAP, "size", "()I", o, &[])?);
        if other_size != self.size_of(Site::Virtual, ABSTRACT_MAP, this)? {
            return Ok(boolean(false));
        }
        // `catch (ClassCastException | NullPointerException unused)`.
        let walk = (|| {
            let i = self.entry_iterator(this)?;
            while self.has_next(&i)? {
                let e = self.next(&i)?;
                let key = self.entry_part(&e, "getKey")?;
                let value = self.entry_part(&e, "getValue")?;
                let theirs = self.call_virtual(
                    Site::Interface,
                    MAP,
                    "get",
                    "(Ljava/lang/Object;)Ljava/lang/Object;",
                    o,
                    std::slice::from_ref(&key),
                )?;
                let same = if value == Value::Ref(None) {
                    theirs == Value::Ref(None)
                        && truth(&self.call_virtual(
                            Site::Interface,
                            MAP,
                            "containsKey",
                            "(Ljava/lang/Object;)Z",
                            o,
                            &[key],
                        )?)
                } else {
                    self.java_equals(&value, &theirs)?
                };
                if !same {
                    return Ok(false);
                }
            }
            Ok(true)
        })();
        match walk {
            Ok(equal) => Ok(boolean(equal)),
            Err(error)
                if self.catches(&error, "java/lang/ClassCastException")
                    || self.catches(&error, "java/lang/NullPointerException") =>
            {
                Ok(boolean(false))
            }
            Err(error) => Err(error),
        }
    }

    /// A `SimpleEntry`/`SimpleImmutableEntry` member, over its own fields.
    fn entry_body(
        &mut self,
        owner: &str,
        name: &str,
        this: &Value,
        other: &Value,
    ) -> Result<Value, NativeError> {
        let key = self.jdk_field(this, object_field(owner, "key"));
        let value = self.jdk_field(this, object_field(owner, "value"));
        match name {
            "getKey" => Ok(key),
            "getValue" => Ok(value),
            "setValue" if owner == IMMUTABLE_ENTRY => {
                Err(throw("java/lang/UnsupportedOperationException", None))
            }
            "setValue" => {
                self.put_jdk_field(this, object_field(owner, "value"), other.clone());
                Ok(value)
            }
            "equals" => {
                if !self.java_instance_of(other, ENTRY)? {
                    return Ok(boolean(false));
                }
                let their_key = self.entry_part(other, "getKey")?;
                if !self.entry_eq(&key, &their_key)? {
                    return Ok(boolean(false));
                }
                let their_value = self.entry_part(other, "getValue")?;
                Ok(boolean(self.entry_eq(&value, &their_value)?))
            }
            "hashCode" => {
                let k = if key == Value::Ref(None) {
                    0
                } else {
                    self.java_hash(&key)?
                };
                let v = if value == Value::Ref(None) {
                    0
                } else {
                    self.java_hash(&value)?
                };
                Ok(Value::Int(k ^ v))
            }
            _ => {
                let text = format!("{}={}", self.java_text(&key)?, self.java_text(&value)?);
                Ok(Value::Ref(Some(text)))
            }
        }
    }

    /// A member a view class declares itself.
    pub(super) fn map_view_body(
        &mut self,
        class: &str,
        name: &str,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        match class {
            KEY_SET | VALUES => {
                let map = self.jdk_field(this, outer_field(class));
                let (member, descriptor) = match name {
                    "iterator" => {
                        let iterator_class = if class == KEY_SET {
                            KEY_ITERATOR
                        } else {
                            VALUE_ITERATOR
                        };
                        let iterator = self.new_object(iterator_class);
                        self.put_jdk_field(&iterator, outer_field(iterator_class), map.clone());
                        let inner = self.entry_iterator(&map)?;
                        self.put_jdk_field(
                            &iterator,
                            (iterator_class, "i", "Ljava/util/Iterator;"),
                            inner,
                        );
                        return Ok(iterator);
                    }
                    "contains" if class == KEY_SET => ("containsKey", "(Ljava/lang/Object;)Z"),
                    "contains" => ("containsValue", "(Ljava/lang/Object;)Z"),
                    "size" => ("size", "()I"),
                    "isEmpty" => ("isEmpty", "()Z"),
                    _ => ("clear", "()V"),
                };
                self.call_virtual(Site::Virtual, ABSTRACT_MAP, member, descriptor, &map, args)
            }
            _ => {
                let inner = self.jdk_field(this, (class, "i", "Ljava/util/Iterator;"));
                match name {
                    "hasNext" => Ok(boolean(self.has_next(&inner)?)),
                    "remove" => {
                        self.remove_current(&inner)?;
                        Ok(Value::Void)
                    }
                    _ => {
                        let entry = self.next(&inner)?;
                        let part = if class == KEY_ITERATOR {
                            "getKey"
                        } else {
                            "getValue"
                        };
                        self.entry_part(&entry, part)
                    }
                }
            }
        }
    }

    /// A `Map` default; its calls on `this` are `invokeinterface Map`.
    fn map_default(
        &mut self,
        name: &str,
        descriptor: &str,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Ref(None));
        let key = arg(0);
        let call = |runtime: &mut Self, name: &str, descriptor: &str, args: &[Value]| {
            runtime.call_virtual(Site::Interface, MAP, name, descriptor, this, args)
        };
        let get = |runtime: &mut Self, key: &Value| {
            call(
                runtime,
                "get",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                std::slice::from_ref(key),
            )
        };
        let contains = |runtime: &mut Self, key: &Value| {
            Ok::<_, NativeError>(truth(&call(
                runtime,
                "containsKey",
                "(Ljava/lang/Object;)Z",
                std::slice::from_ref(key),
            )?))
        };
        let put = |runtime: &mut Self, key: &Value, value: &Value| {
            call(runtime, "put", BODIES[5].2, &[key.clone(), value.clone()])
        };
        let remove = |runtime: &mut Self, key: &Value| {
            call(
                runtime,
                "remove",
                "(Ljava/lang/Object;)Ljava/lang/Object;",
                std::slice::from_ref(key),
            )
        };
        let bi_apply = |runtime: &mut Self, function: &Value, a: Value, b: Value| {
            runtime.call_virtual(
                Site::Interface,
                "java/util/function/BiFunction",
                "apply",
                "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
                function,
                &[a, b],
            )
        };
        let null = Value::Ref(None);
        match (name, descriptor) {
            ("getOrDefault", _) => {
                let v = get(self, &key)?;
                Ok(if v != null || contains(self, &key)? {
                    v
                } else {
                    arg(1)
                })
            }
            ("forEach" | "replaceAll", _) => {
                let function = arg(0);
                require_non_null(&function)?;
                let entries = call(self, "entrySet", "()Ljava/util/Set;", &[])?;
                let i = self.call_virtual(
                    Site::Interface,
                    "java/util/Set",
                    "iterator",
                    "()Ljava/util/Iterator;",
                    &entries,
                    &[],
                )?;
                while self.has_next(&i)? {
                    let entry = self.next(&i)?;
                    let parts = (|| {
                        Ok::<_, NativeError>((
                            self.entry_part(&entry, "getKey")?,
                            self.entry_part(&entry, "getValue")?,
                        ))
                    })();
                    let (k, v) = self.state_as_cme(parts)?;
                    if name == "forEach" {
                        self.call_virtual(
                            Site::Interface,
                            "java/util/function/BiConsumer",
                            "accept",
                            "(Ljava/lang/Object;Ljava/lang/Object;)V",
                            &function,
                            &[k, v],
                        )?;
                    } else {
                        let replaced = bi_apply(self, &function, k, v)?;
                        let set = self
                            .call_virtual(
                                Site::Interface,
                                ENTRY,
                                "setValue",
                                "(Ljava/lang/Object;)Ljava/lang/Object;",
                                &entry,
                                &[replaced],
                            )
                            .map(drop);
                        self.state_as_cme(set)?;
                    }
                }
                Ok(Value::Void)
            }
            ("putIfAbsent", _) => {
                let v = get(self, &key)?;
                if v == null {
                    put(self, &key, &arg(1))
                } else {
                    Ok(v)
                }
            }
            ("remove", _)
            | ("replace", "(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z") => {
                let current = get(self, &key)?;
                if !self.objects_equal(&current, &arg(1))?
                    || (current == null && !contains(self, &key)?)
                {
                    return Ok(boolean(false));
                }
                if name == "remove" {
                    remove(self, &key)?;
                } else {
                    put(self, &key, &arg(2))?;
                }
                Ok(boolean(true))
            }
            ("replace", _) => {
                let current = get(self, &key)?;
                if current != null || contains(self, &key)? {
                    return put(self, &key, &arg(1));
                }
                Ok(current)
            }
            ("computeIfAbsent", _) => {
                let function = arg(1);
                require_non_null(&function)?;
                let v = get(self, &key)?;
                if v == null {
                    let created = self.call_virtual(
                        Site::Interface,
                        "java/util/function/Function",
                        "apply",
                        "(Ljava/lang/Object;)Ljava/lang/Object;",
                        &function,
                        std::slice::from_ref(&key),
                    )?;
                    if created != null {
                        put(self, &key, &created)?;
                        return Ok(created);
                    }
                }
                Ok(v)
            }
            ("computeIfPresent", _) => {
                let function = arg(1);
                require_non_null(&function)?;
                let old = get(self, &key)?;
                if old == null {
                    return Ok(null);
                }
                let created = bi_apply(self, &function, key.clone(), old)?;
                if created != null {
                    put(self, &key, &created)?;
                    Ok(created)
                } else {
                    remove(self, &key)?;
                    Ok(null)
                }
            }
            ("compute", _) => {
                let function = arg(1);
                require_non_null(&function)?;
                let old = get(self, &key)?;
                let created = bi_apply(self, &function, key.clone(), old.clone())?;
                if created == null {
                    if old != null || contains(self, &key)? {
                        remove(self, &key)?;
                    }
                    return Ok(null);
                }
                put(self, &key, &created)?;
                Ok(created)
            }
            _ => {
                // `merge(key, value, remappingFunction)`.
                let (value, function) = (arg(1), arg(2));
                require_non_null(&function)?;
                require_non_null(&value)?;
                let old = get(self, &key)?;
                let created = if old == null {
                    value
                } else {
                    bi_apply(self, &function, old, value)?
                };
                if created == null {
                    remove(self, &key)?;
                } else {
                    put(self, &key, &created)?;
                }
                Ok(created)
            }
        }
    }

    /// `catch (IllegalStateException ise) { throw new
    /// ConcurrentModificationException(ise); }` around an entry access.
    fn state_as_cme<T>(&mut self, result: Result<T, NativeError>) -> Result<T, NativeError> {
        match result {
            Err(error) if self.catches(&error, "java/lang/IllegalStateException") => {
                Err(self.throw_caused("java/util/ConcurrentModificationException", error))
            }
            other => other,
        }
    }
}
