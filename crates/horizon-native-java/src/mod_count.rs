//! `modCount`: JDK 25's structural-modification counter, per collection.
//!
//! Java's fail-fast iterators, `forEach`, `removeIf` and the `compute`
//! family compare a collection's `modCount` with the value they started
//! from. A change that undoes itself (`add` then `remove`, `remove` then
//! `put` of the same key) still moved it, and some members move it without
//! changing anything: `ArrayList.addAll` of an empty collection, `sort` of
//! a sorted list, `clear` of an empty one. Comparing contents misses all of
//! these, so this module keeps the counter itself.
//!
//! Every call the interpreter dispatches to a host collection passes
//! through [`Runtime::mod_count_before`] and [`Runtime::mod_count_after`],
//! which bump the receiver's counter by JDK 25's rule for that member
//! ([`mod_rule`]):
//!
//! - **Always**: `clear` everywhere; `ArrayList`/`Vector` `addAll`, `sort`,
//!   `replaceAll` and `trimToSize`; `Vector.removeElement`/`setSize`/
//!   `removeAllElements`; `PriorityQueue`'s `removeIf`/`removeAll`/
//!   `retainAll` (`bulkRemove` bumps first).
//! - **When the size changed**: every other member that can insert or
//!   remove (`add`, `remove`, `put` of a new key, `poll`, `compute`...).
//!   For one call, a size change is exactly a structural change: no member
//!   inserts and removes at once, and the ones that change nothing
//!   structural (`set`, `put` of a present key, `LinkedList.sort`, a map's
//!   `replaceAll`) leave the size alone.
//! - **Unknown**: `ensureCapacity` (it bumps only when the backing array
//!   grows, which the host does not model) and the sequenced `putFirst`/
//!   `putLast`/`addFirst`/`addLast` of the linked hash collections (they
//!   bump when they move a present key). These bump an "uncertain" count
//!   next to the counter, and a check that sees it moved is counted, not
//!   answered.
//!
//! The only counts that matter are equal or not, so a call that bumps more
//! than once (`removeAll`) bumps once here.
//!
//! **`subList`** is a view in Java; the host hands out a copy. This module
//! links the copy to its parent ([`SubListLink`]): every call on the copy
//! first checks that the parent's counter is where the view last saw it
//! (`checkForComodification`, a `ConcurrentModificationException` if not)
//! and re-reads its range, and after the call writes the copy back into
//! the parent's range, bumping the parent for a structural change. A
//! sub-list of a sub-list writes through both.

use super::*;

/// A collection's `modCount`, and how many of its bumps were guesses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModStamp {
    count: u64,
    uncertain: u64,
}

/// How a `modCount` check ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModVerdict {
    /// `modCount == expectedModCount`.
    Same,
    /// A structural change: Java throws `ConcurrentModificationException`.
    Changed,
    /// A member that may or may not have bumped it ran: counted.
    Unknown,
}

impl ModStamp {
    /// The check of `expected` against this (live) stamp.
    pub(crate) fn verdict(self, expected: ModStamp) -> ModVerdict {
        if self == expected {
            ModVerdict::Same
        } else if self.uncertain != expected.uncertain {
            ModVerdict::Unknown
        } else {
            ModVerdict::Changed
        }
    }

    fn bumped(self, uncertain: bool) -> Self {
        ModStamp {
            count: self.count + 1,
            uncertain: self.uncertain + u64::from(uncertain),
        }
    }
}

/// Where a `subList` copy sits in its parent, and the parent's counter when
/// the view last synchronised with it.
#[derive(Clone, Debug, PartialEq)]
pub struct SubListLink {
    parent: String,
    offset: usize,
    size: usize,
    expected: ModStamp,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ModRule {
    /// Bumps on every call.
    Always,
    /// Bumps when the call inserted or removed (the size changed).
    Resized,
    /// May bump; the host cannot tell.
    Uncertain,
}

/// The collections whose `modCount` is kept. `ArrayDeque` has none, but its
/// iterator reads the live array, so a change under it is noted the same
/// way (and counted there).
fn tracked(class: &str) -> bool {
    matches!(
        class,
        "java/util/ArrayList"
            | "java/util/Vector"
            | "java/util/Stack"
            | "java/util/LinkedList"
            | "java/util/ArrayDeque"
            | "java/util/PriorityQueue"
            | "java/util/HashMap"
            | "java/util/LinkedHashMap"
            | "java/util/TreeMap"
            | "java/util/IdentityHashMap"
            | "java/util/WeakHashMap"
            | "java/util/HashSet"
            | "java/util/LinkedHashSet"
            | "java/util/TreeSet"
    )
}

/// Members that may change a collection's structure.
fn may_modify(name: &str) -> bool {
    matches!(
        name,
        "add"
            | "addAll"
            | "addElement"
            | "insertElementAt"
            | "addFirst"
            | "addLast"
            | "offer"
            | "offerFirst"
            | "offerLast"
            | "push"
            | "pop"
            | "poll"
            | "pollFirst"
            | "pollLast"
            | "pollFirstEntry"
            | "pollLastEntry"
            | "remove"
            | "removeAll"
            | "retainAll"
            | "removeIf"
            | "removeFirst"
            | "removeLast"
            | "removeFirstOccurrence"
            | "removeLastOccurrence"
            | "removeElement"
            | "removeElementAt"
            | "removeAllElements"
            | "removeRange"
            | "clear"
            | "put"
            | "putAll"
            | "putIfAbsent"
            | "putFirst"
            | "putLast"
            | "compute"
            | "computeIfAbsent"
            | "computeIfPresent"
            | "merge"
            | "replaceAll"
            | "sort"
            | "trimToSize"
            | "ensureCapacity"
            | "setSize"
    )
}

/// JDK 25's `modCount` rule for `name` on a collection of `class`.
fn mod_rule(class: &str, name: &str) -> Option<ModRule> {
    if !may_modify(name) {
        return None;
    }
    let array_list = matches!(
        class,
        "java/util/ArrayList" | "java/util/Vector" | "java/util/Stack"
    );
    let vector = matches!(class, "java/util/Vector" | "java/util/Stack");
    let linked_hash = matches!(class, "java/util/LinkedHashMap" | "java/util/LinkedHashSet");
    Some(match name {
        "clear" => ModRule::Always,
        "addAll" | "sort" | "replaceAll" | "trimToSize" if array_list => ModRule::Always,
        "removeElement" | "removeAllElements" | "setSize" if vector => ModRule::Always,
        "removeIf" | "removeAll" | "retainAll" if class == "java/util/PriorityQueue" => {
            ModRule::Always
        }
        "ensureCapacity" if array_list => ModRule::Uncertain,
        "putFirst" | "putLast" | "addFirst" | "addLast" if linked_hash => ModRule::Uncertain,
        _ => ModRule::Resized,
    })
}

/// The rule for a call on a `subList` view: the root's `removeRange`
/// (`clear`) always bumps an `ArrayList`/`Vector`; everything else reaches
/// the root as an insert or a remove, or not at all (`set`, and `sort`/
/// `replaceAll`, which a sub-list runs through `set`).
fn sub_list_rule(root_class: &str, name: &str) -> Option<ModRule> {
    if !may_modify(name) {
        return None;
    }
    let array_list = matches!(
        root_class,
        "java/util/ArrayList" | "java/util/Vector" | "java/util/Stack"
    );
    Some(if name == "clear" && array_list {
        ModRule::Always
    } else {
        ModRule::Resized
    })
}

/// A call in flight on a tracked collection.
pub(crate) struct ModToken {
    marker: String,
    name: String,
    rule: Option<ModRule>,
    size: usize,
    stamp: ModStamp,
    /// `subList(from, to)`: the view to link once the host made it.
    creates: Option<(usize, usize)>,
    /// An `ArrayDeque`'s layout to follow (`array_deque.rs`).
    deque: DequeStep,
    /// A stale `ArrayList.SubList` member that checks `modCount` after its
    /// work (`equals`, `hashCode`, `indexOf`...): throw once it returned.
    throw_after: bool,
}

/// What a call does to an `ArrayDeque`'s layout.
enum DequeStep {
    None,
    /// A constructor: the layout it gives (`None`: unknown).
    Init(Option<array_deque::DequeLayout>),
    /// A call on a deque: its elements before, and the `Object` argument.
    Call {
        before: Vec<Value>,
        needle: Option<Value>,
    },
}

/// Where a `SubList` member calls `checkForComodification`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SubCheck {
    /// Before its work: a stale view throws.
    Before,
    /// After its work over the root's range (`ArrayList.SubList`'s
    /// `equals`/`hashCode`/`indexOf`/`lastIndexOf`/`contains`).
    After,
    /// Not at all (`subList`, an argument check that fails or returns
    /// first, `equals(this)`), or not against this view
    /// (`ArrayList.SubList.replaceAll` works on the root's range).
    Never,
    /// Its answer over a stale view is not modelled: counted.
    CountIfStale,
}

/// How a call on a `subList` copy goes ahead.
enum SubSync {
    Ready,
    Counted,
    ThrowAfter,
}

impl<H: Host> Runtime<'_, H> {
    /// The live `modCount` of a collection.
    pub(crate) fn mod_stamp(&self, marker: &str) -> ModStamp {
        self.host.mod_stamp(marker)
    }

    /// Bump a collection's `modCount` (`uncertain`: it may not have moved).
    pub(crate) fn bump_mod_count(&mut self, marker: &str, uncertain: bool) {
        let stamp = self.host.mod_stamp(marker).bumped(uncertain);
        self.host.store_mod_stamp(marker, stamp);
    }

    /// The collection whose `modCount` a view's iterator checks: the root
    /// of a chain of sub-lists, the map of a `keySet()`/`values()` copy, the
    /// list under an unmodifiable view.
    pub(crate) fn mod_count_root(&self, marker: &str) -> String {
        let mut at = marker.to_owned();
        for _ in 0..MAX_SUPERCLASS_WALK {
            let next = self
                .host
                .sub_list_link(&at)
                .map(|link| link.parent)
                .or_else(|| self.host.view_copy_source(&at))
                .or_else(|| self.host.unmodifiable_view_source(&at));
            match next {
                Some(next) if next != at => at = next,
                _ => break,
            }
        }
        at
    }

    fn element_count(&self, value: &Value) -> usize {
        self.host
            .mutable_collection_values(value)
            .map_or(0, |values| values.len())
    }

    /// Before a call: whether it may move a tracked collection's `modCount`
    /// (or is a call on a `subList` copy, which is checked and refreshed
    /// first). `Collections.sort`/`addAll` act on their first argument.
    pub(crate) fn mod_count_before(
        &mut self,
        r: &MethodRef,
        op: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<ModToken>, NativeError> {
        // A sub-list read as an argument (copied, compared, printed) checks
        // its list first, as its `toArray`/`iterator`/`toString` would.
        self.sync_sub_list_arguments(r, op, receiver, args)?;
        if op == 0xb7 && r.name == "<init>" && r.owner == "java/util/ArrayDeque" {
            let Some(marker) = receiver.and_then(expect_ref).cloned() else {
                return Ok(None);
            };
            let layout = self.deque_constructed(args);
            return Ok(Some(ModToken {
                marker,
                name: r.name.clone(),
                rule: None,
                size: 0,
                stamp: ModStamp::default(),
                creates: None,
                deque: DequeStep::Init(layout),
                throw_after: false,
            }));
        }
        let (target, name) = match op {
            0xb8 if r.owner == "java/util/Collections" => match (r.name.as_str(), args.first()) {
                (name @ ("sort" | "addAll"), Some(target)) => (target, name),
                _ => return Ok(None),
            },
            0xb6 | 0xb7 | 0xb9 if r.name != "<init>" => match receiver {
                Some(receiver) => (receiver, r.name.as_str()),
                None => return Ok(None),
            },
            _ => return Ok(None),
        };
        let Some(marker) = expect_ref(target) else {
            return Ok(None);
        };
        let creates = name == "subList" && r.descriptor == "(II)Ljava/util/List;";
        let sub_list = self.host.sub_list_link(marker).is_some();
        // A list with views: even a `set` must reach them.
        let has_views = !self.host.sub_lists_of(marker).is_empty()
            || !self.host.view_copies_of(marker).is_empty();
        if !sub_list && !creates && !has_views && !may_modify(name) {
            return Ok(None);
        }
        let Some(class) = self
            .host_collection_class(target)
            .filter(|class| tracked(class))
        else {
            return Ok(None);
        };
        let mut throw_after = false;
        if sub_list {
            let point = self.sub_list_check_point(r, marker, target, args)?;
            match self.sync_sub_list(r, marker, point)? {
                SubSync::Ready => {}
                // Counted: the view may be stale; the host answers from its copy.
                SubSync::Counted => return Ok(None),
                SubSync::ThrowAfter => throw_after = true,
            }
        }
        let rule = if sub_list {
            let root = self.mod_count_root(marker);
            let root_class = self
                .host_collection_class(&Value::Ref(Some(root)))
                .unwrap_or_default();
            sub_list_rule(&root_class, name)
        } else if op == 0xb8 && name == "addAll" {
            // `Collections.addAll`: one `c.add` per element.
            Some(ModRule::Resized)
        } else {
            mod_rule(&class, name)
        };
        let creates = match (creates, args) {
            (true, [Value::Int(from), Value::Int(to)]) if 0 <= *from && from <= to => {
                Some((*from as usize, *to as usize))
            }
            _ => None,
        };
        let deque = if class == "java/util/ArrayDeque"
            && may_modify(name)
            && self.host.deque_layout(marker).is_some()
        {
            DequeStep::Call {
                before: self.collection_elements(target)?,
                needle: match args {
                    [needle] if r.descriptor == "(Ljava/lang/Object;)Z" => Some(needle.clone()),
                    _ => None,
                },
            }
        } else {
            DequeStep::None
        };
        Ok(Some(ModToken {
            marker: marker.clone(),
            name: name.to_owned(),
            rule,
            size: self.element_count(target),
            stamp: self.host.mod_stamp(marker),
            creates,
            deque,
            throw_after,
        }))
    }

    /// After a call that returned: bump by the member's rule, write a
    /// `subList` copy back into its parent, link a new `subList`, and
    /// re-read the views of a list whose elements a `set` changed.
    pub(crate) fn mod_count_after(
        &mut self,
        token: ModToken,
        value: &Value,
    ) -> Result<(), NativeError> {
        match &token.deque {
            DequeStep::Init(layout) => {
                self.host.store_deque_layout(&token.marker, *layout);
                return Ok(());
            }
            DequeStep::Call { before, needle } => {
                self.deque_stepped(&token.marker, &token.name, needle.as_ref(), before);
            }
            DequeStep::None => {}
        }
        let target = Value::Ref(Some(token.marker.clone()));
        let size = self.element_count(&target);
        match token.rule {
            Some(ModRule::Always) => self.bump_mod_count(&token.marker, false),
            Some(ModRule::Resized) if size != token.size => {
                self.bump_mod_count(&token.marker, false)
            }
            Some(ModRule::Uncertain) => self.bump_mod_count(&token.marker, true),
            _ => {}
        }
        let after = self.host.mod_stamp(&token.marker);
        if self.host.sub_list_link(&token.marker).is_some() {
            let structural =
                (after != token.stamp).then_some(after.uncertain != token.stamp.uncertain);
            self.write_back_sub_list(&token.marker, structural);
        }
        if let (Some((from, to)), Some(view)) = (token.creates, expect_ref(value))
            && self.host.mutable_collection_values(value).is_some()
        {
            self.host.store_sub_list_link(
                view,
                Some(SubListLink {
                    parent: token.marker.clone(),
                    offset: from,
                    size: to - from,
                    expected: after,
                }),
            );
        }
        self.refresh_sub_lists_of(&token.marker);
        // `keySet()`/`values()`/`entrySet()` kept from this map follow it
        // (`hash_collections.rs`).
        self.refresh_map_views(&token.marker);
        if token.throw_after {
            return Err(NativeError::UncaughtException {
                class: "java/util/ConcurrentModificationException".into(),
                message: None,
                frame: None,
            });
        }
        Ok(())
    }

    /// Where a `SubList` member checks `modCount`, by JDK 25's
    /// `ArrayList.SubList` (an `ArrayList` root) or `AbstractList.SubList`
    /// (a `LinkedList`'s or `Vector`'s): the index and argument checks that
    /// throw or return before it, the members that check after their work,
    /// and the ones that never do.
    fn sub_list_check_point(
        &self,
        r: &MethodRef,
        marker: &str,
        target: &Value,
        args: &[Value],
    ) -> Result<SubCheck, NativeError> {
        let root = self.mod_count_root(marker);
        let array_list = self
            .host_collection_class(&Value::Ref(Some(root)))
            .as_deref()
            == Some("java/util/ArrayList");
        let size = self.element_count(target) as i64;
        let empty = |value: &Value| {
            self.host_collection_kind(value).is_some() && self.element_count(value) == 0
        };
        let outside = |index: &i32, limit: i64| i64::from(*index) < 0 || i64::from(*index) >= limit;
        Ok(match (r.name.as_str(), r.descriptor.as_str(), args) {
            ("subList", _, _) => SubCheck::Never,
            ("equals", "(Ljava/lang/Object;)Z", [other]) if other == target => SubCheck::Never,
            ("get" | "remove", "(I)Ljava/lang/Object;", [Value::Int(index)])
            | ("set", "(ILjava/lang/Object;)Ljava/lang/Object;", [Value::Int(index), _])
                if outside(index, size) =>
            {
                SubCheck::Never
            }
            ("add", "(ILjava/lang/Object;)V", [Value::Int(index), _])
                if outside(index, size + 1) =>
            {
                SubCheck::Never
            }
            ("addAll", "(Ljava/util/Collection;)Z", [other]) if empty(other) => SubCheck::Never,
            ("addAll", "(ILjava/util/Collection;)Z", [Value::Int(index), other])
                if outside(index, size + 1) || empty(other) =>
            {
                SubCheck::Never
            }
            ("containsAll", _, [other]) if empty(other) => SubCheck::Never,
            ("containsAll", _, _) if array_list => SubCheck::CountIfStale,
            ("equals" | "hashCode" | "indexOf" | "lastIndexOf" | "contains", _, _)
                if array_list =>
            {
                SubCheck::After
            }
            ("replaceAll", _, _) if array_list => SubCheck::CountIfStale,
            _ => SubCheck::Before,
        })
    }

    /// `checkForComodification` for a call on a `subList` copy, up its chain
    /// of parents, where the member checks it; then its range re-read from
    /// the parent.
    fn sync_sub_list(
        &mut self,
        r: &MethodRef,
        marker: &str,
        point: SubCheck,
    ) -> Result<SubSync, NativeError> {
        let mut chain = Vec::new();
        let mut at = marker.to_owned();
        while let Some(link) = self.host.sub_list_link(&at) {
            if chain.len() > MAX_SUPERCLASS_WALK {
                break;
            }
            chain.push((at.clone(), link.clone()));
            at = link.parent;
        }
        let verdict = chain
            .iter()
            .map(|(_, link)| self.host.mod_stamp(&link.parent).verdict(link.expected))
            .find(|verdict| *verdict != ModVerdict::Same)
            .unwrap_or(ModVerdict::Same);
        let reslice_all = |this: &mut Self| -> bool {
            // Outermost first: each view re-reads its parent's current range.
            for (view, link) in chain.iter().rev() {
                if !this.reslice(view, link) {
                    return false;
                }
            }
            true
        };
        match (verdict, point) {
            (ModVerdict::Same, _) => {
                reslice_all(self);
                Ok(SubSync::Ready)
            }
            // A stale view a member does not check: left as it is.
            (_, SubCheck::Never) => Ok(SubSync::Ready),
            (ModVerdict::Changed, SubCheck::Before) => Err(NativeError::UncaughtException {
                class: "java/util/ConcurrentModificationException".into(),
                message: None,
                frame: None,
            }),
            // The work runs over the root's current range, then throws.
            (ModVerdict::Changed, SubCheck::After) if reslice_all(self) => Ok(SubSync::ThrowAfter),
            _ => {
                self.host.unknown_operand_call(r)?;
                Ok(SubSync::Counted)
            }
        }
    }

    /// Sub-lists passed where they are read: a `Collection`/`List`/
    /// `Iterable` parameter (`new ArrayList<>(s)`, `addAll(s)`), a
    /// collection's `equals(s)`, and the text and hash calls
    /// (`String.valueOf(s)`, `StringBuilder.append(s)`, `Objects.hash`...).
    fn sync_sub_list_arguments(
        &mut self,
        r: &MethodRef,
        op: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<(), NativeError> {
        if args.is_empty() || self.host.sub_lists_of_any() == 0 {
            return Ok(());
        }
        let params = descriptor_arguments(&r.descriptor);
        let text_call = matches!(
            r.owner.as_str(),
            "java/lang/String"
                | "java/lang/StringBuilder"
                | "java/lang/StringBuffer"
                | "java/io/PrintStream"
                | "java/util/Objects"
        );
        let collection_receiver = receiver
            .is_some_and(|receiver| op != 0xb8 && self.host_collection_kind(receiver).is_some());
        for (index, arg) in args.iter().enumerate() {
            let Some(marker) = expect_ref(arg).cloned() else {
                continue;
            };
            if self.host.sub_list_link(&marker).is_none() || receiver == Some(arg) {
                continue;
            }
            let param = params.get(index).map(String::as_str).unwrap_or("");
            let reads = matches!(
                param,
                "Ljava/util/Collection;" | "Ljava/util/List;" | "Ljava/lang/Iterable;"
            ) || (param == "Ljava/lang/Object;"
                && (text_call || (collection_receiver && r.name == "equals")));
            if reads {
                self.sync_sub_list(r, &marker, SubCheck::Before)?;
            }
        }
        Ok(())
    }

    /// Sub-lists a string concatenation prints: `toString` iterates them.
    pub(crate) fn sync_sub_list_operands(&mut self, values: &[Value]) -> Result<(), NativeError> {
        if self.host.sub_lists_of_any() == 0 {
            return Ok(());
        }
        let to_string = MethodRef {
            owner: "java/util/List".into(),
            name: "toString".into(),
            descriptor: "()Ljava/lang/String;".into(),
        };
        for value in values {
            if let Some(marker) = expect_ref(value)
                && self.host.sub_list_link(marker).is_some()
            {
                self.sync_sub_list(&to_string, &marker.clone(), SubCheck::Before)?;
            }
        }
        Ok(())
    }

    /// Re-read a view's range from its parent (a parent's `set` shows).
    fn reslice(&mut self, view: &str, link: &SubListLink) -> bool {
        let parent = Value::Ref(Some(link.parent.clone()));
        let Ok(elements) = self.collection_elements(&parent) else {
            return false;
        };
        if link.offset + link.size > elements.len() {
            return false;
        }
        let range = elements[link.offset..link.offset + link.size].to_vec();
        self.host
            .replace_mutable_collection(&Value::Ref(Some(view.to_owned())), range);
        true
    }

    /// Write a `subList` copy into its parent's range; a structural change
    /// (`Some(uncertain)`) bumps the parent, as `root.add`/`root.remove`/
    /// `root.removeRange` do, and the view takes the parent's new counter
    /// (`updateSizeAndModCount`). Up the chain for a sub-list's sub-list.
    fn write_back_sub_list(&mut self, view: &str, structural: Option<bool>) {
        let mut view = view.to_owned();
        for _ in 0..MAX_SUPERCLASS_WALK {
            let Some(mut link) = self.host.sub_list_link(&view) else {
                return;
            };
            let parent = Value::Ref(Some(link.parent.clone()));
            let copy = Value::Ref(Some(view.clone()));
            let (Ok(content), Ok(mut elements)) = (
                self.collection_elements(&copy),
                self.collection_elements(&parent),
            ) else {
                return;
            };
            if link.offset + link.size > elements.len()
                || (structural.is_none()
                    && elements[link.offset..link.offset + link.size] == content[..])
            {
                return;
            }
            elements.splice(
                link.offset..link.offset + link.size,
                content.iter().cloned(),
            );
            self.host.replace_mutable_collection(&parent, elements);
            if let Some(uncertain) = structural {
                self.bump_mod_count(&link.parent, uncertain);
            }
            link.size = content.len();
            link.expected = self.host.mod_stamp(&link.parent);
            self.host.store_sub_list_link(&view, Some(link.clone()));
            view = link.parent;
        }
    }

    /// Views of `parent` still in step with it see its elements again (a
    /// `set` on the parent shows through them).
    fn refresh_sub_lists_of(&mut self, parent: &str) {
        for (view, link) in self.host.sub_lists_of(parent) {
            if self.host.mod_stamp(parent) == link.expected {
                self.reslice(&view, &link);
                self.refresh_sub_lists_of(&view);
            }
        }
    }
}

impl SubListLink {
    /// The list this view was taken from.
    pub(crate) fn parent(&self) -> &str {
        &self.parent
    }
}

/// The fail-fast protocol for iterators and spliterators kept outside this
/// module (a `ListIterator`, a stream's source spliterator), the JDK's way:
///
/// ```text
/// let expected = runtime.capture_mod_count(&list);        // at creation
/// ...
/// if !runtime.check_mod_count(r, &list, expected)? { .. }  // in next() /
///                                                          // at forEachRemaining's end
/// // the iterator's own add/remove, applied to the host's row directly:
/// let expected = runtime.own_structural_change(&list);
/// ```
///
/// Calls the interpreter dispatches to the collection (`list.add(x)` in
/// jar code, `list::add`) already move its counter; `own_structural_change`
/// is only for a change the caller makes to the host's row itself.
impl<H: Host> Runtime<'_, H> {
    /// Whether `collection` is a host collection with a `modCount` (its
    /// iterators fail fast). `ArrayDeque` and the immutable collections
    /// have none.
    pub(crate) fn has_mod_count(&self, collection: &Value) -> bool {
        self.host_collection_class(collection)
            .is_some_and(|class| tracked(&class) && class != "java/util/ArrayDeque")
    }

    /// `expectedModCount = modCount`, of the collection whose counter an
    /// iterator over `collection` checks (a sub-list's or a view's root).
    pub(crate) fn capture_mod_count(&self, collection: &Value) -> ModStamp {
        let marker = expect_ref(collection).map_or("", String::as_str);
        self.host.mod_stamp(&self.mod_count_root(marker))
    }

    /// `if (modCount != expectedModCount) throw new
    /// ConcurrentModificationException()`: `Ok(true)` when unchanged,
    /// `Err` when changed, `Ok(false)` when a member that may or may not
    /// have bumped it ran (the call `r` is then counted).
    pub(crate) fn check_mod_count(
        &mut self,
        r: &MethodRef,
        collection: &Value,
        expected: ModStamp,
    ) -> Result<bool, NativeError> {
        match self.capture_mod_count(collection).verdict(expected) {
            ModVerdict::Same => Ok(true),
            ModVerdict::Changed => Err(NativeError::UncaughtException {
                class: "java/util/ConcurrentModificationException".into(),
                message: None,
                frame: None,
            }),
            ModVerdict::Unknown => self.host.unknown_operand_call(r).map(|_| false),
        }
    }

    /// An iterator's own structural change (`ListIterator.add`/`remove`,
    /// `Iterator.remove`) made to the host's row directly: `modCount++`,
    /// and the new value for `expectedModCount = modCount`.
    pub(crate) fn own_structural_change(&mut self, collection: &Value) -> ModStamp {
        let marker = expect_ref(collection).map_or("", String::as_str);
        let root = self.mod_count_root(marker);
        self.bump_mod_count(&root, false);
        self.host.mod_stamp(&root)
    }
}
