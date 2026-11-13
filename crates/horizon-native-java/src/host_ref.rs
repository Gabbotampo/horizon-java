//! Host objects as typed handles: `Value::Host(HostRef { kind, key })` (#156).
//!
//! The runtime used to name every host object with text (`optional/…`,
//! `identifier/…`, `list/…`), the same `Value::Ref` a Java `String` is, so a
//! jar string spelled like a marker was that object to every reader that
//! checked the prefix. A [`HostRef`] is a separate `Value` kind: nothing a
//! jar spells can be one.
//!
//! A handle is its family ([`HostKind`]) and the family's identity text
//! (`key`). Everything else about the object is a property of the kind's row
//! in the kind table ([`KINDS`]), read through the handle:
//!
//! * **class**: [`HostRef::class`], from the row's [`ClassRule`]: one fixed
//!   class (`java/util/Optional`), or, for a family (jar objects, collections,
//!   game objects), a function of the key;
//! * **identity**: [`HostRef::canonical`], the row's predicate on the key
//!   (below);
//! * **`toString`/`hashCode`/`equals`/`instanceof`**: the row's [`Methods`].
//!   They are the only place a kind's object methods live; the runtime
//!   (`object_methods.rs`, `jar_text.rs`) and the host (`StdHost`) both call
//!   them, each passing a [`Nested`] that answers for the values a handle
//!   holds: the runtime's runs a held jar object's own `hashCode`/`equals`/
//!   `toString` through the interpreter, the host's (which cannot run jar
//!   code) answers `None` for one, and the caller counts it;
//! * **casts**: `checkcast` and `instanceof` read [`HostRef::instance_of`],
//!   so `(String) Optional.empty()` throws Java's `ClassCastException`.
//!
//! # Keys
//!
//! A key is plain text, compared whole. It holds **one way**: two handles
//! with equal keys are equal objects (`equals` is `true`, `hashCode` agrees),
//! so a key is built from the canonical form of the kind's `equals` where the
//! kind can (an `OptionalDouble` keys `Double.doubleToLongBits`: every `NaN`
//! one key, `-0.0` not `0.0`). The converse does not hold: two equal objects
//! may have different keys when a held value is not canonical text (two
//! `List.of(1)` are two allocation markers; two jar objects equal by their
//! own `equals`). So a reader never treats different keys as "not equal":
//! `equals` is the row's method, which asks the held values. What does hold
//! both ways is identity: one object never gets two keys, so different keys
//! are different objects (`if_acmp`, [`HostRef::same_object`]).
//!
//! A key that holds values (an optional's value, a pair, a list's elements)
//! is built by [`HostRef::of_values`] and read by [`HostRef::values`], the one
//! encoder/decoder for nested values: each value's `value_marker`, joined
//! with `,`, separators escaped. A handle's own text form (in a collection, a
//! capture) is [`HostRef::encode`]/[`HostRef::decode`]; the list layer around
//! it (`value_marker`/`split_capture_markers`) escapes and unescapes it as it
//! does a string, so a key reads back byte for byte however deep it nests.
//!
//! # Identity
//!
//! A kind's canonical predicate says whether two handles with equal keys are
//! one object:
//!
//! * **canonical** (`true`): the JDK hands out one object per key (an enum
//!   constant, a `Class`, `Optional.empty()`): `==` is key equality, never
//!   counted;
//! * **not canonical** (`false`): equal keys are equal values but may be
//!   different objects (`Optional.of(5)` twice). An identity operation on two
//!   of them (`==`, `!=`, an `IdentityHashMap` key) is counted, unless the
//!   same handle provably flows to both sides, which the identity analysis
//!   decides (`reference_identity.rs`). Equal keys never answer identity.
//!
//! # Adding a kind (P1–P17)
//!
//! Add a [`HostKind`] variant and its row: a unique tag, the class rule, the
//! canonical predicate, and a `Methods` (the kind's module, like
//! `host_optional.rs`). Nothing else matches on kinds for object methods.

use std::{
    cmp::Ordering,
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering as AtomicOrdering},
    },
};

use crate::{NativeError, Value};

/// A host object: its family and the family's identity text.
///
/// Equality, hashing and order are the `(kind, key)` pair's: two handles with
/// equal keys are equal values. `origin` is not part of the value: it names
/// the construction that made this handle, so a clone (the same Java
/// reference flowing on through locals, fields, arrays and the host's
/// collections) shares it, and two constructions never do. Same origin is
/// therefore the same object; different origins prove nothing (a model may
/// wrap one Java object twice), see [`HostRef::same_object`].
#[derive(Clone)]
pub struct HostRef {
    kind: HostKind,
    key: Arc<str>,
    origin: u64,
}

/// The next [`HostRef::origin`]: only ever compared for equality, never shown.
static NEXT_ORIGIN: AtomicU64 = AtomicU64::new(1);

impl PartialEq for HostRef {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.key == other.key
    }
}

impl Eq for HostRef {}

impl Hash for HostRef {
    fn hash<S: Hasher>(&self, state: &mut S) {
        self.kind.hash(state);
        self.key.hash(state);
    }
}

impl PartialOrd for HostRef {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HostRef {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.kind, &self.key).cmp(&(other.kind, &other.key))
    }
}

impl std::fmt::Debug for HostRef {
    /// The value only: an origin differs run to run and would make any text
    /// that prints a handle unstable.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostRef")
            .field("kind", &self.kind)
            .field("key", &self.key)
            .finish()
    }
}

/// The families moved onto handles so far; the rest are still `Ref` text
/// (#156's inventory).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum HostKind {
    /// `java.util.Optional`: key `""` when empty, else [`HostRef::of_values`]
    /// of the held value.
    Optional,
    /// `java.util.OptionalInt`: key `""` when empty, else the `int`.
    OptionalInt,
    /// `java.util.OptionalLong`: key `""` when empty, else the `long`.
    OptionalLong,
    /// `java.util.OptionalDouble`: key `""` when empty, else the `double`'s
    /// `Double.doubleToLongBits` (every `NaN` one key, `-0.0` not `0.0`).
    OptionalDouble,
}

/// How a kind's Java class follows from its key.
#[derive(Clone, Copy)]
pub(crate) enum ClassRule {
    /// Every object of the kind is this class.
    Fixed(&'static str),
    /// A family whose objects are of many classes (jar objects, collections,
    /// game objects): the class is read from the key.
    #[allow(dead_code)] // The first family kinds arrive in P12/P14/P16.
    FromKey(fn(&str) -> &str),
}

/// Answers for the values a handle holds, for a kind's [`Methods`]. `None`
/// is "unknown here": the caller counts the call instead of guessing.
pub(crate) trait Nested {
    /// `Objects.hashCode(value)`.
    fn hash(&mut self, value: &Value) -> Result<Option<i32>, NativeError>;
    /// `Objects.equals(a, b)`.
    fn equals(&mut self, a: &Value, b: &Value) -> Result<Option<bool>, NativeError>;
    /// `String.valueOf(value)`.
    fn text(&mut self, value: &Value) -> Result<Option<String>, NativeError>;
}

/// `toString()` of a handle.
pub(crate) type ToStringFn = fn(&HostRef, &mut dyn Nested) -> Result<Option<String>, NativeError>;
/// `hashCode()` of a handle.
pub(crate) type HashCodeFn = fn(&HostRef, &mut dyn Nested) -> Result<Option<i32>, NativeError>;
/// `handle.equals(other)`.
pub(crate) type EqualsFn =
    fn(&HostRef, &Value, &mut dyn Nested) -> Result<Option<bool>, NativeError>;
/// `handle instanceof wanted` (`wanted` an internal class name), asking
/// `hierarchy` about classes; `None` when the hierarchy cannot tell (the
/// caller counts the check).
pub(crate) type InstanceOfFn = fn(&HostRef, &str, &dyn ClassHierarchy) -> Option<bool>;
/// Whether a handle means the same in another jar's runtime (NeoForge IMC):
/// `crosses` answers for a held value.
pub(crate) type CrossesFn = fn(&HostRef, &dyn Fn(&Value) -> bool) -> bool;

/// The class hierarchy a kind's `instance_of` reads: the JDK's supertypes,
/// and in the runtime the jar's own classes (a jar object's superclasses and
/// interfaces, a Mixin's added interface), which a family kind needs.
pub(crate) trait ClassHierarchy {
    /// Whether an object of class `class` is a `wanted` (internal names);
    /// `None` when unknown (a class whose chain leaves what is held).
    fn is_instance(&self, class: &str, wanted: &str) -> Option<bool>;
}

/// The JDK's own hierarchy, for a caller with no runtime: a JDK class never
/// extends or implements a jar type.
pub(crate) struct JdkHierarchy;

impl ClassHierarchy for JdkHierarchy {
    fn is_instance(&self, class: &str, wanted: &str) -> Option<bool> {
        Some(wanted == "java/lang/Object" || crate::jdk_model::class_instance_of(class, wanted))
    }
}

/// A kind's `Object` methods, as the JDK class implements them, and the other
/// per-kind behaviour a shared caller needs.
pub(crate) struct Methods {
    pub(crate) to_string: ToStringFn,
    pub(crate) hash_code: HashCodeFn,
    pub(crate) equals: EqualsFn,
    pub(crate) instance_of: InstanceOfFn,
    pub(crate) crosses_jars: CrossesFn,
}

/// One row of the kind table.
struct KindInfo {
    kind: HostKind,
    /// The tag in a handle's text form (`h<tag>:<key>`); unique per kind.
    tag: &'static str,
    class: ClassRule,
    /// Whether two handles with this key are one object.
    canonical: fn(&str) -> bool,
    methods: &'static Methods,
}

/// The canonical predicate of a kind whose empty key is its singleton
/// (`Optional.empty()`) and whose other keys are values.
pub(crate) fn canonical_when_empty(key: &str) -> bool {
    key.is_empty()
}

/// The canonical predicate of a kind with one object per key.
#[allow(dead_code)] // The first canonical families arrive in P1 (identifiers, enums).
pub(crate) fn always_canonical(_key: &str) -> bool {
    true
}

/// The canonical predicate of a kind whose every object is a fresh value.
#[allow(dead_code)]
pub(crate) fn never_canonical(_key: &str) -> bool {
    false
}

/// [`InstanceOfFn`] for a kind whose class is all it is: the class and its
/// supertypes, as `hierarchy` knows them.
pub(crate) fn instance_of_class(
    handle: &HostRef,
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    hierarchy.is_instance(handle.class(), wanted)
}

/// Every kind: its tag, its class, its identity, its methods.
const KINDS: &[KindInfo] = &[
    KindInfo {
        kind: HostKind::Optional,
        tag: "opt",
        class: ClassRule::Fixed("java/util/Optional"),
        canonical: canonical_when_empty,
        methods: &crate::host_optional::OPTIONAL,
    },
    KindInfo {
        kind: HostKind::OptionalInt,
        tag: "opti",
        class: ClassRule::Fixed("java/util/OptionalInt"),
        canonical: canonical_when_empty,
        methods: &crate::host_optional::OPTIONAL_INT,
    },
    KindInfo {
        kind: HostKind::OptionalLong,
        tag: "optl",
        class: ClassRule::Fixed("java/util/OptionalLong"),
        canonical: canonical_when_empty,
        methods: &crate::host_optional::OPTIONAL_LONG,
    },
    KindInfo {
        kind: HostKind::OptionalDouble,
        tag: "optd",
        class: ClassRule::Fixed("java/util/OptionalDouble"),
        canonical: canonical_when_empty,
        methods: &crate::host_optional::OPTIONAL_DOUBLE,
    },
];

impl HostKind {
    /// The kind's row: `KINDS` is in declaration order, so the discriminant
    /// indexes it (a unit test checks every row's place).
    fn info(self) -> &'static KindInfo {
        &KINDS[self as usize]
    }

    fn from_tag(tag: &str) -> Option<Self> {
        static BY_TAG: OnceLock<HashMap<&'static str, HostKind>> = OnceLock::new();
        BY_TAG
            .get_or_init(|| KINDS.iter().map(|info| (info.tag, info.kind)).collect())
            .get(tag)
            .copied()
    }

    /// The kind of a fixed-class kind whose class is `class` (asked on every
    /// `checkcast`, so a map, not a scan).
    pub(crate) fn of_class(class: &str) -> Option<Self> {
        static BY_CLASS: OnceLock<HashMap<&'static str, HostKind>> = OnceLock::new();
        BY_CLASS
            .get_or_init(|| {
                KINDS
                    .iter()
                    .filter_map(|info| match info.class {
                        ClassRule::Fixed(class) => Some((class, info.kind)),
                        ClassRule::FromKey(_) => None,
                    })
                    .collect()
            })
            .get(class)
            .copied()
    }
}

impl HostRef {
    /// A handle with `key` as it is (a number, a name). A key that holds
    /// values is built with [`Self::of_values`].
    pub(crate) fn new(kind: HostKind, key: impl Into<Arc<str>>) -> Self {
        Self {
            kind,
            key: key.into(),
            origin: NEXT_ORIGIN.fetch_add(1, AtomicOrdering::Relaxed),
        }
    }

    /// A handle whose key holds `values`: each value's marker, separators
    /// escaped, joined with `,`. No values is the empty key.
    pub(crate) fn of_values(kind: HostKind, values: &[Value]) -> Self {
        let key = values
            .iter()
            .map(crate::value_marker)
            .collect::<Vec<_>>()
            .join(",");
        Self::new(kind, key)
    }

    /// The values [`Self::of_values`] put in the key, `None` when one does
    /// not parse.
    pub(crate) fn values(&self) -> Option<Vec<Value>> {
        if self.key.is_empty() {
            return Some(Vec::new());
        }
        crate::split_capture_markers(&self.key)
            .ok()?
            .iter()
            .map(|marker| crate::parse_value_marker(marker).ok())
            .collect()
    }

    pub fn kind(&self) -> HostKind {
        self.kind
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    /// The class this object is (`getClass()`), internal form.
    pub fn class(&self) -> &str {
        match self.kind.info().class {
            ClassRule::Fixed(class) => class,
            ClassRule::FromKey(class_of) => class_of(&self.key),
        }
    }

    /// Whether two handles equal to this one are the same object.
    pub fn canonical(&self) -> bool {
        (self.kind.info().canonical)(&self.key)
    }

    /// `self == other` as the JVM compares references, where it is known:
    /// different values are different objects; equal values of a canonical
    /// kind, or a handle and its own clone (one construction flowing to both
    /// sides), are one object. `None` for two equal handles of a kind that
    /// is not canonical from different constructions: maybe two objects,
    /// maybe one Java object a model wrapped twice. The caller counts it.
    pub(crate) fn same_object(&self, other: &HostRef) -> Option<bool> {
        if self != other {
            return Some(false);
        }
        (self.canonical() || self.origin == other.origin).then_some(true)
    }

    fn methods(&self) -> &'static Methods {
        self.kind.info().methods
    }

    /// `toString()`; `None` when `nested` cannot print a held value.
    pub(crate) fn to_string(&self, nested: &mut dyn Nested) -> Result<Option<String>, NativeError> {
        (self.methods().to_string)(self, nested)
    }

    /// `hashCode()`; `None` when `nested` cannot hash a held value.
    pub(crate) fn hash_code(&self, nested: &mut dyn Nested) -> Result<Option<i32>, NativeError> {
        (self.methods().hash_code)(self, nested)
    }

    /// `equals(other)`; `None` when `nested` cannot compare held values.
    pub(crate) fn equals(
        &self,
        other: &Value,
        nested: &mut dyn Nested,
    ) -> Result<Option<bool>, NativeError> {
        (self.methods().equals)(self, other, nested)
    }

    /// `this instanceof wanted`, and whether `checkcast wanted` passes;
    /// `None` when `hierarchy` cannot tell.
    pub(crate) fn instance_of(&self, wanted: &str, hierarchy: &dyn ClassHierarchy) -> Option<bool> {
        (self.methods().instance_of)(self, wanted, hierarchy)
    }

    /// Whether this handle means the same in another jar's runtime, with
    /// `crosses` answering for the values it holds.
    pub(crate) fn crosses_jars(&self, crosses: &dyn Fn(&Value) -> bool) -> bool {
        (self.methods().crosses_jars)(self, crosses)
    }

    /// The handle's text form, `h<tag>:<key>`, the key as it is. A list of
    /// markers (`value_marker`) escapes it as it escapes a string.
    pub(crate) fn encode(&self) -> String {
        format!("h{}:{}", self.kind.info().tag, self.key)
    }

    /// Read [`Self::encode`] back.
    pub(crate) fn decode(text: &str) -> Option<Self> {
        let (tag, key) = text.strip_prefix('h')?.split_once(':')?;
        Some(Self::new(HostKind::from_tag(tag)?, key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_one_row_with_a_unique_tag_and_class() {
        for (index, info) in KINDS.iter().enumerate() {
            // `HostKind::info` indexes the table by discriminant.
            assert_eq!(info.kind as usize, index, "{:?}", info.kind);
            assert_eq!(HostKind::from_tag(info.tag), Some(info.kind));
            assert!(
                KINDS[index + 1..].iter().all(|other| other.tag != info.tag
                    && other.kind != info.kind
                    && !matches!(
                        (info.class, other.class),
                        (ClassRule::Fixed(a), ClassRule::Fixed(b)) if a == b
                    )),
                "{:?}",
                info.kind
            );
            let handle = HostRef::new(info.kind, "a,b\\c:d");
            assert_eq!(HostRef::decode(&handle.encode()), Some(handle.clone()));
            if let ClassRule::Fixed(class) = info.class {
                assert_eq!(HostKind::of_class(class), Some(info.kind));
            }
        }
    }

    /// A kind's `instance_of` asks the hierarchy it is handed, so a family
    /// kind (P12 jar objects) can answer from the jar's own classes, and an
    /// unknown answer stays unknown for the caller to count.
    #[test]
    fn instance_of_reads_the_hierarchy_it_is_given() {
        struct Stub;
        impl ClassHierarchy for Stub {
            fn is_instance(&self, class: &str, wanted: &str) -> Option<bool> {
                match (class, wanted) {
                    ("java/util/Optional", "demo/Added") => Some(true),
                    ("java/util/Optional", "demo/Unknown") => None,
                    _ => Some(false),
                }
            }
        }
        let handle = HostRef::new(HostKind::Optional, "");
        assert_eq!(handle.instance_of("demo/Added", &Stub), Some(true));
        assert_eq!(handle.instance_of("demo/Unknown", &Stub), None);
        assert_eq!(handle.instance_of("java/lang/Object", &Stub), Some(false));
        // The JDK's own hierarchy: an `Optional` is an `Object` and nothing else.
        assert_eq!(
            handle.instance_of("java/lang/Object", &JdkHierarchy),
            Some(true)
        );
        assert_eq!(
            handle.instance_of("java/io/Serializable", &JdkHierarchy),
            Some(false)
        );
    }

    /// A small deterministic generator (xorshift): the property tests need
    /// no extra crate.
    struct Gen(u64);

    impl Gen {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        /// A string over the characters every layer treats specially (the
        /// separators and escapes, the markers' prefixes and tags), plus
        /// non-ASCII text and a NUL.
        fn text(&mut self) -> String {
            const ALPHABET: &[&str] = &[
                ",", "\\", ":", "h", "r", "i", "l", "f", "B", "5", "-", "opt", "null", "void", "/",
                "#", " ", "é", "ü", "😀", "\0", "\\,", ",\\",
            ];
            let length = self.next() % 9;
            (0..length)
                .map(|_| ALPHABET[(self.next() % ALPHABET.len() as u64) as usize])
                .collect()
        }

        /// A value: a string, a number, `null`, or an optional nesting one.
        fn value(&mut self, depth: u32) -> Value {
            match self.next() % 6 {
                0 if depth < 3 => Value::Host(HostRef::of_values(
                    HostKind::Optional,
                    &[self.value(depth + 1)],
                )),
                1 => Value::Int(self.next() as i32),
                2 => Value::Host(HostRef::new(HostKind::OptionalInt, "")),
                _ => Value::Ref(Some(self.text())),
            }
        }
    }

    /// The one nested-key encoder/decoder round-trips every value, compared
    /// by value (not just its kind), through every layer a handle passes:
    /// its key, its own text form, a list of markers (a capture, an immutable
    /// collection), a collection element, and an optional of an optional.
    #[test]
    fn nested_keys_round_trip_by_value_for_arbitrary_strings() {
        let mut fixed = vec![
            "a,b",
            "x\\",
            "x\\,y",
            "k,1",
            "z,z",
            "é,\\ü",
            "",
            ",",
            "\\",
            "\\\\,",
            "hopt:ra",
            "hopt:r",
            "null",
            "r",
            "😀,😀",
        ]
        .into_iter()
        .map(|text| Value::Ref(Some(text.to_owned())))
        .collect::<Vec<_>>();
        let mut generator = Gen(0x9e37_79b9_7f4a_7c15);
        fixed.extend((0..3000).map(|_| generator.value(0)));
        for (index, held) in fixed.iter().enumerate() {
            let other = &fixed[(index * 7 + 3) % fixed.len()];
            let pair = HostRef::of_values(HostKind::Optional, &[held.clone(), other.clone()]);
            assert_eq!(
                pair.values(),
                Some(vec![held.clone(), other.clone()]),
                "{held:?}"
            );
            let optional = HostRef::of_values(HostKind::Optional, std::slice::from_ref(held));
            assert_eq!(optional.values(), Some(vec![held.clone()]), "{held:?}");
            let handle = Value::Host(optional.clone());
            // Its own text form.
            assert_eq!(
                HostRef::decode(&optional.encode()).as_ref(),
                Some(&optional)
            );
            // A list of markers: split, then each part parsed.
            let list = [handle.clone(), held.clone()]
                .iter()
                .map(crate::value_marker)
                .collect::<Vec<_>>()
                .join(",");
            let parts = crate::split_capture_markers(&list)
                .unwrap()
                .iter()
                .map(|part| crate::parse_value_marker(part).ok())
                .collect::<Vec<_>>();
            assert_eq!(
                parts,
                vec![Some(handle.clone()), Some(held.clone())],
                "{held:?}"
            );
            // A collection element.
            assert_eq!(
                crate::parse_value_marker(&crate::collection_item_marker(&handle)).ok(),
                Some(handle.clone()),
                "{held:?}"
            );
            // An optional of the optional, read back two levels down.
            let outer = HostRef::of_values(HostKind::Optional, std::slice::from_ref(&handle));
            let inner = match outer.values().as_deref() {
                Some([Value::Host(inner)]) => inner.clone(),
                other => panic!("{other:?}"),
            };
            assert_eq!(inner.values(), Some(vec![held.clone()]), "{held:?}");
        }
    }
}
