//! Count a host model's answer about a jar object it has no state for.
//!
//! The host models JDK collections, maps, iterators and text by their own
//! markers and tables. A jar object (`class#n`) whose class does not extend a
//! concrete JDK class has no row there: its state lives in the interpreter's
//! fields, and only its own bytecode can read it. A model handed one used to
//! answer from nothing, neutrally and uncounted. A real case: an interpreted
//! fastutil `EntrySet` reached `Collection.toArray(T[])`, which it inherits
//! from the JDK's `AbstractCollection`. That body iterates `this.iterator()`.
//! The host's model answered an empty array from the empty row the
//! `AbstractSet()` constructor had left, so a decoded map came out empty and
//! the run still read `ran-clean`.
//!
//! [`jvm_dispatch`](crate::runtime::jvm_dispatch) already runs every member the jar
//! declares itself. What still reaches the models for such an object is one of
//! these:
//!
//! * **A member the object inherits from a JDK skeleton** (`AbstractCollection`,
//!   `AbstractList`, `AbstractSet`, `AbstractMap`, `SimpleEntry`...). The JDK
//!   body is a template over the jar's own methods (`toArray`/`contains`/
//!   `toString` over `iterator()`; `equals`/`hashCode` over the elements), or
//!   it reads state the JDK class keeps (`SimpleEntry.getKey`). Neither is
//!   anything the host holds.
//! * **A JDK interface default on a data type** (`Iterable.forEach`,
//!   `Collection.stream`, `Map.getOrDefault`, `Iterator.forEachRemaining`...).
//!   The JDK body calls back into the jar's methods.
//! * **The object passed as an argument** of a data type (`Collection`, `Map`,
//!   `Iterable`, `CharSequence`...) to a model: `new ArrayList<>(jarSet)`,
//!   `list.addAll(jarList)`, `Map.copyOf(jarMap)`, `String.join(",", jarIterable)`.
//!   The model reads the operand's elements from its own table.
//!
//! Each one is now counted as an unknown host call, under the JDK member that
//! would run (`java/util/AbstractCollection#toArray(...)` for the inherited
//! case, the call itself for an argument), and answers the neutral default.
//! A run that reaches one reads `ran-hollow`, never a silent `ran-clean`.
//! Running these templates through the interpreter is the next step. Until
//! then, the census shows how often each one is reached.
//!
//! Exempt:
//!
//! * an object whose class extends a **concrete** JDK class (`ArrayList`,
//!   `HashMap`, `ConcurrentHashMap`, `Thread`...): its `super(...)` call made
//!   the host's row, and that row is the object's real state;
//! * `Object`'s identity members (`getClass`, `hashCode`, `equals`,
//!   `toString`) on an object whose chain reaches `Object`, `Record` or `Enum`:
//!   identity, the record and the enum models answer those exactly;
//! * `isEmpty()` on a skeleton subclass, which `collection_base.rs` already
//!   runs as `size() == 0` through the interpreter, exactly as the JDK does.

use super::*;

/// The JDK data interfaces whose default methods and parameters a host model
/// answers from its own tables.
const DATA_TYPES: &[&str] = &[
    "java/lang/Iterable",
    "java/lang/CharSequence",
    "java/util/Collection",
    "java/util/SequencedCollection",
    "java/util/List",
    "java/util/Set",
    "java/util/SequencedSet",
    "java/util/SortedSet",
    "java/util/NavigableSet",
    "java/util/Queue",
    "java/util/Deque",
    "java/util/Map",
    "java/util/SequencedMap",
    "java/util/SortedMap",
    "java/util/NavigableMap",
    "java/util/concurrent/ConcurrentMap",
    "java/util/Map$Entry",
    "java/util/Iterator",
    "java/util/ListIterator",
];

/// `Object`'s members that never depend on the object's data.
const IDENTITY_MEMBERS: &[&str] = &["getClass", "notify", "notifyAll", "wait"];

/// Whether `class` is one of the JDK skeleton classes `jvm_dispatch` knows.
fn is_skeleton(class: &str) -> bool {
    jvm_dispatch::JDK_SKELETONS
        .iter()
        .any(|(skeleton, _, _)| *skeleton == class)
}

/// The skeleton class on `class`'s JDK chain that declares `name descriptor`
/// itself, for the counted key.
pub(crate) fn skeleton_declaring(
    class: &str,
    name: &str,
    descriptor: &str,
) -> Option<&'static str> {
    let mut current = class;
    for _ in 0..jvm_dispatch::JDK_SKELETONS.len() {
        let (skeleton, superclass, declared) = jvm_dispatch::JDK_SKELETONS
            .iter()
            .find(|(skeleton, _, _)| *skeleton == current)?;
        if declared.iter().any(|member| {
            member
                .strip_prefix(name)
                .is_some_and(|rest| rest == descriptor)
        }) {
            return Some(skeleton);
        }
        current = superclass;
    }
    None
}

/// The class names in a method descriptor's parameter list (`Ljava/util/List;`
/// → `java/util/List`), with `None` for primitives and arrays, in order.
fn parameter_classes(descriptor: &str) -> Vec<Option<String>> {
    descriptor_arguments(descriptor)
        .into_iter()
        .map(|parameter| {
            parameter
                .strip_prefix('L')
                .and_then(|name| name.strip_suffix(';'))
                .map(str::to_owned)
        })
        .collect()
}

impl<H: Host> Runtime<'_, H> {
    /// The JDK superclass a jar object's chain ends at, when the host keeps no
    /// state for it: `Object`, `Record`, `Enum` or a JDK skeleton. `None` for
    /// every other value (host markers, strings, boxes, objects of host or
    /// library-modelled classes, subclasses of concrete JDK or game classes).
    pub(crate) fn stateless_jar_object(&self, value: &Value) -> Option<String> {
        let class = jvm_dispatch::constructed_object_class(expect_ref(value)?)?;
        if self.host.models_class(class) || jvm_dispatch::library_modelled(class) {
            return None;
        }
        let mut current = class.to_owned();
        for _ in 0..MAX_SUPERCLASS_WALK {
            let Some(file) = self.class(&current) else {
                return (matches!(
                    current.as_str(),
                    "java/lang/Object" | "java/lang/Record" | "java/lang/Enum"
                ) || is_skeleton(&current))
                .then_some(current);
            };
            if file.super_name.is_empty() {
                return None;
            }
            current = file.super_name.clone();
        }
        None
    }

    /// Count an `invokevirtual`/`invokeinterface`/`invokestatic`/
    /// `invokespecial` of `r` that a host model would answer about a jar object
    /// it holds no state for, and answer the default. `Ok(None)` leaves the
    /// call to the chain. Runs after [`Self::jvm_selected_jar_body`], so a
    /// member the jar declares never gets here.
    pub(crate) fn count_unmodelled_jar_operand(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        // A call whose body is the jar's own (a jar owner, `invokespecial` of
        // a jar superclass, a jar static) passes its operands to bytecode,
        // which reads them through their own methods.
        let jar_body = self
            .member_owner(&r.owner, &r.name, &r.descriptor)
            .is_some_and(|(_, jar)| jar);
        if jar_body {
            return Ok(None);
        }
        if matches!(opcode, 0xb6 | 0xb9)
            && let Some(receiver) = receiver
            && let Some(base) = self.stateless_jar_object(receiver)
            && let Some(counted) = self.inherited_member(r, &base)
        {
            // `AbstractCollection.isEmpty` is `size() == 0`, run through the
            // interpreter by `collection_base.rs`.
            if let Some(value) = self.invoke_collection_default(r, Some(receiver), args)? {
                return Ok(Some(value));
            }
            return self.host.unknown_operand_call(&counted).map(Some);
        }
        let classes = parameter_classes(&r.descriptor);
        let unreadable = classes.iter().zip(args).any(|(class, value)| {
            class
                .as_deref()
                .is_some_and(|class| DATA_TYPES.contains(&class) || is_skeleton(class))
                && self.stateless_jar_object(value).is_some()
        });
        if !unreadable {
            return Ok(None);
        }
        let value = self.host.unknown_operand_call(r)?;
        // A counted constructor still leaves an (empty) object behind, as the
        // no-argument constructor would, so later calls on it are answered
        // rather than refused as an unknown marker.
        if r.name == "<init>"
            && let Some(receiver) = receiver
        {
            let empty = MethodRef {
                owner: r.owner.clone(),
                name: "<init>".into(),
                descriptor: "()V".into(),
            };
            self.host.invoke_instance(&empty, receiver, &[])?;
        }
        Ok(Some(value))
    }

    /// The JDK member that would run for `r` on a jar object whose JDK
    /// superclass is `base`, when its answer depends on the object's data:
    /// a skeleton's own member (template or JDK state), or a data
    /// interface's default. `None` for identity members and `Object`'s
    /// exactly-modelled ones.
    fn inherited_member(&self, r: &MethodRef, base: &str) -> Option<MethodRef> {
        if r.name.starts_with('<') || IDENTITY_MEMBERS.contains(&r.name.as_str()) {
            return None;
        }
        if is_skeleton(base) {
            if let Some(owner) = skeleton_declaring(base, &r.name, &r.descriptor) {
                return Some(MethodRef {
                    owner: owner.to_owned(),
                    ..r.clone()
                });
            }
            // An `Object` member the skeleton does not override keeps
            // `Object`'s identity answer (`AbstractQueue.hashCode`).
            if matches!(r.name.as_str(), "hashCode" | "equals" | "toString") {
                return None;
            }
        }
        // A data interface's default (the call site names the interface or a
        // jar type that inherits it).
        if DATA_TYPES.contains(&r.owner.as_str())
            && !matches!(r.name.as_str(), "hashCode" | "equals" | "toString")
        {
            return Some(r.clone());
        }
        None
    }
}
