//! A host list's equality members with jar elements, run as the JDK class's
//! own body runs them.
//!
//! The host finds a list element by its marker, which is Java's answer only
//! while the element keeps `Object.equals`. With a record or any jar class
//! that has its own `equals`, `contains`/`indexOf`/`remove(Object)` missed the
//! equal element, `removeAll`/`retainAll` removed nothing, and `equals`
//! between two lists of records answered `false`, all uncounted.
//!
//! Each member below now runs the jar's `equals`/`hashCode` exactly as many
//! times and in the order the receiver's JDK 25 class does, because a jar
//! body may count, log or throw. The classes differ in small ways that are
//! observable through such a body:
//!
//! | class | `indexOf` | `equals` | `null` argument |
//! |---|---|---|---|
//! | `ArrayList` | `o.equals(e)` | `Objects.equals(e, o2)`, sizes first against another `ArrayList` | allowed |
//! | `LinkedList`, `Vector`, `Arrays.asList`, `AbstractList` | `o.equals(e)` | `e.equals(o2)`, no `==` shortcut | allowed |
//! | `CopyOnWriteArrayList` | `o.equals(e)` | `Objects.equals(e, o2)` | allowed |
//! | `List.of` with 1–2 elements (`List12`) | `o.equals(e)` | `Objects.equals(e, o2)` | `NullPointerException` |
//! | `List.of` otherwise (`ListN`) | `Objects.equals(o, e)` | `Objects.equals(e, o2)` | `NullPointerException` |
//!
//! `containsAll(c)` asks `this.contains(e)` for each element of `c` in `c`'s
//! order; `removeAll(c)`/`retainAll(c)` ask `c.contains(e)` for each element of
//! this list in order, so the argument's own rules apply there. `hashCode` is
//! `31 * h + hash(e)` from `1`, and is counted when an element's hash is an
//! identity hash.
//!
//! A member is answered here only when a jar `equals` is involved (or for
//! `hashCode`, which the host never modelled); every other call keeps the
//! host's answer, which is already Java's for strings and boxes.

use super::*;
use object_methods::{CollectionKind, Equality};

/// How one JDK list class runs its equality members (the table above).
#[derive(Clone, Copy, Debug, PartialEq)]
struct ListRules {
    /// `indexOf` compares with `Objects.equals(o, e)` (an identical element
    /// matches without a call) rather than `o.equals(e)`.
    index_shortcut: bool,
    /// `equals` compares with `Objects.equals(e, o2)` rather than
    /// `e.equals(o2)`.
    equals_shortcut: bool,
    /// `equals` against another `ArrayList` compares the sizes first.
    array_list: bool,
    /// `indexOf(null)` throws `NullPointerException`.
    null_hostile: bool,
}

const ARRAY_LIST: ListRules = ListRules {
    index_shortcut: false,
    equals_shortcut: true,
    array_list: true,
    null_hostile: false,
};
const ABSTRACT_LIST: ListRules = ListRules {
    index_shortcut: false,
    equals_shortcut: false,
    array_list: false,
    null_hostile: false,
};
const COPY_ON_WRITE: ListRules = ListRules {
    index_shortcut: false,
    equals_shortcut: true,
    array_list: false,
    null_hostile: false,
};
const LIST_12: ListRules = ListRules {
    index_shortcut: false,
    equals_shortcut: true,
    array_list: false,
    null_hostile: true,
};
const LIST_N: ListRules = ListRules {
    index_shortcut: true,
    equals_shortcut: true,
    array_list: false,
    null_hostile: true,
};

/// A host list: its rules, and the list it delegates to when it is an
/// unmodifiable view (`Collections.unmodifiableList`).
struct HostList {
    rules: ListRules,
    view: bool,
    /// The host's inline `list/..` value, which keeps no table of its own
    /// to mutate (the host answers its mutators).
    inline: bool,
}

fn null_pointer() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    }
}

impl<H: Host> Runtime<'_, H> {
    /// The rules of the JDK list class `value` is, or `None` for a value that
    /// is not a host list (or a list class this module does not know).
    fn host_list(&self, value: &Value) -> Option<HostList> {
        if self.host_collection_kind(value) != Some(CollectionKind::List) {
            return None;
        }
        let marker = expect_ref(value)?;
        // The host's inline lists (`list/a,b`) come from `Arrays.asList` and
        // the varargs factories: `AbstractList`'s bodies.
        if marker.starts_with("list/") || marker.starts_with("list#") || marker == "list" {
            return Some(HostList {
                rules: ABSTRACT_LIST,
                view: false,
                inline: true,
            });
        }
        let class = self.host_collection_class(value)?;
        let rules = match class.as_str() {
            "java/util/ArrayList" => ARRAY_LIST,
            "java/util/LinkedList" | "java/util/Vector" | "java/util/Stack" => ABSTRACT_LIST,
            "java/util/concurrent/CopyOnWriteArrayList" => COPY_ON_WRITE,
            "java/util/ImmutableCollections$ListN" => {
                // `List.of`/`copyOf` build a `List12` for one or two elements.
                let size = self.host.mutable_collection_values(value)?.len();
                if (1..=2).contains(&size) {
                    LIST_12
                } else {
                    LIST_N
                }
            }
            "java/util/Collections$UnmodifiableRandomAccessList"
            | "java/util/Collections$UnmodifiableList" => {
                return Some(HostList {
                    rules: ABSTRACT_LIST,
                    view: true,
                    inline: false,
                });
            }
            _ => return None,
        };
        Some(HostList {
            rules,
            view: false,
            inline: false,
        })
    }

    /// The list an unmodifiable view reads.
    fn view_source(&self, value: &Value) -> Option<Value> {
        let marker = expect_ref(value)?;
        // The host keeps the view's source beside it; reading the elements
        // already goes through it, so only the rules need the source.
        let source = self.host.unmodifiable_view_source(marker)?;
        Some(Value::Ref(Some(source)))
    }

    /// Whether the host's marker comparison can differ from Java for
    /// `value`: it runs a jar `equals` body, or it is `null`, which `List.of`
    /// refuses to look up.
    fn jar_equals(&self, value: &Value) -> bool {
        *value == Value::Ref(None)
            || matches!(
                self.equality_of(value),
                Equality::Jar {
                    equals: Some(_),
                    ..
                }
            )
    }

    /// `list.indexOf(o)` (`last` for `lastIndexOf`) under `rules`.
    fn list_index_of(
        &mut self,
        rules: ListRules,
        elements: &[Value],
        needle: &Value,
        last: bool,
    ) -> Result<i32, NativeError> {
        if rules.null_hostile && *needle == Value::Ref(None) {
            return Err(null_pointer());
        }
        let order: Vec<usize> = if last {
            (0..elements.len()).rev().collect()
        } else {
            (0..elements.len()).collect()
        };
        for index in order {
            let element = &elements[index];
            let equal = if *needle == Value::Ref(None) {
                *element == Value::Ref(None)
            } else if rules.index_shortcut {
                self.objects_equals(needle, element)?
            } else {
                self.java_equals(needle, element)?
            };
            if equal {
                return Ok(index as i32);
            }
        }
        Ok(-1)
    }

    /// `this.equals(other)` for two host lists.
    pub(crate) fn list_equals(&mut self, this: &Value, other: &Value) -> Result<bool, NativeError> {
        let Some(list) = self.host_list(this) else {
            return Ok(this == other);
        };
        if list.view {
            // `UnmodifiableList.equals`: `o == this || list.equals(o)`.
            if this == other {
                return Ok(true);
            }
            let Some(source) = self.view_source(this) else {
                return Ok(false);
            };
            return self.list_equals(&source, other);
        }
        if this == other {
            return Ok(true);
        }
        if self.host_collection_kind(other) != Some(CollectionKind::List) {
            return Ok(false);
        }
        let mine = self.collection_elements(this)?;
        let theirs = self.collection_elements(other)?;
        let rules = list.rules;
        // `ArrayList.equalsArrayList` (the other's class is exactly
        // `ArrayList`) checks the sizes before any element.
        let exactly_array_list = expect_ref(other)
            .and_then(|marker| marker.split_once('#'))
            .is_some_and(|(class, _)| class == "java/util/ArrayList");
        if rules.array_list && exactly_array_list && mine.len() != theirs.len() {
            return Ok(false);
        }
        for (index, left) in mine.iter().enumerate() {
            let Some(right) = theirs.get(index) else {
                return Ok(false);
            };
            let equal = if rules.equals_shortcut {
                self.objects_equals(left, right)?
            } else if *left == Value::Ref(None) {
                *right == Value::Ref(None)
            } else {
                self.java_equals(left, right)?
            };
            if !equal {
                return Ok(false);
            }
        }
        Ok(theirs.len() == mine.len())
    }

    /// `c.contains(e)` for the argument of `removeAll`/`retainAll`: a list by
    /// its own rules; any other collection only while every value involved
    /// is pure, when the answer cannot depend on how the collection looks it
    /// up. `None` when this runtime cannot answer it exactly.
    fn argument_contains(
        &mut self,
        collection: &Value,
        element: &Value,
    ) -> Result<Option<bool>, NativeError> {
        if let Some(list) = self.host_list(collection) {
            let rules = if list.view {
                self.view_source(collection)
                    .and_then(|source| self.host_list(&source))
                    .map_or(ABSTRACT_LIST, |list| list.rules)
            } else {
                list.rules
            };
            let elements = self.collection_elements(collection)?;
            return Ok(Some(
                self.list_index_of(rules, &elements, element, false)? >= 0,
            ));
        }
        if self.host_collection_kind(collection) != Some(CollectionKind::Set) {
            return Ok(None);
        }
        let elements = self.collection_elements(collection)?;
        if !self.pure_equality(element) || !elements.iter().all(|e| self.pure_equality(e)) {
            return Ok(None);
        }
        for candidate in &elements {
            if self.objects_equals(element, candidate)? {
                return Ok(Some(true));
            }
        }
        Ok(Some(false))
    }

    /// Whether [`Self::argument_contains`] answers every one of `elements`.
    fn argument_contains_is_exact(
        &mut self,
        collection: &Value,
        elements: &[Value],
    ) -> Result<bool, NativeError> {
        if self.host_list(collection).is_some() {
            return Ok(true);
        }
        if self.host_collection_kind(collection) != Some(CollectionKind::Set) {
            return Ok(false);
        }
        let members = self.collection_elements(collection)?;
        Ok(elements
            .iter()
            .chain(&members)
            .all(|value| self.pure_equality(value)))
    }

    /// A host list's `contains`/`indexOf`/`lastIndexOf`/`remove(Object)`/
    /// `containsAll`/`removeAll`/`retainAll`/`equals`/`hashCode` when a jar
    /// `equals` decides it. `Ok(None)` leaves the call to the host.
    pub(crate) fn invoke_list_equality(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if !matches!(opcode, 0xb6 | 0xb9) {
            return Ok(None);
        }
        let Some(receiver) = receiver else {
            return Ok(None);
        };
        if self.host_collection_class(receiver).as_deref() == Some("java/util/ArrayDeque") {
            return self.array_deque_equality(r, receiver, args);
        }
        let Some(list) = self.host_list(receiver) else {
            return Ok(None);
        };
        // A view answers through its list, except for its mutators, which
        // the host refuses.
        let rules = if list.view {
            match self
                .view_source(receiver)
                .and_then(|source| self.host_list(&source))
            {
                Some(source) => source.rules,
                None => return Ok(None),
            }
        } else {
            list.rules
        };
        let immutable = list.view || list.inline || rules.null_hostile;
        match (r.name.as_str(), r.descriptor.as_str(), args) {
            ("hashCode", "()I", []) => {
                let Some(hash) = self.java_hash(receiver)? else {
                    return self.host.unknown_operand_call(r).map(Some);
                };
                Ok(Some(Value::Int(hash)))
            }
            ("equals", "(Ljava/lang/Object;)Z", [other]) => {
                if self.host_collection_kind(other) != Some(CollectionKind::List) {
                    return Ok(None);
                }
                let mine = self.collection_elements(receiver)?;
                let theirs = self.collection_elements(other)?;
                if !mine
                    .iter()
                    .chain(&theirs)
                    .any(|value| self.jar_equals(value))
                {
                    return Ok(None);
                }
                let equal = self.list_equals(receiver, other)?;
                Ok(Some(Value::Int(i32::from(equal))))
            }
            (
                name @ ("contains" | "indexOf" | "lastIndexOf"),
                "(Ljava/lang/Object;)Z" | "(Ljava/lang/Object;)I",
                [needle],
            ) => {
                if !self.jar_equals(needle) {
                    return Ok(None);
                }
                let elements = self.collection_elements(receiver)?;
                let index = self.list_index_of(rules, &elements, needle, name == "lastIndexOf")?;
                Ok(Some(Value::Int(if name == "contains" {
                    i32::from(index >= 0)
                } else {
                    index
                })))
            }
            ("remove", "(Ljava/lang/Object;)Z", [needle]) if !immutable => {
                if !self.jar_equals(needle) {
                    return Ok(None);
                }
                let mut elements = self.collection_elements(receiver)?;
                let index = self.list_index_of(rules, &elements, needle, false)?;
                if index < 0 {
                    return Ok(Some(Value::Int(0)));
                }
                elements.remove(index as usize);
                self.host.replace_mutable_collection(receiver, elements);
                Ok(Some(Value::Int(1)))
            }
            ("containsAll", "(Ljava/util/Collection;)Z", [collection]) => {
                if self.host_collection_kind(collection).is_none() {
                    return Ok(None);
                }
                let wanted = self.collection_elements(collection)?;
                if !wanted.iter().any(|value| self.jar_equals(value)) {
                    return Ok(None);
                }
                let elements = self.collection_elements(receiver)?;
                for value in &wanted {
                    if self.list_index_of(rules, &elements, value, false)? < 0 {
                        return Ok(Some(Value::Int(0)));
                    }
                }
                Ok(Some(Value::Int(1)))
            }
            (name @ ("removeAll" | "retainAll"), "(Ljava/util/Collection;)Z", [collection])
                if !immutable =>
            {
                if *collection == Value::Ref(None) {
                    return Err(null_pointer());
                }
                let elements = self.collection_elements(receiver)?;
                let wanted = self.collection_elements(collection).unwrap_or_default();
                if !elements
                    .iter()
                    .chain(&wanted)
                    .any(|value| self.jar_equals(value))
                {
                    return Ok(None);
                }
                if !self.argument_contains_is_exact(collection, &elements)? {
                    // A hash or tree set's lookup with a jar `equals`.
                    return self.host.unknown_operand_call(r).map(Some);
                }
                let retain = name == "retainAll";
                let mut kept = Vec::with_capacity(elements.len());
                for (index, element) in elements.iter().enumerate() {
                    let contained = match self.argument_contains(collection, element) {
                        Ok(Some(contained)) => contained,
                        Ok(None) => return self.host.unknown_operand_call(r).map(Some),
                        Err(error) => {
                            // `ArrayList.batchRemove` keeps what it already
                            // decided and every element from the one whose
                            // `contains` threw (`AbstractCollection`'s
                            // iterator removal leaves the same list).
                            if kept.len() != index {
                                kept.extend_from_slice(&elements[index..]);
                                self.host.replace_mutable_collection(receiver, kept);
                            }
                            return Err(error);
                        }
                    };
                    if contained == retain {
                        kept.push(element.clone());
                    }
                }
                let changed = kept.len() != elements.len();
                if changed {
                    self.host.replace_mutable_collection(receiver, kept);
                }
                Ok(Some(Value::Int(i32::from(changed))))
            }
            _ => Ok(None),
        }
    }

    /// `ArrayDeque`'s `contains`/`remove(Object)`/`removeFirstOccurrence`
    /// (head to tail) and `removeLastOccurrence` (tail to head): `o.equals(e)`,
    /// and `null` is never there. The host compared by marker.
    fn array_deque_equality(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let (name, [needle]) = (r.name.as_str(), args) else {
            return Ok(None);
        };
        if r.descriptor != "(Ljava/lang/Object;)Z"
            || !matches!(
                name,
                "contains" | "remove" | "removeFirstOccurrence" | "removeLastOccurrence"
            )
        {
            return Ok(None);
        }
        if *needle == Value::Ref(None) {
            return Ok(Some(Value::Int(0)));
        }
        if !self.jar_equals(needle) {
            return Ok(None);
        }
        let mut elements = self.collection_elements(receiver)?;
        let order: Vec<usize> = if name == "removeLastOccurrence" {
            (0..elements.len()).rev().collect()
        } else {
            (0..elements.len()).collect()
        };
        for index in order {
            if self.java_equals(needle, &elements[index])? {
                if name != "contains" {
                    elements.remove(index);
                    self.host.replace_mutable_collection(receiver, elements);
                }
                return Ok(Some(Value::Int(1)));
            }
        }
        Ok(Some(Value::Int(0)))
    }
}
