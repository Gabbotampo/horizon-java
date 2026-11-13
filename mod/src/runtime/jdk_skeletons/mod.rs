//! The JDK's abstract collection skeletons, run for jar subclasses exactly as
//! JDK 25 runs them.
//!
//! Library code builds its collections on the JDK's skeleton classes:
//! Guava's `ImmutableCollection` and fastutil's `AbstractObjectCollection`
//! extend `AbstractCollection`, a mod's `LruSet` extends `AbstractSet`, and
//! every one of them inherits most of its members as **template methods**
//! that call back into the subclass (`AbstractCollection.toArray` is
//! `size()`, `iterator()`, then `hasNext()`/`next()` until the iterator ends).
//! The runtime holds no JDK class files, so these bodies used to have no
//! implementation at all: the host's collection model answered from a row
//! it did not have, and #97 made those answers counted instead of silent.
//! This module runs them.
//!
//! **Approach.** Each JDK body is written here in Rust from its specification
//! (the `@implSpec` text, checked instruction by instruction against JDK
//! 25.0.5's `javap -c` listing for the order of its calls, never copied).
//! A body's calls go out through `jdk_call.rs`, so each one dispatches the
//! way the same call in a jar frame would: the jar's overrides, the host's
//! models, a lambda, or another skeleton body. That keeps the observable
//! contract exact: the same calls into the subclass in the same order with
//! the same arguments, the same exceptions with the same messages (an
//! exception a callback throws passes through unchanged, and a JDK body's
//! `catch` catches what the JDK's would), and the same answers.
//!
//! Executing the JDK's own bytecode was the alternative. A player has no JDK
//! to read it from (Horizon runs without Java, and `horizon-prepare` keeps no
//! runtime image), shipping it is out, and the bodies lean on JDK internals
//! (`ArraysSupport`, `SharedSecrets`, `jdk.internal` checks) the interpreter
//! would then have to model too.
//!
//! **Dispatch.** [`Runtime::invoke_jdk_skeleton`] runs right after the
//! receiver's own jar body (`jvm_dispatch.rs`) and before every host model,
//! so it sees exactly the calls JVMS §5.4.6 resolves to a JDK body:
//!
//! * `invokevirtual`/`invokeinterface` on an object of a jar class whose
//!   superclass chain leaves the jar at a skeleton, for a member no jar class
//!   on the chain declares: the most derived skeleton that declares it, or
//!   else the JDK interface default the class inherits;
//! * the same for a jar class implementing a JDK data interface directly
//!   (`implements Iterator` inherits `Iterator.remove`);
//! * `invokespecial` (`super.toString()`, `Iterator.super.remove()`) naming a
//!   skeleton or a data interface, without virtual selection;
//! * a skeleton constructor (`super()`), which keeps no host state.
//!
//! Each body runs as one frame: it counts against the jar-frame bound and
//! pushes the JDK frame a `StackWalker` would see.

use super::*;
use crate::runtime::jdk_call::{Site, throw};

mod collection;
mod defaults;
mod list;
mod map;
mod set_queue;

pub(crate) use list::{ABSTRACT_LIST, SEQUENTIAL_LIST};

/// The file whose body runs a selected JDK member.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Body {
    /// `collection.rs`: `AbstractCollection`.
    Collection,
    /// `set_queue.rs`: `AbstractSet`, `AbstractQueue`.
    SetQueue,
    /// `defaults.rs`: a JDK interface default.
    Default,
    /// `list.rs`: `AbstractList`, `AbstractSequentialList`, `List`'s defaults.
    List,
    /// `list.rs`/`map.rs`: a member a private JDK class declares
    /// (`AbstractList.Itr`, `AbstractMap`'s key set).
    Internal,
    /// `map.rs`: `AbstractMap`, its entries, `Map`'s defaults.
    Map,
}

/// Which file implements `owner.name descriptor`, if any.
fn body_for(owner: &str, name: &str, descriptor: &str) -> Option<Body> {
    let key = (owner, name, descriptor);
    if collection::BODIES.contains(&key) {
        Some(Body::Collection)
    } else if set_queue::BODIES.contains(&key) {
        Some(Body::SetQueue)
    } else if defaults::BODIES.contains(&key) {
        Some(Body::Default)
    } else if list::BODIES.contains(&key) {
        Some(Body::List)
    } else if map::BODIES.contains(&key) {
        Some(Body::Map)
    } else {
        None
    }
}

/// `java.util.AbstractCollection`.
pub(crate) const ABSTRACT_COLLECTION: &str = "java/util/AbstractCollection";
/// `java.util.AbstractSet`.
pub(crate) const ABSTRACT_SET: &str = "java/util/AbstractSet";
/// `java.util.AbstractQueue`.
pub(crate) const ABSTRACT_QUEUE: &str = "java/util/AbstractQueue";

/// `java.util.Collection`.
const COLLECTION: &str = "java/util/Collection";
/// `java.util.Iterator`.
const ITERATOR: &str = "java/util/Iterator";

/// Every supertype of each skeleton class: JDK 25.0.5's
/// `getSuperclass()`/`getInterfaces()` closure, printed by reflection.
const SUPERTYPES: &[(&str, &[&str])] = &[
    (
        ABSTRACT_COLLECTION,
        &["java/lang/Iterable", "java/lang/Object", COLLECTION],
    ),
    (
        ABSTRACT_SET,
        &[
            "java/lang/Iterable",
            "java/lang/Object",
            ABSTRACT_COLLECTION,
            COLLECTION,
            "java/util/Set",
        ],
    ),
    (
        ABSTRACT_QUEUE,
        &[
            "java/lang/Iterable",
            "java/lang/Object",
            ABSTRACT_COLLECTION,
            COLLECTION,
            "java/util/Queue",
        ],
    ),
    (
        "java/util/AbstractList",
        &[
            "java/lang/Iterable",
            "java/lang/Object",
            ABSTRACT_COLLECTION,
            COLLECTION,
            "java/util/List",
            "java/util/SequencedCollection",
        ],
    ),
    (
        "java/util/AbstractSequentialList",
        &[
            "java/lang/Iterable",
            "java/lang/Object",
            ABSTRACT_COLLECTION,
            "java/util/AbstractList",
            COLLECTION,
            "java/util/List",
            "java/util/SequencedCollection",
        ],
    ),
    (
        "java/util/AbstractMap",
        &["java/lang/Object", "java/util/Map"],
    ),
    (
        "java/util/AbstractMap$SimpleEntry",
        &[
            "java/io/Serializable",
            "java/lang/Object",
            "java/util/Map$Entry",
        ],
    ),
    (
        "java/util/AbstractMap$SimpleImmutableEntry",
        &[
            "java/io/Serializable",
            "java/lang/Object",
            "java/util/Map$Entry",
        ],
    ),
];

/// Whether the skeleton class `class` is `wanted` or one of its supertypes,
/// for `instanceof` on a jar subclass whose chain reaches it. `None` when
/// `class` is not a skeleton.
pub(crate) fn skeleton_is_instance(class: &str, wanted: &str) -> Option<bool> {
    if let Some(answer) = internal_is_instance(class, wanted) {
        return Some(answer);
    }
    let (_, supertypes) = SUPERTYPES.iter().find(|(skeleton, _)| *skeleton == class)?;
    Some(class == wanted || supertypes.contains(&wanted))
}

/// The JDK superclasses whose subclasses this module runs: their own bodies
/// and the interface defaults they inherit. A default on a subclass of any
/// other skeleton calls that skeleton's members (`AbstractList.iterator`),
/// which stay counted until they are implemented here.
const IMPLEMENTED_BASES: &[&str] = &[
    "java/lang/Object",
    "java/lang/Record",
    "java/lang/Enum",
    ABSTRACT_COLLECTION,
    ABSTRACT_SET,
    ABSTRACT_QUEUE,
    ABSTRACT_LIST,
    SEQUENTIAL_LIST,
    map::ABSTRACT_MAP,
    map::SIMPLE_ENTRY,
    map::IMMUTABLE_ENTRY,
];

/// The private JDK classes the bodies construct: `(class, superclass,
/// direct interfaces)` (JDK 25.0.5).
const INTERNAL: &[(&str, &str, &[&str])] = &[
    (list::ITR, "java/lang/Object", &[ITERATOR]),
    (list::LIST_ITR, list::ITR, &["java/util/ListIterator"]),
    (list::SUB_LIST, ABSTRACT_LIST, &[]),
    (
        list::RANDOM_ACCESS_SUB_LIST,
        list::SUB_LIST,
        &["java/util/RandomAccess"],
    ),
    (
        list::SUB_LIST_ITERATOR,
        "java/lang/Object",
        &["java/util/ListIterator"],
    ),
    (map::KEY_SET, ABSTRACT_SET, &[]),
    (map::VALUES, ABSTRACT_COLLECTION, &[]),
    (map::KEY_ITERATOR, "java/lang/Object", &[ITERATOR]),
    (map::VALUE_ITERATOR, "java/lang/Object", &[ITERATOR]),
];

/// Whether `class` is a private JDK class `wanted` is a supertype of (or is),
/// through its internal chain and then its skeleton's supertypes.
fn internal_is_instance(class: &str, wanted: &str) -> Option<bool> {
    let mut current = class;
    INTERNAL
        .iter()
        .find(|(internal, _, _)| *internal == class)?;
    for _ in 0..=INTERNAL.len() {
        if current == wanted {
            return Some(true);
        }
        let Some((_, superclass, interfaces)) = INTERNAL
            .iter()
            .find(|(internal, _, _)| *internal == current)
        else {
            return Some(
                skeleton_is_instance(current, wanted).unwrap_or(false)
                    || wanted == "java/lang/Object",
            );
        };
        if interfaces.iter().any(|interface| {
            *interface == wanted || (*interface == "java/util/ListIterator" && wanted == ITERATOR)
        }) {
            return Some(true);
        }
        current = superclass;
    }
    Some(false)
}

/// The class that declares the instance field `name descriptor` among the
/// skeletons `class` is or extends: `AbstractList.modCount`, the one field a
/// jar subclass can see. `None` for any other field.
pub(crate) fn skeleton_field_owner(
    class: &str,
    name: &str,
    descriptor: &str,
) -> Option<&'static str> {
    ((name, descriptor) == ("modCount", "I")
        && [
            ABSTRACT_LIST,
            SEQUENTIAL_LIST,
            list::SUB_LIST,
            list::RANDOM_ACCESS_SUB_LIST,
        ]
        .contains(&class))
    .then_some(ABSTRACT_LIST)
}

/// A boolean result as Java's `int`.
fn boolean(value: bool) -> Value {
    Value::Int(i32::from(value))
}

/// A `boolean` answer.
fn truth(value: &Value) -> bool {
    matches!(value, Value::Int(flag) if *flag != 0)
}

/// An `int` answer (`0` for anything else, which a well-typed call never is).
fn int_of(value: &Value) -> i32 {
    match value {
        Value::Int(value) => *value,
        _ => 0,
    }
}

/// `Objects.requireNonNull(value)`: a message-less `NullPointerException`.
fn require_non_null(value: &Value) -> Result<(), NativeError> {
    if *value == Value::Ref(None) {
        return Err(throw("java/lang/NullPointerException", None));
    }
    Ok(())
}

impl<H: Host> Runtime<'_, H> {
    /// Run the JDK skeleton body or interface default `r` selects on
    /// `receiver`, when it is one (see the module docs). `Ok(None)` leaves the
    /// call to the rest of the chain.
    pub(crate) fn invoke_jdk_skeleton(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(this) = receiver else {
            return Ok(None);
        };
        if let Some(answer) = self.host_entry_equals(r, opcode, this, args)? {
            return Ok(Some(answer));
        }
        if let Some(class) =
            expect_ref(this).and_then(|marker| jvm_dispatch::constructed_object_class(marker))
            && INTERNAL.iter().any(|(internal, _, _)| *internal == class)
        {
            let class = class.to_owned();
            return self.invoke_internal(&class, r, opcode, this, args);
        }
        // A jar object on a skeleton, or a `SimpleEntry`/`SimpleImmutableEntry`
        // itself (its state is the same two fields either way).
        let direct_entry = expect_ref(this)
            .and_then(|marker| jvm_dispatch::constructed_object_class(marker))
            .filter(|class| [map::SIMPLE_ENTRY, map::IMMUTABLE_ENTRY].contains(class))
            .map(str::to_owned);
        let Some(base) = self.stateless_jar_object(this).or(direct_entry) else {
            return Ok(None);
        };
        let Some(owner) = self.skeleton_selection(r, opcode, this, &base) else {
            return Ok(None);
        };
        // A skeleton constructor: `AbstractCollection()` and its kin keep no
        // state the subclass can observe.
        if r.name == "<init>" {
            if map::is_entry_constructor(&owner, &r.descriptor) {
                self.entry_init(&owner, this, args)?;
                return Ok(Some(Value::Void));
            }
            return Ok(jdk_skeleton_constructor(&owner, &r.descriptor).then_some(Value::Void));
        }
        if !IMPLEMENTED_BASES.contains(&base.as_str()) {
            return Ok(None);
        }
        self.run_skeleton_body(&owner, r, this, args)
    }

    /// A call on an object of a private JDK class: its own member, else what
    /// it inherits from its skeleton superclass or its interfaces.
    fn invoke_internal(
        &mut self,
        class: &str,
        r: &MethodRef,
        opcode: u8,
        this: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if !matches!(opcode, 0xb6 | 0xb9) || r.name.starts_with('<') {
            return Ok(None);
        }
        let mut current = class;
        let mut direct = Vec::new();
        while let Some((_, superclass, interfaces)) = INTERNAL
            .iter()
            .find(|(internal, _, _)| *internal == current)
        {
            let member = (current, r.name.as_str(), r.descriptor.as_str());
            if list::OWN_MEMBERS.contains(&member) || map::OWN_MEMBERS.contains(&member) {
                let owner = current.to_owned();
                return self.run_body(Body::Internal, &owner, r, this, args);
            }
            direct.extend_from_slice(interfaces);
            current = superclass;
        }
        let owner =
            jar_operands::skeleton_declaring(current, &r.name, &r.descriptor).or_else(|| {
                if matches!(r.name.as_str(), "equals" | "hashCode" | "toString") {
                    return None;
                }
                let interfaces = jar_operands::data_interfaces_of(&direct, current);
                jar_operands::data_default_owner(&interfaces, &r.name, &r.descriptor)
            });
        match owner {
            Some(owner) => self.run_skeleton_body(owner, r, this, args),
            None => Ok(None),
        }
    }

    /// Which JDK body `r` selects on a jar object whose chain leaves the jar
    /// at `base`: the skeleton or JDK interface that declares it.
    fn skeleton_selection(
        &self,
        r: &MethodRef,
        opcode: u8,
        this: &Value,
        base: &str,
    ) -> Option<String> {
        let skeleton_owner = |class: &str| {
            jar_operands::skeleton_declaring(class, &r.name, &r.descriptor).map(str::to_owned)
        };
        let interface_default = |interfaces: &[&'static str]| {
            jar_operands::data_default_owner(interfaces, &r.name, &r.descriptor).map(str::to_owned)
        };
        match opcode {
            // `super(...)`, `super.m()` and `Interface.super.m()`: the named
            // class's member, no virtual selection.
            0xb7 => {
                if r.name == "<init>" {
                    return jar_operands::is_skeleton(&r.owner).then(|| r.owner.clone());
                }
                let (owner, jar) = self.member_owner(&r.owner, &r.name, &r.descriptor)?;
                if jar {
                    return None;
                }
                skeleton_owner(&owner).or_else(|| interface_default(&[data_interface(&owner)?]))
            }
            0xb6 | 0xb9 if !r.name.starts_with('<') => {
                let class = jvm_dispatch::constructed_object_class(expect_ref(this)?)?;
                // A jar declaration (abstract, or a jar interface's default
                // the JVM dispatch already declined) is not a JDK body.
                if self
                    .member_owner(class, &r.name, &r.descriptor)
                    .is_some_and(|(_, jar)| jar)
                {
                    return None;
                }
                skeleton_owner(base).or_else(|| {
                    if matches!(r.name.as_str(), "equals" | "hashCode" | "toString") {
                        return None;
                    }
                    interface_default(&self.implemented_data_interfaces(this, base))
                })
            }
            _ => None,
        }
    }

    /// Run one selected body as a JDK frame: bounded like a jar frame, and
    /// seen by `StackWalker`. `Ok(None)` when the body is not implemented
    /// here (the caller's chain keeps it, and `jar_operands.rs` counts it).
    fn run_skeleton_body(
        &mut self,
        owner: &str,
        r: &MethodRef,
        this: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(body) = body_for(owner, &r.name, &r.descriptor) else {
            return Ok(None);
        };
        if body == Body::Default && !self.default_runs_here(owner, &r.name, this) {
            return Ok(None);
        }
        self.run_body(body, owner, r, this, args)
    }

    /// Run `body` as the JDK frame `owner.name`.
    fn run_body(
        &mut self,
        body: Body,
        owner: &str,
        r: &MethodRef,
        this: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if !frame_bound::admits_frame(self.depth) {
            return Err(NativeError::CallDepthExceeded {
                class: owner.to_owned(),
                method: r.name.clone(),
                limit: self.depth,
            });
        }
        self.depth += 1;
        stack_walk::enter(owner, &r.name);
        let (name, descriptor) = (r.name.as_str(), r.descriptor.as_str());
        let result = match body {
            Body::Collection => self.collection_body(owner, name, descriptor, this, args),
            Body::SetQueue => self.set_queue_body(owner, name, descriptor, this, args),
            Body::Default => self.interface_default_body(owner, name, descriptor, this, args),
            Body::List => self.list_body(owner, name, descriptor, this, args),
            Body::Internal if owner.starts_with(map::ABSTRACT_MAP) => {
                self.map_view_body(owner, name, this, args)
            }
            Body::Internal => self.internal_body(owner, name, descriptor, this, args),
            Body::Map => self.map_body(owner, name, descriptor, this, args),
        };
        stack_walk::leave();
        self.depth -= 1;
        match result {
            // A host answer the body needed was counted and null: the body
            // stops with its own neutral answer (`jdk_call::ABANDONED`).
            Err(NativeError::Metadata(message)) if message == jdk_call::ABANDONED => {
                Ok(Some(Value::default_for(&r.descriptor)))
            }
            other => other.map(Some),
        }
    }

    /// `hostEntry.equals(o)` where `o` is not a host value: the JDK's entry
    /// classes (`HashMap.Node`, `KeyValueHolder`, `TreeMap.Entry`) answer
    /// `o instanceof Map.Entry e && eq(key, e.getKey()) && eq(value,
    /// e.getValue())`, reading a jar entry's own accessors. The host compares
    /// markers and would answer `false` without asking the jar.
    fn host_entry_equals(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        this: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if !matches!(opcode, 0xb6 | 0xb9)
            || (r.name.as_str(), r.descriptor.as_str()) != ("equals", "(Ljava/lang/Object;)Z")
            || !expect_ref(this).is_some_and(|marker| marker.starts_with("map-entry/"))
        {
            return Ok(None);
        }
        let other = args.first().cloned().unwrap_or(Value::Ref(None));
        let Some(class) =
            expect_ref(&other).and_then(|marker| jvm_dispatch::constructed_object_class(marker))
        else {
            return Ok(None);
        };
        if self.class(class).is_none() && !jar_operands::is_skeleton(class) {
            return Ok(None);
        }
        if !self.java_instance_of(&other, "java/util/Map$Entry")? {
            return Ok(Some(boolean(false)));
        }
        for part in ["getKey", "getValue"] {
            let mine = self.call_virtual(
                Site::Interface,
                "java/util/Map$Entry",
                part,
                "()Ljava/lang/Object;",
                this,
                &[],
            )?;
            let theirs = self.call_virtual(
                Site::Interface,
                "java/util/Map$Entry",
                part,
                "()Ljava/lang/Object;",
                &other,
                &[],
            )?;
            let equal =
                mine == theirs || (mine != Value::Ref(None) && self.java_equals(&mine, &theirs)?);
            if !equal {
                return Ok(Some(boolean(false)));
            }
        }
        Ok(Some(boolean(true)))
    }

    // ---- calls a JDK body makes ----

    /// `it.hasNext()` (`invokeinterface Iterator.hasNext`).
    fn has_next(&mut self, it: &Value) -> Result<bool, NativeError> {
        let answer = self.call_virtual(Site::Interface, ITERATOR, "hasNext", "()Z", it, &[])?;
        Ok(truth(&answer))
    }

    /// `it.next()`.
    fn next(&mut self, it: &Value) -> Result<Value, NativeError> {
        self.call_virtual(
            Site::Interface,
            ITERATOR,
            "next",
            "()Ljava/lang/Object;",
            it,
            &[],
        )
    }

    /// `it.remove()`.
    fn remove_current(&mut self, it: &Value) -> Result<(), NativeError> {
        self.call_virtual(Site::Interface, ITERATOR, "remove", "()V", it, &[])
            .map(drop)
    }

    /// `this.iterator()` as the body's own `invokevirtual owner.iterator` (or
    /// `invokeinterface` for an interface default).
    fn iterator_of(&mut self, site: Site, owner: &str, this: &Value) -> Result<Value, NativeError> {
        self.call_virtual(site, owner, "iterator", "()Ljava/util/Iterator;", this, &[])
    }

    /// `this.size()`.
    fn size_of(&mut self, site: Site, owner: &str, this: &Value) -> Result<i32, NativeError> {
        Ok(int_of(&self.call_virtual(
            site,
            owner,
            "size",
            "()I",
            this,
            &[],
        )?))
    }

    /// `a.equals(b)` (`invokevirtual Object.equals`).
    fn java_equals(&mut self, a: &Value, b: &Value) -> Result<bool, NativeError> {
        let answer = self.call_virtual(
            Site::Virtual,
            "java/lang/Object",
            "equals",
            "(Ljava/lang/Object;)Z",
            a,
            std::slice::from_ref(b),
        )?;
        Ok(truth(&answer))
    }

    /// `value.hashCode()` (`invokevirtual Object.hashCode`).
    fn java_hash(&mut self, value: &Value) -> Result<i32, NativeError> {
        Ok(int_of(&self.call_virtual(
            Site::Virtual,
            "java/lang/Object",
            "hashCode",
            "()I",
            value,
            &[],
        )?))
    }

    /// `String.valueOf(value)`, as `StringBuilder.append(Object)` prints it.
    fn java_text(&mut self, value: &Value) -> Result<String, NativeError> {
        let text = self.call_static(
            "java/lang/String",
            "valueOf",
            "(Ljava/lang/Object;)Ljava/lang/String;",
            std::slice::from_ref(value),
        )?;
        Ok(expect_ref(&text).cloned().unwrap_or_else(|| "null".into()))
    }

    /// One functional-interface call (`Consumer.accept`, `Predicate.test`).
    fn call_functional(
        &mut self,
        interface: &str,
        name: &str,
        descriptor: &str,
        callback: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        self.call_virtual(Site::Interface, interface, name, descriptor, callback, args)
    }

    // ---- arrays a JDK body builds ----

    /// A new array of `component` holding `values`.
    fn new_reference_array(&mut self, component: &str, values: Vec<Value>) -> Value {
        let marker = self.new_array("java/util/AbstractCollection", "toArray");
        self.arrays.insert(marker.clone(), values);
        self.record_array_component(&marker, component);
        Value::Ref(Some(marker))
    }

    /// The component class an array was created with (`Object` when the
    /// runtime did not record one).
    fn array_component(&self, array: &Value) -> String {
        expect_ref(array)
            .and_then(|marker| {
                self.fields
                    .get(&format!("{marker}{}", value_class::ARRAY_COMPONENT_FIELD))
            })
            .and_then(expect_ref)
            .cloned()
            .unwrap_or_else(|| "java/lang/Object".into())
    }

    /// `array[index] = value` on an array this run owns.
    fn store_element(
        &mut self,
        array: &Value,
        index: usize,
        value: Value,
    ) -> Result<(), NativeError> {
        let marker = expect_array_marker(array)?;
        let values = self
            .arrays
            .get_mut(marker)
            .ok_or_else(|| NativeError::Metadata("array is not owned by this invocation".into()))?;
        let length = values.len();
        let slot = values
            .get_mut(index)
            .ok_or_else(|| array_index_out_of_bounds(index as i32, length))?;
        *slot = value;
        Ok(())
    }
}

/// The data interface `class` is, when it is one (an `invokespecial
/// Iterator.remove` names the interface itself).
fn data_interface(class: &str) -> Option<&'static str> {
    [
        "java/lang/Iterable",
        COLLECTION,
        "java/util/List",
        "java/util/Set",
        "java/util/Queue",
        "java/util/Map",
        ITERATOR,
        "java/util/ListIterator",
    ]
    .into_iter()
    .find(|interface| *interface == class)
}

/// Whether `owner descriptor` is a skeleton constructor whose JDK body keeps
/// no state (every skeleton's no-argument constructor).
fn jdk_skeleton_constructor(owner: &str, descriptor: &str) -> bool {
    descriptor == "()V"
        && [
            ABSTRACT_COLLECTION,
            ABSTRACT_SET,
            ABSTRACT_QUEUE,
            ABSTRACT_LIST,
            SEQUENTIAL_LIST,
            map::ABSTRACT_MAP,
        ]
        .contains(&owner)
}
