//! Milestone 3: a jar's recorded registrations become content before the game freezes its
//! registries.
//!
//! The game asks native mods for blocks and items at the **Registration** phase
//! (`mod_api::Hooks::blocks`/`items`, NeoForge's `RegisterEvent`), before any resource or world
//! loads and while the content root can still grow. M1 ran jars at `server_about_to_start`, which
//! is too late for that, so the jars run here instead: once per process, in this module, and the
//! remaining hooks fire the listeners they registered.
//!
//! Reading the jars needs the game's `config/` folder, and the scoped config service
//! (`Host::config_files`) first exists in the Config phase. This module reads
//! `config/horizon_java/java-mods` with `std::fs` instead: the game folder is the process's working
//! directory (`--game-dir` is a `set_current_dir`), and native mods are trusted code. The report
//! still goes through the config service, where it exists.
//!
//! Transcription is deliberately narrow. `runtime::content` maps a typed
//! `RegistryDeclaration::{Block, Item}` into the content vocabulary (the template and the property
//! keys the fork reads); this module only applies the event fields, skips ids the game's
//! namespace fence does not let the mod own, and leaves the rest in the jar's row as
//! `docs/m3-content-design.md` maps it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use mod_api::{BlocksEvent, ItemsEvent, PackageKind, RegistrationContext};

use crate::runtime;
use crate::{JARS, LOG, State, load_jar};

/// This mod's manifest id (`mod/horizon-mod.toml`): the package whose namespaces the game's
/// declaration fence checks.
pub(crate) const MOD_ID: &str = "horizon_java";

impl State {
    /// Runs every jar once, the first time content is asked for, and keeps its runtime for the
    /// server lifecycle hooks. The game asks for blocks before items, so this guard also makes
    /// the two hooks one pass.
    pub(crate) fn ensure_loaded(&mut self, context: &RegistrationContext) {
        let owned = owned_namespaces(context);
        self.ensure_loaded_with(read_jar_files, &owned);
    }

    /// [`State::ensure_loaded`] with the folder read injected, so a test can count how often the
    /// content phase reaches for the jars: blocks and items both ask, but a jar runs once.
    fn ensure_loaded_with(
        &mut self,
        read: impl FnOnce() -> Vec<(String, Vec<u8>)>,
        owned: &[String],
    ) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        for (file, bytes) in read() {
            match load_jar(file, &bytes) {
                Ok(loaded) => {
                    log::info!("{LOG}: {}", loaded.row);
                    self.jars.push(loaded);
                }
                Err(row) => {
                    log::info!("{LOG}: {row}");
                    self.refused.push(row);
                }
            }
        }
        for jar in &mut self.jars {
            let declarations = runtime::content::declarations(jar.host.registrations());
            jar.row.content_skipped = unowned_counts(
                owned,
                declarations
                    .iter()
                    .map(|declaration| (declaration.id(), declaration.is_block())),
            );
        }
        for jar in &self.jars {
            for (namespace, (blocks, items)) in &jar.row.content_skipped {
                log::info!(
                    "{LOG}: {}: content: {blocks} block(s), {items} item(s) in namespace \
                     {namespace} not declared: not owned by {MOD_ID}; run tools/bake.py",
                    jar.row.file
                );
            }
        }
    }
}

/// The namespaces this mod may declare content in, exactly as the game's fence checks them
/// (`horizon-mod-api/src/parts/owned_namespaces.rs`): its own id, then the installed package's
/// manifest `namespaces`. A jar's namespace is learned at install time, so it is not here unless
/// the manifest was built with it; its content belongs in a baked content package instead.
pub(crate) fn owned_namespaces(context: &RegistrationContext) -> Vec<String> {
    match context.mods().get(MOD_ID) {
        Some(package) => package.namespaces().map(str::to_owned).collect(),
        None => vec![MOD_ID.to_owned()],
    }
}

/// Whether `id` (`namespace:path`) is in one of `owned`'s namespaces.
fn owns(owned: &[String], id: &str) -> bool {
    id.split_once(':')
        .is_some_and(|(namespace, _)| owned.iter().any(|owned| owned == namespace))
}

/// The typed declarations `owned` does not cover, per namespace: (blocks, items). The game's
/// dispatch would refuse every one of them, so none may be pushed; the counts become the jar's
/// `content.skipped` row and one log line.
fn unowned_counts<'a>(
    owned: &[String],
    declarations: impl Iterator<Item = (&'a str, bool)>,
) -> BTreeMap<String, (usize, usize)> {
    let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (id, block) in declarations {
        if !usable_id(id) || owns(owned, id) {
            continue;
        }
        let (namespace, _) = id.split_once(':').expect("usable id has a namespace");
        let entry = counts.entry(namespace.to_owned()).or_default();
        if block {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    counts
}

/// Pushes one `content::Block` per typed block declaration. A block keeps the runtime's inferred
/// template (`minecraft:stone`) and the properties the content vocabulary has a key for; its item
/// comes with it (`item: true`).
pub(crate) fn declare_blocks(
    state: &mut State,
    context: &RegistrationContext,
    event: &mut BlocksEvent<'_>,
) {
    state.ensure_loaded(context);
    let owned = owned_namespaces(context);
    let mut declared = BTreeSet::new();
    let mut count = 0;
    for jar in &mut state.jars {
        for declaration in runtime::content::declarations(jar.host.registrations()) {
            let runtime::content::ContentDeclaration::Block(declaration) = declaration else {
                continue;
            };
            let id = declaration.id.clone();
            if !usable_id(&id) {
                log::warn!(
                    "{LOG}: {}: block {} is not an id this mod can declare; left out",
                    jar.row.id,
                    id
                );
                continue;
            }
            if !owns(&owned, &id) {
                // The game's declaration fence would refuse it, and a refusal is fatal at start.
                // It is in the row's `content.skipped`; a baked content package declares it.
                continue;
            }
            if !declared.insert(id.clone()) {
                log::warn!(
                    "{LOG}: {}: block {} is declared more than once; only the first is used",
                    jar.row.id,
                    id
                );
                continue;
            }
            // Push the default before naming a field: the element type is the event's, so this
            // module never has to spell `horizon_mod_package::content::Block`, a crate a native
            // mod may not depend on (docs/m3-content-design.md, seam 1).
            event.blocks.push(Default::default());
            let block = event.blocks.last_mut().expect("just pushed");
            block.id = id;
            block.template = declaration.template.clone();
            block.item = declaration.item;
            block.properties.hardness = declaration.hardness;
            block.properties.explosion_resistance = declaration.explosion_resistance;
            block.properties.friction = declaration.friction;
            block.properties.requires_tool = declaration.requires_tool;
            // A content package of the same namespace supplies the block item's own model; with
            // none, the item keeps the template's so the block has the template's look.
            if !owns_assets(context, &block.id) {
                block.components.push((
                    "minecraft:item_model".to_owned(),
                    Some(mod_api::Tag::String(declaration.template.as_str().into())),
                ));
            }
            jar.row.content_blocks += 1;
            count += 1;
        }
    }
    log::info!(
        "{LOG}: content: {count} block declaration(s) from {} jar(s)",
        state.jars.len()
    );
}

/// Pushes one `content::Item` per typed item declaration, except the ones a declared block already
/// gives its own id. The template and the keys `item_declaration_properties` exports carry over.
pub(crate) fn declare_items(
    state: &mut State,
    context: &RegistrationContext,
    event: &mut ItemsEvent<'_>,
) {
    state.ensure_loaded(context);
    let owned = owned_namespaces(context);
    let block_items = block_item_ids(state);
    let mut declared = BTreeSet::new();
    let mut count = 0;
    for jar in &mut state.jars {
        for declaration in runtime::content::declarations(jar.host.registrations()) {
            let runtime::content::ContentDeclaration::Item(declaration) = declaration else {
                continue;
            };
            let id = declaration.id.clone();
            if !usable_id(&id) {
                log::warn!(
                    "{LOG}: {}: item {} is not an id this mod can declare; left out",
                    jar.row.id,
                    id
                );
                continue;
            }
            if !owns(&owned, &id) {
                // See the block counterpart: refused by the game's fence, counted in the row,
                // and declared by a baked content package instead.
                continue;
            }
            if block_items.contains(&id) {
                // A block's item is declared with the block (`item: true`); declaring the same id
                // again refuses the whole namespace at the content root.
                continue;
            }
            if !declared.insert(id.clone()) {
                log::warn!(
                    "{LOG}: {}: item {} is declared more than once; only the first is used",
                    jar.row.id,
                    id
                );
                continue;
            }
            event.items.push(Default::default());
            let item = event.items.last_mut().expect("just pushed");
            item.id = id;
            item.template = declaration.template.clone();
            item.max_stack_size = declaration.max_stack_size;
            item.rarity = declaration.rarity.map(|rarity| match rarity {
                runtime::content::Rarity::Common => mod_api::Rarity::Common,
                runtime::content::Rarity::Uncommon => mod_api::Rarity::Uncommon,
                runtime::content::Rarity::Rare => mod_api::Rarity::Rare,
                runtime::content::Rarity::Epic => mod_api::Rarity::Epic,
            });
            item.crafting_remainder = declaration.crafting_remainder.clone();
            // The content plan renames the template's `minecraft:item_model` to the new id, and
            // nothing then defines that id. A content package of the same namespace (the bake in
            // docs/m3-content-design.md) supplies the real definition; without one, keep the
            // template's model so the item draws with the vanilla art.
            if !owns_assets(context, &item.id) {
                item.components.push((
                    "minecraft:item_model".to_owned(),
                    Some(mod_api::Tag::String(declaration.template.as_str().into())),
                ));
            }
            jar.row.content_items += 1;
            count += 1;
        }
    }
    log::info!(
        "{LOG}: content: {count} item declaration(s) from {} jar(s)",
        state.jars.len()
    );
}

/// Whether an installed content package owns the id's namespace, and so supplies that content's
/// assets (models, item definitions, textures). A runtime declaration then leaves the template's
/// `minecraft:item_model` alone; the package's files decide what is drawn.
fn owns_assets(context: &RegistrationContext, id: &str) -> bool {
    let Some((namespace, _)) = id.split_once(':') else {
        return false;
    };
    context.mods().iter().any(|package| {
        package.kind() == PackageKind::Content
            && package.namespaces().any(|owned| owned == namespace)
    })
}

/// The ids of blocks that bring their own item, so the item registry's own row for that id is not
/// a second declaration.
fn block_item_ids(state: &State) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for jar in &state.jars {
        for declaration in runtime::content::declarations(jar.host.registrations()) {
            if let runtime::content::ContentDeclaration::Block(block) = declaration
                && block.item
                && usable_id(&block.id)
            {
                ids.insert(block.id);
            }
        }
    }
    ids
}

/// A content id this bridge may declare: `namespace:path` in the characters a content namespace
/// and path accept (`Content::namespaced_id`). `minecraft:` is vanilla's and is never redefined by
/// a mod; registering one is recorded in the jar's row, but not declared as content.
fn usable_id(id: &str) -> bool {
    let Some((namespace, path)) = id.split_once(':') else {
        return false;
    };
    let ok = |byte: u8, extra: &[u8]| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || extra.contains(&byte)
    };
    !namespace.is_empty()
        && namespace != "minecraft"
        && !path.is_empty()
        && namespace.bytes().all(|b| ok(b, b"_.-"))
        && path.bytes().all(|b| ok(b, b"_./-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The game asks for blocks and items in two hook calls. Each jar's entrypoints must run once:
    /// a second run would register every listener and command twice and load the jar's classes
    /// into a second host.
    #[test]
    fn the_content_phase_reaches_for_the_jars_once() {
        let mut state = State::default();
        let mut reads = 0;
        state.ensure_loaded_with(
            || {
                reads += 1;
                Vec::new()
            },
            &[],
        );
        state.ensure_loaded_with(
            || {
                reads += 1;
                Vec::new()
            },
            &[],
        );
        assert_eq!(reads, 1);
        assert!(state.loaded);
    }

    /// A server that opens a second world reuses the jars: its start resets the per-server
    /// counters, not the content pass.
    #[test]
    fn a_second_server_start_does_not_reload_the_jars() {
        let mut state = State::default();
        let mut reads = 0;
        state.ensure_loaded_with(
            || {
                reads += 1;
                Vec::new()
            },
            &[],
        );
        state.reset_server();
        state.ensure_loaded_with(
            || {
                reads += 1;
                Vec::new()
            },
            &[],
        );
        assert_eq!(reads, 1);
        assert!(state.loaded);
    }

    /// The game's fence (`horizon-mod-api/src/parts/owned_namespaces.rs`) lets a native mod
    /// declare only in its id and its manifest `namespaces`; every other id is a fatal refusal at
    /// start, so the bridge must filter them before they reach the dispatcher.
    #[test]
    fn only_manifest_owned_namespaces_are_declared() {
        let owned = ["horizon_java".to_owned(), "m3_fixture".to_owned()];
        assert!(owns(&owned, "horizon_java:thing"));
        assert!(owns(&owned, "m3_fixture:ruby"));
        assert!(!owns(&owned, "other_mod:ruby"));
        assert!(!owns(&owned, "minecraft:stone"));
        assert!(!owns(&owned, "bare"));
    }

    /// A jar's own namespace is not in the manifest, so its declarations are skipped and counted
    /// per namespace; a bare or malformed id is not a declaration this bridge counts either.
    #[test]
    fn unowned_declarations_are_counted_per_namespace() {
        let owned = ["horizon_java".to_owned()];
        let counts = unowned_counts(
            &owned,
            [
                ("m3_fixture:ruby", false),
                ("m3_fixture:ruby_block", true),
                ("m3_fixture:ruby_ore", true),
                ("horizon_java:own", true),
                ("minecraft:stone", true),
                ("bare", false),
            ]
            .into_iter(),
        );
        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get("m3_fixture"), Some(&(2, 1)));
    }
}

/// The jars under the game's `config/horizon_java/java-mods/`, sorted by file name so one run's
/// report reads the same as the next. An absent folder is no jars; an unreadable one is a warning,
/// not a failed start.
fn read_jar_files() -> Vec<(String, Vec<u8>)> {
    let Ok(game_dir) = std::env::current_dir() else {
        log::warn!("{LOG}: the game folder is unknown; no jars are read");
        return Vec::new();
    };
    let folder = game_dir.join("config").join(JARS);
    let entries = match std::fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            log::warn!("{LOG}: config/{JARS} could not be listed: {error}");
            return Vec::new();
        }
    };
    let mut files: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.ends_with(".jar").then(|| (name, entry.path()))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
        .into_iter()
        .filter_map(|(name, path)| match std::fs::read(&path) {
            Ok(bytes) => Some((name, bytes)),
            Err(error) => {
                log::warn!("{LOG}: config/{JARS}/{name} could not be read: {error}");
                None
            }
        })
        .collect()
}
