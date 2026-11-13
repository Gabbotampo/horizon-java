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
    rule: Option<ModRule>,
    size: usize,
    stamp: ModStamp,
    /// `subList(from, to)`: the view to link once the host made it.
    creates: Option<(usize, usize)>,
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
        if !sub_list && !creates && !may_modify(name) {
            return Ok(None);
        }
        let Some(class) = self
            .host_collection_class(target)
            .filter(|class| tracked(class))
        else {
            return Ok(None);
        };
        if sub_list && !self.sync_sub_list(r, marker)? {
            // Counted: the view may be stale; the host answers from its copy.
            return Ok(None);
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
        Ok(Some(ModToken {
            marker: marker.clone(),
            rule,
            size: self.element_count(target),
            stamp: self.host.mod_stamp(marker),
            creates,
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
        Ok(())
    }

    /// `checkForComodification` for a call on a `subList` copy, up its chain
    /// of parents, then its range re-read from the parent. `Ok(false)` when
    /// a parent may have moved (counted).
    fn sync_sub_list(&mut self, r: &MethodRef, marker: &str) -> Result<bool, NativeError> {
        let mut chain = Vec::new();
        let mut at = marker.to_owned();
        while let Some(link) = self.host.sub_list_link(&at) {
            if chain.len() > MAX_SUPERCLASS_WALK {
                break;
            }
            chain.push((at.clone(), link.clone()));
            at = link.parent;
        }
        for (_, link) in &chain {
            match self.host.mod_stamp(&link.parent).verdict(link.expected) {
                ModVerdict::Same => {}
                ModVerdict::Changed => {
                    return Err(NativeError::UncaughtException {
                        class: "java/util/ConcurrentModificationException".into(),
                        message: None,
                        frame: None,
                    });
                }
                ModVerdict::Unknown => {
                    self.host.unknown_operand_call(r)?;
                    return Ok(false);
                }
            }
        }
        // Outermost first: each view re-reads its parent's current range.
        for (view, link) in chain.iter().rev() {
            self.reslice(view, link);
        }
        Ok(true)
    }

    /// Re-read a view's range from its parent (a parent's `set` shows).
    fn reslice(&mut self, view: &str, link: &SubListLink) {
        let parent = Value::Ref(Some(link.parent.clone()));
        let Ok(elements) = self.collection_elements(&parent) else {
            return;
        };
        if link.offset + link.size <= elements.len() {
            let range = elements[link.offset..link.offset + link.size].to_vec();
            self.host
                .replace_mutable_collection(&Value::Ref(Some(view.to_owned())), range);
        }
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
