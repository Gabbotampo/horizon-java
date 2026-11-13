# M3: a Java mod's items and blocks in the game

The question M3 answers is not *how* a Java `Item` or `Block` becomes content — the runtime already
records that — but *when*. A Java jar registers content while its entrypoint runs; the game's item and
block registries are built once, before the first resource or world loads, and never change again in
that process. This page lists every moment the fork (arismoko/horizon; written against pinned commit
`c4bcfcca5effdfa7d6bd759eeae60550c48ab5ac`, the mod now pins `1cc882b8e089a240354d142e3fd115938ce140d7`)
lets a mod add an item or block, what the runtime can
say about one, and the options for carrying a jar's declarations across that timing gap.

Findings only; the implementation is the second commit on this branch.

## What the runtime records

A jar that runs `Registry.register(Registries.ITEM, Identifier.fromNamespaceAndPath(...), new Item(new Item.Properties()))`
leaves a `RegistryEntry` in `StdHost::registrations()` (`crates/horizon-native-java/src/lib.rs`,
`Host::invoke` for `net/minecraft/core/Registry#register`). The entry's `declaration` is a typed
`RegistryDeclaration`:

- `Item { template, properties }` — inferred from the `Item.Properties` builder chain; the template
  is fixed at `minecraft:stick` (`INFERRED_ITEM_TEMPLATE`).
- `Block { template, properties, item }` — inferred from the `BlockBehaviour.Properties` chain; the
  template is fixed at `minecraft:stone` (`INFERRED_BLOCK_TEMPLATE`), and `item` is true.
- `MobEffect`, `Recipe`, `Ingredient` and `Custom` — recorded, but no game declaration for the first
  and only data files for the rest.
- `META-INF/horizon-native.json` can declare an explicit template and properties instead, and stays
  authoritative over inference (`parse_native_metadata`). A jar can therefore control its template
  without the bridge guessing.

The property vocabulary is the one the content loader accepts, not the whole builder:
`max_stack_size`, `rarity`, `crafting_remainder` for items; `hardness`, `explosion_resistance`,
`requires_tool`, `friction` for blocks. Everything else the builder recorded (`durability`, `light`,
`speedFactor`, ...) stays in `StdHost::item_properties`/`block_properties` for a later adapter.
Inference covers `new Item(Item.Properties)` and `new Block(BlockBehaviour.Properties)` only: a
`FenceBlock`, `StairBlock` or `BlockItem` constructor is not modelled, so those registrations stay
opaque.

The gap is timing. This mod runs jars at `server_about_to_start` (M1), which is after the game has
built its item and block tables. Running them there can log what a jar *would* declare, but the
registry is already immutable.

## (a) Every way the fork lets a mod add an item or block

| # | Way | Declared at | Read at | Assets |
|---|---|---|---|---|
| 1 | Content declaration files `data/<id>/horizon/{block,item}/<path>.json` in a `.hmod` (API 0.2, `capabilities = ["registries"]`) | package time (`cargo horizon-mod build`, or a folder) | game start, before anything loads: `mods::content_root` → `Content::read` → generated tables; cached and rebuilt when a mod file or the game changes (`horizon-mod-content.stamp`) | The same `.hmod`'s `assets/` and `data/` are lazy pack layers: blockstates, models, item definitions, textures, lang, loot, recipes, tags |
| 2 | Native code declarations: `Hooks::blocks` (`native_blocks`) and `Hooks::items` (`native_items`), NeoForge's `RegisterEvent` drained from `DeferredRegister.Blocks`/`.Items` | at the game's **Registration** phase, by `mods::freeze_declarations` (`crates/horizon-client/src/game.rs`), once per process | immediately, into the same content root that reads (1); together with a content mod of the same id when one is on, else as a synthetic mod with no files ("the template's look") | None of its own. A native mod's package may carry `assets/<its own id>/`, but the content fence (`Content::check_namespace`) refuses another namespace |
| 3 | A native mod's own `assets/`/`data/` in its `.hmod` | package time | every resource/data load as a pack layer | Its own namespaces only, plus vanilla/convention tags and vanilla atlases |
| 4 | World/game-load registration | — | — | There is none. The item/block name↔id tables are immutable once built; world load only maps saved names to numeric ids, `/reload` reloads resources and data, not content. Content selection is frozen while a world is open, and a mod installed then is read on the next start |

Phases: `Packages → Registration → Setup → Config → World` (`crates/horizon-mod-api/src/phase.rs`).
Registration is the last moment before content is built, and its only hooks are `native_*`
declaration collectors (`blocks`, `items`, component types, tint sources, model properties,
standalone models, block entity types). They take a `RegistrationContext`, which has no host, no
config-file service and no game-directed read; the config service first exists in `Config`.

The two declaration routes are the same content vocabulary: `content::Block`/`content::Item` are
what a `data/<id>/horizon/block|item/*.json` file deserialises to. A native declaration is checked
exactly like a file (`Content::declare`: the id belongs to the declaring namespace, the template is
a vanilla id, the item is declared once, a crafting remainder exists), and a refused namespace is
left out whole.

## (b) Options for Java content

### Option A — declare at Registration from the jar's recorded registrations

At `blocks`/`items`, read `config/horizon_java/java-mods/*.jar`, run each jar's server entrypoints
once with `StdHost` exactly as M1 does, read `host.registrations()`, translate every typed
`RegistryDeclaration` into `content::Block`/`content::Item`, and push it into the event.

- **Same process, same run.** The item exists in the world that start opens.
- **Ids and inferred properties only.** The template is `stick`/`stone` unless the jar ships
  `META-INF/horizon-native.json`.
- **Template look, no jar assets.** The synthetic content mod has no files, so the item draws with
  the template's item model/texture and the block with the template's states and model. `lang`, loot,
  recipes, blockstates and the jar's own textures are not there; the item's name falls back to its id
  key.
- **One pass per process.** A jar added while the game runs is read on the next start; that is
  already true of content itself.
- **File access.** `config_files` is not available in Registration, so the pass uses `std::fs` on the
  game folder. Native mods are trusted native code with the player's rights, and the game folder is
  the process's working directory (`--game-dir` is a `set_current_dir`), but this is the API gap in
  (d), not a designed contract.
- **Side.** The pass runs the jars once as `SERVER` on both client and dedicated server, so the two
  sides declare the same ids and pass the join content check. That is NeoForge's common-registration
  behaviour, not a side-specific one.

### Option B — bake a content package at install/build time

An offline tool (or a launcher step) runs the same entrypoints with `NativeJar`/`StdHost`, reads
`registrations()`, and writes a `.hmod` for the jar:

- `data/<jar id>/horizon/block/<path>.json` and `.../item/<path>.json` from the typed declarations,
  in the same vocabulary as (1);
- the jar's `assets/` and `data/` copied verbatim: blockstates, models, item definitions, textures,
  lang, loot tables, recipes, tags;
- the `.hmod` manifest (`id` = the jar's id, `api = "0.2"`, `side = "both"`,
  `capabilities = ["registries"]`).

The game reads it on the next start through route (1), with full fidelity and no synthetic mod.

- **Full assets and names.** The mod's own models, textures, lang, loot and recipes work because the
  same `.hmod` owns the namespace.
- **A step and a restart.** Something has to run before the game and the result has to be installed;
  the jar's bytes are not part of the horizon-java mod package.
- **Static.** Re-bake when the jar changes. Recipes/loot copied from the jar can name items the
  runtime did not translate, so they may not bind; bad files are logged and skipped, as any content
  mod's.
- **Still bounded by translation.** The tool can only emit what the runtime recorded; assets do not
  create behaviour the interpreter could not model.

### Option C — both: A now, B for assets

Route A's declarations join the content mod whose id is the same namespace when one is on
(`mod_content::accept_declared` re-uses `mods[i]`). A baked `.hmod` and the same jar's runtime
declarations therefore compose: the files supply assets and the synthetic mod is never created.
This is the recommendation, delivered in two steps.

### Option D — register at mod load or world load (fork change)

Not available, and not cheap to add. The only hook moment before the registries freeze is
Registration; Setup (mod load) and Config come after `freeze_declarations`, and World after content
tables are generated. A new hook there would need the game to rebuild immutable registry tables,
world saves' name→id maps and the network registry snapshot per world — NeoForge's `RegisterEvent`
and Fabric's `onInitialize` both run *before* the registry freeze, which is how they avoid that. This
is the seam named in (d).

## (c) Recommendation

**Ship C**, with A in this milestone and B as the follow-up:

1. **A (this branch)**: at Registration, the mod reads the jars it already knows how to run, records
   their content declarations, and pushes them as native content. It needs no fork change and puts a
   Java item in the world in the same run. The M3 proof is one Fabric jar declaring `m3_fixture:ruby`
   (an `Item`, template `minecraft:stick`) and `m3_fixture:ruby_block` (a `Block`, template
   `minecraft:stone`, with its block item): `/give @s m3_fixture:ruby` and the inventory show both,
   drawn with the vanilla stick/stone models.
2. **B (next)**: a `bake` command that writes the same declarations as `data/.../horizon/*.json`
   beside the jar's assets into a `.hmod`, so names, models, textures, loot and recipes follow. A
   jar whose content is baked needs no runtime declaration for its assets, but keeps A for anything
   the bake did not know.

### How the declaration carries across

| Java | Runtime record | Fork content field |
|---|---|---|
| `Registry.register(Registries.ITEM, Identifier.fromNamespaceAndPath("m", "ruby"), new Item(new Item.Properties().stacksTo(16).rarity(Rarity.RARE)))` | `RegistryEntry { registry: "minecraft:item", id: "m:ruby", declaration: Item { template: "minecraft:stick", properties: { max_stack_size: "16", rarity: "rare" } } }` | `content::Item { id: "m:ruby", template: "minecraft:stick", max_stack_size: Some(16), rarity: Some(Rare), crafting_remainder: None, ... }` |
| `new Block(BlockBehaviour.Properties.of().strength(3.0F).requiresCorrectToolForDrops())` | `Block { template: "minecraft:stone", properties: { hardness: "3", explosion_resistance: "3", requires_tool: "true" }, item: true }` | `content::Block { id: "m:ruby_block", template: "minecraft:stone", properties.hardness = 3.0, properties.explosion_resistance = 3.0, properties.requires_tool = true, item: true }` |
| a jar's `assets/m/...`, `data/m/lang/...`, `data/m/loot_table/...` | not read by the runtime | only route B can put them in the namespace |

`Identifier`'s namespace must be a valid content namespace (`[a-z0-9_.-]`, the path `[a-z0-9_./-]`);
a `minecraft:` id is vanilla's and is never redefined by a mod, so the bridge skips it. A duplicate
id across two jars is declared once with a log line, because a duplicate refuses the whole namespace
at the content root.

### What cannot map yet, and why

- **Jar assets** (models, textures, item definitions, blockstates, lang, loot, recipes, tags): route
  A has no files under the Java mod's namespace, and a native mod may not ship another namespace's
  files. Route B is the answer.
- **A behaviour class other than `Block`/`Item`**: `FenceBlock`, `StairBlock`, `DoorBlock`, crops,
  `BlockItem`, custom `Item` subclasses. The interpreter's builder inference only sees the
  `new Item(Properties)` / `new Block(Properties)` shapes; the fork's template vocabulary has
  fence/stairs/door templates but the runtime does not name the class a registration constructed.
- **A template other than `stick`/`stone`** without `META-INF/horizon-native.json`. The class a jar
  registered does not say "oak fence" or "iron block": that is `BlockBehaviour.Properties`/sound/
  map-colour detail the inference does not read.
- **Properties with no content key**: item `durability` (could become a `minecraft:max_damage`
  component later), `fireResistant`, food/consumable, tool components; block `light`, `map_color`,
  `sounds`, `randomTicks`, `noOcclusion`, `speedFactor`, `jumpFactor`, state properties and shapes.
  The runtime keeps them; the declaration vocabulary is narrower.
- **Data component patches** (`components`): `content_builders` records component types, but the
  bridge does not attach them to an item declaration.
- **Mob effects, entity types, menus, creative tabs**: only content files declare them; there is no
  native declaration hook for them (mob effects would need a `native_effects` collector; entity types
  a `native_entity_types` one).
- **Client-only assets and models** from the jar: route A cannot serve them; the client draws the
  template.

## (d) Fork seams, named

1. **The declaration types are not exported to mods.** `Hooks::blocks`/`items` take
   `horizon_mod_package::content::{Block, Item}` (generated `BlocksEvent`/`ItemsEvent`), but
   `horizon-mod-api` re-exports neither the crate nor the types, and a native mod may not depend on
   `horizon-mod-package` (path dependencies are not allowed, and a git copy is a different crate
   instance). A mod can only build them as `let mut b = Default::default(); b.id = ...` and let
   inference find the type. *NeoForge*: `DeferredRegister.Blocks`/`.Items` and
   `BlockBehaviour.Properties` are on the mod's classpath; *Fabric*: `Registry.register`,
   `Item.Settings`/`FabricItemSettings`. Fix: `pub use horizon_mod_package::content::{Block, Item,
   Rarity, BlockProperties};` (or named aliases) from `horizon-mod-api`, and document it.
2. **No game/config folder in `RegistrationContext`.** A mod that must read files before the
   registries freeze has no `ConfigFiles` (first available in Config) and no game-directory read, so
   the bridge uses `std::fs` on the working directory. *NeoForge*: `FMLPaths.GAMEDIR`/
   `FMLPaths.CONFIGDIR` are available from mod construction; *Fabric*:
   `FabricLoader.getInstance().getGameDir()`/`getConfigDir()`. Fix: expose the game folder, or a
   read-only `ConfigFiles`, on `RegistrationContext`.
3. **A native mod cannot own another namespace's assets.** Even when it declares `m:ruby`, its
   `.hmod` may not carry `assets/m/...` (`Content::check_namespace`); only a content `.hmod` can.
   *NeoForge/Fabric*: a mod's code and `assets/<modid>` are in the same jar and namespace. Fix (if B
   should not be a separate package): let a manifest list additional owned namespaces for assets
   with the same fence, or let a native declaration name the files it contributes.
4. **No post-start content registration.** Content is frozen at Registration; there is no in-process
   hook after Setup, and a mod installed while a world is open waits for the next start. This is the
   correct design for immutable registry tables, but it means a Java jar that is only discovered
   after the game started can never register in that session. *NeoForge*: `RegisterEvent`, fired
   during mod loading, before the freeze; *Fabric*: `onInitialize` and `Registry.register` before the
   freeze. No fix is needed for M3 (route A runs at Registration); a bake (route B) plus a restart is
   the fallback for anything added later.
