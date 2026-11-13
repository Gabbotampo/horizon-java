# Running inside Horizon: the native mod

`mod/` is a Horizon **native mod**: the first way this runtime runs inside the game rather than in
the probe. It is built against the game-side mod API on
[arismoko/horizon](https://github.com/arismoko/horizon) (crate `horizon-mod-api`, library
`mod_api`), which is where that API lives. Since the fork's `a5ca9a72fd` the types themselves are in
`crates/horizon-mod-vocab` (library `mod_vocab`) and `horizon-mod-api` re-exports them; a mod still
depends on `horizon-mod-api`.

Nothing in `crates/horizon-native-java` or `crates/horizon-jvm` moves or changes shape for this:
the mod carries a copy of the runtime (`tools/vendor.py`), because a native mod is one flat crate.

## What M0 to M3 do

| Milestone | Behaviour |
|---|---|
| **M0, the shell** | `Hooks::server_about_to_start` and `Hooks::client_setup` log one line each. The server start writes this mod's report, `config/horizon_java/java-runtime.json`, through the mod API's own config service (`Host::config_files`), so the game files it under `config/` beside every other mod's. With no jars it is `{"schema":1,"jars":[]}`. |
| **M1, run jars** | Every `.jar` in `config/horizon_java/java-mods/` is read as bytes (at `server_about_to_start` until M3 moved the read to the content phase, with `std::fs`; see M3), loaded by the runtime from memory (`NativeJar::from_bytes`) and run with its own `StdHost`. One log line per jar, one row in the report: the jar's id, `ran-clean` / `ran-hollow` / `ran-partial` / `refused`, its host-call and unknown-host-call counts, and the reason a jar was refused or ran partially. |
| **M2a, lifecycle** | Each jar's `StdHost` (heap, statics, recorded listeners) is kept for the server's life. `server_about_to_start` fires the Fabric `SERVER_STARTING` and NeoForge `ServerAboutToStartEvent` listeners, `server_starting` the NeoForge `ServerStartingEvent` ones, `server_started` the Fabric `SERVER_STARTED` and NeoForge `ServerStartedEvent` ones, and `server_stopping`/`server_stopped` their own. Each hook logs callbacks run/failed and unmodelled host calls; every row's `callbacks` object totals them; a jar's `System.out` lines reach the game log with its id. |
| **M2b, commands** | At `register_commands`, the recorded Fabric `CommandRegistrationCallback` (and NeoForge `RegisterCommandsEvent`) listeners build their Brigadier tree on the runtime's façade, and its literal, `word`/`text`, `integer` and player-selector (`EntityArgument.player()`/`players()`, as the mod API's `player` kind) nodes become typed `NativeCommand` usages. `run_mod_command` maps the game's invocation back (deepest matching usage), runs the Java `Command.run`, and sends `sendSuccess`/`sendFailure`/`sendSystemMessage` replies through `Commands::feedback` (or `PlayerRef::tell` for a player the command named). Any other node — other argument kinds, redirects/forks, an unevaluable requirement — is refused by name in the log and left out. |
| **M2c, game events** | Every other modelled server event reaches its matching mod API hook, moment for moment: server and level ticks, level load/unload, player join/leave/respawn/clone/dimension change, entity join/leave, living damage before/after, item and block use, block and entity attack, and block break (whose cancel also fires Fabric's `CANCELED`). A cancellable hook carries the firing's cancel and, where the hook has one, the Fabric `InteractionResult` back as the hook's cancellation result. The full table, arguments and departures: [docs/mod-hooks.md](mod-hooks.md). |
| **M3, content** | The jars now run at the game's **Registration** phase, before any resource or world loads and while the content root can still grow, instead of at `server_about_to_start`. Each `RegistryDeclaration::{Item, Block}` the runtime recorded becomes a native content declaration (`Hooks::blocks`/`items`, NeoForge's `RegisterEvent`): an item keeps the template and the properties the content vocabulary carries, and a block brings its block item. The game joins them under the jar's namespace, so `/give`, `setblock` and the inventory see `<jar id>:<path>`. A jar's own assets, lang and loot need the bake route in [m3-content-design.md](m3-content-design.md). |
| **M3b, baked assets and declarations** | An install-time bake (`tools/bake.py`) runs each jar's server entrypoints through the offline runtime (`horizon-native-java --json <jar>`, the code the mod vendors), keeps the resource kinds a content package accepts, and writes one `.hmod` per namespace into `<game-dir>/horizon-mods/`, turned on in `horizon-mods.txt`. The package carries the jar's models, textures, item definitions, blockstates, lang and loot **and** its blocks/items as `data/<ns>/horizon/{block,item}/*.json` (the runtime's own mapping, `crates/horizon-native-java/src/content.rs`). It owns its namespace, so the game loads it without the native ownership fence; the mod's runtime declaration for that namespace is skipped (see the fence note below). Class files, metadata and unsafe paths never enter the package; everything left out is in `config/horizon_java/bake-report.json` with its reason. |

The verdicts are the ones the corpus ledgers already use:

- **`ran-clean`** — every server entrypoint stage ran and no host call was unmodelled.
- **`ran-hollow`** — every stage ran, but at least one host call was answered neutrally
  (`unknown_host_call_count`).
- **`ran-partial`** — at least one stage ran and at least one failed; the listeners the running
  stages registered stay active, and `reason` names each failed stage and its error. A real loader
  would have thrown, so this is never counted as clean, whatever its host-call count.
- **`refused`** — unreadable jar, no server-side entrypoint (a client-only jar is named as such),
  an unsupported entrypoint adapter, or every entrypoint stage failed; `reason` says which.

Jars run one at a time, so a jar cannot link against its neighbours yet: cross-jar classes are a
later milestone.

## M2 proofs

The fixture jars are built by `tools/java_mod_fixture.py` and never committed. With `m2a_fixture.jar`
and `m2b_fixture.jar` in `config/horizon_java/java-mods/`, one integrated-server run shows (from
`horizon-ctl`'s log for the run):

```
Horizon Java: m2a_fixture: m2a: SERVER_STARTED ran
Horizon Java: server_started (dedicated=false): 1 callback(s) ran, 0 failed, 0 unknown host call(s)
Horizon Java: m2b_fixture: command registration: 1 callback(s) ran, 0 failed
Horizon Java: m2b_fixture: 3 command usage(s) translated, 0 refused
Horizon Java: m2b_fixture: /javahello <name:word> ran (returned 1, 1 message(s))
Horizon Java: m2a_fixture: m2a: SERVER_STOPPING ran
Horizon Java: server_stopping (dedicated=false): 1 callback(s) ran, 0 failed, 0 unknown host call(s)
```

`horizon-ctl … command "/javahello Viola"` answers `{"chat":["Hello, Viola!"]}`, and the stopped
report's `callbacks` for `m2a_fixture` is `{"ran":2,"failed":0,"unknown_host_call_count":0}`.

The fixture also registers `/javawhere`, whose body reads `getLevel()`. Run as a player it refuses:

```
Horizon Java: m2b_fixture: /javawhere refused: unknown native host call net/minecraft/commands/CommandSourceStack#getLevel()Lnet/minecraft/server/level/ServerLevel;
```

and the player sees vanilla's `command.failed`; the row's `commands` object records it:
`{"ran":1,"refused":1,"unknown_host_call_count":1}`. The call is on the command's path (the
runtime's strict mode during a command), not silently converted into an overworld level.

### Departures from the Java behaviour

Named here, not just in comments:

- **Split `server_started`.** The runtime records Fabric `SERVER_STARTED`, NeoForge
  `ServerStartingEvent` and NeoForge `ServerStartedEvent` on one event; the mod splits it by the
  listener's recorded source so each runs at its own mod API hook (`server_starting` or
  `server_started`). Without the split the NeoForge `ServerStartingEvent` listeners would run one
  hook late.
- **Command sources that are not players run as the console.** The mod API's `CommandSource` has a
  block/cart/function origin, but the runtime's `CommandSourceStack` façade has one non-player
  shape, so `getTextName()` is `Server` for all of them. The origin's permission level is kept.
- **A player source's level is unknown until the fork adds a dimension read.** No mod API read
  gives a `PlayerRef`'s dimension (see [Fork seams needed](#fork-seams-needed)), so a player
  source's `CommandSourceStack.getLevel()` goes through the runtime's unknown-host-call path: the
  call is counted in the command's report and the command fails visibly rather than quietly acting
  on the overworld while the player stands in the Nether. Console and other non-player sources keep
  the overworld, which is vanilla's server-console source level, not a guess.
- **Styled component replies lose their style.** The runtime hands back plain text beside a
  component JSON; `Text` here is a literal or a translation key, so colours, siblings and
  translation arguments are dropped (`Hello, <name>!` is unaffected).
- **`sendSuccess(supplier, broadcast)` sends one feedback line** through `Commands::feedback`,
  which follows vanilla's source rules; the explicit broadcast flag is not carried onto a second
  audience.
- **A jar's command set is fixed at registration.** `Commands::refresh_commands` re-offers the same
  usages: no Java API here re-runs a registration callback with a different set.
- **`EntityArgument.players()` takes one player.** The mod API's only selector kind is `player`:
  the game resolves it to exactly one online player before the command runs. `player()` maps onto
  it exactly. `players()` maps onto it as its one-player subset, so `/carryon clear @a` with two or
  more players online is refused by the game's parser (with its feedback; a vanilla client already
  flags the multi-player selector as it is typed, since the tree it gets says single player)
  instead of running once per player as vanilla does. A selection of one player, `@s`, `@p`, a name
  or a UUID behaves as vanilla. `EntityArgument.entity()` and `.entities()` are refused at
  registration by name: narrowing "any entity" to "one player" would run the command on a set the
  jar did not ask for. Both wait on the fork seam below ("Entity selectors in typed commands").
- **`getInput()` names the selected player, not the selector typed.** The game hands the mod the
  resolved `PlayerRef`, not the text, so the input line carries the player's name (or their UUID
  when the game has no name for them, which vanilla also accepts as a selector) where the player
  typed `@s` or `@p`.

## M3 proof

With `m3_fixture.jar` (built by `tools/java_mod_fixture.py --kind content`) in
`config/horizon_java/java-mods/`, one integrated-server run shows the jars run before content is
built and both declarations reach the game:

```
Horizon Java: m3_fixture.jar (m3_fixture) ran-clean (9 host calls, 0 unknown host calls)
Horizon Java: content: 1 block declaration(s) from 1 jar(s)
Horizon Java: content: 1 item declaration(s) from 1 jar(s)
Horizon: content from 2 mod(s) in .horizon-cache/mod-content/<stamp>
Horizon Java 0.1.0: server about to start (dedicated=false); 1 jar(s) in config/horizon_java/java-mods: 1 ran-clean, 0 ran-hollow, 0 ran-partial, 0 refused; report in config/horizon_java/java-runtime.json
```

The fixture registers `m3_fixture:ruby` (a plain `Item`, template `minecraft:stick`) and
`m3_fixture:ruby_block` (a plain `Block`, template `minecraft:stone`) through
`Registry.register(Registries.ITEM|BLOCK, ...)`. `horizon-ctl` then answers:

```
give m3_fixture:ruby 5          {"given":5,"ok":true}
give m3_fixture:ruby_block 4    {"given":4,"ok":true}
inventory                       counts {"m3_fixture:ruby":5,"m3_fixture:ruby_block":4}
command "/setblock -1 69 4 m3_fixture:ruby_block"   Changed the block at -1, 69, 4
block -1 69 4                   {"block":"m3_fixture:ruby_block","state":"m3_fixture:ruby_block"}
```

Both draw with the template's art — the runtime sets the item's `minecraft:item_model` to the
template (the stick and stone textures in the hotbar), and the block renders as stone — so no art
is needed for the proof. The stopped report's row is
`{"content":{"blocks":1,"items":1},"verdict":"ran-clean","host_calls":9,"unknown_host_call_count":0, ...}`.

### Departures from the Java behaviour

- **The jars run at the content phase, not per server.** The entrypoints now run once per process,
  at Registration, so their content exists in the same start; M2's listeners still fire at their
  server hooks. A second world this process hosts reuses the jars and resets only the per-server
  counters.
- **The jar is run as a server on both sides.** Content ids must match between a client and a
  dedicated server, so the pass uses one `SERVER` environment rather than running the common
  entrypoint twice with side-dependent results.
- **Jars are read with `std::fs`, not `config_files`.** The scoped config service first exists in
  Config, after Registration; `config/horizon_java/java-mods` is the mod's own folder and the game
  folder is the process's working directory. The report still goes through `config_files`.

### The pinned fork's ownership fence

The pinned commit added a namespace-ownership check to the generated `dispatch_blocks`/
`dispatch_items` (`crates/horizon-mod-api/src/parts/owned_namespaces.rs`): a native mod may declare
content only in its manifest id and its manifest `namespaces`, and any other id is a fatal refusal
at start. A Java jar's namespace is learned at install time and is not in the mod's manifest, so the
bridge declares only ids it owns; every other typed declaration is counted in the jar's row
(`content.skipped`, per namespace with block and item counts) and logged:

```
Horizon Java: m3_fixture.jar: content: 1 block(s), 1 item(s) in namespace m3_fixture not declared: not owned by horizon_java; run tools/bake.py
```

`tools/bake.py` (M3b) writes those declarations into a content package of the jar's namespace, which
the game accepts without the fence — the package owns the namespace, and no native declaration is
involved. Without the skip, one jar with content would stop the game at start, and the M3 proof
above would not run at this pin.

## M3b proof

The bake needs the runtime probe; it is the same `horizon-native-java` crate the mod vendors, so
the declarations it records offline are exactly what the mod would have declared at Registration:

```
cargo build -p horizon-native-java
python3 tools/bake.py --probe target/debug/horizon-native-java --game-dir SCRATCH/game-dir
m3_fixture.jar: 1 package(s) [m3_fixture], 2 file(s) skipped
report: SCRATCH/game-dir/config/horizon_java/bake-report.json
```

`sorted(zipfile.namelist())` of `SCRATCH/game-dir/horizon-mods/m3_fixture.hmod` is the jar's own
`assets/` and `data/` (blockstate, models, item definitions, the two generated 16x16 ruby textures,
`lang/en_us.json`, the block loot table), its `data/minecraft/tags/block/mineable/pickaxe.json`, and
the package's own declarations:

```
data/m3_fixture/horizon/block/ruby_block.json  {"item":true,"properties":{},"template":"minecraft:stone"}
data/m3_fixture/horizon/item/ruby.json         {"template":"minecraft:stick"}
```

The manifest is `id = "m3_fixture"`, `api = "0.2"`, `side = "both"`,
`capabilities = ["registries"]`; the report lists `fabric.mod.json` and `m3fixture/Content.class` as
skipped (not a content path / code) and `declarations: {"blocks":1,"items":1}`. The game loads the
package beside the native mod, and the jar's own declaration is skipped by the fence:

```
Horizon Java: m3_fixture.jar: content: 1 block(s), 1 item(s) in namespace m3_fixture not declared: not owned by horizon_java; run tools/bake.py
Horizon: content from 2 mod(s) in .horizon-cache/mod-content/<stamp>
```

`horizon-ctl` then shows the jar's name, art and loot:

```
give m3_fixture:ruby 1        {"given":1,"ok":true}
messages 1                    Gave 1 [Ruby] to Player...
select m3_fixture:ruby        (hotbar shows the red gem and the name "Ruby" above it)
screenshot ruby-hotbar.png    the ruby texture, not the stick
setblock 7 73 3 m3_fixture:ruby_block
break 7 73 3                  {"block_before":"m3_fixture:ruby_block",
                               "drops":{"m3_fixture:ruby":1},
                               "picked_up":{"m3_fixture:ruby":1}, ...}
inventory                     counts {"m3_fixture:ruby":1} -> {"m3_fixture:ruby":2}
```

The block drops only with a pickaxe because a `minecraft:stone` template keeps stone's
`requires_tool`; the jar's own `mineable/pickaxe` tag is what makes the wooden pickaxe "correct for
drops", and it travels through the same bake. Without the package the item is named
`block.m3_fixture.ruby` and draws as the stick.

## Fork seams needed

- **A namespace a manifest id cannot spell.** A package id is `[a-z0-9_]{1,64}`
  (`Manifest::parse`), while a namespace is `[a-z0-9_.-]`; a jar that registers in `my-mod` or
  `my.mod` has no package id its declarations can join by (`accept_declared` matches `m.id == ns`).
  The bake refuses such a namespace by name instead of writing a package that cannot join.
  *NeoForge/Fabric*: a mod id may contain `-` and is used as the namespace directly. Fix: a content
  package may declare an owned namespace that differs from its id, or the join matches
  `namespaces` rather than `id`.

- **`block_break_after`.** The runtime records Fabric `PlayerBlockBreakEvents.AFTER`; NeoForge has
  no counterpart (`BreakBlockEvent` is before the block goes). `break_block` fires before the block
  is destroyed, so there is no moment to fire the Fabric listener from. Add a listen-only hook
  after the block and its drops are gone.
- **The hand on `entity_interact` (#2797).** The hook's contract says `self.hand` and it fires once
  per hand, but the game's `native_entity_interact(entity, kind, stack, claimed)` in
  `crates/horizon-client/src/game/native_hooks/base.rs` never takes a hand, so the event cannot
  carry one. The mod names the hand from its stack when exactly one hand holds it, and leaves it
  unknown (a counted unknown read) when both hands agree, two empty hands included. Filed as
  `arismoko/horizon#2797`; the fix is to pass `hand: Hand` through that call and its event.
- **A player's level (#2747).** `PlayerRef` names a player's identity, permission and messages, and
  `host.players().acting_player()` names the player a block or entity hook is about, but no read
  gives the level that player stands in. So the `Level` argument of Fabric's `AttackBlockCallback`,
  `UseEntityCallback`, `PlayerBlockBreakEvents` and NeoForge's
  `BreakBlockEvent`/`AttackEntityEvent`/`PlayerInteractEvent` cannot be filled: each call that needs
  it is counted as an unknown host call instead of answering the overworld. Filed as
  `arismoko/horizon#2747` (closed by #2784's `player_level`, after this pin); the acting player
  itself is already carried (M2c). A player's command source does answer `getLevel()`: it is
  `player.level()` in vanilla, and the pin reads it as the player's entity's level
  (`EntitiesBackend::entity_level`, which the bridge already uses for the player list).

- **The old player on `player_clone`.** Filed as [arismoko/horizon#3040](https://github.com/arismoko/horizon/issues/3040); the issue text, in the fork's template:

  > **Ask.** Let a `player_clone` listener read the player as they were before the copy: a
  > handle (say `event.original`), valid only during the hook, whose reads answer the old player's
  > state, not the new one's. At minimum its attachments with their pre-copy values (every type,
  > not only `copy_on_death` ones), its level and position, and its inventory. Done when, on a
  > death respawn, a listener reads a non-`copy_on_death` attachment's pre-death value and the
  > death position and dimension from the original while the new player reads its fresh default;
  > and on an End return the original reads the End and the values from before the copy; native
  > and wasm.
  >
  > **Background.** NeoForge posts `PlayerEvent.Clone(newPlayer, oldPlayer, wasDeath)` from
  > `ServerPlayer.restoreFrom(old, keepEverything)` (@371-382 of the 26.3 patched server, with
  > `wasDeath = !keepEverything`); the old `ServerPlayer` is still whole at that moment and keeps
  > all its attachments. Horizon's `player_clone` (`native_player_clone(player, name, was_death)`,
  > `crates/horizon-client/src/game/network/lan_actions/respawn.rs:146` and
  > `game/world/server_travel.rs:557`; unchanged between `cf0113879e` and `d3e649f849`) names one
  > uuid for both players, and by the time it fires on a death the guest's store has already been
  > replaced by its `copy_on_death` subset (`respawn.rs:19-25`, `copy_on_death` then
  > `replace_contents`). So nothing can answer the old player. Example: carryon 2.12.1 (NeoForge
  > 26.3) `CommonEvents#onClone` calls `getOriginal()` first (@1), then
  > `PlacementHandler.placeCarriedOnDeath(original, getEntity(), isWasDeath())`, which reads the
  > original's non-`copyOnDeath` `CarryOnData` attachment and places what the player was carrying
  > where they died; without it the carried block or entity is lost on death. Horizon Java answers
  > `getEntity()` and `isWasDeath()` today and refuses `getOriginal()` (a counted unknown host call)
  > rather than hand back the new player.
  >
  > **Hook or handle.** `player_clone` (an original-player view on `PlayerCloneEvent`)
  >
  > **NeoForge 26.3 counterpart.** `PlayerEvent.Clone#getOriginal` (posted by
  > `EventHooks.onPlayerClone` from `ServerPlayer.restoreFrom`); Fabric's
  > `ServerPlayerEvents.COPY_FROM(oldPlayer, newPlayer, alive)`
  >
  > **Who uses it.** Horizon Java (NeoForge `Clone` and Fabric `COPY_FROM` listeners, carryon's
  > `onClone` first); any mod that moves state from the old player to the new one itself
  >
  > **Phase.** Hooks

- **Entity selectors in typed commands.** Filed as [arismoko/horizon#3039](https://github.com/arismoko/horizon/issues/3039); the issue text, in the fork's template:

  > **Ask.** Let a native command take an entity selector that resolves to entities, not only to one
  > online player: usage kinds `entity` (exactly one entity of any type), `entities` (one or more
  > entities) and `players` (one or more online players), delivered after the server has parsed,
  > permission-checked and resolved the selector as vanilla's `EntitySelector` does, as new
  > `CommandArgumentValue` variants carrying `EntityRef`s / `PlayerRef`s in selection order. Done
  > when `Command::new("heal <targets:players>")` run as `/heal @a` with two players online calls
  > `run_mod_command` once with both `PlayerRef`s, `<target:entity>` given
  > `@e[type=minecraft:cow,limit=1,sort=nearest]` delivers that cow's `EntityRef`, and an empty or
  > too-large selection is refused with vanilla's feedback (`argument.entity.notfound.entity`,
  > `argument.entity.notfound.player`, `argument.entity.toomany`, `argument.player.toomany`)
  > without calling the listener, on native and wasm.
  >
  > **Background.** The mod API's typed commands have one selector kind, `player`
  > (`CommandArgumentValue::Player`, "must select exactly one online player",
  > `crates/horizon-mod-vocab/src/command/`; unchanged between `cf0113879e` and `d3e649f849`).
  > Vanilla's `EntityArgument` has four: `entity()`, `entities()`, `player()`, `players()`.
  > Horizon Java translates the Brigadier trees Fabric/NeoForge jars register onto native typed
  > commands; it can carry `player()` exactly and `players()` only as its one-player subset
  > (`@a` with two players online is refused), and it refuses `entity()`/`entities()` at
  > registration. Example: carryon 2.12.1 (NeoForge 26.3) registers `/carryon clear [<target>]` and
  > `/carryon place [<target>]` with `EntityArgument.players()` and reads them with
  > `EntityArgument.getPlayers`, iterating the collection. The selector permission
  > (`Permissions.COMMANDS_ENTITY_SELECTORS`, `EntitySelectorParser.allowSelectors`) and resolution
  > against the source's position belong on the server, as for `player` today.
  >
  > **Hook or handle.** `CommandArgumentValue` (new `Entity`/`Entities`/`Players` values) and the
  > `entity`/`entities`/`players` usage kinds of `Command`
  >
  > **NeoForge 26.3 counterpart.** `net.minecraft.commands.arguments.EntityArgument.entity()`,
  > `entities()`, `player()`, `players()` and `getEntity`/`getEntities`/`getOptionalEntities`/
  > `getPlayer`/`getPlayers`/`getOptionalPlayers` (vanilla; NeoForge and Fabric use it unchanged)
  >
  > **Who uses it.** Horizon Java (Brigadier commands from Fabric/NeoForge jars, carryon's
  > `/carryon clear` and `/carryon place` first); any native command that acts on several players
  > or on a mob
  >
  > **Phase.** Host split

### Mod-owned data attachments (`Player#getData`)

> The design for bridging these onto the fork's store is [attachments-design.md](attachments-design.md)
> (a proposal under review); the note below is the reasoning it starts from.

NeoForge's `AttachmentType<T>` is the contract behind `getData`: a mod declares a key with a
default supplier, an optional `IAttachmentSerializer` (so a value may be unserializable, in which
case it lives only while the holder does), `copyOnDeath`, and an optional `StreamCodec` plus
`syncData` for clients. `IAttachmentHolder` answers `getData`, `getExistingData` and `hasData`.
carryon's `ServerTickEvent.Post` listener reads its own attachment every tick, which is why the
first in-game survey counts 660 failures on `Player#getData(Supplier)`.

The pinned fork already owns this contract on the **Rust** side: `horizon-mod-runtime`'s
`AttachmentType<T>` has the default, optional codec, `copy_on_death`, `sync`, and a store for
players, entities, chunks, block entities and levels; values persist under
`horizon:attachments` (`{value, copy_on_death, sync}`), copy on respawn, and sync to Horizon
clients (`docs/modding/src/concepts/trait-mods/028-server-attachments.md`). What is missing is the
**Java** bridge, not storage:

- the runtime's `Host`/`StdHost` has no attachment read or write member, so a listener cannot
  reach that store;
- no Java-facing `AttachmentType`/`IAttachmentHolder` marker exists, so the runtime cannot read a
  mod's key, its serializer or its codec from bytecode;
- serializer calls (`IAttachmentSerializer.serialize`/`deserialize`) run mod bytecode at save,
  load and `getData` time, which needs a bridge that hands the runtime the holder and the value
  and gets a `Compound` back — no such call path is modelled;
- death copying and the client sync stream have no Java-side equivalent either.

Honest mod-owned storage today would be a `HashMap<holder, Compound>` inside the runtime, filled
from the mod's serializer on write and never persisted, copied or synced: it would make
`getData` return a value on the same server lifetime but silently lose it on restart and on
death, which is worse than the counted refusal. So `Player#getData(Supplier)` stays counted until
the attachment seam exists; `ServerPlayer#level()` remains separately blocked on
`arismoko/horizon#2747`. This note does not implement either.

## Pin bump to `1cc882b8e0`

The fork's mod API now tells a mod which Minecraft version it runs in: `ConfigHost::game_version`
(`#[access(Metadata)]`) and `RegistrationContext::game_version`, from the 2026-11-12 review rollup
(`b1d4985cbf`). The pin moved from `cf0113879e` to **`1cc882b8e089a240354d142e3fd115938ce140d7`**
(fork `main`, #3822, 2026-11-13) so the jars can be told it.

**Which commit.** No fork `main` commit at or after `b1d4985cbf` has a fully green `dyna-ci` run:
the `ignored-tests-shards` jobs (the slow `#[ignore]` tests) fail on every `main` run of 12 and 13
November, and most runs are cancelled by the next push. `1cc882b8e0` is the newest `main` commit
whose `verify`, `verify-static`, `lint` and `launcher` jobs passed. It is the merge of #3822, which
fixes three of those slow-shard tests. Its own `horizon-world` slow shard still failed. The next
commit, rollup 65 (`2d3964c330`), had no CI result yet.

**Survey** (`git log cf0113879e..1cc882b8e0 -- crates/horizon-mod-api crates/horizon-mod-vocab
crates/horizon-mod-package crates/cargo-horizon-mod docs/native-mods.md docs/modding`: 7,059 commits,
957 files; then the diff of every hook this mod implements and every host method it calls):

- **The mod compiles unchanged against the new API.** Every `mod_api` path it names still resolves.
  The only source change the bump forced is in a test: `PlayerCloneEvent` gained a field, `original`.
- **Hooks this mod implements whose files changed**: `blocks`, `items`, `entity_interact`,
  `player_clone`, `right_click_block`, `right_click_item`, `level_tick_pre`/`post`, `server_tick`
  (doc lines: `host.server_has_time()`), and doc renames from `native_*` to plain hook names in
  `entity_leave_level`, `level_load`/`unload`, `living_damage_post`, `player_changed_dimension`,
  `player_logged_in`/`out`, `server_about_to_start`, `server_stopping`. The rest
  (`attack_entity`, `break_block`, `left_click_block`, `living_death*`, `entity_join_level`,
  `register_commands`, `run_mod_command`, `server_started`/`starting`/`stopped`,
  `player_respawned`, `game_rule_changed`, `client_setup`) are byte-identical. What changed in
  meaning:
  - `blocks`: **a code-declared block now gets a block item by default** (#3119:
    `Block::default()` has `item: true`, "as in JSON"). This mod sets `block.item = false` explicitly
    (`content.rs::block_entry`), so its declarations are unchanged.
  - `entity_interact`: the event carries `hand` (#2797's carry). This mod still reads the acting hand
    from the host. Wiring the field is a follow-up.
  - `player_clone`: the event carries `original: OriginalPlayerRef`, a hook-scoped view of the old
    player (#3040). Not used yet.
  - `right_click_block`: it now fires for spectators too, and `use_block`/`use_item` `Default` got
    exact vanilla rules. This mod never writes either field. It only fires `UseBlock` and cancels
    with the jar's `InteractionResult`, as before. Fabric's `UseBlockCallback` also fires for a
    spectator: it is injected at the head of `interactBlock`, before the spectator check.
  - `right_click_item`: `Success`, `SuccessServer` and `Consume` now all end the client's hand loop
    (before, only `Success` did). This mod answers on the server only.
- **Host methods.** None this mod calls was removed or changed signature. New ones include
  `ConfigHost::{dist, game_version, horizon_version, server_has_time}`,
  `ConfigServerHost::game_tests_enabled`, the `original_player_*` reads, `player_level`/
  `player_dimension`, `entity_by_uuid`, `level_datapack_*` and the container `*_kind` transfers.
  `net_send_to_player`/`net_send_to_server` were removed in favour of the payload area. This mod
  calls neither.
- **Access levels.** Every host method still declares one `#[access(...)]` level (`WorldOnly`,
  `Metadata`, `ClientSetup`, `PublishedResources`). The game checks it against the load phase
  (`world/phase.rs::access_allowed`; a native caller panics on a refusal). A mod declares nothing
  for it. `game_version` is `Metadata`, allowed in Setup, Reloading and World. This mod asks the host
  for it only in `server_about_to_start`, which runs in World.
- **Commands.** `CommandArgumentValue` gained `Entity`, `Entities` and `Players` (#3039's
  selectors). The bridge still declares only `player`. It still refuses `EntityArgument.entity()`
  and `.entities()` at registration, as before.
- **Manifest.** `Manifest::parse` still requires `api = "0.2"` and `capabilities = ["registries"]`.
  The `[native]` fields are unchanged. The fork's `MIXINS_REV` moved to
  `e59da9094f0c2fdda6c627fe9eff22dec6a906f0`, and `mixins` follows it (the launcher builds the
  newest `mixins` any mod names). This mod runs no mixins.
- **Launcher staging.** The generated `main.rs` template now calls
  `horizon_mod_host::{prepare_main_stack, exit_if_compile_child, main_with}`. The generated package
  depends on `horizon-mod-host` (default features off). Its `MOD_ENTRIES` line is still
  `"horizon_java" => mod_0::hooks`. The "Build" recipe below stages the same way.
- **Dependencies.** At `1cc882b8e0`, Horizon's lock still resolves `zip 9.1.3`,
  `serde_json 1.0.151`, `libc 0.2.190` and `log 0.4.34`, so the `=` pins in `mod/Cargo.toml` stand.
  Staging adds exactly two packages to the lock (`horizon-java-mod`, `horizon-native-game`).

**The game version reaches the jars.** `State::ensure_loaded` takes the game's version and
`load_jar` seeds every jar's host with it (`StdHost::with_game_version`). In the content hooks the
version comes from the `RegistrationContext`. In the `server_about_to_start` fallback it comes from
the server's host (`ConfigHost::game_version`). `RegistrationContext::game_version` panics on a
context without registration data. The fallback makes such a context, so the version is asked only
of a context that carries its instance folder, which comes from the same data. With the version in
place, `minecraft`'s mod container (Fabric's `getAllMods`/`getModContainer`, FML's `ModList`) and
`DetectedVersion`/`SharedConstants.getCurrentVersion()` answer `26.3` instead of a counted `null`.

**Build and proofs, unpatched.** The game was built from a worktree of `arismoko/horizon` at
`1cc882b8e089a240354d142e3fd115938ce140d7`. Only the launcher's generated files were added: the
workspace members, `crates/native-game/`, and `native-api/horizon_java` with its `horizon-mod-api`
path rewrite. The build is the dev profile on elysium. The game's own version line is
`Horizon 0.1.0`; this game logs no commit field, so the worktree's `HEAD` above is the build. One
run had every fixture (`m0`, `m2a`, `m2b`, `m2c`, `m2d`, `m3`, baked), a non-jar and a client-only
jar. It shows what the `cf0113879e` proof showed:

```
Horizon Java: broken.jar (broken) refused (0 host calls, 0 unknown host calls, reason: not a readable jar: Zip("invalid Zip archive: Could not find EOCD"))
Horizon Java: client_only_demo.jar (client_only_demo) refused (0 host calls, 0 unknown host calls, reason: client-only entrypoint: a server never runs it)
Horizon Java: m3_fixture.jar: content: 1 block(s), 1 item(s) in namespace m3_fixture not declared: not owned by horizon_java; run tools/bake.py
Horizon: content from 2 mod(s) in .horizon-cache/mod-content/<stamp>
Horizon Java 0.1.0: server about to start (dedicated=false); 8 jar(s) in config/horizon_java/java-mods: 6 ran-clean, 0 ran-hollow, 0 ran-partial, 2 refused; report in config/horizon_java/java-runtime.json
Horizon Java: m2a_fixture: m2a: SERVER_STARTED ran
command "javahello Viola"             {"chat":["Hello, Viola!"]}
command "javawhere"                   {"chat":["level game/level/minecraft:overworld"]}   (#88's player-source level)
give m3_fixture:ruby 1                Gave 1 [Ruby] to Player…; hotbar shows the ruby texture and "Ruby"
break <ruby_block> (wooden pickaxe)   {"block_before":"m3_fixture:ruby_block","drops":{"m3_fixture:ruby":1}}
break <stone>                         {"block":"minecraft:stone"}; m2c: kept stone / break canceled
interact-entity <cow> (hands empty)   m2d: interact hand=null id=3, twice; use_entity: 1 callback(s) ran, 0 failed, 1 unknown host call(s)
kill @e[type=minecraft:cow]           m2d: after death id=3
Horizon Java: m2a_fixture: m2a: SERVER_STOPPING ran
```

An empty `java-mods/` writes `{"jars": [], "schema": 1}` (M0). The full in-game survey before and
after the bump is in [in-game-survey.md](in-game-survey.md), "Sixth survey". Against an old-pin
build of the same runtime, every sample row is unchanged. The one difference: servux, malilib and
litematica now run (refused before, on `minecraft`'s null version).

**Seams the fork now has** (not wired here; each is its own follow-up):

| Seam | On fork `main` at `1cc882b8e0` |
|---|---|
| #2747 a player's level → #2784 | yes: `PlayerRef::level`/`dimension` (`player_level`/`player_dimension`); this mod already reads it since `cf0113879e` |
| #2797 the hand on `entity_interact` | yes: `EntityInteractEvent.hand` (`12eb2e7ad3`, rollup 21) |
| #2806 a completed player break (Fabric `PlayerBlockBreakEvents.AFTER`) | yes, as the fork's own `player_destroyed_block` hook: after `Level.removeBlock`, before `playerDestroy`'s drops, with the removed state and the block entity's saved data |
| #3039 entity selectors in commands | yes: `CommandArgumentValue::{Entity, Entities, Players}` |
| #3040 the old player on `player_clone` | yes: `PlayerCloneEvent.original` and the `original_player_*` reads (uuid, position, dimension, level, inventory slot, health, experience, effects, attachments) |
| #3273 `server_starting` after the levels load; the world folder and game rules before them | **no**: the issue is open; `ConfigServerHost::world_dir` is `None` until a world is open, and the mod's server-start facts still log `world folder unknown, 0 game rules` |
| #3089 which fluid a container item holds | yes, client side: the `contained_fluid` hook |
| `ServerTickEvent.hasTime`/`LevelTickEvent.hasTime` | new: `ConfigHost::server_has_time`. Wired: `server_tick` and `level_tick_pre`/`post` read it when they fire and `hasTime()` answers it (before, `true` always). **Departure:** read once per firing, where NeoForge calls its `BooleanSupplier` (`this::haveTime`, a clock comparison) on every `hasTime()` |

## Pin bump to `cf0113879e`

The survey the bump from `1376e3bcb6` was made from (`git log 1376e3bcb6..cf0113879e -- crates/horizon-mod-api
crates/horizon-mod-vocab crates/horizon-mod-package crates/cargo-horizon-mod docs/native-mods.md
docs/modding`, then the diffs). It changed nothing this mod must rewrite; the pin alone moves it.

**Crate and path moves.**

- The mod API's trait, vocabulary, host traits, wire types and hook declarations moved verbatim to a
  new `crates/horizon-mod-vocab` (library `mod_vocab`; `a5ca9a72fd`, "Move mod vocabulary sources
  verbatim", followed by the dispatch dependency closure in `fbcc919f8c` and the facade wiring in
  `3e98e1cc21`). `horizon-mod-api` is now a 13-line facade: `pub use mod_vocab::*;` plus the wasm
  `wire` module. Its `extern crate self as mod_api` makes the re-export transparent, so every
  `mod_api::<name>` path this mod uses (`Hooks`, the events, `Hand`, `InteractionResult`, `Text`,
  `RegistrationContext`, `Block`/`Item`/`Rarity`/`Tag`, `#[mod_api::horizon_mod]`,
  `__private::game::RegistrationContextNew`) still resolves. The mod still depends on
  **`horizon-mod-api`**, not on `horizon-mod-vocab` directly: the crate docs
  (`docs/modding/src/how-to/native-mods/016-the-mods-crate.md`) and both builders name that package.
  The launcher rewrites the `horizon-mod-api` dependency to `../../crates/horizon-mod-api`
  (`launcher/src/native_mods/api.rs`, `PACKAGES` unchanged); that crate's own path dependency
  `../horizon-mod-vocab` resolves inside the fork checkout, which `cargo horizon-mod` and the
  launcher both have. No `mod/Cargo.toml` dependency change was needed.
- `horizon-platform` (new, `3e98e1cc21`) is the layer-split facade. It is not in the native mod
  build: `PACKAGES` in the launcher still lists only `horizon-mod-api`, `horizon-mod-points` and
  `horizon-internals`.

**API renames.** None on the surface this mod implements or calls. The `Hooks` trait, every hook
file it implements, and its event types are byte-identical between the two commits (`git diff
--find-renames` reports only additions: `hooks/entity_invulnerability_check.rs` is a new hook this
mod does not listen to, and `hooks/notes.rs` loses four lines of comment). What did change:

- `RegistrationContext::refuse(&self, reason: &str)` is new (`parts/owned_namespaces.rs`): a mod can
  refuse its own registration with a bounded reason (first wins, 4096 UTF-8 bytes, truncated at a
  character boundary). The registrar rolls back that mod's declarations from a checkpoint
  (`parts/registration_refusal.rs`) and records `"mod <id> refused registration: <reason>"`. This
  mod never calls it, so nothing changes.
- The wasm guest's `world::phase::install_guest_packages` is now `install_guest_packages` at the
  root; wasm-only, unused here.
- `parts/menus.rs` moved out of the facade into the vocabulary crate, and a mod's menu declarations
  are now validated (`menu_books::validate_registered`, called from
  `game/modding/mods/mod.rs::register_menu_types`, which is now fallible). This mod declares no
  menus.

**New fences and validation.**

- **Namespace ownership is unchanged.** `parts/owned_namespaces.rs` only gained `refuse`; a native
  mod may still declare content only in its manifest id and `namespaces`, and `mod/src/content.rs`
  still skips other namespaces and points at the bake. `Content::check_namespace`,
  `accept_declared` and the content package reader are untouched (no diff in
  `crates/horizon-mod-package/src/content*`), so a baked `.hmod` is read exactly as before:
  `id` = the jar namespace, `api = "0.2"`, `side = "both"`, `capabilities = ["registries"]`.
- **Manifest: mod points only.** `Manifest::parse` gained `mod_point_dependency`: a wasm
  `internals` listener may be backed by a declared dependency whose mod id (with `-` as `_`) starts
  its `at`, and a mod point library's crate must be named after its mod id (`Refusal::LibraryCrate`).
  This mod is native, declares no `[inject]`, no `[internals]` and no `[dependencies]`, so neither
  rule is reachable. `capabilities = ["registries"]` is still required and still the only spelling.
- **Mod self-refusal** (above) is the only new admission path, and it is opt-in.
- **Menu layouts** are refused on a declaration mismatch instead of published half-decoded; no menu
  declarations here.

**Dependencies.** `zip 9.1.3` and `serde_json 1.0.151` are still Horizon's locked versions, so
`mod/Cargo.toml`'s `=` pins are unchanged. `MIXINS_REV` is still
`5a64af938ec86b00d53d16a408aabc0038fb3e57`.

**Build check.** A game built from `cf0113879e` unpatched (`crates/native-game` generated as the
launcher does, `mod/` staged with `horizon-mod-api` rewritten to `../../crates/horizon-mod-api`)
compiles `horizon-java-mod 0.1.0` with no source change: the re-export covers the moved vocabulary.

**In-game proof.** One run of that game (log's own line `Horizon 0.1.0`; fork worktree HEAD
`cf0113879ebee600bff86070466f75b2db90b40a`, no edit to the checkout), six fixture jars plus the
baked `m3_fixture.hmod`, offscreen test terrain:

```
Horizon Java: m0_fixture.jar (m0_fixture) ran-clean (11 host calls, 0 unknown host calls)
Horizon Java: m3_fixture.jar: content: 1 block(s), 1 item(s) in namespace m3_fixture not declared: not owned by horizon_java; run tools/bake.py
Horizon Java: content: 0 block declaration(s) from 6 jar(s)
Horizon: content from 2 mod(s) in .horizon-cache/mod-content/<stamp>
```

`horizon-ctl` / the log then showed, in order:

```
command "javahello Viola"                {"chat":["Hello, Viola!"]}
command "give @s m3_fixture:ruby 1"      Gave 1 [Ruby] to Player00316
select m3_fixture:ruby                   hotbar icon; screenshot ruby-hotbar.png
command "setblock 0 71 7 m3_fixture:ruby_block"   Changed the block at 0, 71, 7
break 0 71 7                             {"block":"minecraft:air","block_before":"m3_fixture:ruby_block","drops":{"m3_fixture:ruby":1}}
inventory                                counts {"m3_fixture:ruby":3} after collect
break 1 71 7 (stone)                     {"block":"minecraft:stone","drops":{}} and block 1 71 7 still stone
  Horizon Java: m2c_fixture: m2c: kept stone at 1,71,7
  Horizon Java: m2c_fixture: m2c: break canceled at 1,71,7
interact-entity 7 (both hands empty)     Horizon Java: m2d_fixture: m2d: interact hand=null id=7
  Horizon Java: use_entity (dedicated=false): 1 callback(s) ran, 0 failed, 1 unknown host call(s)   (twice)
kill @e[type=minecraft:cow]              Horizon Java: m2d_fixture: m2d: after death id=7
  Horizon Java: living_after_death (dedicated=false): 1 callback(s) ran, 0 failed, 0 unknown host call(s)
```

Every jar's report row is `ran-clean`; `m2d_fixture`'s `callbacks` is
`{"ran":3,"failed":0,"unknown_host_call_count":2}` (the two empty-hand reads), and `m3_fixture`'s
`content.skipped` is `[{"namespace":"m3_fixture","blocks":1,"items":1}]` while the baked package
supplies them.

## Pinned fork commit

The mod targets **`arismoko/horizon` at `1cc882b8e089a240354d142e3fd115938ce140d7`** (2026-11-13),
recorded in `mod/horizon-mod.toml`'s `[native] horizon` and in `mod/Cargo.toml`'s
`horizon-mod-api` dependency. Why that commit and not `main`'s head:

- it is the newest fork `main` commit whose required CI jobs passed when the bump was made (see
  "Pin bump to `1cc882b8e0`"), so a fresh clone of the shared checkout reproduces the build exactly;
- the launcher downloads source archives from `arismoko/horizon`
  (`horizon_mod_package::index::native::PREBUILT_REPO`), so a commit of that repository is the only
  kind of pin it can fetch;
- a native mod pins one exact revision on purpose (Horizon's own
  `docs/modding/src/how-to/native-mods/005-native-mod-source-revisions.md`): hooks, host methods and
  their call sites are the game's source, and a later game may not build the mod unchanged.

This commit replaced `cf0113879e` (2026-11-11), which replaced `1376e3bcb6` (2026-11-10), which had
replaced `c4bcfcca5e` (2026-11-07). The earlier bumps picked up our own merged fork changes:

- from `c4bcfcca5e`, #2591 `level_tick_pre`/`level_tick_post` on `HostSide`/`LevelRef`, #2592 the
  `living_death_post` hook, #2626 the load-owned registry key tables;
- from `1376e3bcb6`, the mod API's own renames (`Command`, `Authority`, `InteractionResult`,
  `BreakBlockEvent.state` as a `BlockState`, a level tick's dimension, `Text::translate` by value);
- from `cf0113879e`, the vocabulary move to `horizon-mod-vocab` and the self-refusal, mod-point and
  menu-layout changes surveyed below;
- this one carries `ConfigHost::game_version`, which the jars' hosts are now seeded with, and the
  seams listed in its survey above.

The M0–M3b proofs were re-run on this pin, unpatched, with the game's own version line.

The game version is pinned beside it as `game = "26.3"`, and `mixins` at the fork's
`MIXINS_REV` (`e59da9094f0c2fdda6c627fe9eff22dec6a906f0`) because the manifest requires all three.
The mod runs no mixins yet.

The vendored runtime in `mod/src/runtime/` is the **horizon-java** runtime of the same commit:
re-vendor it with `python3 tools/vendor.py` in every commit that changes the runtime.

## One crate, vendored

A native mod cannot have a path dependency, so `crates/horizon-native-java/src` is copied into
`mod/src/runtime/`:

```sh
python3 tools/vendor.py          # write (or refresh) mod/src/runtime
python3 tools/vendor.py --check  # the copy is exactly what that writes from the runtime source now
```

- **The mod carries the runtime of its own commit.** Every commit that changes
  `crates/horizon-native-java/src` re-vendors in the same commit, and CI runs `--check` on every PR
  and push. `--check` regenerates the copy in memory and compares it byte for byte with
  `mod/src/runtime/`, so one check catches a hand edit to the copy, a runtime change that was not
  re-vendored (an embedded data file included), a missing file and a stray one.
- **Vendored files are committed.** A clean clone builds the mod with no generation step, and
  `tests/test_vendor_mod.py` proves a one-byte hand edit, an un-vendored runtime change and an
  orphaned vendored file each fail `--check`.
- **No lock file.** `mod/vendor.lock` (per-file SHA-256s and a source commit) used to pin the copy
  instead. It changed in nearly every runtime PR, so any two runtime PRs conflicted on it, and it
  guarded nothing the regenerate-and-compare check does not, so it was removed.
- `lib.rs` becomes `runtime/mod.rs` and every `crate::` path becomes `crate::runtime::`, because the
  runtime is no longer a crate root but a module of the mod's crate.
- `main.rs` (the command-line probe) is not vendored.
- The vendored root module silences `dead_code` and `unused_imports`: the runtime keeps an item for
  every capability the probe, the corpus tools and the weaver use, and a mod calls a handful.

## Dependencies

A native mod may only use crates and versions that the game's `Cargo.lock` already resolves
(`docs/modding/src/how-to/native-mods/016-the-mods-crate.md`). `horizon-native-java` therefore pins
**`zip = 9.1.3`** and **`serde_json = 1.0.151`**, the fork's versions, instead of the `zip 2` /
`serde_json 1.0.154` they were built with. Zip 9 reports an entry name its encoding cannot decode as
an error (`ZipResult<Cow<str>>`), so both crates skip such an entry; nothing else in the jar-reading
code changed, and every existing test still passes.

`mod/Cargo.toml` names the same three crates (`horizon-mod-api`, `serde_json`, `zip`, plus `log`).

## Build

The documented route is Horizon's launcher, which stages the crate into the game's source revision,
downloads the pinned commit and compiles it with its own Rust:

```sh
# docs/modding/src/how-to/native-mods/028-what-the-launcher-does.md
horizon-launcher native-build mod/ --server-output OUT --trust-native-code
```

**That route is blocked on a machine with no installed Horizon release** (see
[What is blocked](#what-is-blocked)): `native-build` needs an installed, prepared version to reuse
the game's files from (`native_mods::pipeline` → `backend::need_prepared(base)`, and the base comes
from `native_mods::base_for(None, &backend::installed(), ..)` → `backend::no_version_installed()`).

The equivalent, done by hand, is what the launcher does after staging, and is what built the proof
below (`$FORK` is a checkout of `arismoko/horizon` at the pinned commit):

```sh
cd $FORK
mkdir -p crates/native-game/src native-api
cp -r <horizon-java>/mod native-api/horizon_java
# the launcher rewrites the mod API dependency to the staged game source
sed -i 's|horizon-mod-api = { git = .*|horizon-mod-api = { path = "../../crates/horizon-mod-api" }|' \
  native-api/horizon_java/Cargo.toml
# the generated executable: crates/native-game/{Cargo.toml,src/main.rs} from
# crates/horizon-client/src/native_main.rs.template, with one MOD_ENTRIES line
# ("horizon_java" => mod_0::hooks) and one BUILT_MODS row; both packages added to the
# workspace members. (launcher/src/native_mods/api.rs does this in code.)
cargo build -p horizon-native-game            # CARGO_TARGET_DIR outside the checkout
```

Then the game finds the mod's flat project beside its executable, where the launcher puts it
(`docs/modding/src/how-to/native-mods/006-runtime-packages-and-identity.md`):

```sh
cp -r <horizon-java>/mod <target-dir>/debug/horizon-native-mods/horizon_java
```

## Run

Everything in a scratch folder, with fresh homes (Horizon's own developer acceptance workflow,
`docs/modding/src/how-to/native-mods/036-developer-acceptance-workflow.md`):

```sh
SCRATCH=/tmp/m0-env; mkdir -p $SCRATCH/{game-home,launcher,data,config,trash,clipboard,desktop,game-dir}
export HORIZON_HOME=$SCRATCH/game-home HORIZON_LAUNCHER_HOME=$SCRATCH/launcher
export XDG_DATA_HOME=$SCRATCH/data XDG_CONFIG_HOME=$SCRATCH/config
export HORIZON_LAUNCHER_TRASH=$SCRATCH/trash HORIZON_LAUNCHER_CLIPBOARD=$SCRATCH/clipboard
export HORIZON_LAUNCHER_DESKTOP=$SCRATCH/desktop
export HORIZON_LAUNCHER_NO_SELF_UPDATE=1 HORIZON_NO_AUDIO=1

# a jar to run, compiled here and never committed
python3 tools/java_mod_fixture.py --out $SCRATCH/game-dir/config/horizon_java/java-mods

cd $SCRATCH/game-dir
$TARGET/debug/horizon-client --game-dir $SCRATCH/game-dir \
  --resources $PREPARED/26.3 --offscreen --world M0 --test-terrain \
  --frames 200 --timeout-seconds 240
```

`--resources` is a prepared 26.3 install (`horizon-prepare`; on this machine
`<horizon>/.local/horizon-client/26.3`). `--offscreen --world M0` opens a world on the integrated
server thread, which is what fires `server_about_to_start`; `--frames` bounds the run.

### What the proof shows

M0, an empty `java-mods/`:

```
Horizon Java 0.1.0: client setup
Horizon Java 0.1.0: server about to start (dedicated=false); 0 jar(s) in config/horizon_java/java-mods: 0 ran-clean, 0 ran-hollow, 0 ran-partial, 0 refused; report in config/horizon_java/java-runtime.json
```

and `config/horizon_java/java-runtime.json` is `{"jars": [], "schema": 1}`.

M1, with the fixture jar:

```
Horizon Java: m0_fixture.jar (m0_fixture) ran-clean (11 host calls, 0 unknown host calls)
Horizon Java 0.1.0: server about to start (dedicated=false); 1 jar(s) in config/horizon_java/java-mods: 1 ran-clean, 0 ran-hollow, 0 ran-partial, 0 refused; report in config/horizon_java/java-runtime.json
```

M1, with a file that is not a jar and a client-only jar:

```
Horizon Java: broken.jar (broken) refused (0 host calls, 0 unknown host calls, reason: not a readable jar: Zip("invalid Zip archive: Could not find EOCD"))
Horizon Java: client_only_demo.jar (client_only_demo) refused (0 host calls, 0 unknown host calls, reason: client-only entrypoint: a server never runs it)
```

## What is blocked

- **The launcher route.** `horizon-launcher native-build mod/ --server-output OUT
  --trust-native-code` starts by resolving the version whose game files the build reuses. With no
  installed version it fails with `No version is installed yet. Run `horizon-launcher install
  latest` to get one.` (`backend::no_version_installed`), and an installed version must then be
  *prepared* from the owner's Minecraft (`backend::need_prepared`). The machine this was built on
  has neither, so the staged hand build above stands in for it. Everything after staging is the
  launcher's own code path, and the built game is the game.
- **A compiled mod's package must exist beside the executable.** The game validates the mod's flat
  project at `<executable dir>/horizon-native-mods/<id>/` before installing hooks
  (`006-runtime-packages-and-identity.md`); without it startup is refused with
  `Compiled native mods cannot be loaded: horizon_java 0.1.0: ...: No such file or directory`.
  The launcher writes that folder, so a hand build has to copy it.
- **`capabilities` cannot be empty.** `Manifest::parse` requires exactly `["registries"]` for every
  mod today, so `mod/horizon-mod.toml` declares it although M0/M1 register nothing (the manifest
  comment says so). A purely behavioural mod with no registries has no spelling yet: a **candidate
  issue for the fork**, not something this repository can fix.

## Next milestones

1. **Game bindings over the mod API's hooks.** Give the runtime a `Host` that answers from the game
   instead of `StdHost`: blocks, items, entities, levels, server tick and player events through
   `ServerHost`/`ClientHost`, so a jar's registrations and listeners reach the world. Each hook that
   maps to a game hook site is one hook row in `[inject]`-free form: `Hooks` methods the mod
   implements and forwards to the runtime's event table.
2. **Content assets.** Blocks and items a jar registers are in the world (M3), and its models,
   textures, lang, loot and recipes now bake into a content `.hmod` beside it, which declares them
   too (M3b). Mob effects, entity types and menus still have no native declaration hook.
3. **Mixins as hook rows** rather than a weaver: bind the plans the runtime discovers to the game's
   named hook sites, as `docs/modding/src/how-to/native-mods/011-named-hook-points.md` describes,
   and leave the bounded weaver for jars that need their own classes rewritten.
