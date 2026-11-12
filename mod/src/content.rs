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
use std::sync::Arc;

use mod_api::{BlocksEvent, ItemsEvent, PackageKind, RegistrationContext};

use crate::runtime;
use crate::{JARS, JarRow, LOG, LoadedJar, State, jar_id, load_jar};

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
        // Parse every jar before running any: a declared dependency is a sibling
        // jar's metadata, so the graph is only knowable once the whole folder
        // has been read.
        let mut parsed: Vec<(String, Arc<runtime::NativeJar>)> = Vec::new();
        for (file, bytes) in read() {
            match runtime::NativeJar::from_bytes(&bytes) {
                Ok(jar) => parsed.push((file, Arc::new(jar))),
                Err(error) => {
                    let id = file.strip_suffix(".jar").unwrap_or(&file).to_owned();
                    let row = JarRow::refused(&file, id, format!("not a readable jar: {error}"));
                    log::info!("{LOG}: {row}");
                    self.refused.push(row);
                }
            }
        }
        let refs: Vec<(&str, &runtime::NativeJar)> = parsed
            .iter()
            .map(|(file, jar)| (file.as_str(), jar.as_ref()))
            .collect();
        let plan = runtime::dependencies::plan(&refs);
        for &index in &plan.order {
            let (file, jar) = parsed[index].clone();
            if let Some(missing) = plan.missing.get(&index) {
                let row = JarRow::refused(
                    &file,
                    jar_id(&jar, &file),
                    format!("requires {} (not in {JARS})", missing.join(", ")),
                );
                log::info!("{LOG}: {row}");
                self.refused.push(row);
                continue;
            }
            let links: Vec<_> = plan
                .links_for(index)
                .iter()
                .map(|&dependency| {
                    let (file, jar) = &parsed[dependency];
                    (file.clone(), Arc::clone(jar))
                })
                .collect();
            match load_jar(file, jar, &links) {
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
        // NeoForge's loading stages after the constructors (`runtime::neoforge_lifecycle`): each
        // stage posts its event to every jar's containers, then runs every jar's deferred work, as
        // FML runs a step's `DeferredWorkQueue` once every mod's event was posted.
        for stage in runtime::neoforge_lifecycle::LoadingStage::ALL {
            for jar in &mut self.jars {
                jar.run_loading_stage(stage);
            }
            for jar in &mut self.jars {
                jar.run_deferred_work(stage);
            }
            // NeoForge's `RegisterEvent` for `neoforge:attachment_types`, in load order
            // (`runtime::neoforge_attachments`): the registry step that follows construction.
            if stage == runtime::neoforge_lifecycle::LoadingStage::Construct {
                for jar in &mut self.jars {
                    let budget = runtime::ExecutionBudget::within(crate::JAR_BUDGET);
                    for problem in jar.jar.register_attachment_types(&mut jar.host, &budget) {
                        log::warn!(
                            "{LOG}: {}: attachment type not registered: {problem}",
                            jar.row.id
                        );
                    }
                }
            }
        }
        for jar in &mut self.jars {
            jar.finish_loading();
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
/// template (`minecraft:stone`) and the properties the content vocabulary has a key for.
///
/// It does **not** bring an item. Vanilla registers block items one by one in `Items`
/// (`Items.STONE`, `Items.DIRT`...), not by walking the block registry, and a Fabric/NeoForge mod
/// registers its own `BlockItem` or none at all. The runtime's inferred `item` flag says only
/// that a block was built from `BlockBehaviour.Properties`, which is no evidence of an item. An
/// item this bridge declares is one the jar registered through the item registry
/// (`declare_items`); a block's `item` stays false and `Block.block_item`/`pick_item` stay unset
/// unless the vocabulary carries the mod's own registration later.
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
            event.blocks.push(block_entry(&declaration, &id));
            jar.row.content_blocks += 1;
            count += 1;
        }
    }
    log::info!(
        "{LOG}: content: {count} block declaration(s) from {} jar(s)",
        state.jars.len()
    );
}

/// Pushes one `content::Item` per typed item declaration. The template and the keys
/// `item_declaration_properties` exports carry over.
///
/// A block's own `BlockItem` registration is exactly one of these rows, so it is declared with the
/// properties the jar gave it. Nothing is skipped because a block has the same id: the block no
/// longer invents an item (`declare_blocks`), so the item registry is the only source of items.
pub(crate) fn declare_items(
    state: &mut State,
    context: &RegistrationContext,
    event: &mut ItemsEvent<'_>,
) {
    state.ensure_loaded(context);
    let owned = owned_namespaces(context);
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
            if !declared.insert(id.clone()) {
                log::warn!(
                    "{LOG}: {}: item {} is declared more than once; only the first is used",
                    jar.row.id,
                    id
                );
                continue;
            }
            event
                .items
                .push(item_entry(&declaration, &id, !owns_assets(context, &id)));
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

/// One block declaration as the content vocabulary's block. `item` is false: an item exists only
/// when the jar registered one, and `components` is the item's patch (the vocabulary says so), so
/// a block without an item carries none.
fn block_entry(declaration: &runtime::content::Block, id: &str) -> mod_api::Block {
    let mut block = mod_api::Block::default();
    block.id = id.to_owned();
    block.template = declaration.template.clone();
    block.item = false;
    block.properties.hardness = declaration.hardness;
    block.properties.explosion_resistance = declaration.explosion_resistance;
    block.properties.friction = declaration.friction;
    block.properties.requires_tool = declaration.requires_tool;
    block
}

/// One item declaration as the content vocabulary's item, with the template's model when no
/// content package of the item's namespace supplies its own.
fn item_entry(
    declaration: &runtime::content::Item,
    id: &str,
    keep_template_model: bool,
) -> mod_api::Item {
    let mut item = mod_api::Item::default();
    item.id = id.to_owned();
    item.template = declaration.template.clone();
    item.max_stack_size = declaration.max_stack_size;
    item.rarity = declaration.rarity.map(|rarity| match rarity {
        runtime::content::Rarity::Common => mod_api::Rarity::Common,
        runtime::content::Rarity::Uncommon => mod_api::Rarity::Uncommon,
        runtime::content::Rarity::Rare => mod_api::Rarity::Rare,
        runtime::content::Rarity::Epic => mod_api::Rarity::Epic,
    });
    item.crafting_remainder = declaration.crafting_remainder.clone();
    // The content plan renames the template's `minecraft:item_model` to the new id, and nothing
    // then defines that id. A content package of the same namespace (the bake in
    // docs/m3-content-design.md) supplies the real definition; without one, keep the template's
    // model so the item draws with the vanilla art.
    if keep_template_model {
        item.components.push((
            "minecraft:item_model".to_owned(),
            Some(mod_api::Tag::String(declaration.template.as_str().into())),
        ));
    }
    item
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

impl LoadedJar {
    /// One loading stage's posts to this jar's containers. A listener that throws is a failure of
    /// the jar's load, as it fails NeoForge's (departure L1: the later stages still run).
    fn run_loading_stage(&mut self, stage: runtime::neoforge_lifecycle::LoadingStage) {
        let budget = runtime::ExecutionBudget::within(crate::JAR_BUDGET);
        let (jar, host) = (&self.jar, &mut self.host);
        match crate::caught(&format!("{} {}", self.row.id, stage.name()), || {
            jar.run_loading_stage(stage, host, &budget)
        }) {
            Ok(problems) => self.failures.extend(problems),
            Err(panic) => self
                .failures
                .push(format!("{}: panicked: {panic}", stage.name())),
        }
    }

    /// The deferred work this jar's containers queued during `stage`.
    fn run_deferred_work(&mut self, stage: runtime::neoforge_lifecycle::LoadingStage) {
        let budget = runtime::ExecutionBudget::within(crate::JAR_BUDGET);
        let (jar, host) = (&self.jar, &mut self.host);
        match crate::caught(
            &format!("{} {}: deferred work", self.row.id, stage.name()),
            || jar.run_deferred_work(host, &budget),
        ) {
            Ok(problems) => self.failures.extend(problems),
            Err(panic) => self
                .failures
                .push(format!("{}: deferred work panicked: {panic}", stage.name())),
        }
    }

    /// After the loading stages: the listeners the jar now holds (a setup listener may register
    /// more), and the row's verdict over everything its load ran. The log line is written again
    /// when the stages changed it.
    fn finish_loading(&mut self) {
        let registered = self.host.game_listeners(&self.jar, true);
        self.listeners = registered.listeners;
        self.orderings = registered.phase_orderings;
        let unknown = self.host.unknown_host_call_count();
        let verdict = runtime::verdict::verdict(self.stages_ran, self.failures.len(), unknown);
        let reason = (!self.failures.is_empty())
            .then(|| format!("entrypoint(s) failed: {}", self.failures.join("; ")));
        let changed = verdict != self.row.verdict
            || unknown != self.row.unknown_host_calls
            || reason != self.row.reason;
        self.row.verdict = verdict;
        self.row.unknown_host_calls = unknown;
        self.row.reason = reason;
        if changed {
            log::info!("{LOG}: after the loading stages: {}", self.row);
        }
    }
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

    /// A NeoForge jar's loading stages run in the content pass, after every jar's constructors:
    /// its lifecycle listeners and deferred work run (the jar's own two failures, a listener and a
    /// deferred task that throw, make it `ran-partial`), and a game-bus listener its setup added is
    /// one the game holds (`runtime::neoforge_lifecycle`).
    #[test]
    fn the_content_pass_runs_neoforges_loading_stages() {
        let bytes = runtime::neoforge_lifecycle::tests::fixture_jar_bytes().to_vec();
        let mut state = State::default();
        state.ensure_loaded_with(|| vec![("life.jar".to_owned(), bytes)], &[]);
        let jar = &mut state.jars[0];
        let printed = jar.host.take_stdout();
        for line in [
            "common",
            "common-work-supply",
            "load-complete-work",
            "payloads",
        ] {
            assert!(printed.lines().any(|out| out == line), "{line}: {printed}");
        }
        assert_eq!(jar.row.verdict, "ran-partial");
        assert_eq!(jar.failures.len(), 2, "{:?}", jar.failures);
        assert_eq!(
            jar.listeners
                .iter()
                .map(|listener| listener.event)
                .collect::<Vec<_>>(),
            [runtime::events::GameEvent::RegisterCommands]
        );
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

    /// A block declaration never invents an item: the runtime's inferred `item` flag only says the
    /// block was built from `BlockBehaviour.Properties`, while vanilla and mods register block
    /// items one by one in the item registry.
    #[test]
    fn a_block_declaration_brings_no_item_of_its_own() {
        let declaration = runtime::content::Block {
            id: "horizon_java:proof_block".to_owned(),
            template: "minecraft:stone".to_owned(),
            // What the runtime infers for any block built from a property chain.
            item: true,
            hardness: Some(2.0),
            explosion_resistance: Some(6.0),
            friction: None,
            requires_tool: Some(true),
        };
        let block = block_entry(&declaration, &declaration.id);
        assert!(!block.item);
        assert!(
            block.components.is_empty(),
            "an item's component patch has no item to belong to"
        );
        assert_eq!(block.template, "minecraft:stone");
        assert_eq!(block.properties.hardness, Some(2.0));
        assert_eq!(block.properties.explosion_resistance, Some(6.0));
        assert_eq!(block.properties.requires_tool, Some(true));
    }

    /// A mod's own `BlockItem` is an item registration and is declared with its properties even
    /// when a block shares the id; nothing is dropped as a "duplicate" of an invented item.
    #[test]
    fn an_item_declaration_is_kept_even_when_a_block_shares_its_id() {
        let declaration = runtime::content::Item {
            id: "horizon_java:proof_block".to_owned(),
            template: "minecraft:stick".to_owned(),
            max_stack_size: Some(1),
            rarity: Some(runtime::content::Rarity::Rare),
            crafting_remainder: None,
        };
        let item = item_entry(&declaration, &declaration.id, true);
        assert_eq!(item.id, "horizon_java:proof_block");
        assert_eq!(item.template, "minecraft:stick");
        assert_eq!(item.max_stack_size, Some(1));
        assert_eq!(item.rarity, Some(mod_api::Rarity::Rare));
        assert!(
            item.components
                .iter()
                .any(|(name, value)| name == "minecraft:item_model" && value.is_some())
        );
        // A content package of the namespace supplies the model: no template model is pinned.
        let packaged = item_entry(&declaration, &declaration.id, false);
        assert!(packaged.components.is_empty());
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
