//! The server-wide facts and per-firing reads the listener façade answers from.
//!
//! The runtime's event façade answers `MinecraftServer#getWorldPath`,
//! `registryAccess()` and their kin from a [`ServerFacts`] snapshot
//! (`runtime/facade_server.rs`), and a player's held items from the firing's
//! [`HeldItems`] (`runtime/facade_world.rs`). Until the fourth in-game survey
//! the bridge filled neither, so every such read refused: collective's three
//! listener failures and stackrefill's one (`docs/in-game-survey.md`). This
//! module reads them from the fork's existing host calls:
//!
//! - **World folder.** `ConfigServerHost::world_dir`, spelled inside the
//!   interpreter's virtual game folder (`/horizon/game`, which
//!   `FabricLoader.getGameDir()` answers): `saves/<name>` on an integrated
//!   server, `<level-name>` on a dedicated one, as vanilla's
//!   `LevelStorageSource` lays them out under the game folder.
//! - **Registries.** `KeysHost`'s five key lists (blocks, items, entity types,
//!   menus, enchantments), named with `key.id(host)`, each with the order
//!   vanilla registers it in, which decides `keySet()`'s `HashMap` order.
//! - **Held items.** The use-item hook's own stack for the clicked hand, the
//!   other hand from `LivingRef::hand_item`, and `PlayerRef::is_creative`.
//! - **Game rules.** `level.game_rules(host)` of an open level: every rule
//!   the world has with its current value (rules are the server's, so any
//!   level reads the same ones). Read again when a level loads and whenever
//!   a rule changes (`game_rule_changed`), so `getGameRules().get(rule)`
//!   answers the value a `/gamerule` just set.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use mod_api::{Hand, Host, ItemStack, PlayerRef, ServerHost};

use crate::runtime::events::{FacadeStack, GameRuleValue, HeldItems, ServerFacts};

/// The interpreter's virtual game folder (`FabricLoader.getGameDir()`).
const GAME_DIR: &str = "/horizon/game";

/// The facts the host gives now. A registry whose key list is empty (its
/// table not complete yet) is left out, so the façade refuses it instead of
/// answering an empty registry.
pub(crate) fn server_facts(host: &dyn ServerHost) -> ServerFacts {
    let world_dir = host
        .world_dir()
        .and_then(|world| virtual_world_dir(&host.instance_dir(), &world));
    let mut facts = ServerFacts {
        world_dir,
        game_rules: game_rules(host),
        ..ServerFacts::default()
    };
    let lists: [(&str, Vec<String>); 5] = [
        (
            "minecraft:block",
            host.block_keys().into_iter().map(|k| k.id(host)).collect(),
        ),
        (
            "minecraft:item",
            host.item_keys().into_iter().map(|k| k.id(host)).collect(),
        ),
        (
            "minecraft:entity_type",
            host.entity_type_keys()
                .into_iter()
                .map(|k| k.id(host))
                .collect(),
        ),
        (
            "minecraft:menu",
            host.menu_keys().into_iter().map(|k| k.id(host)).collect(),
        ),
        (
            "minecraft:enchantment",
            data_pack_registration_order(
                host.enchantment_keys()
                    .into_iter()
                    .map(|k| k.id(host))
                    .collect(),
            ),
        ),
    ];
    for (registry, ids) in lists {
        if ids.is_empty() {
            continue;
        }
        let set: BTreeSet<String> = ids.iter().cloned().collect();
        facts.registries.insert(registry.to_owned(), Arc::new(set));
        facts
            .registration_order
            .insert(registry.to_owned(), Arc::new(ids));
    }
    facts
}

/// Every game rule the world has now, by vanilla's id without the namespace
/// (`keep_inventory`), as the façade keys them. Empty when no level is open
/// (at `server_about_to_start`) or the host cannot read one, so
/// `getGameRules()` refuses instead of answering defaults.
pub(crate) fn game_rules(host: &dyn ServerHost) -> BTreeMap<String, GameRuleValue> {
    host.levels()
        .list_levels()
        .into_iter()
        .find_map(|level| level.game_rules(host).ok())
        .map(rule_facts)
        .unwrap_or_default()
}

/// The host's `(minecraft:id, value)` list as the façade keys it. A rule
/// outside the `minecraft` namespace (a native mod's) is left out: no
/// `GameRules` field names it.
pub(crate) fn rule_facts(
    rules: Vec<(String, mod_api::GameRuleValue)>,
) -> BTreeMap<String, GameRuleValue> {
    rules
        .into_iter()
        .filter_map(|(id, value)| {
            let id = id.strip_prefix("minecraft:")?.to_owned();
            let value = match value {
                mod_api::GameRuleValue::Bool(value) => GameRuleValue::Bool(value),
                mod_api::GameRuleValue::Int(value) => GameRuleValue::Int(value),
                // A value kind this runtime does not know: left out, so its
                // read refuses.
                _ => return None,
            };
            Some((id, value))
        })
        .collect()
}

/// One log line's worth of what the façade can answer: the world folder (or
/// that the host gave none yet) and each listed registry's size.
pub(crate) fn describe(facts: &ServerFacts) -> String {
    let registries: Vec<String> = facts
        .registries
        .iter()
        .map(|(id, entries)| format!("{id} {}", entries.len()))
        .collect();
    format!(
        "world folder {}, {} game rules, registries [{}]",
        facts.world_dir.as_deref().unwrap_or("unknown"),
        facts.game_rules.len(),
        registries.join(", ")
    )
}

/// The world folder as the façade spells it: under the virtual game folder
/// when it is inside the instance folder, else by its own name there.
/// `None` for a folder with no name or with a part that is not UTF-8.
pub(crate) fn virtual_world_dir(instance: &Path, world: &Path) -> Option<String> {
    let relative = world.strip_prefix(instance).ok().filter(|r| {
        r.components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
            && r.components().next().is_some()
    });
    let parts: Vec<&str> = match relative {
        Some(relative) => relative
            .components()
            .map(|part| part.as_os_str().to_str())
            .collect::<Option<_>>()?,
        None => vec![world.file_name()?.to_str()?],
    };
    Some(format!("{GAME_DIR}/{}", parts.join("/")))
}

/// A data-pack registry's ids in the order vanilla registers them:
/// `RegistryDataLoader` lists `<namespace>:<registry>/<path>.json` files and
/// registers them sorted by `Identifier.compareTo`, which compares the path
/// first and the namespace second. The `.json` suffix takes part in that
/// comparison (`a-b.json` sorts before `a.json`).
pub(crate) fn data_pack_registration_order(mut ids: Vec<String>) -> Vec<String> {
    ids.sort_by_cached_key(|id| {
        let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
        (format!("{path}.json"), namespace.to_owned())
    });
    ids
}

/// One mod-API stack as the façade carries it.
fn facade_stack(host: &dyn Host, stack: &ItemStack) -> FacadeStack {
    FacadeStack {
        item: stack.item().id(host),
        count: i32::try_from(stack.count()).unwrap_or(i32::MAX),
    }
}

/// What `player` holds and their game mode, for a use-item firing: the
/// clicked hand is the hook's own stack (what `getItemInHand(hand)` answers
/// at `UseItemCallback` time), the other hand is read from the host. `None`
/// when the host cannot name the player's living entity, so the reads refuse.
pub(crate) fn held_items(
    host: &dyn ServerHost,
    player: PlayerRef,
    clicked: Hand,
    stack: &ItemStack,
) -> Option<HeldItems> {
    let living = host.living(player.entity())?;
    let other = match clicked {
        Hand::Main => Hand::Off,
        _ => Hand::Main,
    };
    // The handle was just resolved, so `None` here is an empty hand.
    let other_stack = living
        .hand_item(host, other)
        .map(|stack| facade_stack(host, &stack));
    let clicked_stack = Some(facade_stack(host, stack));
    let (main_hand, off_hand) = match clicked {
        Hand::Main => (clicked_stack, other_stack),
        _ => (other_stack, clicked_stack),
    };
    Some(HeldItems {
        main_hand,
        off_hand,
        creative: player.is_creative(host),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// An integrated world sits in `saves/` and a dedicated one beside the
    /// server, both under the game folder; a world outside the instance keeps
    /// its own name there.
    #[test]
    fn the_world_folder_is_spelled_under_the_virtual_game_folder() {
        let instance = PathBuf::from("/home/a/.horizon");
        assert_eq!(
            virtual_world_dir(&instance, &instance.join("saves").join("New World")).as_deref(),
            Some("/horizon/game/saves/New World")
        );
        assert_eq!(
            virtual_world_dir(&instance, &instance.join("world")).as_deref(),
            Some("/horizon/game/world")
        );
        assert_eq!(
            virtual_world_dir(&instance, Path::new("/srv/worlds/survival")).as_deref(),
            Some("/horizon/game/survival")
        );
    }

    /// The host names rules `minecraft:<id>`; the façade keys vanilla's by
    /// the bare id, and a native mod's own rule is no `GameRules` field.
    #[test]
    fn host_rules_are_keyed_by_their_vanilla_id() {
        let rules = rule_facts(vec![
            (
                "minecraft:keep_inventory".into(),
                mod_api::GameRuleValue::Bool(true),
            ),
            (
                "minecraft:random_tick_speed".into(),
                mod_api::GameRuleValue::Int(7),
            ),
            ("example:active".into(), mod_api::GameRuleValue::Bool(true)),
        ]);
        assert_eq!(
            rules,
            BTreeMap::from([
                ("keep_inventory".to_owned(), GameRuleValue::Bool(true)),
                ("random_tick_speed".to_owned(), GameRuleValue::Int(7)),
            ])
        );
    }

    /// `RegistryDataLoader` sorts resource files by path, then namespace, with
    /// the `.json` suffix in the path: `a-b` (`'-'` sorts before `'.'`) comes
    /// before `a`, and a namespace only breaks a tie on the path.
    #[test]
    fn data_pack_entries_register_in_resource_file_order() {
        let ids = [
            "minecraft:a",
            "zz:a",
            "minecraft:a-b",
            "aa:b",
            "minecraft:a_b",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(
            data_pack_registration_order(ids),
            [
                "minecraft:a-b",
                "minecraft:a",
                "zz:a",
                "minecraft:a_b",
                "aa:b"
            ]
        );
    }
}
