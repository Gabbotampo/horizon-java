//! What a registry marker iterates over.
//!
//! The host models a game registry as one `registry/<id>` marker (the key and
//! the registry are the same value, see `vanilla_registries`), and records the
//! entries a jar registers into it. A real `MappedRegistry.iterator()` yields
//! every value registered in that registry; the only entries this runtime has
//! values for are the ones the jar registered, so that is what the iterator
//! yields, in registration order.
//!
//! This is a deliberate bound, named here and in the PR: the game's own
//! entries (vanilla's ~140 entity types for `minecraft:entity_type`) are not
//! materialised, because the host has no `EntityType` objects to hand out and
//! inventing them would be worse than an empty iteration. A jar that walks a
//! registry to initialise its own per-type table therefore initialises nothing
//! for the game's types, while still seeing every entry it registered itself.
//! Before this, the call answered null, and `Iterator.hasNext` on it failed the
//! whole entrypoint (`entity-view-distance`'s `main`), which is what
//! `docs/in-game-survey.md` records.

use crate::runtime::StdHost;

/// The values registered into one registry marker, in registration order.
pub(crate) fn values(host: &StdHost, registry_marker: &str) -> Vec<crate::runtime::Value> {
    let id = registry_marker.trim_start_matches("registry/");
    host.registrations
        .iter()
        .filter(|entry| entry.registry == id)
        .map(|entry| entry.value.clone())
        .collect()
}

/// Whether `owner` names one of the registry interfaces a registry marker is
/// called through.
pub(crate) fn is_registry_owner(owner: &str) -> bool {
    matches!(
        owner,
        "net/minecraft/core/Registry"
            | "net/minecraft/core/DefaultedRegistry"
            | "net/minecraft/core/WritableRegistry"
            | "net/minecraft/core/MappedRegistry"
            | "net/minecraft/core/DefaultedMappedRegistry"
    )
}
