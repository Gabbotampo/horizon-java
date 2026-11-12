//! Unmodifiable collections: `List.of`/`Set.of`/`Map.of`, `copyOf`, the
//! `Collections.empty*`/`singleton*` factories, Guava's `ImmutableList.of`/
//! `copyOf`, and `Collections.unmodifiable*` views.
//!
//! These used to answer the content-addressed `list/a,b` value markers, which
//! the host also uses for mutable results (`Collectors.toList()` is an
//! `ArrayList`), so a mutator could not tell them apart and `List.of("a")
//! .add("b")` succeeded. `ImmutableList.copyOf(source)` answered `source`
//! itself, so the "copy" grew with it.
//!
//! Each such collection is now an object of its own (`<class>#immutable<n>`)
//! whose elements live in the host's collection table like any `new
//! ArrayList`, and which is marked unmodifiable: every mutator raises
//! `UnsupportedOperationException` (no message, as the JDK and Guava throw
//! it). A `Collections.unmodifiable*` result is a view: it reads its source's
//! current elements. `equals` compares contents, as `AbstractList`/
//! `AbstractSet`/`AbstractMap` do, for every collection the host keeps.

use crate::runtime::{MethodRef, NativeError, StdHost, Value, expect_ref};

/// The members that change a collection, on any collection interface.
const MUTATORS: &[&str] = &[
    "add",
    "addAll",
    "addFirst",
    "addLast",
    "remove",
    "removeAll",
    "removeIf",
    "removeFirst",
    "removeLast",
    "retainAll",
    "clear",
    "set",
    "sort",
    "replaceAll",
    "put",
    "putAll",
    "putIfAbsent",
    "putFirst",
    "putLast",
    "compute",
    "computeIfAbsent",
    "computeIfPresent",
    "merge",
    "replace",
];

/// Whether `name` is one of the members above that change a collection.
pub(crate) fn is_mutator(name: &str) -> bool {
    MUTATORS.contains(&name)
}

fn throw(class: &str, message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message,
        frame: None,
    }
}

/// The class a JDK unmodifiable collection of `kind` reports.
fn class_of(kind: &str, view: bool) -> &'static str {
    match (kind, view) {
        ("list", false) => "java/util/ImmutableCollections$ListN",
        ("set", false) => "java/util/ImmutableCollections$SetN",
        (_, false) => "java/util/ImmutableCollections$MapN",
        ("list", true) => "java/util/Collections$UnmodifiableRandomAccessList",
        ("set", true) => "java/util/Collections$UnmodifiableSet",
        (_, true) => "java/util/Collections$UnmodifiableMap",
    }
}

/// The supertypes `instanceof` sees for one of the classes above.
pub(crate) fn supertypes(class: &str) -> &'static [&'static str] {
    const LIST: &[&str] = &[
        "java/util/List",
        "java/util/SequencedCollection",
        "java/util/Collection",
        "java/lang/Iterable",
        "java/util/RandomAccess",
        "java/io/Serializable",
    ];
    const SET: &[&str] = &[
        "java/util/Set",
        "java/util/Collection",
        "java/lang/Iterable",
        "java/io/Serializable",
    ];
    const MAP: &[&str] = &["java/util/Map", "java/io/Serializable"];
    match class {
        "java/util/ImmutableCollections$ListN"
        | "java/util/Collections$UnmodifiableRandomAccessList" => LIST,
        "java/util/ImmutableCollections$SetN" | "java/util/Collections$UnmodifiableSet" => SET,
        "java/util/ImmutableCollections$MapN" | "java/util/Collections$UnmodifiableMap" => MAP,
        _ => crate::runtime::sequenced::supertypes(class),
    }
}

impl StdHost {
    /// A new unmodifiable collection holding `values`.
    fn unmodifiable(&mut self, kind: &str, values: Vec<Value>) -> Value {
        let marker = format!(
            "{}#immutable{}",
            class_of(kind, false),
            self.unmodifiable_collections.len()
        );
        self.mutable_collections.insert(marker.clone(), values);
        self.unmodifiable_collections.insert(marker.clone(), None);
        Value::Ref(Some(marker))
    }

    /// The elements a collection argument holds, parsed.
    fn elements(&self, value: &Value) -> Result<Vec<Value>, NativeError> {
        crate::runtime::host_collection_values(self, value)?
            .iter()
            .map(|item| crate::runtime::parse_collection_value(item))
            .collect()
    }

    /// The unmodifiable factories, or `None` for any other static call.
    pub(crate) fn unmodifiable_factory(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let varargs = r.descriptor.starts_with("([");
        let kind = match (r.owner.as_str(), r.name.as_str()) {
            ("java/util/List", "of") | ("com/google/common/collect/ImmutableList", "of")
                if !varargs =>
            {
                "list"
            }
            ("java/util/Set", "of") if !varargs => "set",
            ("java/util/Map", "of") => "map",
            ("java/util/List" | "java/util/Set" | "java/util/Map", "copyOf") => {
                let copy = match self.collection_copy_of(r, args)? {
                    Ok(copy) => copy,
                    Err(error) => return Some(Err(error)),
                };
                let kind = r
                    .owner
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_lowercase();
                return Some(
                    self.elements(&copy)
                        .map(|values| self.unmodifiable(&kind, values)),
                );
            }
            (
                "com/google/common/collect/ImmutableList"
                | "com/google/common/collect/ImmutableSet",
                "copyOf",
            ) if r.descriptor.starts_with("(Ljava/util/Collection;")
                || r.descriptor.starts_with("(Ljava/lang/Iterable;") =>
            {
                let set = r.owner.ends_with("ImmutableSet");
                return Some(self.elements(args.first()?).map(|mut values| {
                    if set {
                        let mut seen = std::collections::BTreeSet::new();
                        values.retain(|value| seen.insert(crate::runtime::value_marker(value)));
                    }
                    self.unmodifiable(if set { "set" } else { "list" }, values)
                }));
            }
            ("java/util/Collections", "emptyList" | "singletonList") => "list",
            ("java/util/Collections", "emptySet" | "singleton") => "set",
            ("java/util/Collections", "emptyMap" | "singletonMap") => "map",
            ("java/util/Collections", name) if name.starts_with("unmodifiable") => {
                let kind = match name {
                    "unmodifiableList" => "list",
                    "unmodifiableSet" => "set",
                    "unmodifiableMap" => "map",
                    _ => return None,
                };
                let source = expect_ref(args.first()?)?.clone();
                let marker = format!(
                    "{}#immutable{}",
                    class_of(kind, true),
                    self.unmodifiable_collections.len()
                );
                self.unmodifiable_collections
                    .insert(marker.clone(), Some(source));
                return Some(Ok(Value::Ref(Some(marker))));
            }
            _ => return None,
        };
        // `List.of`/`Set.of`/`Map.of` refuse null elements and duplicates.
        let of = r.name == "of";
        if of && args.iter().any(|value| matches!(value, Value::Ref(None))) {
            return Some(Err(throw("java/lang/NullPointerException", None)));
        }
        let mut seen = std::collections::BTreeSet::new();
        let step = if kind == "map" { 2 } else { 1 };
        for key in args.iter().step_by(step) {
            if of && kind != "list" && !seen.insert(crate::runtime::value_marker(key)) {
                let what = if kind == "map" { "key" } else { "element" };
                let text = crate::runtime::value_text(key);
                return Some(Err(throw(
                    "java/lang/IllegalArgumentException",
                    Some(format!("duplicate {what}: {text}")),
                )));
            }
        }
        Some(Ok(self.unmodifiable(kind, args.to_vec())))
    }

    /// The elements a view reads: its source's current ones.
    pub(crate) fn view_values(&self, marker: &str) -> Option<Vec<String>> {
        let source = self.unmodifiable_collections.get(marker)?.as_ref()?;
        let source = Value::Ref(Some(source.clone()));
        self.mutable_values(&source)
            .or_else(|| crate::runtime::collection_values(&source).ok())
    }

    /// A collection member this module answers first: a mutator on an
    /// unmodifiable collection throws, and `equals` compares contents.
    pub(crate) fn unmodifiable_member(
        &mut self,
        r: &MethodRef,
        marker: &str,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        if MUTATORS.contains(&r.name.as_str()) && self.unmodifiable_collections.contains_key(marker)
        {
            return Some(Err(throw("java/lang/UnsupportedOperationException", None)));
        }
        if (r.name.as_str(), r.descriptor.as_str()) != ("equals", "(Ljava/lang/Object;)Z") {
            return None;
        }
        let receiver = Value::Ref(Some(marker.to_owned()));
        let mine = self.elements(&receiver).ok()?;
        let Ok(theirs) = self.elements(args.first()?) else {
            return Some(Ok(Value::Int(0)));
        };
        let is_list = |owner: &str| crate::runtime::is_list_owner(owner) || owner == "java/util/Collection";
        let equal = if is_list(&r.owner) {
            mine == theirs
        } else {
            // A set or a map: the same members (map entries as pairs) in any
            // order.
            let step = if crate::runtime::is_map_owner(&r.owner) { 2 } else { 1 };
            let members = |values: &[Value]| {
                let mut members = values
                    .chunks(step)
                    .map(|chunk| chunk.iter().map(crate::runtime::value_marker).collect::<Vec<_>>())
                    .collect::<Vec<_>>();
                members.sort();
                members
            };
            members(&mine) == members(&theirs)
        };
        Some(Ok(Value::Int(i32::from(equal))))
    }
}
