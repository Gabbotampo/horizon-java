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

/// The JDK data interfaces' direct superinterfaces and their own default
/// methods, read from JDK 25.0.5 by reflection (`getInterfaces()`,
/// `getDeclaredMethods()` filtered by `isDefault()`): API names only.
const DATA_INTERFACES: &[(&str, &[&str], &[&str])] = &[
    (
        "java/lang/Iterable",
        &[],
        &[
            "forEach(Ljava/util/function/Consumer;)V",
            "spliterator()Ljava/util/Spliterator;",
        ],
    ),
    (
        "java/lang/CharSequence",
        &[],
        &[
            "chars()Ljava/util/stream/IntStream;",
            "codePoints()Ljava/util/stream/IntStream;",
            "getChars(II[CI)V",
            "isEmpty()Z",
        ],
    ),
    (
        "java/util/Collection",
        &["java/lang/Iterable"],
        &[
            "parallelStream()Ljava/util/stream/Stream;",
            "removeIf(Ljava/util/function/Predicate;)Z",
            "spliterator()Ljava/util/Spliterator;",
            "stream()Ljava/util/stream/Stream;",
            "toArray(Ljava/util/function/IntFunction;)[Ljava/lang/Object;",
        ],
    ),
    (
        "java/util/SequencedCollection",
        &["java/util/Collection"],
        &[
            "addFirst(Ljava/lang/Object;)V",
            "addLast(Ljava/lang/Object;)V",
            "getFirst()Ljava/lang/Object;",
            "getLast()Ljava/lang/Object;",
            "removeFirst()Ljava/lang/Object;",
            "removeLast()Ljava/lang/Object;",
        ],
    ),
    (
        "java/util/List",
        &["java/util/SequencedCollection"],
        &[
            "addFirst(Ljava/lang/Object;)V",
            "addLast(Ljava/lang/Object;)V",
            "getFirst()Ljava/lang/Object;",
            "getLast()Ljava/lang/Object;",
            "removeFirst()Ljava/lang/Object;",
            "removeLast()Ljava/lang/Object;",
            "replaceAll(Ljava/util/function/UnaryOperator;)V",
            "reversed()Ljava/util/List;",
            "reversed()Ljava/util/SequencedCollection;",
            "sort(Ljava/util/Comparator;)V",
            "spliterator()Ljava/util/Spliterator;",
        ],
    ),
    (
        "java/util/Set",
        &["java/util/Collection"],
        &["spliterator()Ljava/util/Spliterator;"],
    ),
    (
        "java/util/SequencedSet",
        &["java/util/SequencedCollection", "java/util/Set"],
        &["reversed()Ljava/util/SequencedCollection;"],
    ),
    (
        "java/util/SortedSet",
        &["java/util/Set", "java/util/SequencedSet"],
        &[
            "addFirst(Ljava/lang/Object;)V",
            "addLast(Ljava/lang/Object;)V",
            "getFirst()Ljava/lang/Object;",
            "getLast()Ljava/lang/Object;",
            "removeFirst()Ljava/lang/Object;",
            "removeLast()Ljava/lang/Object;",
            "reversed()Ljava/util/SequencedCollection;",
            "reversed()Ljava/util/SequencedSet;",
            "reversed()Ljava/util/SortedSet;",
            "spliterator()Ljava/util/Spliterator;",
        ],
    ),
    (
        "java/util/NavigableSet",
        &["java/util/SortedSet"],
        &[
            "removeFirst()Ljava/lang/Object;",
            "removeLast()Ljava/lang/Object;",
            "reversed()Ljava/util/NavigableSet;",
            "reversed()Ljava/util/SequencedCollection;",
            "reversed()Ljava/util/SequencedSet;",
            "reversed()Ljava/util/SortedSet;",
        ],
    ),
    ("java/util/Queue", &["java/util/Collection"], &[]),
    (
        "java/util/Deque",
        &["java/util/Queue", "java/util/SequencedCollection"],
        &[
            "reversed()Ljava/util/Deque;",
            "reversed()Ljava/util/SequencedCollection;",
        ],
    ),
    (
        "java/util/Map",
        &[],
        &[
            "compute(Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
            "computeIfAbsent(Ljava/lang/Object;Ljava/util/function/Function;)Ljava/lang/Object;",
            "computeIfPresent(Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
            "forEach(Ljava/util/function/BiConsumer;)V",
            "getOrDefault(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            "merge(Ljava/lang/Object;Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
            "putIfAbsent(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            "remove(Ljava/lang/Object;Ljava/lang/Object;)Z",
            "replace(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            "replace(Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;)Z",
            "replaceAll(Ljava/util/function/BiFunction;)V",
        ],
    ),
    (
        "java/util/SequencedMap",
        &["java/util/Map"],
        &[
            "firstEntry()Ljava/util/Map$Entry;",
            "lastEntry()Ljava/util/Map$Entry;",
            "pollFirstEntry()Ljava/util/Map$Entry;",
            "pollLastEntry()Ljava/util/Map$Entry;",
            "putFirst(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            "putLast(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            "sequencedEntrySet()Ljava/util/SequencedSet;",
            "sequencedKeySet()Ljava/util/SequencedSet;",
            "sequencedValues()Ljava/util/SequencedCollection;",
        ],
    ),
    (
        "java/util/SortedMap",
        &["java/util/SequencedMap"],
        &[
            "putFirst(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            "putLast(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            "reversed()Ljava/util/SequencedMap;",
            "reversed()Ljava/util/SortedMap;",
        ],
    ),
    (
        "java/util/NavigableMap",
        &["java/util/SortedMap"],
        &[
            "reversed()Ljava/util/NavigableMap;",
            "reversed()Ljava/util/SequencedMap;",
            "reversed()Ljava/util/SortedMap;",
        ],
    ),
    (
        "java/util/concurrent/ConcurrentMap",
        &["java/util/Map"],
        &[
            "compute(Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
            "computeIfAbsent(Ljava/lang/Object;Ljava/util/function/Function;)Ljava/lang/Object;",
            "computeIfPresent(Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
            "forEach(Ljava/util/function/BiConsumer;)V",
            "getOrDefault(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            "merge(Ljava/lang/Object;Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;",
            "replaceAll(Ljava/util/function/BiFunction;)V",
        ],
    ),
    ("java/util/Map$Entry", &[], &[]),
    (
        "java/util/Iterator",
        &[],
        &[
            "forEachRemaining(Ljava/util/function/Consumer;)V",
            "remove()V",
        ],
    ),
    ("java/util/ListIterator", &["java/util/Iterator"], &[]),
];

/// The data interfaces each skeleton class implements directly (JDK 25
/// `getInterfaces()`); a skeleton's superclass adds its own.
const SKELETON_INTERFACES: &[(&str, &str)] = &[
    ("java/util/AbstractCollection", "java/util/Collection"),
    ("java/util/AbstractList", "java/util/List"),
    ("java/util/AbstractSet", "java/util/Set"),
    ("java/util/AbstractQueue", "java/util/Queue"),
    ("java/util/AbstractMap", "java/util/Map"),
    ("java/util/AbstractMap$SimpleEntry", "java/util/Map$Entry"),
    (
        "java/util/AbstractMap$SimpleImmutableEntry",
        "java/util/Map$Entry",
    ),
];

/// Members of a concrete JDK collection whose JDK body calls back into an
/// overridable method of the same object, and those methods: when a jar
/// subclass overrides one, the host's answer (from its own row) is not the
/// JDK's. Read from the JDK's documented implementations:
/// `AbstractCollection.toString`/`containsAll`/`addAll`/`removeAll`/
/// `retainAll`/`toArray` (over `iterator()`, `contains`, `add`, `remove`,
/// `size`), `AbstractMap.toString`/`equals`/`hashCode` (over `entrySet()`),
/// `AbstractList`/`AbstractSet` `equals`/`hashCode`, `ArrayList.contains`
/// (over `indexOf`), `Iterable.forEach`, and `LinkedHashMap`'s insertions
/// (`afterNodeInsertion` → `removeEldestEntry`). The list is conservative: a
/// concrete class that does not call back for one of them is counted too.
const CALLBACKS: &[(&str, &[&str])] = &[
    ("toString", &["iterator", "entrySet"]),
    ("containsAll", &["contains", "iterator"]),
    (
        "equals",
        &[
            "iterator",
            "entrySet",
            "size",
            "contains",
            "containsAll",
            "get",
            "containsKey",
        ],
    ),
    ("hashCode", &["iterator", "entrySet"]),
    ("contains", &["indexOf"]),
    ("addAll", &["add"]),
    ("removeAll", &["iterator", "remove", "contains", "size"]),
    ("retainAll", &["iterator", "remove", "contains"]),
    ("toArray", &["iterator", "size"]),
    ("forEach", &["iterator"]),
    ("put", &["removeEldestEntry"]),
    ("putAll", &["removeEldestEntry"]),
    ("putIfAbsent", &["removeEldestEntry"]),
    ("merge", &["removeEldestEntry"]),
    ("compute", &["removeEldestEntry"]),
    ("computeIfAbsent", &["removeEldestEntry"]),
];

/// The methods a model reads a collection argument through: a jar subclass
/// of a concrete JDK collection that overrides one of them is not what the
/// host's row says.
const ARGUMENT_HOOKS: &[&str] = &[
    "iterator", "toArray", "size", "entrySet", "keySet", "values", "forEach", "get", "contains",
];

/// The data interface that declares `name descriptor` as a default among
/// `implemented` (closed over superinterfaces), the most specific one when
/// several do (`List.getFirst` over `SequencedCollection.getFirst`).
pub(crate) fn data_default_owner(
    implemented: &[&'static str],
    name: &str,
    descriptor: &str,
) -> Option<&'static str> {
    let declares = |interface: &str| {
        DATA_INTERFACES.iter().any(|(owner, _, defaults)| {
            *owner == interface
                && defaults.iter().any(|member| {
                    member
                        .strip_prefix(name)
                        .is_some_and(|rest| rest == descriptor)
                })
        })
    };
    let candidates = implemented
        .iter()
        .copied()
        .filter(|interface| declares(interface))
        .collect::<Vec<_>>();
    candidates.iter().copied().find(|candidate| {
        !candidates
            .iter()
            .any(|other| other != candidate && data_extends(other, candidate))
    })
}

/// Whether the data interface `interface` extends `wanted` (directly or not).
fn data_extends(interface: &str, wanted: &str) -> bool {
    let mut queue = vec![interface];
    let mut steps = 0;
    while let Some(current) = queue.pop() {
        steps += 1;
        if steps > DATA_INTERFACES.len() * 4 {
            return false;
        }
        if current == wanted {
            return true;
        }
        if let Some((_, supers, _)) = DATA_INTERFACES
            .iter()
            .find(|(owner, _, _)| *owner == current)
        {
            queue.extend(supers.iter().copied());
        }
    }
    false
}

/// `interface` and every data interface it extends, added to `out`.
fn close_data_interface(interface: &str, out: &mut Vec<&'static str>) {
    let Some((owner, supers, _)) = DATA_INTERFACES
        .iter()
        .find(|(owner, _, _)| *owner == interface)
    else {
        return;
    };
    if out.contains(owner) {
        return;
    }
    out.push(owner);
    for parent in *supers {
        close_data_interface(parent, out);
    }
}

/// `Object`'s members that never depend on the object's data.
const IDENTITY_MEMBERS: &[&str] = &["getClass", "notify", "notifyAll", "wait"];

/// Whether `class` is one of the JDK skeleton classes `jvm_dispatch` knows.
pub(crate) fn is_skeleton(class: &str) -> bool {
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
        // A jar subclass of a concrete JDK collection keeps its elements in
        // the host's row, so the host's answer stands; but where the JDK body
        // would call back into one of the jar's overrides (an LRU map's
        // `removeEldestEntry`, its own `iterator()` under `toString`), that
        // answer is not Java's, so the call is counted. `super.put(k, v)`
        // from an override runs the JDK body as well, so `invokespecial`
        // counts too.
        if matches!(opcode, 0xb6 | 0xb7 | 0xb9)
            && !r.name.starts_with('<')
            && let Some(receiver) = receiver
            && let Some(counted) = self.concrete_subclass_callback(r, receiver)
        {
            self.host.unknown_operand_call(&counted)?;
        }
        if matches!(opcode, 0xb6 | 0xb9)
            && let Some(receiver) = receiver
            && let Some(base) = self.stateless_jar_object(receiver)
            && let Some(counted) = self.inherited_member(r, receiver, &base)
        {
            // `AbstractCollection.isEmpty` is `size() == 0`, run through the
            // interpreter by `collection_base.rs`.
            if let Some(value) = self.invoke_collection_default(r, Some(receiver), args)? {
                return Ok(Some(value));
            }
            return self.host.unknown_operand_call(&counted).map(Some);
        }
        let classes = parameter_classes(&r.descriptor);
        let data_arguments = classes
            .iter()
            .zip(args)
            .filter(|(class, _)| {
                class
                    .as_deref()
                    .is_some_and(|class| DATA_TYPES.contains(&class) || is_skeleton(class))
            })
            .map(|(_, value)| value)
            .collect::<Vec<_>>();
        // A concrete subclass argument the model reads from its row while the
        // jar overrides how it is read (`String.join(",", listWithOwnIterator)`):
        // counted, and the host's answer stands, as for a receiver.
        if data_arguments.iter().any(|value| {
            self.concrete_subclass_hooks(value)
                .is_some_and(|(_, hooks)| {
                    hooks
                        .iter()
                        .any(|hook| ARGUMENT_HOOKS.contains(&hook.as_str()))
                })
        }) {
            self.host.unknown_operand_call(r)?;
        }
        let unreadable = data_arguments
            .iter()
            .any(|value| self.stateless_jar_object(value).is_some());
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

    /// Every data interface `receiver`'s class implements: through its jar
    /// classes and interfaces, and through its JDK skeleton superclass
    /// `base`, closed over their superinterfaces.
    pub(crate) fn implemented_data_interfaces(
        &self,
        receiver: &Value,
        base: &str,
    ) -> Vec<&'static str> {
        let mut out = Vec::new();
        let Some(class) =
            expect_ref(receiver).and_then(|marker| jvm_dispatch::constructed_object_class(marker))
        else {
            return out;
        };
        let mut queue = vec![class.to_owned()];
        let mut seen = Vec::new();
        while let Some(current) = queue.pop() {
            if seen.len() >= MAX_SUPERCLASS_WALK * 2 || seen.contains(&current) {
                continue;
            }
            seen.push(current.clone());
            match self.class(&current) {
                Some(file) => {
                    queue.extend(file.interface_names());
                    if !file.super_name.is_empty() {
                        queue.push(file.super_name.clone());
                    }
                }
                None => close_data_interface(&current, &mut out),
            }
        }
        // The skeleton chain's own interfaces (`AbstractList` → `List`, and
        // its superclass `AbstractCollection` → `Collection`).
        let mut skeleton = Some(base);
        while let Some(current) = skeleton {
            if let Some((_, interface)) = SKELETON_INTERFACES.iter().find(|(s, _)| *s == current) {
                close_data_interface(interface, &mut out);
            }
            skeleton = jvm_dispatch::JDK_SKELETONS
                .iter()
                .find(|(s, _, _)| *s == current)
                .map(|(_, superclass, _)| *superclass);
        }
        out
    }

    /// A jar object whose class extends a concrete JDK collection or map
    /// (`extends ArrayList`, `extends LinkedHashMap`): that JDK class, and the
    /// names of the methods the jar chain declares (its overrides).
    fn concrete_subclass_hooks(&self, value: &Value) -> Option<(String, Vec<String>)> {
        let class = jvm_dispatch::constructed_object_class(expect_ref(value)?)?;
        if self.host.models_class(class) || jvm_dispatch::library_modelled(class) {
            return None;
        }
        let mut hooks = Vec::new();
        let mut current = class.to_owned();
        for _ in 0..MAX_SUPERCLASS_WALK {
            let Some(file) = self.class(&current) else {
                return (is_collection_owner(&current) && !is_skeleton(&current))
                    .then_some((current, hooks));
            };
            hooks.extend(
                file.methods
                    .iter()
                    .filter(|method| !method.is_static() && method.access & 0x0002 == 0)
                    .map(|method| method.name.clone()),
            );
            if file.super_name.is_empty() {
                return None;
            }
            current = file.super_name.clone();
        }
        None
    }

    /// `r` on a jar subclass of a concrete JDK collection, when the JDK body
    /// of `r` calls back into a method the jar overrides ([`CALLBACKS`]): the
    /// call to count, named by the JDK class. The host answers from its row,
    /// which never runs the override (an LRU map's `removeEldestEntry`, a
    /// subclass's own `iterator()` under `toString`).
    pub(crate) fn concrete_subclass_callback(
        &self,
        r: &MethodRef,
        receiver: &Value,
    ) -> Option<MethodRef> {
        let (base, hooks) = self.concrete_subclass_hooks(receiver)?;
        let calls = CALLBACKS
            .iter()
            .find(|(member, _)| *member == r.name)
            .map_or(&[][..], |(_, calls)| *calls);
        // `TreeMap.putAll` is `AbstractMap.putAll`, a loop of `put` calls
        // (`HashMap.putAll` stores directly and does not call `put`).
        let tree_put_all = r.name == "putAll" && base.starts_with("java/util/TreeMap");
        hooks
            .iter()
            .any(|hook| calls.contains(&hook.as_str()) || (tree_put_all && hook == "put"))
            .then(|| MethodRef {
                owner: base,
                ..r.clone()
            })
    }

    /// The JDK member that would run for `r` on a jar object whose JDK
    /// superclass is `base`, when its answer depends on the object's data:
    /// a skeleton's own member (template or JDK state), or a data
    /// interface's default. `None` for identity members and `Object`'s
    /// exactly-modelled ones.
    fn inherited_member(&self, r: &MethodRef, receiver: &Value, base: &str) -> Option<MethodRef> {
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
        // A data interface's default the receiver's class inherits, whatever
        // type the call site names (`Squares s; s.forEach(...)` names the jar
        // class itself).
        if !matches!(r.name.as_str(), "hashCode" | "equals" | "toString")
            && let Some(owner) = data_default_owner(
                &self.implemented_data_interfaces(receiver, base),
                &r.name,
                &r.descriptor,
            )
        {
            return Some(MethodRef {
                owner: owner.to_owned(),
                ..r.clone()
            });
        }
        // A member named through a data interface the tables above do not
        // place (a call site typed by the interface itself).
        if DATA_TYPES.contains(&r.owner.as_str())
            && !matches!(r.name.as_str(), "hashCode" | "equals" | "toString")
        {
            return Some(r.clone());
        }
        None
    }
}
