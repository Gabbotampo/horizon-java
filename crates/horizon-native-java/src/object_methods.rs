//! `equals` and `hashCode` as the JVM runs them, for the values the JDK's
//! collections and `Objects` helpers compare.
//!
//! The interpreter keeps values untyped: a `String` is its text, a box is a
//! number, an object is its `class#n` marker. Comparing two values by their
//! marker is Java's answer only for a value whose class keeps `Object.equals`.
//! A record, or a jar class with its own `equals`, compares by value on a JVM,
//! and its `hashCode` is whatever its body computes. This module answers both
//! the JVM way:
//!
//! * [`Runtime::java_equals`] is `a.equals(b)`: the jar's own body when the
//!   class has one (run through the interpreter, so a side effect or an
//!   exception happens as it would), the value's own equality for a string or
//!   a box, and identity for everything else.
//! * [`Runtime::java_hash`] is `Objects.hashCode(o)`: the jar's body, the
//!   JDK's formula for a string, a box and a host list/set/map, and `None`
//!   when the answer is an identity hash (`Object.hashCode`, `Enum.hashCode`,
//!   `System.identityHashCode`), which no run can reproduce.
//! * A record's generated `equals`/`hashCode`/`toString` follow
//!   `java.lang.runtime.ObjectMethods` (JDK 25) exactly: the components are
//!   read from the record's **fields** (the bootstrap's getter handles, never
//!   the accessors), `hashCode` folds `31 * h + hash(c)` over the components
//!   in declaration order starting from `0`, and `equals` is `this == o`, then
//!   `o instanceof R`, then the components compared **last to first** (the
//!   bootstrap nests one `guardWithTest` per component around the previous
//!   ones), each reference component with `Objects.equals`.
//! * `Objects.equals`/`hashCode`/`hash` and `Arrays.hashCode(Object[])` use
//!   the two above, and a direct read of an identity hash is counted instead
//!   of answered with a made-up number.
//!
//! [`Runtime::pure_equality`] says whether running a value's `equals` and
//! `hashCode` can have an observable effect. A string, a box, an identity
//! object and a record whose components are all pure cannot, so a model may
//! ask about them in any order and as often as it likes; anything else must
//! be asked exactly when and as often as the JDK asks.

use super::*;

/// `Object.equals(Object)`.
pub(crate) const EQUALS: &str = "(Ljava/lang/Object;)Z";
/// `Object.hashCode()`.
pub(crate) const HASH_CODE: &str = "()I";
/// How deep [`Runtime::pure_equality`] follows record components.
const MAX_PURITY_DEPTH: usize = 8;

/// Whose `equals` and `hashCode` a value runs.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Equality {
    Null,
    /// A `String` or a box: compared and hashed by value, with no jar code.
    Platform,
    /// `Object`'s `equals` and `hashCode`: an enum constant, or a jar object
    /// whose class overrides neither.
    Identity,
    /// A jar object with a body of its own for `equals` and/or `hashCode`
    /// (`None` keeps `Object`'s member for that one).
    Jar {
        equals: Option<String>,
        hash_code: Option<String>,
    },
    /// A value the host models (a collection, a builder, a game object): the
    /// host keeps comparing it by marker, as before.
    Host,
}

/// The box marker families (`class_of_value`) and the hash their JDK class
/// computes from the text after the `:`.
fn boxed_marker_hash(marker: &str) -> Option<Option<i32>> {
    match marker {
        "true" | "boolean:true" => return Some(Some(1231)),
        "false" | "boolean:false" => return Some(Some(1237)),
        _ => {}
    }
    let (kind, text) = marker.split_once(':')?;
    let hash = match kind {
        "integer" | "short" | "byte" => text.parse::<i32>().ok(),
        "long" => text.parse::<i64>().ok().map(long_hash),
        "character" => text.encode_utf16().next().map(i32::from),
        "double" => text.parse::<f64>().ok().map(double_hash),
        "float" => text.parse::<f64>().ok().map(|v| java_float_bits(v) as i32),
        _ => return None,
    };
    Some(hash)
}

/// `Long.hashCode(v)`.
pub(crate) fn long_hash(value: i64) -> i32 {
    (value ^ ((value as u64) >> 32) as i64) as i32
}

/// `Double.hashCode(v)`: the canonical bits, folded like a `long`.
pub(crate) fn double_hash(value: f64) -> i32 {
    long_hash(java_double_bits(value) as i64)
}

/// Whether a reference is one of the host's box markers.
fn is_box_marker(marker: &str) -> bool {
    boxed_marker_hash(marker).is_some()
}

/// The host's unmodifiable-collection and value markers keep `Object`'s
/// owner names; these are the JDK classes whose `equals` and `hashCode` are
/// `Object`'s identity members.
fn keeps_object_members(owner: &str) -> bool {
    matches!(
        owner,
        "java/lang/Object" | "java/lang/Record" | "java/lang/Enum"
    )
}

/// A generated record member's `invokedynamic` index: javac compiles a
/// record's implicit `equals`/`hashCode`/`toString` to `aload_0 [aload_1]
/// invokedynamic #i 0 0 <return>`, with `ObjectMethods.bootstrap` behind `#i`.
fn generated_record_site(file: &ClassFile, name: &str, descriptor: &str) -> Option<u16> {
    let code = file.method(name, descriptor)?.code.as_deref()?;
    let rest = code.strip_prefix(&[0x2a])?;
    let rest = rest.strip_prefix(&[0x2b]).unwrap_or(rest);
    let [0xba, high, low, 0, 0, 0xac | 0xb0] = rest else {
        return None;
    };
    let index = u16::from_be_bytes([*high, *low]);
    file.record_component_names(index).map(|_| index)
}

/// One record component as `ObjectMethods` sees it: its name and the field
/// its getter handle reads.
struct Component {
    name: String,
    field: MethodRef,
}

/// The components an `ObjectMethods.bootstrap` call site names: the
/// `;`-separated names (its second static argument) paired with the
/// `REF_getField` handles that follow.
fn record_components(file: &ClassFile, index: u16) -> Option<Vec<Component>> {
    let names = file.record_component_names(index)?;
    let bootstrap = match file.pool.get(index as usize) {
        Some(Cp::InvokeDynamic { bootstrap, .. }) => {
            file.bootstrap_methods.get(*bootstrap as usize)?
        }
        _ => return None,
    };
    // `names` is `""` for a record without components.
    let names = names.into_iter().filter(|name| !name.is_empty());
    let getters = bootstrap.arguments.get(2..)?;
    let mut components = Vec::new();
    for (name, getter) in names.zip(getters) {
        let field = match file.pool.get(*getter as usize) {
            Some(Cp::MethodHandle { reference_index }) => file.field_ref(*reference_index).ok()?,
            _ => return None,
        };
        components.push(Component {
            name: name.clone(),
            field,
        });
    }
    Some(components)
}

impl<H: Host> Runtime<'_, H> {
    /// The jar class of an object this run built whose class the interpreter
    /// runs (not a host- or library-modelled copy).
    pub(crate) fn interpreted_object_class(&self, value: &Value) -> Option<String> {
        let class = jvm_dispatch::constructed_object_class(expect_ref(value)?)?;
        (!self.host.models_class(class)
            && !jvm_dispatch::library_modelled(class)
            && self.class(class).is_some())
        .then(|| class.to_owned())
    }

    /// Whose `equals` and `hashCode` `value` runs.
    pub(crate) fn equality_of(&self, value: &Value) -> Equality {
        let marker = match value {
            Value::Ref(None) => return Equality::Null,
            Value::Int(_) | Value::Long(_) | Value::Float(_) => return Equality::Platform,
            Value::Void => return Equality::Host,
            Value::Ref(Some(marker)) => marker,
        };
        if let Some(class) = self.interpreted_object_class(value) {
            let member = |name: &str, descriptor: &str| -> Result<Option<String>, ()> {
                match self.member_owner(&class, name, descriptor) {
                    Some((owner, true)) => Ok(Some(owner)),
                    Some((owner, false)) if keeps_object_members(&owner) => Ok(None),
                    // A JDK superclass with its own members (`extends
                    // AbstractList`, `extends HashMap`): the host's model.
                    _ => Err(()),
                }
            };
            return match (member("equals", EQUALS), member("hashCode", HASH_CODE)) {
                (Ok(None), Ok(None)) => Equality::Identity,
                (Ok(equals), Ok(hash_code)) => Equality::Jar { equals, hash_code },
                _ => Equality::Host,
            };
        }
        if self.enum_constants.contains_key(marker) {
            return Equality::Identity;
        }
        if is_box_marker(marker) || value_class::is_plain_string(marker) {
            return Equality::Platform;
        }
        Equality::Host
    }

    /// Whether running `value`'s `equals` and `hashCode` has no observable
    /// effect: a string, a box, `null`, an identity object, and a record whose
    /// generated members compare only pure components.
    pub(crate) fn pure_equality(&self, value: &Value) -> bool {
        self.pure_equality_at(value, 0)
    }

    fn pure_equality_at(&self, value: &Value, depth: usize) -> bool {
        match self.equality_of(value) {
            Equality::Null | Equality::Platform | Equality::Identity => true,
            Equality::Host => false,
            Equality::Jar { equals, hash_code } => {
                if depth >= MAX_PURITY_DEPTH {
                    return false;
                }
                let Some(class) = self.interpreted_object_class(value) else {
                    return false;
                };
                // Both members must be the record's generated ones.
                if equals.as_deref() != Some(class.as_str())
                    || hash_code.as_deref() != Some(class.as_str())
                {
                    return false;
                }
                let Some(file) = self.class(&class) else {
                    return false;
                };
                let (Some(index), Some(_)) = (
                    generated_record_site(file, "equals", EQUALS),
                    generated_record_site(file, "hashCode", HASH_CODE),
                ) else {
                    return false;
                };
                let Some(components) = record_components(file, index) else {
                    return false;
                };
                components.iter().all(|component| {
                    let field = self.record_field(value, &component.field);
                    self.pure_equality_at(&field, depth + 1)
                })
            }
        }
    }

    /// A record field's value, read as the bootstrap's getter handle reads it.
    fn record_field(&self, receiver: &Value, field: &MethodRef) -> Value {
        instance_field_key(receiver, field)
            .and_then(|key| self.fields.get(&key).cloned())
            .unwrap_or_else(|| Value::default_for(&field.descriptor))
    }

    /// `Objects.hashCode(value)` as the JVM computes it, running the jar's
    /// `hashCode` body when the class has one. `None` when the answer is an
    /// identity hash, or a value whose class the runtime cannot tell (a
    /// `Value::Float` is a `Float` or a `Double`).
    pub(crate) fn java_hash(&mut self, value: &Value) -> Result<Option<i32>, NativeError> {
        match self.equality_of(value) {
            Equality::Null => Ok(Some(0)),
            Equality::Identity
            | Equality::Jar {
                hash_code: None, ..
            } => Ok(None),
            Equality::Platform => Ok(match value {
                // An `Integer` (a `Boolean` stored as an int hashes 1231/1237
                // on a JVM: the runtime cannot tell the two apart).
                Value::Int(value) => Some(*value),
                Value::Long(value) => Some(long_hash(*value)),
                Value::Float(_) => None,
                Value::Ref(Some(marker)) => match boxed_marker_hash(marker) {
                    Some(hash) => hash,
                    None => Some(hash_map_order::java_string_hash(marker)),
                },
                _ => None,
            }),
            Equality::Jar {
                hash_code: Some(owner),
                ..
            } => {
                if let Some(components) = self.generated_record(&owner, "hashCode", HASH_CODE) {
                    return self.record_hash(value, &components);
                }
                match self.invoke(&owner, "hashCode", HASH_CODE, Some(value.clone()), &[])? {
                    Value::Int(hash) => Ok(Some(hash)),
                    other => Err(NativeError::Metadata(format!(
                        "{owner}.hashCode answered {}",
                        describe_value(&other)
                    ))),
                }
            }
            Equality::Host => self.host_value_hash(value),
        }
    }

    /// The hash of a host-modelled value: a list, set or map by the JDK's
    /// formula over its elements, an identifier by `Identifier.hashCode`.
    fn host_value_hash(&mut self, value: &Value) -> Result<Option<i32>, NativeError> {
        let Some(marker) = expect_ref(value) else {
            return Ok(None);
        };
        if let Some(id) = marker.strip_prefix("identifier/") {
            return Ok(Some(hash_map_order::identifier_hash(id)));
        }
        let Some(kind) = self.host_collection_kind(value) else {
            return Ok(None);
        };
        let elements = self.collection_elements(value)?;
        match kind {
            CollectionKind::List => {
                // `AbstractList.hashCode`/`ArrayList.hashCodeRange`.
                let mut hash = 1_i32;
                for element in &elements {
                    let Some(element) = self.java_hash(element)? else {
                        return Ok(None);
                    };
                    hash = hash.wrapping_mul(31).wrapping_add(element);
                }
                Ok(Some(hash))
            }
            CollectionKind::Set => {
                // `AbstractSet.hashCode`: the sum, `null` adding nothing.
                let mut hash = 0_i32;
                for element in &elements {
                    let Some(element) = self.java_hash(element)? else {
                        return Ok(None);
                    };
                    hash = hash.wrapping_add(element);
                }
                Ok(Some(hash))
            }
            CollectionKind::Map => {
                // `AbstractMap.hashCode`: the sum of `hash(key) ^ hash(value)`.
                let mut hash = 0_i32;
                for pair in elements.chunks(2) {
                    let [key, entry] = pair else {
                        return Ok(None);
                    };
                    let (Some(key), Some(entry)) = (self.java_hash(key)?, self.java_hash(entry)?)
                    else {
                        return Ok(None);
                    };
                    hash = hash.wrapping_add(key ^ entry);
                }
                Ok(Some(hash))
            }
        }
    }

    /// `a.equals(b)` for a non-null `a`, as the JVM runs it.
    pub(crate) fn java_equals(&mut self, a: &Value, b: &Value) -> Result<bool, NativeError> {
        match self.equality_of(a) {
            Equality::Null => Err(NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: None,
                frame: None,
            }),
            Equality::Platform => Ok(platform_equal(a, b)),
            Equality::Identity | Equality::Jar { equals: None, .. } => Ok(a == b),
            Equality::Jar {
                equals: Some(owner),
                ..
            } => {
                if let Some(components) = self.generated_record(&owner, "equals", EQUALS) {
                    return self.record_equals(&owner, a, b, &components);
                }
                match self.invoke(
                    &owner,
                    "equals",
                    EQUALS,
                    Some(a.clone()),
                    std::slice::from_ref(b),
                )? {
                    Value::Int(flag) => Ok(flag != 0),
                    other => Err(NativeError::Metadata(format!(
                        "{owner}.equals answered {}",
                        describe_value(&other)
                    ))),
                }
            }
            Equality::Host => {
                if self.host_collection_kind(a) == Some(CollectionKind::List)
                    && self.host_collection_kind(b) == Some(CollectionKind::List)
                {
                    return self.list_equals(a, b);
                }
                Ok(a == b)
            }
        }
    }

    /// `Objects.equals(a, b)`: `a == b || a != null && a.equals(b)`.
    pub(crate) fn objects_equals(&mut self, a: &Value, b: &Value) -> Result<bool, NativeError> {
        if a == b && !matches!(a, Value::Float(_)) {
            return Ok(true);
        }
        if *a == Value::Ref(None) {
            return Ok(false);
        }
        self.java_equals(a, b)
    }

    /// The components of `class`'s generated record member `name`, or `None`
    /// when the member is hand-written (or `class` is not a record).
    fn generated_record(
        &self,
        class: &str,
        name: &str,
        descriptor: &str,
    ) -> Option<Vec<Component>> {
        let file = self.class(class)?;
        let index = generated_record_site(file, name, descriptor)?;
        record_components(file, index)
    }

    /// `ObjectMethods`' `hashCode`: `0`, then `31 * h + hash(component)` in
    /// declaration order, each component read from its field.
    fn record_hash(
        &mut self,
        receiver: &Value,
        components: &[Component],
    ) -> Result<Option<i32>, NativeError> {
        let mut hash = 0_i32;
        for component in components {
            let value = self.record_field(receiver, &component.field);
            let Some(part) = self.component_hash(&value, &component.field.descriptor)? else {
                return Ok(None);
            };
            hash = hash.wrapping_mul(31).wrapping_add(part);
        }
        Ok(Some(hash))
    }

    /// The hasher `ObjectMethods` picks for one component type: the boxed
    /// class's static `hashCode` for a primitive, `Objects.hashCode` for a
    /// reference.
    fn component_hash(
        &mut self,
        value: &Value,
        descriptor: &str,
    ) -> Result<Option<i32>, NativeError> {
        Ok(match (descriptor, value) {
            ("Z", Value::Int(flag)) => Some(if *flag != 0 { 1231 } else { 1237 }),
            ("B" | "S" | "C" | "I", Value::Int(int)) => Some(*int),
            ("J", Value::Long(long)) => Some(long_hash(*long)),
            ("F", Value::Float(float)) => Some(java_float_bits(*float) as i32),
            ("D", Value::Float(double)) => Some(double_hash(*double)),
            (descriptor, value) if descriptor.starts_with(['L', '[']) => self.java_hash(value)?,
            _ => None,
        })
    }

    /// `ObjectMethods`' `equals`: `this == o`, then `o instanceof R`, then the
    /// components from the last to the first.
    fn record_equals(
        &mut self,
        class: &str,
        receiver: &Value,
        other: &Value,
        components: &[Component],
    ) -> Result<bool, NativeError> {
        if receiver == other {
            return Ok(true);
        }
        // A record class is final, so `isInstance` is "of exactly this class".
        if self.interpreted_object_class(other).as_deref() != Some(class) {
            return Ok(false);
        }
        for component in components.iter().rev() {
            let left = self.record_field(receiver, &component.field);
            let right = self.record_field(other, &component.field);
            let equal = match component.field.descriptor.as_str() {
                // `Float.compare(a, b) == 0` / `Double.compare(a, b) == 0`.
                "F" | "D" => match (&left, &right) {
                    (Value::Float(a), Value::Float(b)) => {
                        java_double_bits(*a) == java_double_bits(*b)
                    }
                    _ => left == right,
                },
                descriptor if descriptor.starts_with(['L', '[']) => {
                    self.objects_equals(&left, &right)?
                }
                _ => left == right,
            };
            if !equal {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// `ObjectMethods`' `toString`: `Simple[a=.., b=..]`, each component as
    /// `String.valueOf` of its field, in declaration order.
    fn record_text(
        &mut self,
        file: &ClassFile,
        class: &str,
        receiver: &Value,
        components: &[Component],
    ) -> Result<String, NativeError> {
        // `getSimpleName()`: `Point`, not `Probe$Point`.
        let mut text = match file.nested_simple_name() {
            Some(Some(simple)) => simple,
            _ => class.rsplit('/').next().unwrap_or(class).to_owned(),
        };
        text.push('[');
        for (position, component) in components.iter().enumerate() {
            if position > 0 {
                text.push_str(", ");
            }
            let value = self.record_field(receiver, &component.field);
            text.push_str(&component.name);
            text.push('=');
            text.push_str(&self.value_of_text(
                &value,
                &component.field.descriptor,
                class,
                "toString",
            )?);
        }
        text.push(']');
        Ok(text)
    }

    /// Answer a record's generated `toString`/`equals`/`hashCode` call site
    /// (`java.lang.runtime.ObjectMethods.bootstrap`), as JDK 25 computes them.
    /// A hash that is an identity hash (a component of a class keeping
    /// `Object.hashCode`) is counted as `Object#hashCode` and answers `0`.
    pub(crate) fn invoke_record_object_method(
        &mut self,
        file: &ClassFile,
        index: u16,
        class: &str,
        name: &str,
        captured: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(components) = record_components(file, index) else {
            return Ok(None);
        };
        let Some(receiver) = captured.first() else {
            return Ok(None);
        };
        match (name, captured.len()) {
            ("toString", 1) => {
                let text = self.record_text(file, class, receiver, &components)?;
                Ok(Some(Value::Ref(Some(text))))
            }
            ("hashCode", 1) => match self.record_hash(receiver, &components)? {
                Some(hash) => Ok(Some(Value::Int(hash))),
                None => self.count_identity_hash().map(Some),
            },
            ("equals", 2) => {
                let other = captured.get(1).cloned().unwrap_or(Value::Ref(None));
                let equal = self.record_equals(class, receiver, &other, &components)?;
                Ok(Some(Value::Int(i32::from(equal))))
            }
            _ => Ok(None),
        }
    }

    /// Count a read of an identity hash, which no run can reproduce.
    fn count_identity_hash(&mut self) -> Result<Value, NativeError> {
        self.host.unknown_operand_call(&MethodRef {
            owner: "java/lang/Object".into(),
            name: "hashCode".into(),
            descriptor: HASH_CODE.into(),
        })
    }

    /// `Objects.equals`/`hashCode`/`hash`, `Arrays.hashCode(Object[])`, and a
    /// direct read of an identity hash (`Object.hashCode`/`Enum.hashCode` on a
    /// value that keeps it, `System.identityHashCode`), which is counted.
    /// `Ok(None)` leaves the call to the rest of the chain.
    pub(crate) fn invoke_object_methods(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            ("java/util/Objects", "equals", "(Ljava/lang/Object;Ljava/lang/Object;)Z") => {
                let [a, b] = args else { return Ok(None) };
                let equal = self.objects_equals(a, b)?;
                return Ok(Some(Value::Int(i32::from(equal))));
            }
            ("java/util/Objects", "hashCode", "(Ljava/lang/Object;)I") => {
                let [value] = args else { return Ok(None) };
                return match self.java_hash(value)? {
                    Some(hash) => Ok(Some(Value::Int(hash))),
                    None => self.host.unknown_operand_call(r).map(Some),
                };
            }
            ("java/util/Objects", "hash", "([Ljava/lang/Object;)I")
            | ("java/util/Arrays", "hashCode", "([Ljava/lang/Object;)I") => {
                let [array] = args else { return Ok(None) };
                if *array == Value::Ref(None) {
                    return Ok(Some(Value::Int(0)));
                }
                let Ok(elements) = self.array(array).cloned() else {
                    return Ok(None);
                };
                // `Arrays.hashCode(Object[])`: `1`, then `31 * h + hash(e)`.
                let mut hash = 1_i32;
                for element in &elements {
                    let Some(part) = self.java_hash(element)? else {
                        return self.host.unknown_operand_call(r).map(Some);
                    };
                    hash = hash.wrapping_mul(31).wrapping_add(part);
                }
                return Ok(Some(Value::Int(hash)));
            }
            ("java/lang/System", "identityHashCode", "(Ljava/lang/Object;)I") => {
                let [value] = args else { return Ok(None) };
                if *value == Value::Ref(None) {
                    return Ok(Some(Value::Int(0)));
                }
                return self.host.unknown_operand_call(r).map(Some);
            }
            _ => {}
        }
        // `o.hashCode()` that reaches `Object`'s or `Enum`'s body (a jar
        // class without its own, an enum constant, a `super.hashCode()`).
        if r.name == "hashCode"
            && r.descriptor == HASH_CODE
            && matches!(opcode, 0xb6 | 0xb7 | 0xb9)
            && let Some(receiver) = receiver
        {
            let keeps_identity = match self.equality_of(receiver) {
                Equality::Identity
                | Equality::Jar {
                    hash_code: None, ..
                } => true,
                // `super.hashCode()` from a class that overrides it.
                Equality::Jar { .. } => opcode == 0xb7 && keeps_object_members(&r.owner),
                _ => false,
            };
            if keeps_identity {
                return self.count_identity_hash().map(Some);
            }
            // A box typed as `Object` (`Object i = 7; i.hashCode()`).
            if self.equality_of(receiver) == Equality::Platform
                && let Some(hash) = self.java_hash(receiver)?
            {
                return Ok(Some(Value::Int(hash)));
            }
        }
        self.invoke_list_equality(r, opcode, receiver, args)
    }
}

/// `a.equals(b)` for a string or a box `a`: equal values of the same kind. A
/// `Double`/`Float` compares canonical bits (`NaN` equals `NaN`, `0.0` is not
/// `-0.0`).
fn platform_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(a), Value::Float(b)) => java_double_bits(*a) == java_double_bits(*b),
        _ => a == b,
    }
}

/// The JDK collection families [`Runtime::host_collection_kind`] tells apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CollectionKind {
    List,
    Set,
    Map,
}

impl<H: Host> Runtime<'_, H> {
    /// The JDK class a host collection marker (or a jar object whose class
    /// extends a concrete JDK collection, which the host keeps a row for)
    /// is, as the receiver's own class rather than the call site's owner.
    pub(crate) fn host_collection_class(&self, value: &Value) -> Option<String> {
        let marker = expect_ref(value)?;
        let (head, _) = marker
            .split_once(['#', '/'])
            .unwrap_or((marker.as_str(), ""));
        match head {
            "list" => return Some("java/util/ArrayList".into()),
            "set" => return Some("java/util/LinkedHashSet".into()),
            "map" => return Some("java/util/LinkedHashMap".into()),
            _ => {}
        }
        let mut class = jvm_dispatch::constructed_object_class(marker)
            .or_else(|| marker.split_once('#').map(|(class, _)| class))?
            .to_owned();
        // A jar subclass: the first JDK class up its chain.
        for _ in 0..MAX_SUPERCLASS_WALK {
            match self.class(&class) {
                Some(file) => class = file.super_name.clone(),
                None => break,
            }
        }
        (class.starts_with("java/util/") && self.host.mutable_collection_values(value).is_some())
            .then_some(class)
    }

    /// Whether a host collection is a list, a set or a map.
    pub(crate) fn host_collection_kind(&self, value: &Value) -> Option<CollectionKind> {
        let class = self.host_collection_class(value)?;
        if is_map_owner(&class) || java_immutable::supertypes(&class).contains(&"java/util/Map") {
            Some(CollectionKind::Map)
        } else if is_set_owner(&class)
            || java_immutable::supertypes(&class).contains(&"java/util/Set")
        {
            Some(CollectionKind::Set)
        } else if is_list_like_owner(&class)
            || java_immutable::supertypes(&class).contains(&"java/util/List")
        {
            Some(CollectionKind::List)
        } else {
            None
        }
    }

    /// A host collection's elements (a map's keys and values interleaved),
    /// in the order the host keeps them.
    pub(crate) fn collection_elements(&self, value: &Value) -> Result<Vec<Value>, NativeError> {
        runtime_collection_values(self.host, value)?
            .iter()
            .map(|item| parse_collection_value(item))
            .collect()
    }
}
