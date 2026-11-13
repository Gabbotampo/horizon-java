//! `java.util.TreeMap` and `TreeSet` as JDK 25 runs them.
//!
//! The host kept a tree map's entries in insertion order and found keys by
//! marker: `TreeMap<Integer>` of 5,1,3 iterated `[5, 1, 3]`, and with a record
//! key ordered by one component, `put` of a second key that `compareTo`s as
//! equal added an entry instead of replacing the value. All uncounted.
//!
//! A tree map finds, orders and replaces keys only through `compareTo` (or
//! its comparator), along the path of its red-black tree, so the calls depend
//! on the tree's shape. This module keeps the tree the JDK would have: the
//! `put` walk (an empty map first compares the key with itself, as
//! `addEntryToEmptyMap` does), `fixAfterInsertion`'s recolouring and
//! rotations, `deleteEntry` (a node with two children takes its successor's
//! key) and `fixAfterDeletion`, the `getCeilingEntry`/`getFloorEntry`/
//! `getHigherEntry`/`getLowerEntry` walks, and `buildFromSorted` for a copy
//! of a sorted map or set (balanced, no comparisons). The host's list is the
//! tree's in-order sequence.
//!
//! Answered here: the constructors, `put`/`putIfAbsent`/`get`/`containsKey`/
//! `getOrDefault`/`remove`/`replace`/`containsValue`/`putAll`/`clear`, a
//! set's `add`/`contains`/`remove`/`addAll`, `first*`/`last*`/`poll*`, the
//! `floor`/`ceiling`/`higher`/`lower` lookups and `comparator()`, matched by
//! exact name; `computeIfAbsent`/`computeIfPresent`/`compute`/`merge` and
//! `forEach`, which run their callbacks with the tree put back and throw a
//! `ConcurrentModificationException` when a callback changed its structure
//! (`RedBlackTree::mod_count`). The
//! navigable views (`headMap`, `subSet`, `descendingMap`...) are counted. A
//! key another path added is placed by the next call while the order of its
//! comparisons cannot show (pure keys, natural order); otherwise the tree's
//! order is unknown and a call that shows it is counted.

use super::*;
use object_methods::CollectionKind;

/// One node: its key and links.
#[derive(Clone, Debug, PartialEq)]
struct Node {
    key: Value,
    left: Option<usize>,
    right: Option<usize>,
    parent: Option<usize>,
    red: bool,
}

/// A `TreeMap`'s red-black tree over the keys (the host keeps the values).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RedBlackTree {
    nodes: Vec<Option<Node>>,
    root: Option<usize>,
    /// A key was added by a path that could not place it.
    pub(crate) tainted: bool,
    /// `modCount`: bumped by every insertion and deletion (a value replaced
    /// in place is not structural), so a callback that changed the tree
    /// shows as a different count.
    mod_count: u64,
}

impl RedBlackTree {
    fn node(&self, at: usize) -> &Node {
        self.nodes[at].as_ref().expect("a live node")
    }

    fn node_mut(&mut self, at: usize) -> &mut Node {
        self.nodes[at].as_mut().expect("a live node")
    }

    fn left(&self, at: Option<usize>) -> Option<usize> {
        at.and_then(|at| self.node(at).left)
    }

    fn right(&self, at: Option<usize>) -> Option<usize> {
        at.and_then(|at| self.node(at).right)
    }

    fn parent(&self, at: Option<usize>) -> Option<usize> {
        at.and_then(|at| self.node(at).parent)
    }

    /// `colorOf`: `null` is black.
    fn is_red(&self, at: Option<usize>) -> bool {
        at.is_some_and(|at| self.node(at).red)
    }

    fn set_red(&mut self, at: Option<usize>, red: bool) {
        if let Some(at) = at {
            self.node_mut(at).red = red;
        }
    }

    fn new_node(&mut self, key: Value, parent: Option<usize>) -> usize {
        self.mod_count += 1;
        self.nodes.push(Some(Node {
            key,
            left: None,
            right: None,
            parent,
            red: false,
        }));
        self.nodes.len() - 1
    }

    fn first(&self) -> Option<usize> {
        let mut p = self.root?;
        while let Some(left) = self.node(p).left {
            p = left;
        }
        Some(p)
    }

    fn last(&self) -> Option<usize> {
        let mut p = self.root?;
        while let Some(right) = self.node(p).right {
            p = right;
        }
        Some(p)
    }

    /// `successor(t)`.
    fn successor(&self, t: usize) -> Option<usize> {
        if let Some(mut p) = self.node(t).right {
            while let Some(left) = self.node(p).left {
                p = left;
            }
            return Some(p);
        }
        let (mut p, mut ch) = (self.node(t).parent, t);
        while let Some(parent) = p {
            if self.node(parent).right != Some(ch) {
                break;
            }
            ch = parent;
            p = self.node(parent).parent;
        }
        p
    }

    /// The keys in order.
    pub(crate) fn keys(&self) -> Vec<Value> {
        let mut keys = Vec::new();
        let mut at = self.first();
        while let Some(node) = at {
            keys.push(self.node(node).key.clone());
            at = self.successor(node);
        }
        keys
    }

    fn find_marker(&self, marker: &str) -> Option<usize> {
        self.nodes.iter().position(|node| {
            node.as_ref()
                .is_some_and(|node| collection_item_marker(&node.key) == marker)
        })
    }

    fn rotate_left(&mut self, p: Option<usize>) {
        let Some(p) = p else { return };
        let r = self.node(p).right.expect("rotateLeft needs a right child");
        let r_left = self.node(r).left;
        self.node_mut(p).right = r_left;
        if let Some(rl) = r_left {
            self.node_mut(rl).parent = Some(p);
        }
        let parent = self.node(p).parent;
        self.node_mut(r).parent = parent;
        match parent {
            None => self.root = Some(r),
            Some(parent) if self.node(parent).left == Some(p) => {
                self.node_mut(parent).left = Some(r)
            }
            Some(parent) => self.node_mut(parent).right = Some(r),
        }
        self.node_mut(r).left = Some(p);
        self.node_mut(p).parent = Some(r);
    }

    fn rotate_right(&mut self, p: Option<usize>) {
        let Some(p) = p else { return };
        let l = self.node(p).left.expect("rotateRight needs a left child");
        let l_right = self.node(l).right;
        self.node_mut(p).left = l_right;
        if let Some(lr) = l_right {
            self.node_mut(lr).parent = Some(p);
        }
        let parent = self.node(p).parent;
        self.node_mut(l).parent = parent;
        match parent {
            None => self.root = Some(l),
            Some(parent) if self.node(parent).right == Some(p) => {
                self.node_mut(parent).right = Some(l)
            }
            Some(parent) => self.node_mut(parent).left = Some(l),
        }
        self.node_mut(l).right = Some(p);
        self.node_mut(p).parent = Some(l);
    }

    /// `fixAfterInsertion(x)`.
    fn fix_after_insertion(&mut self, x: usize) {
        let mut x = Some(x);
        self.set_red(x, true);
        while x.is_some() && x != self.root && self.is_red(self.parent(x)) {
            let grand = self.parent(self.parent(x));
            if self.parent(x) == self.left(grand) {
                let y = self.right(grand);
                if self.is_red(y) {
                    self.set_red(self.parent(x), false);
                    self.set_red(y, false);
                    self.set_red(grand, true);
                    x = grand;
                } else {
                    if x == self.right(self.parent(x)) {
                        x = self.parent(x);
                        self.rotate_left(x);
                    }
                    self.set_red(self.parent(x), false);
                    self.set_red(self.parent(self.parent(x)), true);
                    self.rotate_right(self.parent(self.parent(x)));
                }
            } else {
                let y = self.left(grand);
                if self.is_red(y) {
                    self.set_red(self.parent(x), false);
                    self.set_red(y, false);
                    self.set_red(grand, true);
                    x = grand;
                } else {
                    if x == self.left(self.parent(x)) {
                        x = self.parent(x);
                        self.rotate_right(x);
                    }
                    self.set_red(self.parent(x), false);
                    self.set_red(self.parent(self.parent(x)), true);
                    self.rotate_left(self.parent(self.parent(x)));
                }
            }
        }
        let root = self.root;
        self.set_red(root, false);
    }

    /// `addEntry(key, parent, addToLeft)`.
    fn add_entry(&mut self, key: Value, parent: usize, to_left: bool) {
        let at = self.new_node(key, Some(parent));
        if to_left {
            self.node_mut(parent).left = Some(at);
        } else {
            self.node_mut(parent).right = Some(at);
        }
        self.fix_after_insertion(at);
    }

    /// `deleteEntry(p)`.
    fn delete_entry(&mut self, p: usize) {
        self.mod_count += 1;
        let mut p = p;
        if self.node(p).left.is_some() && self.node(p).right.is_some() {
            let s = self
                .successor(p)
                .expect("a node with a right child has a successor");
            let key = self.node(s).key.clone();
            self.node_mut(p).key = key;
            p = s;
        }
        let replacement = self.node(p).left.or(self.node(p).right);
        let parent = self.node(p).parent;
        if let Some(replacement) = replacement {
            self.node_mut(replacement).parent = parent;
            match parent {
                None => self.root = Some(replacement),
                Some(parent) if self.node(parent).left == Some(p) => {
                    self.node_mut(parent).left = Some(replacement)
                }
                Some(parent) => self.node_mut(parent).right = Some(replacement),
            }
            let black = !self.node(p).red;
            self.nodes[p] = None;
            if black {
                self.fix_after_deletion(replacement);
            }
        } else if parent.is_none() {
            self.root = None;
            self.nodes[p] = None;
        } else {
            if !self.node(p).red {
                self.fix_after_deletion(p);
            }
            if let Some(parent) = self.node(p).parent {
                if self.node(parent).left == Some(p) {
                    self.node_mut(parent).left = None;
                } else if self.node(parent).right == Some(p) {
                    self.node_mut(parent).right = None;
                }
            }
            self.nodes[p] = None;
        }
    }

    /// `fixAfterDeletion(x)`.
    fn fix_after_deletion(&mut self, x: usize) {
        let mut x = Some(x);
        while x != self.root && !self.is_red(x) {
            if x == self.left(self.parent(x)) {
                let mut sib = self.right(self.parent(x));
                if self.is_red(sib) {
                    self.set_red(sib, false);
                    self.set_red(self.parent(x), true);
                    self.rotate_left(self.parent(x));
                    sib = self.right(self.parent(x));
                }
                if !self.is_red(self.left(sib)) && !self.is_red(self.right(sib)) {
                    self.set_red(sib, true);
                    x = self.parent(x);
                } else {
                    if !self.is_red(self.right(sib)) {
                        self.set_red(self.left(sib), false);
                        self.set_red(sib, true);
                        self.rotate_right(sib);
                        sib = self.right(self.parent(x));
                    }
                    let parent_red = self.is_red(self.parent(x));
                    self.set_red(sib, parent_red);
                    self.set_red(self.parent(x), false);
                    self.set_red(self.right(sib), false);
                    self.rotate_left(self.parent(x));
                    x = self.root;
                }
            } else {
                let mut sib = self.left(self.parent(x));
                if self.is_red(sib) {
                    self.set_red(sib, false);
                    self.set_red(self.parent(x), true);
                    self.rotate_right(self.parent(x));
                    sib = self.left(self.parent(x));
                }
                if !self.is_red(self.right(sib)) && !self.is_red(self.left(sib)) {
                    self.set_red(sib, true);
                    x = self.parent(x);
                } else {
                    if !self.is_red(self.left(sib)) {
                        self.set_red(self.right(sib), false);
                        self.set_red(sib, true);
                        self.rotate_left(sib);
                        sib = self.left(self.parent(x));
                    }
                    let parent_red = self.is_red(self.parent(x));
                    self.set_red(sib, parent_red);
                    self.set_red(self.parent(x), false);
                    self.set_red(self.left(sib), false);
                    self.rotate_right(self.parent(x));
                    x = self.root;
                }
            }
        }
        self.set_red(x, false);
    }

    /// `buildFromSorted`: a balanced tree over `keys` (in order), the nodes
    /// at `computeRedLevel(size)` red.
    fn build_from_sorted(keys: &[Value]) -> Self {
        let mut tree = Self::default();
        if keys.is_empty() {
            return tree;
        }
        let red_level = 31 - (keys.len() as u32 + 1).leading_zeros() as usize;
        tree.root = Some(tree.build(0, 0, keys.len() - 1, red_level, keys));
        tree
    }

    fn build(
        &mut self,
        level: usize,
        lo: usize,
        hi: usize,
        red_level: usize,
        keys: &[Value],
    ) -> usize {
        let mid = (lo + hi) >> 1;
        let left = (lo < mid).then(|| self.build(level + 1, lo, mid - 1, red_level, keys));
        let middle = self.new_node(keys[mid].clone(), None);
        self.node_mut(middle).red = level == red_level;
        if let Some(left) = left {
            self.node_mut(middle).left = Some(left);
            self.node_mut(left).parent = Some(middle);
        }
        if mid < hi {
            let right = self.build(level + 1, mid + 1, hi, red_level, keys);
            self.node_mut(middle).right = Some(right);
            self.node_mut(right).parent = Some(middle);
        }
        middle
    }
}

/// Which navigation walk to run.
#[derive(Clone, Copy, PartialEq)]
enum Nearest {
    Ceiling,
    Floor,
    Higher,
    Lower,
}

fn thrown(class: &str) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message: None,
        frame: None,
    }
}

/// The views a tree map hands out, which this module does not model.
fn navigable_view(name: &str) -> bool {
    matches!(
        name,
        "headMap"
            | "tailMap"
            | "subMap"
            | "headSet"
            | "tailSet"
            | "subSet"
            | "descendingMap"
            | "descendingKeySet"
            | "descendingSet"
            | "descendingIterator"
            | "navigableKeySet"
    )
}

impl<H: Host> Runtime<'_, H> {
    /// Whether `value` is a host `TreeMap` (`Some(true)`) or `TreeSet`
    /// (`Some(false)`), or a jar subclass's row of one.
    fn tree_kind(&self, value: &Value) -> Option<bool> {
        match self.host_collection_class(value)?.as_str() {
            "java/util/TreeMap" => Some(true),
            "java/util/TreeSet" => Some(false),
            _ => None,
        }
    }

    /// `compare(a, b)`: the comparator, or `a.compareTo(b)`.
    fn tree_compare(
        &mut self,
        comparator: &Option<Value>,
        a: &Value,
        b: &Value,
    ) -> Result<i32, NativeError> {
        self.ordered_compare(comparator, a, b)
    }

    /// The `put` walk: `Ok(Err(node))` for an equal key, `Ok(Ok((parent,
    /// left)))` for where a new key goes (`None` parent: an empty tree).
    #[allow(clippy::type_complexity)]
    fn tree_search(
        &mut self,
        tree: &RedBlackTree,
        comparator: &Option<Value>,
        key: &Value,
    ) -> Result<Result<Option<(usize, bool)>, usize>, NativeError> {
        let Some(mut t) = tree.root else {
            return Ok(Ok(None));
        };
        if comparator.is_none() && *key == Value::Ref(None) {
            return Err(thrown("java/lang/NullPointerException"));
        }
        loop {
            let cmp = self.tree_compare(comparator, key, &tree.node(t).key.clone())?;
            let next = match cmp.cmp(&0) {
                std::cmp::Ordering::Less => tree.node(t).left,
                std::cmp::Ordering::Greater => tree.node(t).right,
                std::cmp::Ordering::Equal => return Ok(Err(t)),
            };
            match next {
                Some(next) => t = next,
                None => return Ok(Ok(Some((t, cmp < 0)))),
            }
        }
    }

    /// `getEntry(key)`.
    fn tree_get(
        &mut self,
        tree: &RedBlackTree,
        comparator: &Option<Value>,
        key: &Value,
    ) -> Result<Option<usize>, NativeError> {
        if tree.root.is_none() {
            if comparator.is_none() && *key == Value::Ref(None) {
                return Err(thrown("java/lang/NullPointerException"));
            }
            return Ok(None);
        }
        Ok(self.tree_search(tree, comparator, key)?.err())
    }

    /// The `getCeilingEntry`/`getFloorEntry`/`getHigherEntry`/
    /// `getLowerEntry` walks.
    fn tree_nearest(
        &mut self,
        tree: &RedBlackTree,
        comparator: &Option<Value>,
        key: &Value,
        nearest: Nearest,
    ) -> Result<Option<usize>, NativeError> {
        let mut p = tree.root;
        while let Some(at) = p {
            let cmp = self.tree_compare(comparator, key, &tree.node(at).key.clone())?;
            // Go left when the key is below this node (or not above it, for
            // `higher`), right when above (or not below it, for `lower`).
            let go_left = match nearest {
                Nearest::Ceiling => cmp < 0,
                Nearest::Higher => cmp < 0,
                Nearest::Floor => cmp < 0,
                Nearest::Lower => cmp <= 0,
            };
            let go_right = match nearest {
                Nearest::Ceiling => cmp > 0,
                Nearest::Higher => cmp >= 0,
                Nearest::Floor => cmp > 0,
                Nearest::Lower => cmp > 0,
            };
            let upward = matches!(nearest, Nearest::Ceiling | Nearest::Higher);
            if go_left {
                match tree.node(at).left {
                    Some(left) => p = Some(left),
                    // Ceiling/higher: this node. Floor/lower: the first
                    // ancestor this subtree is right of.
                    None if upward => return Ok(Some(at)),
                    None => return Ok(Self::climb(tree, at, true)),
                }
            } else if go_right {
                match tree.node(at).right {
                    Some(right) => p = Some(right),
                    None if upward => return Ok(Self::climb(tree, at, false)),
                    None => return Ok(Some(at)),
                }
            } else {
                return Ok(Some(at));
            }
        }
        Ok(None)
    }

    /// Climb while the child is its parent's left (`from_left`) or right
    /// child; the parent reached, if any.
    fn climb(tree: &RedBlackTree, at: usize, from_left: bool) -> Option<usize> {
        let (mut parent, mut ch) = (tree.node(at).parent, at);
        while let Some(p) = parent {
            let side = if from_left {
                tree.node(p).left
            } else {
                tree.node(p).right
            };
            if side != Some(ch) {
                break;
            }
            ch = p;
            parent = tree.node(p).parent;
        }
        parent
    }

    /// The tree of `value`, built from the host's list (in its order, as
    /// `buildFromSorted` would) when this runtime did not see it built.
    fn take_tree(&mut self, marker: &str, entries: &[Value], map: bool) -> RedBlackTree {
        self.host.take_red_black_tree(marker).unwrap_or_else(|| {
            let step = if map { 2 } else { 1 };
            let keys: Vec<Value> = entries.iter().step_by(step).cloned().collect();
            RedBlackTree::build_from_sorted(&keys)
        })
    }

    /// Reconcile the tree with the host's list (entries another path added
    /// or removed): removed keys are deleted, an added key is placed while
    /// its comparisons cannot show (pure keys, natural order), else the
    /// tree's order is unknown.
    fn resync_tree(
        &mut self,
        tree: &mut RedBlackTree,
        comparator: &Option<Value>,
        entries: &[Value],
        map: bool,
    ) -> Result<(), NativeError> {
        let step = if map { 2 } else { 1 };
        let keys: Vec<&Value> = entries.iter().step_by(step).collect();
        let present: std::collections::BTreeSet<String> =
            keys.iter().map(|key| collection_item_marker(key)).collect();
        for key in tree.keys() {
            let marker = collection_item_marker(&key);
            if !present.contains(&marker)
                && let Some(node) = tree.find_marker(&marker)
            {
                tree.delete_entry(node);
            }
        }
        for key in keys {
            if tree.find_marker(&collection_item_marker(key)).is_some() {
                continue;
            }
            // Only a string's or a box's `compareTo` runs unobserved.
            if comparator.is_some() || self.equality_of(key) != object_methods::Equality::Platform {
                tree.tainted = true;
                continue;
            }
            match self.tree_search(tree, comparator, key)? {
                Ok(None) => tree.root = Some(tree.new_node(key.clone(), None)),
                Ok(Some((parent, left))) => tree.add_entry(key.clone(), parent, left),
                Err(_) => tree.tainted = true,
            }
        }
        Ok(())
    }

    /// Write the host's list in the tree's order, with `values` (by key
    /// marker) for a map.
    fn write_tree(
        &mut self,
        receiver: &Value,
        tree: &RedBlackTree,
        map: bool,
        values: &BTreeMap<String, Value>,
    ) {
        let mut entries = Vec::new();
        for key in tree.keys() {
            if map {
                let value = values
                    .get(&collection_item_marker(&key))
                    .cloned()
                    .unwrap_or(Value::Ref(None));
                entries.extend([key, value]);
            } else {
                entries.push(key);
            }
        }
        self.host.replace_mutable_collection(receiver, entries);
    }

    /// `TreeMap`/`TreeSet` members. `Ok(None)` leaves the call to the rest of
    /// the dispatch.
    pub(crate) fn invoke_tree_collection(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(receiver) = receiver.filter(|_| matches!(opcode, 0xb6 | 0xb7 | 0xb9)) else {
            return Ok(None);
        };
        if r.name == "<init>" {
            return match r.owner.as_str() {
                "java/util/TreeMap" => self.construct_tree(receiver, true, args),
                "java/util/TreeSet" => self.construct_tree(receiver, false, args),
                _ => Ok(None),
            };
        }
        let Some(map) = self.tree_kind(receiver) else {
            return Ok(None);
        };
        let Some(marker) = expect_ref(receiver).cloned() else {
            return Ok(None);
        };
        if navigable_view(&r.name) {
            return self.host.unknown_operand_call(r).map(|_| None);
        }
        if let Some(callback) = TreeCallback::of(map, r) {
            return self.tree_callback_member(r, receiver, &marker, map, callback, args);
        }
        let entries = self.collection_elements(receiver)?;
        let comparator = self.host.collection_comparator(&marker);
        let mut tree = self.take_tree(&marker, &entries, map);
        let result = match self.resync_tree(&mut tree, &comparator, &entries, map) {
            Ok(()) => self.tree_member(r, receiver, map, &mut tree, &comparator, entries, args),
            Err(error) => Err(error),
        };
        self.host.store_red_black_tree(&marker, tree);
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn tree_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        map: bool,
        tree: &mut RedBlackTree,
        comparator: &Option<Value>,
        entries: Vec<Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let step = if map { 2 } else { 1 };
        let mut values: BTreeMap<String, Value> = entries
            .chunks(step)
            .map(|chunk| {
                (
                    collection_item_marker(&chunk[0]),
                    chunk.get(1).cloned().unwrap_or(Value::Int(1)),
                )
            })
            .collect();
        let value_of = |values: &BTreeMap<String, Value>, key: &Value| {
            values
                .get(&collection_item_marker(key))
                .cloned()
                .unwrap_or(Value::Ref(None))
        };
        let key_of = |tree: &RedBlackTree, node: Option<usize>| {
            node.map_or(Value::Ref(None), |node| tree.node(node).key.clone())
        };
        let boolean = |flag: bool| Some(Value::Int(i32::from(flag)));
        let name = r.name.as_str();
        if tree.tainted {
            // Keys this tree could not place: the host answers, counted.
            return self.host.unknown_operand_call(r).map(|_| None);
        }
        let answer = match (map, name, args) {
            (true, "put" | "putIfAbsent", [key, value]) | (false, "add", [key @ value]) => {
                let value = if map { value.clone() } else { Value::Int(1) };
                match self.tree_search(tree, comparator, key)? {
                    Ok(None) => {
                        // `addEntryToEmptyMap`: `compare(key, key)` first.
                        self.tree_compare(comparator, key, key)?;
                        tree.root = Some(tree.new_node(key.clone(), None));
                        values.insert(collection_item_marker(key), value);
                        if map { Value::Ref(None) } else { Value::Int(1) }
                    }
                    Ok(Some((parent, left))) => {
                        tree.add_entry(key.clone(), parent, left);
                        values.insert(collection_item_marker(key), value);
                        if map { Value::Ref(None) } else { Value::Int(1) }
                    }
                    Err(node) => {
                        let stored = tree.node(node).key.clone();
                        let old = value_of(&values, &stored);
                        if map && (name == "put" || old == Value::Ref(None)) {
                            values.insert(collection_item_marker(&stored), value);
                        }
                        if map { old } else { Value::Int(0) }
                    }
                }
            }
            (true, "get" | "containsKey", [key]) | (false, "contains", [key]) => {
                let found = self.tree_get(tree, comparator, key)?;
                match (name, found) {
                    ("get", Some(node)) => value_of(&values, &tree.node(node).key.clone()),
                    ("get", None) => Value::Ref(None),
                    (_, found) => Value::Int(i32::from(found.is_some())),
                }
            }
            (true, "getOrDefault", [key, default]) => {
                // `Map.getOrDefault`: `get`, then `containsKey` for a null.
                let found = self.tree_get(tree, comparator, key)?;
                let value = found.map_or(Value::Ref(None), |node| {
                    value_of(&values, &tree.node(node).key.clone())
                });
                if value != Value::Ref(None) {
                    value
                } else if self.tree_get(tree, comparator, key)?.is_some() {
                    Value::Ref(None)
                } else {
                    default.clone()
                }
            }
            (_, "remove", [key]) if r.descriptor.ends_with(")Ljava/lang/Object;") || !map => {
                match self.tree_get(tree, comparator, key)? {
                    Some(node) => {
                        let stored = tree.node(node).key.clone();
                        let old = values
                            .remove(&collection_item_marker(&stored))
                            .unwrap_or(Value::Ref(None));
                        tree.delete_entry(node);
                        if map { old } else { Value::Int(1) }
                    }
                    None if map => Value::Ref(None),
                    None => Value::Int(0),
                }
            }
            (true, "replace", [key, value]) if r.descriptor.ends_with(")Ljava/lang/Object;") => {
                match self.tree_get(tree, comparator, key)? {
                    Some(node) => {
                        let stored = tree.node(node).key.clone();
                        values
                            .insert(collection_item_marker(&stored), value.clone())
                            .unwrap_or(Value::Ref(None))
                    }
                    None => Value::Ref(None),
                }
            }
            (true, "containsValue", [wanted]) => {
                // `valEquals(value, e.value)` in key order.
                let mut found = false;
                for key in tree.keys() {
                    let stored = value_of(&values, &key);
                    let equal = if *wanted == Value::Ref(None) {
                        stored == Value::Ref(None)
                    } else {
                        self.java_equals(wanted, &stored)?
                    };
                    if equal {
                        found = true;
                        break;
                    }
                }
                Value::Int(i32::from(found))
            }
            (_, "clear", []) => {
                *tree = RedBlackTree {
                    mod_count: tree.mod_count + 1,
                    ..RedBlackTree::default()
                };
                values.clear();
                Value::Void
            }
            (_, "comparator", []) => comparator.clone().unwrap_or(Value::Ref(None)),
            (true, "firstKey" | "lastKey", []) | (false, "first" | "last", []) => {
                let node = if matches!(name, "firstKey" | "first") {
                    tree.first()
                } else {
                    tree.last()
                };
                match node {
                    Some(node) => tree.node(node).key.clone(),
                    None => return Err(thrown("java/util/NoSuchElementException")),
                }
            }
            (true, "firstEntry" | "lastEntry" | "pollFirstEntry" | "pollLastEntry", [])
            | (false, "pollFirst" | "pollLast", []) => {
                // Exact names: `firstEntry` has a lowercase `f`.
                let node = if matches!(name, "firstEntry" | "pollFirstEntry" | "pollFirst") {
                    tree.first()
                } else {
                    tree.last()
                };
                let Some(node) = node else {
                    return Ok(Some(Value::Ref(None)));
                };
                let key = tree.node(node).key.clone();
                let value = value_of(&values, &key);
                if matches!(
                    name,
                    "pollFirstEntry" | "pollLastEntry" | "pollFirst" | "pollLast"
                ) {
                    values.remove(&collection_item_marker(&key));
                    tree.delete_entry(node);
                }
                if map {
                    map_entry_value(&key, &value)
                } else {
                    key
                }
            }
            (
                true,
                "ceilingKey" | "floorKey" | "higherKey" | "lowerKey" | "ceilingEntry"
                | "floorEntry" | "higherEntry" | "lowerEntry",
                [key],
            )
            | (false, "ceiling" | "floor" | "higher" | "lower", [key]) => {
                let nearest = match name {
                    "ceilingKey" | "ceilingEntry" | "ceiling" => Nearest::Ceiling,
                    "floorKey" | "floorEntry" | "floor" => Nearest::Floor,
                    "higherKey" | "higherEntry" | "higher" => Nearest::Higher,
                    _ => Nearest::Lower,
                };
                if comparator.is_none() && *key == Value::Ref(None) {
                    return Err(thrown("java/lang/NullPointerException"));
                }
                let node = self.tree_nearest(tree, comparator, key, nearest)?;
                let found = key_of(tree, node);
                if matches!(
                    name,
                    "ceilingEntry" | "floorEntry" | "higherEntry" | "lowerEntry"
                ) {
                    match node {
                        Some(_) => map_entry_value(&found, &value_of(&values, &found)),
                        None => Value::Ref(None),
                    }
                } else {
                    found
                }
            }
            (true, "putAll", [source]) | (false, "addAll", [source]) => {
                let source = source.clone();
                if self.host_collection_kind(&source).is_none() {
                    return Ok(None);
                }
                let incoming = self.collection_elements(&source)?;
                let sorted_source = self.tree_kind(&source) == Some(map)
                    && self
                        .host
                        .collection_comparator(expect_ref(&source).map_or("", |m| m.as_str()))
                        == *comparator;
                if tree.root.is_none() && !incoming.is_empty() && sorted_source {
                    // `buildFromSorted`/`addAllForTreeSet`: no comparisons.
                    let keys: Vec<Value> = incoming.iter().step_by(step).cloned().collect();
                    *tree = RedBlackTree::build_from_sorted(&keys);
                    for chunk in incoming.chunks(step) {
                        values.insert(
                            collection_item_marker(&chunk[0]),
                            chunk.get(1).cloned().unwrap_or(Value::Int(1)),
                        );
                    }
                    if map { Value::Void } else { Value::Int(1) }
                } else {
                    let mut changed = false;
                    for chunk in incoming.chunks(step) {
                        let key = &chunk[0];
                        let value = chunk.get(1).cloned().unwrap_or(Value::Int(1));
                        match self.tree_search(tree, comparator, key)? {
                            Ok(None) => {
                                self.tree_compare(comparator, key, key)?;
                                tree.root = Some(tree.new_node(key.clone(), None));
                                values.insert(collection_item_marker(key), value);
                                changed = true;
                            }
                            Ok(Some((parent, left))) => {
                                tree.add_entry(key.clone(), parent, left);
                                values.insert(collection_item_marker(key), value);
                                changed = true;
                            }
                            Err(node) if map => {
                                let stored = tree.node(node).key.clone();
                                values.insert(collection_item_marker(&stored), value);
                            }
                            Err(_) => {}
                        }
                    }
                    if map {
                        Value::Void
                    } else {
                        Value::Int(i32::from(changed))
                    }
                }
            }
            (false, "containsAll" | "removeAll" | "retainAll", [other])
                if r.descriptor == "(Ljava/util/Collection;)Z" =>
            {
                match self.tree_set_bulk(r, tree, comparator, &mut values, other)? {
                    Some(answer) => answer,
                    None => return Ok(None),
                }
            }
            (false, "equals", [other]) if r.descriptor == "(Ljava/lang/Object;)Z" => {
                match self.tree_set_equals(r, receiver, tree, comparator, other)? {
                    Some(answer) => answer,
                    None => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        self.write_tree(receiver, tree, map, &values);
        Ok(match answer {
            Value::Int(flag) if r.descriptor.ends_with(")Z") => boolean(flag != 0),
            answer => Some(answer),
        })
    }

    /// The constructors: an optional comparator, or a source map/collection
    /// (a sorted one of the same kind is copied with `buildFromSorted`).
    fn construct_tree(
        &mut self,
        receiver: &Value,
        map: bool,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(marker) = expect_ref(receiver).cloned() else {
            return Ok(None);
        };
        let owner = if map {
            "java/util/TreeMap"
        } else {
            "java/util/TreeSet"
        };
        let empty = MethodRef {
            owner: owner.into(),
            name: "<init>".into(),
            descriptor: "()V".into(),
        };
        let source = match args {
            [] => None,
            [arg] if self.host_collection_kind(arg).is_some() => Some(arg.clone()),
            [Value::Ref(None)] => return Err(thrown("java/lang/NullPointerException")),
            [comparator] => {
                self.host.invoke_instance(&empty, receiver, &[])?;
                self.host
                    .set_collection_comparator(&marker, comparator.clone());
                self.host
                    .store_red_black_tree(&marker, RedBlackTree::default());
                return Ok(Some(Value::Void));
            }
            _ => return Ok(None),
        };
        self.host.invoke_instance(&empty, receiver, &[])?;
        self.host
            .store_red_black_tree(&marker, RedBlackTree::default());
        let Some(source) = source else {
            return Ok(Some(Value::Void));
        };
        // A sorted source lends its comparator (`TreeMap(SortedMap)`,
        // `TreeSet(SortedSet)`); `TreeMap(Map)`/`TreeSet(Collection)` start
        // with natural order and copy it with `putAll`/`addAll`.
        let wanted = if map {
            CollectionKind::Map
        } else {
            CollectionKind::Set
        };
        if self.tree_kind(&source) == Some(map)
            && self.host_collection_kind(&source) == Some(wanted)
            && let Some(comparator) =
                expect_ref(&source).and_then(|source| self.host.collection_comparator(source))
        {
            self.host.set_collection_comparator(&marker, comparator);
        }
        let copy = MethodRef {
            owner: owner.into(),
            name: if map { "putAll" } else { "addAll" }.into(),
            descriptor: if map {
                "(Ljava/util/Map;)V"
            } else {
                "(Ljava/util/Collection;)Z"
            }
            .into(),
        };
        self.invoke_tree_collection(&copy, 0xb6, Some(receiver), std::slice::from_ref(&source))?;
        Ok(Some(Value::Void))
    }
}

/// The members that call back into the jar while the tree is live: the
/// `compute` family, `merge` and `forEach`.
#[derive(Clone, Copy, PartialEq)]
enum TreeCallback {
    ComputeIfAbsent,
    ComputeIfPresent,
    Compute,
    Merge,
    /// `TreeMap.forEach(BiConsumer)`.
    MapForEach,
    /// `TreeSet.forEach(Consumer)`, which is `Iterable.forEach`.
    SetForEach,
    /// `TreeMap.replaceAll(BiFunction)`.
    ReplaceAll,
    /// `TreeSet.removeIf`, which is `Collection.removeIf`.
    SetRemoveIf,
}

impl TreeCallback {
    /// The member `r` names on a tree map (`map`) or set, by exact name and
    /// descriptor.
    fn of(map: bool, r: &MethodRef) -> Option<Self> {
        const FUNCTION: &str =
            "(Ljava/lang/Object;Ljava/util/function/Function;)Ljava/lang/Object;";
        const BI_FUNCTION: &str =
            "(Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;";
        const MERGE: &str = "(Ljava/lang/Object;Ljava/lang/Object;Ljava/util/function/BiFunction;)Ljava/lang/Object;";
        Some(match (map, r.name.as_str(), r.descriptor.as_str()) {
            (true, "computeIfAbsent", FUNCTION) => Self::ComputeIfAbsent,
            (true, "computeIfPresent", BI_FUNCTION) => Self::ComputeIfPresent,
            (true, "compute", BI_FUNCTION) => Self::Compute,
            (true, "merge", MERGE) => Self::Merge,
            (true, "forEach", "(Ljava/util/function/BiConsumer;)V") => Self::MapForEach,
            (false, "forEach", "(Ljava/util/function/Consumer;)V") => Self::SetForEach,
            (true, "replaceAll", "(Ljava/util/function/BiFunction;)V") => Self::ReplaceAll,
            (false, "removeIf", "(Ljava/util/function/Predicate;)Z") => Self::SetRemoveIf,
            _ => return None,
        })
    }
}

/// The value the host keeps for `key` in a map's flat `k, v` list.
fn stored_value(entries: &[Value], key: &Value) -> Value {
    let marker = collection_item_marker(key);
    entries
        .chunks(2)
        .find(|pair| collection_item_marker(&pair[0]) == marker)
        .and_then(|pair| pair.get(1).cloned())
        .unwrap_or(Value::Ref(None))
}

fn concurrent_modification() -> NativeError {
    thrown("java/util/ConcurrentModificationException")
}

impl<H: Host> Runtime<'_, H> {
    /// The tree of `receiver`, reconciled with the host's list, and that
    /// list. On an error the tree is put back first.
    fn load_tree(
        &mut self,
        receiver: &Value,
        marker: &str,
        comparator: &Option<Value>,
        map: bool,
    ) -> Result<(RedBlackTree, Vec<Value>), NativeError> {
        let entries = self.collection_elements(receiver)?;
        let mut tree = self.take_tree(marker, &entries, map);
        if let Err(error) = self.resync_tree(&mut tree, comparator, &entries, map) {
            self.host.store_red_black_tree(marker, tree);
            return Err(error);
        }
        Ok((tree, entries))
    }

    /// Run `function` on `args` with the tree put back (the jar may call
    /// the map), then take it again: `Ok(None)` when the call changed the
    /// map's structure, which the JDK's `modCount` check turns into a
    /// `ConcurrentModificationException`.
    #[allow(clippy::type_complexity)]
    fn tree_callback(
        &mut self,
        receiver: &Value,
        marker: &str,
        comparator: &Option<Value>,
        map: bool,
        tree: RedBlackTree,
        call: impl FnOnce(&mut Self) -> Result<Value, NativeError>,
    ) -> Result<(Value, Option<(RedBlackTree, Vec<Value>)>), NativeError> {
        let expected = tree.mod_count;
        self.host.store_red_black_tree(marker, tree);
        let answer = call(self)?;
        let (tree, entries) = self.load_tree(receiver, marker, comparator, map)?;
        if tree.mod_count != expected || tree.tainted {
            self.host.store_red_black_tree(marker, tree);
            return Ok((answer, None));
        }
        Ok((answer, Some((tree, entries))))
    }

    /// `computeIfAbsent`/`computeIfPresent`/`compute`/`merge` and
    /// `forEach`, as JDK 25's `TreeMap` runs them: the `put` walk (or
    /// `getEntry` for `computeIfPresent`) before the function, the function
    /// checked by `callMappingFunctionWithCheck`/`callRemappingFunction-
    /// WithCheck`, then `addEntry` (`addEntryToEmptyMap`, which compares
    /// the key with itself, for an empty map), `deleteEntry` for a `null`
    /// result, or the value replaced in place. `TreeMap.forEach` throws
    /// right after the action that changed the map; `TreeSet.forEach` is a
    /// loop, which throws at the next step unless the change came at the
    /// last element.
    fn tree_callback_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        marker: &str,
        map: bool,
        callback: TreeCallback,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let null = Value::Ref(None);
        let function = match (callback, args) {
            (TreeCallback::Merge, [_, _, function]) | (_, [_, function]) | (_, [function]) => {
                function.clone()
            }
            _ => return Ok(None),
        };
        if function == null {
            return Err(thrown("java/lang/NullPointerException"));
        }
        if callback == TreeCallback::Merge && args[1] == null {
            return Err(thrown("java/lang/NullPointerException"));
        }
        let comparator = self.host.collection_comparator(marker);
        let (tree, entries) = self.load_tree(receiver, marker, &comparator, map)?;
        if tree.tainted {
            // Keys this tree could not place: the host answers, counted.
            self.host.store_red_black_tree(marker, tree);
            return self.host.unknown_operand_call(r).map(|_| None);
        }
        match callback {
            TreeCallback::ReplaceAll => self
                .tree_replace_all(receiver, marker, &comparator, tree, entries, &function)
                .map(Some),
            TreeCallback::SetRemoveIf => self
                .tree_set_remove_if(receiver, marker, &comparator, tree, &function)
                .map(Some),
            TreeCallback::MapForEach | TreeCallback::SetForEach => self
                .tree_for_each(
                    receiver,
                    marker,
                    map,
                    callback,
                    &comparator,
                    tree,
                    &function,
                )
                .map(Some),
            _ => self
                .tree_compute(
                    receiver,
                    marker,
                    callback,
                    &comparator,
                    tree,
                    entries,
                    args,
                    &function,
                )
                .map(Some),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tree_compute(
        &mut self,
        receiver: &Value,
        marker: &str,
        callback: TreeCallback,
        comparator: &Option<Value>,
        tree: RedBlackTree,
        entries: Vec<Value>,
        args: &[Value],
        function: &Value,
    ) -> Result<Value, NativeError> {
        let null = Value::Ref(None);
        let key = args[0].clone();
        // The walk: `Err(node)` for the key's entry, `Ok(place)` for where a
        // new key goes (`None`: the empty map).
        let searched = if callback == TreeCallback::ComputeIfPresent {
            self.tree_get(&tree, comparator, &key)
                .map(|found| found.map_or(Ok(None), Err))
        } else {
            self.tree_search(&tree, comparator, &key)
        };
        let found = match searched {
            Ok(found) => found,
            Err(error) => {
                self.host.store_red_black_tree(marker, tree);
                return Err(error);
            }
        };
        let old = match found {
            Err(node) => stored_value(&entries, &tree.node(node).key.clone()),
            Ok(_) => null.clone(),
        };
        // The function's arguments, or the answer without calling it.
        let call: Option<Vec<Value>> = match (callback, &found) {
            (TreeCallback::ComputeIfAbsent, Err(_)) if old != null => {
                self.host.store_red_black_tree(marker, tree);
                return Ok(old);
            }
            (TreeCallback::ComputeIfAbsent, _) => Some(vec![key.clone()]),
            (TreeCallback::ComputeIfPresent, Err(_)) if old != null => {
                Some(vec![key.clone(), old.clone()])
            }
            (TreeCallback::ComputeIfPresent, _) => {
                self.host.store_red_black_tree(marker, tree);
                return Ok(null);
            }
            (TreeCallback::Compute, _) => Some(vec![key.clone(), old.clone()]),
            // `merge`: the function only for an entry with a value.
            (_, Err(_)) if old != null => Some(vec![old.clone(), args[1].clone()]),
            _ => None,
        };
        let (value, mut tree, entries) = match call {
            Some(call) => {
                let function = function.clone();
                let (value, live) =
                    self.tree_callback(receiver, marker, comparator, true, tree, |this| {
                        this.apply_callback(&function, &call)
                    })?;
                let Some((tree, entries)) = live else {
                    return Err(concurrent_modification());
                };
                (value, tree, entries)
            }
            None => (args[1].clone(), tree, entries),
        };
        let mut values: BTreeMap<String, Value> = entries
            .chunks(2)
            .map(|pair| (collection_item_marker(&pair[0]), pair[1].clone()))
            .collect();
        match found {
            // `computeIfAbsent` keeps a `null` result on an existing entry.
            Err(node) if value == null && callback != TreeCallback::ComputeIfAbsent => {
                values.remove(&collection_item_marker(&tree.node(node).key.clone()));
                tree.delete_entry(node);
            }
            Err(node) => {
                values.insert(
                    collection_item_marker(&tree.node(node).key.clone()),
                    value.clone(),
                );
            }
            Ok(_) if value == null => {}
            Ok(None) => {
                // `addEntryToEmptyMap`: `compare(key, key)` first.
                if let Err(error) = self.tree_compare(comparator, &key, &key) {
                    self.host.store_red_black_tree(marker, tree);
                    return Err(error);
                }
                tree.root = Some(tree.new_node(key.clone(), None));
                values.insert(collection_item_marker(&key), value.clone());
            }
            Ok(Some((parent, left))) => {
                tree.add_entry(key.clone(), parent, left);
                values.insert(collection_item_marker(&key), value.clone());
            }
        }
        self.write_tree(receiver, &tree, true, &values);
        self.host.store_red_black_tree(marker, tree);
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    fn tree_for_each(
        &mut self,
        receiver: &Value,
        marker: &str,
        map: bool,
        callback: TreeCallback,
        comparator: &Option<Value>,
        mut tree: RedBlackTree,
        action: &Value,
    ) -> Result<Value, NativeError> {
        let mut at = tree.first();
        let mut entries = self.collection_elements(receiver)?;
        while let Some(node) = at {
            let key = tree.node(node).key.clone();
            // The iterator's `next` is taken before the action runs.
            let next = tree.successor(node);
            let (target, call): ((&str, &str, &str), Vec<Value>) = if map {
                let value = stored_value(&entries, &key);
                (
                    (
                        "java/util/function/BiConsumer",
                        "accept",
                        "(Ljava/lang/Object;Ljava/lang/Object;)V",
                    ),
                    vec![key, value],
                )
            } else {
                (
                    (
                        "java/util/function/Consumer",
                        "accept",
                        "(Ljava/lang/Object;)V",
                    ),
                    vec![key],
                )
            };
            let action = action.clone();
            let (_, live) =
                self.tree_callback(receiver, marker, comparator, map, tree, |this| {
                    this.run_functional(target, &action, &call)?;
                    Ok(Value::Void)
                })?;
            let Some((live, live_entries)) = live else {
                // `TreeMap.forEach` checks after every action; the loop of
                // `TreeSet.forEach` only on its next step.
                if callback == TreeCallback::MapForEach || next.is_some() {
                    return Err(concurrent_modification());
                }
                return Ok(Value::Void);
            };
            tree = live;
            entries = live_entries;
            at = next;
        }
        self.host.store_red_black_tree(marker, tree);
        Ok(Value::Void)
    }
}

impl<H: Host> Runtime<'_, H> {
    /// Whether `value`'s iteration order is the JDK's: a list, a tree, or a
    /// hash collection whose table order is known.
    fn iteration_order_known(&mut self, value: &Value) -> bool {
        if self.host_collection_kind(value) == Some(CollectionKind::List)
            || self.tree_kind(value).is_some()
        {
            return true;
        }
        let Some(marker) = expect_ref(value) else {
            return false;
        };
        if self.hash_table_kind(value).is_none() {
            return false;
        }
        match self.host.take_hash_table(marker) {
            Some(table) => {
                let known = !table.order_unknown();
                self.host.store_hash_table(marker, table);
                known
            }
            None => false,
        }
    }

    /// Whether a jar `compareTo`/`equals` or a comparator takes part, which
    /// the host's answers skip.
    fn tree_bulk_engaged(
        &self,
        tree: &RedBlackTree,
        comparator: &Option<Value>,
        others: &[Value],
    ) -> bool {
        comparator.is_some()
            || tree
                .keys()
                .iter()
                .chain(others)
                .any(|value| self.equality_of(value) != object_methods::Equality::Platform)
    }

    /// Walk the tree in order, deleting the keys `doomed` picks, as an
    /// iterator's `remove()` does: a node with two children takes its
    /// successor's key, which is then the iterator's `next`.
    fn tree_remove_where(
        &mut self,
        r: &MethodRef,
        tree: &mut RedBlackTree,
        values: &mut BTreeMap<String, Value>,
        mut doomed: impl FnMut(&mut Self, &Value) -> Result<Option<bool>, NativeError>,
    ) -> Result<bool, NativeError> {
        let mut modified = false;
        let mut at = tree.first();
        while let Some(node) = at {
            let key = tree.node(node).key.clone();
            let mut next = tree.successor(node);
            match doomed(self, &key)? {
                Some(true) => {
                    if tree.node(node).left.is_some() && tree.node(node).right.is_some() {
                        next = Some(node);
                    }
                    values.remove(&collection_item_marker(&key));
                    tree.delete_entry(node);
                    modified = true;
                }
                Some(false) => {}
                None => {
                    // `c.contains` this runtime cannot answer: counted.
                    self.host.unknown_operand_call(r)?;
                    break;
                }
            }
            at = next;
        }
        Ok(modified)
    }

    /// `TreeSet.containsAll` (`AbstractCollection`'s: `contains(e)` for each
    /// of `c`'s elements, in `c`'s order, each a walk of the tree),
    /// `removeAll` (`AbstractSet`'s: `remove(e)` for each of `c`'s elements
    /// when this set is larger, else this set's iterator removing what
    /// `c.contains`) and `retainAll` (the iterator removing what `c` does not
    /// contain). `None` leaves the call to the host.
    fn tree_set_bulk(
        &mut self,
        r: &MethodRef,
        tree: &mut RedBlackTree,
        comparator: &Option<Value>,
        values: &mut BTreeMap<String, Value>,
        other: &Value,
    ) -> Result<Option<Value>, NativeError> {
        if *other == Value::Ref(None) {
            return Err(thrown("java/lang/NullPointerException"));
        }
        if self.host_collection_kind(other).is_none() {
            return Ok(None);
        }
        let others = self.collection_elements(other)?;
        if !self.tree_bulk_engaged(tree, comparator, &others) {
            return Ok(None);
        }
        let size = tree.keys().len();
        let walks_other = r.name == "containsAll" || (r.name == "removeAll" && size > others.len());
        if walks_other && !self.iteration_order_known(other) {
            return self.host.unknown_operand_call(r).map(|_| None);
        }
        let answer = match r.name.as_str() {
            "containsAll" => {
                let mut all = true;
                for element in &others {
                    if self.tree_get(tree, comparator, element)?.is_none() {
                        all = false;
                        break;
                    }
                }
                all
            }
            "removeAll" if walks_other => {
                let mut modified = false;
                for element in &others {
                    if let Some(node) = self.tree_get(tree, comparator, element)? {
                        let key = tree.node(node).key.clone();
                        values.remove(&collection_item_marker(&key));
                        tree.delete_entry(node);
                        modified = true;
                    }
                }
                modified
            }
            name => {
                let retain = name == "retainAll";
                let other = other.clone();
                self.tree_remove_where(r, tree, values, |this, key| {
                    Ok(this
                        .hash_aware_contains(&other, key)?
                        .map(|contained| contained != retain))
                })?
            }
        };
        Ok(Some(Value::Int(i32::from(answer))))
    }

    /// `AbstractSet.equals`: the same object; not a set; another size; else
    /// `containsAll(o)`, where a `ClassCastException` or
    /// `NullPointerException` means `false`.
    fn tree_set_equals(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        tree: &mut RedBlackTree,
        comparator: &Option<Value>,
        other: &Value,
    ) -> Result<Option<Value>, NativeError> {
        if other == receiver {
            return Ok(Some(Value::Int(1)));
        }
        match self.host_collection_kind(other) {
            Some(CollectionKind::Set) => {}
            Some(_) => return Ok(Some(Value::Int(0))),
            None => return Ok(None),
        }
        let others = self.collection_elements(other)?;
        if !self.tree_bulk_engaged(tree, comparator, &others) {
            return Ok(None);
        }
        if others.len() != tree.keys().len() {
            return Ok(Some(Value::Int(0)));
        }
        if !self.iteration_order_known(other) {
            return self.host.unknown_operand_call(r).map(|_| None);
        }
        for element in &others {
            match self.tree_get(tree, comparator, element) {
                Ok(Some(_)) => {}
                Ok(None) => return Ok(Some(Value::Int(0))),
                Err(NativeError::UncaughtException { class, .. })
                    if class == "java/lang/ClassCastException"
                        || class == "java/lang/NullPointerException" =>
                {
                    return Ok(Some(Value::Int(0)));
                }
                Err(error) => return Err(error),
            }
        }
        Ok(Some(Value::Int(1)))
    }

    /// `TreeMap.replaceAll`: `e.value = function.apply(e.key, e.value)` in key
    /// order, and a `ConcurrentModificationException` right after the call
    /// that changed the structure (the value is stored first).
    fn tree_replace_all(
        &mut self,
        receiver: &Value,
        marker: &str,
        comparator: &Option<Value>,
        mut tree: RedBlackTree,
        mut entries: Vec<Value>,
        function: &Value,
    ) -> Result<Value, NativeError> {
        let set_value = |entries: &mut Vec<Value>, key: &Value, value: Value| {
            let wanted = collection_item_marker(key);
            if let Some(index) = entries
                .chunks(2)
                .position(|pair| collection_item_marker(&pair[0]) == wanted)
            {
                entries[index * 2 + 1] = value;
            }
        };
        let mut at = tree.first();
        while let Some(node) = at {
            let key = tree.node(node).key.clone();
            let value = stored_value(&entries, &key);
            let function = function.clone();
            let call = [key.clone(), value];
            let (replaced, live) =
                self.tree_callback(receiver, marker, comparator, true, tree, |this| {
                    this.apply_callback(&function, &call)
                })?;
            let Some((live, live_entries)) = live else {
                let mut now = self.collection_elements(receiver)?;
                set_value(&mut now, &key, replaced);
                self.host.replace_mutable_collection(receiver, now);
                return Err(concurrent_modification());
            };
            tree = live;
            entries = live_entries;
            set_value(&mut entries, &key, replaced);
            self.host
                .replace_mutable_collection(receiver, entries.clone());
            at = tree.successor(node);
        }
        self.host.store_red_black_tree(marker, tree);
        Ok(Value::Void)
    }

    /// `Collection.removeIf` on a `TreeSet`: the iterator tests each key in
    /// order and `remove()`s it at once (so a predicate that throws leaves
    /// the earlier removals done); `next()`/`remove()` throw after a change
    /// the predicate made, and a change at the last key ends the loop.
    fn tree_set_remove_if(
        &mut self,
        receiver: &Value,
        marker: &str,
        comparator: &Option<Value>,
        mut tree: RedBlackTree,
        predicate: &Value,
    ) -> Result<Value, NativeError> {
        let mut removed = false;
        let mut at = tree.first();
        while let Some(node) = at {
            let key = tree.node(node).key.clone();
            let mut next = tree.successor(node);
            let predicate = predicate.clone();
            let test = key.clone();
            let (holds, live) =
                self.tree_callback(receiver, marker, comparator, false, tree, |this| {
                    Ok(Value::Int(i32::from(
                        this.predicate_holds(&predicate, &test)?,
                    )))
                })?;
            let holds = holds == Value::Int(1);
            let Some((live, _)) = live else {
                // `remove()` checks first; else `hasNext` is `next != null`.
                if holds || next.is_some() {
                    return Err(concurrent_modification());
                }
                return Ok(Value::Int(i32::from(removed)));
            };
            tree = live;
            if holds {
                if tree.node(node).left.is_some() && tree.node(node).right.is_some() {
                    next = Some(node);
                }
                tree.delete_entry(node);
                self.write_tree(receiver, &tree, false, &BTreeMap::new());
                // The iterator's own `remove()` bumps `modCount` and takes it.
                self.bump_mod_count(marker, false);
                removed = true;
            }
            at = next;
        }
        self.host.store_red_black_tree(marker, tree);
        Ok(Value::Int(i32::from(removed)))
    }
}
