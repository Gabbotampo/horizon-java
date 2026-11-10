# Running inside Horizon: the native mod

`mod/` is a Horizon **native mod**: the first way this runtime runs inside the game rather than in
the probe. It is built against the game-side mod API on
[arismoko/horizon](https://github.com/arismoko/horizon) (crate `horizon-mod-api`, library
`mod_api`), which is where that API lives.

Nothing in `crates/horizon-native-java` or `crates/horizon-jvm` moves or changes shape for this:
the mod carries a copy of the runtime (`tools/vendor.py`), because a native mod is one flat crate.

## What M0 to M3 do

| Milestone | Behaviour |
|---|---|
| **M0, the shell** | `Hooks::server_about_to_start` and `Hooks::client_setup` log one line each. The server start writes this mod's report, `config/horizon_java/java-runtime.json`, through the mod API's own config service (`Host::config_files`), so the game files it under `config/` beside every other mod's. With no jars it is `{"schema":1,"jars":[]}`. |
| **M1, run jars** | Every `.jar` in `config/horizon_java/java-mods/` is read as bytes (at `server_about_to_start` until M3 moved the read to the content phase, with `std::fs`; see M3), loaded by the runtime from memory (`NativeJar::from_bytes`) and run with its own `StdHost`. One log line per jar, one row in the report: the jar's id, `ran-clean` / `ran-hollow` / `refused`, its host-call and unknown-host-call counts, and the refusal reason. |
| **M2a, lifecycle** | Each jar's `StdHost` (heap, statics, recorded listeners) is kept for the server's life. `server_about_to_start` fires the Fabric `SERVER_STARTING` and NeoForge `ServerAboutToStartEvent` listeners, `server_starting` the NeoForge `ServerStartingEvent` ones, `server_started` the Fabric `SERVER_STARTED` and NeoForge `ServerStartedEvent` ones, and `server_stopping`/`server_stopped` their own. Each hook logs callbacks run/failed and unmodelled host calls; every row's `callbacks` object totals them; a jar's `System.out` lines reach the game log with its id. |
| **M2b, commands** | At `register_commands`, the recorded Fabric `CommandRegistrationCallback` (and NeoForge `RegisterCommandsEvent`) listeners build their Brigadier tree on the runtime's façade, and its literal, `word`/`text` and `integer` nodes become typed `NativeCommand` usages. `run_mod_command` maps the game's invocation back (deepest matching usage), runs the Java `Command.run`, and sends `sendSuccess`/`sendFailure`/`sendSystemMessage` replies through `Commands::feedback` (or `PlayerRef::tell` for a player the command named). Any other node — other argument kinds, redirects/forks, an unevaluable requirement — is refused by name in the log and left out. |
| **M2c, game events** | Every other modelled server event reaches its matching mod API hook, moment for moment: server and level ticks, level load/unload, player join/leave/respawn/clone/dimension change, entity join/leave, living damage before/after, item and block use, block and entity attack, and block break (whose cancel also fires Fabric's `CANCELED`). A cancellable hook carries the firing's cancel and, where the hook has one, the Fabric `InteractionResult` back as the hook's cancellation result. The full table, arguments and departures: [docs/mod-hooks.md](mod-hooks.md). |
| **M3, content** | The jars now run at the game's **Registration** phase, before any resource or world loads and while the content root can still grow, instead of at `server_about_to_start`. Each `RegistryDeclaration::{Item, Block}` the runtime recorded becomes a native content declaration (`Hooks::blocks`/`items`, NeoForge's `RegisterEvent`): an item keeps the template and the properties the content vocabulary carries, and a block brings its block item. The game joins them under the jar's namespace, so `/give`, `setblock` and the inventory see `<jar id>:<path>`. A jar's own assets, lang and loot need the bake route in [m3-content-design.md](m3-content-design.md). |

The verdicts are the ones the corpus ledgers already use:

- **`ran-clean`** — the server entrypoints ran and no host call was unmodelled.
- **`ran-hollow`** — they ran, but at least one host call was answered neutrally
  (`unknown_host_call_count`).
- **`refused`** — unreadable jar, no server-side entrypoint (a client-only jar is named as such),
  an unsupported entrypoint adapter, or an entrypoint that failed; `reason` says which.

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

## M3 proof

With `m3_fixture.jar` (built by `tools/java_mod_fixture.py --kind content`) in
`config/horizon_java/java-mods/`, one integrated-server run shows the jars run before content is
built and both declarations reach the game:

```
Horizon Java: m3_fixture.jar (m3_fixture) ran-clean (9 host calls, 0 unknown host calls)
Horizon Java: content: 1 block declaration(s) from 1 jar(s)
Horizon Java: content: 1 item declaration(s) from 1 jar(s)
Horizon: content from 2 mod(s) in .horizon-cache/mod-content/<stamp>
Horizon Java 0.1.0: server about to start (dedicated=false); 1 jar(s) in config/horizon_java/java-mods: 1 ran-clean, 0 ran-hollow, 0 refused; report in config/horizon_java/java-runtime.json
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

## Fork seams needed

- **A player's dimension for command sources.** `CommandSourceStack.getLevel()` has no mod API read
  to answer it: `PlayerRef` exposes identity, permission and messages but no level. Fabric/NeoForge
  mods read it in command bodies; today every player source answers the overworld.
- **`living_after_death`.** The runtime records Fabric `ServerLivingEntityEvents.AFTER_DEATH`, which
  has no NeoForge counterpart event; `living_death` is NeoForge's `LivingDeathEvent`, before the
  death. Add a listen-only hook at `LivingEntity.die`'s return.
- **`block_break_after`.** The runtime records Fabric `PlayerBlockBreakEvents.AFTER`; `break_block`
  is NeoForge's `BreakBlockEvent`, before the block goes. Add a listen-only hook after the block
  and its drops are gone.
- **A dimension on the block and entity action hooks.** `break_block`, `left_click_block`,
  `attack_entity` and `entity_interact` do not name the level (or, for the entity hooks, the acting
  player), but a Fabric `Level` argument needs it. Add `dimension: &str` (and the acting
  `player: u128`) to those events; today the callback gets a marker whose level calls are counted
  unknowns.
- **An item's id from an `ItemStack`/`ItemKey`.** Hooks give an `ItemStack` but no read turns its
  key back into `minecraft:diamond`; the Java callback's item, and a NeoForge `getItemStack()`,
  cannot be filled. Add `Host::item_id(ItemKey) -> Option<&str>`.
- **The hand on `entity_interact`.** The hook fires once per hand but the event carries no `hand`,
  so the Java callback always sees the main hand. Add `hand: Hand` to the event.

## Pinned fork commit

The mod targets **`arismoko/horizon` at `c4bcfcca5effdfa7d6bd759eeae60550c48ab5ac`** (2026-11-10),
recorded in `mod/horizon-mod.toml`'s `[native] horizon` and in `mod/Cargo.toml`'s
`horizon-mod-api` dependency. Why that commit and not `main`'s head:

- it is the fork's `main` the day this was written, so a fresh clone of the shared checkout
  reproduces the build exactly;
- the launcher downloads source archives from `arismoko/horizon`
  (`horizon_mod_package::index::native::PREBUILT_REPO`), so a commit of that repository is the only
  kind of pin it can fetch;
- a native mod pins one exact revision on purpose (Horizon's own
  `docs/modding/src/how-to/native-mods/005-native-mod-source-revisions.md`): hooks, host methods and
  their call sites are the game's source, and a later game may not build the mod unchanged.

The game version is pinned beside it as `game = "26.3"`, and `mixins` at the fork's
`MIXINS_REV` (`5a64af938ec86b00d53d16a408aabc0038fb3e57`) because the manifest requires all three.
The mod runs no mixins yet.

`mod/vendor.lock` records the **horizon-java** commit the vendored runtime was copied from, with a
SHA-256 per file. Update it with `python3 tools/vendor.py` after changing the runtime.

## One crate, vendored

A native mod cannot have a path dependency, so `crates/horizon-native-java/src` is copied into
`mod/src/runtime/`:

```sh
python3 tools/vendor.py             # write mod/src/runtime and mod/vendor.lock
python3 tools/vendor.py --check-lock  # the copy is what mod/vendor.lock says (no hand edits)
python3 tools/vendor.py --check       # ...and it matches the runtime source as it is now
```

- **The mod carries the runtime commit `mod/vendor.lock` records**, not whatever the runtime says on
  every later commit. The two checks say which is which, so a runtime PR is not taxed:
  - `--check-lock` always applies (the Python tests run it, and so CI): it fails on a hand edit to
    `mod/src/runtime/`, never because the runtime crate changed.
  - `--check` is required of a PR that changes `mod/**` or `tools/vendor.py`; CI runs it only for
    those (`.github/workflows/ci.yml` diffs the PR's changed files against its base) and on `main`
    as a non-blocking warning. **Refresh in a mod PR, and before a mod release, by re-running
    `python3 tools/vendor.py`.**
- **Vendored files are committed.** A clean clone builds the mod with no generation step, and
  `tests/test_vendor_mod.py` proves a one-byte edit fails `--check-lock` while a runtime-only change
  fails only `--check`.
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
Horizon Java 0.1.0: server about to start (dedicated=false); 0 jar(s) in config/horizon_java/java-mods: 0 ran-clean, 0 ran-hollow, 0 refused; report in config/horizon_java/java-runtime.json
```

and `config/horizon_java/java-runtime.json` is `{"jars": [], "schema": 1}`.

M1, with the fixture jar:

```
Horizon Java: m0_fixture.jar (m0_fixture) ran-clean (11 host calls, 0 unknown host calls)
Horizon Java 0.1.0: server about to start (dedicated=false); 1 jar(s) in config/horizon_java/java-mods: 1 ran-clean, 0 ran-hollow, 0 refused; report in config/horizon_java/java-runtime.json
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
2. **Content assets.** Blocks and items a jar registers are in the world (M3), but the jar's own
   models, textures, lang, loot and recipes are not: a bake writes them into a content `.hmod`
   beside the declarations ([m3-content-design.md](m3-content-design.md)). Mob effects, entity
   types and menus still have no native declaration hook.
3. **Mixins as hook rows** rather than a weaver: bind the plans the runtime discovers to the game's
   named hook sites, as `docs/modding/src/how-to/native-mods/011-named-hook-points.md` describes,
   and leave the bounded weaver for jars that need their own classes rewritten.
