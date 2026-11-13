//! The bucket table of a `java.util.HashMap` (and the `HashSet`,
//! `LinkedHashMap` and `LinkedHashSet` built on it), as JDK 25 lays it out.
//!
//! The host keeps a map's entries in a plain list. A `HashMap` iterates its
//! table bin by bin, so its order depends on every key's hash, the table's
//! capacity and the history of puts and resizes: `HashSet` of
//! `"pear","b","apple","a"` iterates `apple,a,pear,b` on a JVM. This module
//! keeps the table the JDK would have, so the host's list can be put in that
//! order, and so a lookup knows which entries the JDK compares a key with
//! (only those in the key's bin with an equal hash).
//!
//! What it models, from `HashMap`'s source:
//!
//! - **`hash(key)`** is `h ^ (h >>> 16)`; a key's bin is that masked by
//!   `capacity - 1`.
//! - **Sizing.** A table is allocated by the first insertion: at the
//!   capacity a constructor stored in `threshold` (`tableSizeFor`), or 16.
//!   `resize()` doubles it and its threshold (`capacity * loadFactor`,
//!   computed in `float` while the capacity is below 16), and splits every
//!   bin into its low and high half keeping order.
//! - **`putVal`** (`put`, `putIfAbsent`, `HashSet.add`, `putAll`) appends to
//!   the bin's tail, turns a bin that already held 8 nodes into a tree (or
//!   resizes a table smaller than 64 instead), then resizes when the size
//!   passes the threshold.
//! - **Removal** unlinks a node; the table never shrinks, `clear` keeps it.
//!
//! A tree bin orders and searches its nodes by `compareTo` and identity
//! hashes, which this does not model: once a bin would become a tree the
//! table is marked [`HashTable::treeified`] and its order is unknown. So is
//! the order of a table holding a key whose hash no run can reproduce (an
//! identity hash): such a key is kept in [`HashTable::unplaced`], counted in
//! the size but in no bin.

use std::collections::BTreeSet;

/// `HashMap.DEFAULT_INITIAL_CAPACITY`.
const DEFAULT_CAPACITY: usize = 16;
/// `HashMap.MAXIMUM_CAPACITY`.
const MAXIMUM_CAPACITY: usize = 1 << 30;
/// `HashMap.TREEIFY_THRESHOLD`.
const TREEIFY_THRESHOLD: usize = 8;
/// `HashMap.MIN_TREEIFY_CAPACITY`.
const MIN_TREEIFY_CAPACITY: usize = 64;

/// `HashMap.hash(key)` for a key whose `hashCode()` is `hash`. It is its
/// own inverse: `spread(spread(h)) == h`.
pub(crate) fn spread(hash: i32) -> i32 {
    hash ^ ((hash as u32) >> 16) as i32
}

/// `HashMap.tableSizeFor(cap)`: the power of two at or above `cap`.
pub(crate) fn table_size_for(cap: i64) -> i32 {
    if cap <= 1 {
        return 1;
    }
    let size = (cap as u64).next_power_of_two();
    size.min(MAXIMUM_CAPACITY as u64) as i32
}

/// `(int) Math.ceil(n / (double) 0.75f)`: `HashMap.calculateHashMapCapacity`.
pub(crate) fn capacity_for(mappings: usize) -> i64 {
    (mappings as f64 / f64::from(0.75_f32)).ceil() as i64
}

/// One table's state. Keys are the host's item markers; each node keeps the
/// spread hash it was inserted with, as a `HashMap.Node` does.
#[derive(Clone, Debug, PartialEq)]
pub struct HashTable {
    /// The JDK's `table`, bin by bin; empty until the first insertion.
    bins: Vec<Vec<(String, i32)>>,
    /// The JDK's `threshold` (the initial capacity before allocation).
    threshold: i64,
    load_factor: f32,
    /// Keys whose hash is an identity hash: in the size, in no bin.
    pub(crate) unplaced: BTreeSet<String>,
    /// A bin became a tree, whose order this does not model.
    pub(crate) treeified: bool,
    /// Iterates in insertion order (`LinkedHashMap`/`LinkedHashSet`), which
    /// the host's list already is.
    pub(crate) linked: bool,
    /// A `LinkedHashMap` in access order, whose reordering the host does not
    /// model.
    pub(crate) access_order: bool,
    /// Filled from a source whose order was unknown.
    pub(crate) tainted: bool,
}

impl HashTable {
    /// `new HashMap<>(initialCapacity, loadFactor)`; `None` is `new
    /// HashMap<>()`.
    pub(crate) fn new(initial_capacity: Option<i32>, load_factor: f32) -> Self {
        Self {
            bins: Vec::new(),
            threshold: initial_capacity.map_or(0, |cap| i64::from(table_size_for(cap.into()))),
            load_factor,
            unplaced: BTreeSet::new(),
            treeified: false,
            linked: false,
            access_order: false,
            tainted: false,
        }
    }

    /// Whether the table's iteration order is unknown. One entry (or none)
    /// has only one order, wherever it sits.
    pub(crate) fn order_unknown(&self) -> bool {
        if self.len() <= 1 {
            return false;
        }
        !self.unplaced.is_empty() || self.treeified || self.access_order || self.tainted
    }

    /// Whether a lookup's `equals` calls are unknown: an unplaced key may sit
    /// in any bin, and a tree bin searches by `compareTo`.
    pub(crate) fn lookups_unknown(&self) -> bool {
        !self.unplaced.is_empty() || self.treeified
    }

    pub(crate) fn len(&self) -> usize {
        self.bins.iter().map(Vec::len).sum::<usize>() + self.unplaced.len()
    }

    /// Whether the table has been allocated (`table != null`).
    pub(crate) fn allocated(&self) -> bool {
        !self.bins.is_empty()
    }

    pub(crate) fn contains(&self, key: &str) -> bool {
        self.unplaced.contains(key) || self.bins.iter().flatten().any(|(k, _)| k == key)
    }

    /// The keys a lookup with `hash` compares, in bin order: the bin's nodes
    /// whose stored hash equals `spread(hash)`.
    pub(crate) fn candidates(&self, hash: i32) -> Vec<String> {
        if self.bins.is_empty() {
            return Vec::new();
        }
        let spread = spread(hash);
        let bin = spread as u32 as usize & (self.bins.len() - 1);
        self.bins[bin]
            .iter()
            .filter(|(_, stored)| *stored == spread)
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// The stored (spread) hash of `key`, when it is in a bin.
    pub(crate) fn stored_hash(&self, key: &str) -> Option<i32> {
        self.bins
            .iter()
            .flatten()
            .find(|(k, _)| k == key)
            .map(|(_, hash)| *hash)
    }

    /// The keys in iteration order (bin by bin), unplaced keys last.
    pub(crate) fn order(&self) -> Vec<String> {
        self.bins
            .iter()
            .flatten()
            .map(|(key, _)| key.clone())
            .chain(self.unplaced.iter().cloned())
            .collect()
    }

    /// `HashMap.resize()`.
    fn resize(&mut self) {
        let old_cap = self.bins.len();
        let old_thr = self.threshold;
        let mut new_thr = 0_i64;
        let new_cap;
        if old_cap > 0 {
            if old_cap >= MAXIMUM_CAPACITY {
                self.threshold = i64::from(i32::MAX);
                return;
            }
            new_cap = old_cap << 1;
            if new_cap < MAXIMUM_CAPACITY && old_cap >= DEFAULT_CAPACITY {
                new_thr = old_thr << 1;
            }
        } else if old_thr > 0 {
            new_cap = old_thr as usize;
        } else {
            new_cap = DEFAULT_CAPACITY;
            new_thr = 12;
        }
        if new_thr == 0 {
            let ft = new_cap as f32 * self.load_factor;
            new_thr = if new_cap < MAXIMUM_CAPACITY && ft < MAXIMUM_CAPACITY as f32 {
                ft as i64
            } else {
                i64::from(i32::MAX)
            };
        }
        self.threshold = new_thr;
        let mut bins = vec![Vec::new(); new_cap];
        for (index, bin) in std::mem::take(&mut self.bins).into_iter().enumerate() {
            // Low and high halves, each keeping the bin's order.
            for (key, hash) in bin {
                let high = hash as u32 as usize & old_cap != 0;
                bins[index + if high { old_cap } else { 0 }].push((key, hash));
            }
        }
        self.bins = bins;
    }

    /// `treeifyBin`: a table below 64 bins resizes instead.
    fn treeify_bin(&mut self) {
        if self.bins.len() < MIN_TREEIFY_CAPACITY {
            self.resize();
        } else {
            self.treeified = true;
        }
    }

    /// `putMapEntries`' pre-sizing for `mappings` more entries, before they
    /// are put one by one.
    pub(crate) fn presize(&mut self, mappings: usize) {
        if mappings == 0 {
            return;
        }
        if self.bins.is_empty() {
            let wanted = (mappings as f64 / f64::from(self.load_factor)).ceil();
            let wanted = if wanted < MAXIMUM_CAPACITY as f64 {
                wanted as i64
            } else {
                MAXIMUM_CAPACITY as i64
            };
            if wanted > self.threshold {
                self.threshold = table_size_for(wanted).into();
            }
        } else {
            while mappings as i64 > self.threshold && self.bins.len() < MAXIMUM_CAPACITY {
                self.resize();
            }
        }
    }

    /// `putVal`'s insertion of `key`, whose `hashCode()` is `hash` (`None`:
    /// an identity hash): allocate, append to the bin's tail, treeify a bin
    /// that already held 8 nodes, resize past the threshold.
    pub(crate) fn insert(&mut self, key: String, hash: Option<i32>) {
        let size = self.len() as i64;
        if self.bins.is_empty() {
            self.resize();
        }
        if let Some(hash) = hash {
            let spread = spread(hash);
            let bin = spread as u32 as usize & (self.bins.len() - 1);
            let held = self.bins[bin].len();
            self.bins[bin].push((key, spread));
            if held >= TREEIFY_THRESHOLD {
                self.treeify_bin();
            }
        } else {
            self.unplaced.insert(key);
        }
        if size + 1 > self.threshold {
            self.resize();
        }
    }

    /// Unlink `key`.
    pub(crate) fn remove(&mut self, key: &str) {
        if self.unplaced.remove(key) {
            return;
        }
        for bin in &mut self.bins {
            if let Some(index) = bin.iter().position(|(k, _)| k == key) {
                bin.remove(index);
                return;
            }
        }
    }

    /// `clear()`: every node goes, the table stays.
    pub(crate) fn clear(&mut self) {
        for bin in &mut self.bins {
            bin.clear();
        }
        self.unplaced.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::hash_map_order::java_string_hash;

    fn table_of(keys: &[&str]) -> HashTable {
        let mut table = HashTable::new(None, 0.75);
        for key in keys {
            table.insert((*key).to_owned(), Some(java_string_hash(key)));
        }
        table
    }

    /// OpenJDK 25.0.5: `HashSet` of `"pear","b","apple","a"` iterates
    /// `apple, a, pear, b`.
    #[test]
    fn a_hash_set_of_strings_iterates_in_bucket_order() {
        let table = table_of(&["pear", "b", "apple", "a"]);
        assert_eq!(table.order(), ["apple", "a", "pear", "b"]);
    }

    /// `tableSizeFor` and the threshold `new HashMap<>(n)` starts with.
    #[test]
    fn initial_capacities_round_up_to_powers_of_two() {
        assert_eq!(table_size_for(0), 1);
        assert_eq!(table_size_for(1), 1);
        assert_eq!(table_size_for(13), 16);
        assert_eq!(table_size_for(16), 16);
        assert_eq!(table_size_for(17), 32);
        // `new HashMap<>(0)`: the first put allocates one bin with threshold
        // `(int) (1 * 0.75f) = 0`, so it resizes to two at once.
        let mut table = HashTable::new(Some(0), 0.75);
        table.insert("a".into(), Some(1));
        assert_eq!(table.bins.len(), 2);
        assert_eq!(table.threshold, 1);
    }
}
