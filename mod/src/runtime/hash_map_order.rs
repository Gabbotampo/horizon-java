//! The iteration order of a `java.util.HashMap<Identifier, _>`'s `keySet()`.
//!
//! Vanilla's registries keep their names in `MappedRegistry.byLocation`, a
//! plain `HashMap<Identifier, Holder.Reference<T>>` filled in registration
//! order, and `Registry#keySet()` is that map's key set
//! (`Collections.unmodifiableSet(byLocation.keySet())`, 26.3). A jar that
//! iterates it sees the map's bucket order, not registration order and not
//! sorted order, so the façade (`facade_server.rs`) reproduces it from the
//! registration order the game reports.
//!
//! What decides the order, as `java.util.HashMap` (JDK 21-25) implements it:
//!
//! - **The hash.** `Identifier.hashCode()` is
//!   `31 * namespace.hashCode() + path.hashCode()`; `String.hashCode()` is
//!   `s[0]*31^(n-1) + ... + s[n-1]` over UTF-16 units, wrapping at 32 bits.
//!   `HashMap.hash` spreads it as `h ^ (h >>> 16)`, and a key's bin is that
//!   value masked by `capacity - 1`.
//! - **The capacity.** The table starts at 16 on the first put and doubles
//!   when the size passes `0.75 * capacity`. It also doubles when a put makes
//!   one bin longer than 8 nodes while the table is smaller than 64
//!   (`treeifyBin` resizes instead of building a tree).
//! - **Within a bin.** A put appends at the bin's tail and a resize splits
//!   each bin keeping relative order, so every bin lists its keys in
//!   insertion order. Iteration walks bins from 0 upwards.
//!
//! A bin that grows past 8 nodes on a table of 64 or more becomes a red-black
//! tree whose iteration order this does not model; the order is then `None`
//! and the caller refuses the read instead of guessing.

/// `String.hashCode()`: the polynomial hash over UTF-16 code units.
pub(crate) fn java_string_hash(text: &str) -> i32 {
    text.encode_utf16().fold(0i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(i32::from(unit))
    })
}

/// `Identifier.hashCode()` for `namespace:path` (a bare path is `minecraft`'s,
/// as `Identifier.parse` reads it).
pub(crate) fn identifier_hash(id: &str) -> i32 {
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    java_string_hash(namespace)
        .wrapping_mul(31)
        .wrapping_add(java_string_hash(path))
}

/// `HashMap.hash(key)`: the key's hash with its high half folded into the low.
fn spread(hash: i32) -> u32 {
    let hash = hash as u32;
    hash ^ (hash >> 16)
}

/// `HashMap.TREEIFY_THRESHOLD`: a bin holding this many nodes when another
/// arrives turns into a tree (or forces a resize on a small table).
const TREEIFY_THRESHOLD: usize = 8;
/// `HashMap.MIN_TREEIFY_CAPACITY`.
const MIN_TREEIFY_CAPACITY: usize = 64;
/// `HashMap.DEFAULT_INITIAL_CAPACITY`.
const DEFAULT_CAPACITY: usize = 16;

/// The `keySet()` iteration order of a `new HashMap<>()` whose keys were put
/// in `inserted`'s order, as indices into `inserted`; `hash` is each key's
/// `hashCode()`. `None` when a key repeats (a registry name is put once) or a
/// bin would become a tree.
pub(crate) fn key_set_order<T>(inserted: &[T], hash: impl Fn(&T) -> i32) -> Option<Vec<usize>>
where
    T: Eq + std::hash::Hash,
{
    // A repeated key replaces its value and leaves the order alone in Java;
    // a registry never puts a name twice, so a repeat is a bad input.
    let mut seen = std::collections::HashSet::with_capacity(inserted.len());
    if !inserted.iter().all(|key| seen.insert(key)) {
        return None;
    }
    let spread: Vec<u32> = inserted.iter().map(|key| spread(hash(key))).collect();
    let mut capacity = 0usize;
    // Nodes per bin for the current capacity.
    let mut bins: Vec<usize> = Vec::new();
    let rebin = |capacity: usize, filled: usize| {
        let mut bins = vec![0usize; capacity];
        for &key in &spread[..filled] {
            bins[key as usize & (capacity - 1)] += 1;
        }
        bins
    };
    for (index, &key) in spread.iter().enumerate() {
        if capacity == 0 {
            capacity = DEFAULT_CAPACITY;
            bins = vec![0; capacity];
        }
        let bin = key as usize & (capacity - 1);
        // `putVal` appends, then `treeifyBin` when the bin already held
        // `TREEIFY_THRESHOLD` nodes.
        let held = bins[bin];
        bins[bin] += 1;
        if held >= TREEIFY_THRESHOLD {
            if capacity >= MIN_TREEIFY_CAPACITY {
                return None;
            }
            capacity *= 2;
            bins = rebin(capacity, index + 1);
        }
        // `if (++size > threshold) resize()`; the threshold is
        // `0.75 * capacity`, exact for these powers of two.
        if index + 1 > capacity / 4 * 3 {
            capacity *= 2;
            bins = rebin(capacity, index + 1);
        }
    }
    let mut order: Vec<usize> = (0..inserted.len()).collect();
    // Stable: within a bin, insertion order.
    order.sort_by_key(|&index| spread[index] as usize & capacity.saturating_sub(1));
    Some(order)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `keySet()` of a `HashMap<Identifier, _>` filled in `ids` order.
    fn order<'a>(ids: &[&'a str]) -> Option<Vec<&'a str>> {
        key_set_order(ids, |id| identifier_hash(id))
            .map(|indices| indices.into_iter().map(|index| ids[index]).collect())
    }

    /// The expected values in this module were printed by `Identifier.parse`
    /// and a `java.util.HashMap` on a JVM, with the 26.3 server jar.
    #[test]
    fn identifier_hashes_are_javas() {
        assert_eq!(identifier_hash("minecraft:silk_touch"), 692_947_144);
        assert_eq!(identifier_hash("demo:machine"), 920_781_508);
        // A bare path is `minecraft`'s.
        assert_eq!(identifier_hash("silk_touch"), 692_947_144);
    }

    /// Thirty names cross two resizes (16 → 32 → 64), with bins of two or more.
    #[test]
    fn key_set_order_is_hash_map_bucket_order_after_resizes() {
        let ids = [
            "minecraft:stone",
            "minecraft:granite",
            "minecraft:polished_granite",
            "minecraft:diorite",
            "minecraft:polished_diorite",
            "minecraft:andesite",
            "minecraft:polished_andesite",
            "minecraft:grass_block",
            "minecraft:dirt",
            "minecraft:coarse_dirt",
            "minecraft:podzol",
            "minecraft:cobblestone",
            "minecraft:oak_planks",
            "minecraft:spruce_planks",
            "minecraft:birch_planks",
            "minecraft:jungle_planks",
            "minecraft:acacia_planks",
            "minecraft:cherry_planks",
            "minecraft:dark_oak_planks",
            "minecraft:pale_oak_planks",
            "minecraft:mangrove_planks",
            "minecraft:bamboo_planks",
            "minecraft:bamboo_mosaic",
            "minecraft:oak_sapling",
            "minecraft:spruce_sapling",
            "minecraft:birch_sapling",
            "minecraft:jungle_sapling",
            "minecraft:acacia_sapling",
            "demo:machine",
            "zz:a",
        ];
        assert_eq!(
            order(&ids).unwrap(),
            [
                "minecraft:birch_planks",
                "minecraft:stone",
                "minecraft:cherry_planks",
                "minecraft:mangrove_planks",
                "minecraft:grass_block",
                "minecraft:oak_planks",
                "minecraft:dark_oak_planks",
                "minecraft:acacia_planks",
                "minecraft:andesite",
                "minecraft:cobblestone",
                "minecraft:diorite",
                "minecraft:acacia_sapling",
                "minecraft:podzol",
                "zz:a",
                "minecraft:spruce_sapling",
                "minecraft:polished_granite",
                "demo:machine",
                "minecraft:spruce_planks",
                "minecraft:polished_diorite",
                "minecraft:bamboo_planks",
                "minecraft:oak_sapling",
                "minecraft:jungle_planks",
                "minecraft:granite",
                "minecraft:coarse_dirt",
                "minecraft:jungle_sapling",
                "minecraft:pale_oak_planks",
                "minecraft:dirt",
                "minecraft:polished_andesite",
                "minecraft:birch_sapling",
                "minecraft:bamboo_mosaic",
            ]
        );
    }

    /// Nine `demo:` names share one bin of the first 16-slot table: the ninth
    /// makes it longer than 8 on a table under 64, so `treeifyBin` doubles the
    /// table at ten entries, where the load factor alone would not, and the
    /// bin splits by its fifth hash bit.
    #[test]
    fn a_long_bin_on_a_small_table_doubles_it() {
        let ids = [
            "minecraft:stone",
            "demo:t1",
            "demo:t90",
            "demo:t18",
            "demo:t102",
            "demo:t29",
            "demo:t113",
            "demo:t1709",
            "demo:t124",
            "demo:t1808",
        ];
        assert_eq!(
            order(&ids).unwrap(),
            [
                "minecraft:stone",
                "demo:t1",
                "demo:t18",
                "demo:t29",
                "demo:t1709",
                "demo:t1808",
                "demo:t90",
                "demo:t102",
                "demo:t113",
                "demo:t124",
            ]
        );
    }

    /// Paths built from `ao` and `c1` all hash alike (`'a'*31+'o' == 'c'*31+'1'`),
    /// so one bin keeps growing: at 64 slots it would become a tree, whose order
    /// is not modelled, and the order is refused. A repeated name is refused too.
    #[test]
    fn a_tree_bin_or_a_repeated_name_has_no_order() {
        let paths: Vec<String> = (0..16u32)
            .map(|bits| {
                (0..4)
                    .map(|bit| if bits >> bit & 1 == 1 { "c1" } else { "ao" })
                    .collect()
            })
            .collect();
        let ids: Vec<String> = paths.iter().map(|path| format!("demo:{path}")).collect();
        assert!(
            ids.iter()
                .all(|id| identifier_hash(id) == identifier_hash(&ids[0]))
        );
        assert!(key_set_order(&ids, |id| identifier_hash(id)).is_none());
        // Ten colliding names stop short of a tree (the table grows to 64).
        assert!(key_set_order(&ids[..10], |id| identifier_hash(id)).is_some());
        assert!(order(&["minecraft:stone", "minecraft:dirt", "minecraft:stone"]).is_none());
    }
}
