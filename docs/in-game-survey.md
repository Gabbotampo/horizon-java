# Running real Java mods in game: the first 40-jar survey

This is the first in-game measurement of **real** Fabric and NeoForge jars (not fixtures). Forty
jars from the three corpus runs were loaded one batch at a time into a game built from the pinned
commit, each batch played for about 40 seconds (600+ ticks) through `horizon-ctl`, with no change
to the game source or to `mod/`.

The short version: **33 of 40 jars ran clean, 5 ran hollow, 2 were refused, and one Java command
out of 133 offered usages actually ran.** The game stayed playable in all ten runs (each walked and
broke a block), but most jars did nothing observable in the world, and the two biggest failure
families are the missing player/level read and the lack of cross-jar linking.

## The game build

| | |
|---|---|
| Game source | `arismoko/horizon` at `1376e3bcb68b771373ea75637b0201381ccfa6bd` (the pin in `mod/horizon-mod.toml`), unmodified |
| Game's own version line | `Horizon 0.1.0` (the pinned game logs no commit field; the checkout's `HEAD` above is the build) |
| Mod source | `horizon-java` `main` at `e0d0f1f66d8b1899b5235d06ae2736e0910a5fa5`, `mod/` copied with `tools/vendor.py`'s layout |
| Runtime the mod carries | `main`'s `crates/horizon-native-java` (`python3 tools/vendor.py --check` passes). `mod/vendor.lock`'s `source_commit` still says `45e5a7e5d8`, which is a stale record, not a different runtime: its per-file `source_sha256` values are `main`'s |
| Build | staged hand build (`docs/mod-plan.md`, "Build"): `native-api/horizon_java` + generated `crates/native-game`, `cargo build -p horizon-native-game`, dev profile |
| Resources | a prepared 26.3 install (`.local/horizon-client/26.3`), `--offscreen --control` |

One deviation from the launcher's generated executable is recorded here: the launcher template
writes `mod_0::hooks` for a mod's entry point, and the crate at this commit exports
`HorizonJava` (which implements `Hooks`) but no `hooks()` function. The staged executable binds
`Box::new(mod_0::HorizonJava::default())` instead. The game source is untouched; this is the
generated file, and it is the only difference from what the launcher would write.

## Method

Per batch:

1. A fresh scratch tree (`game-home`, `launcher`, `data`, `config`, `trash`, `clipboard`,
   `desktop`, `game-dir`) under `/tmp`; `HORIZON_HOME`, `HORIZON_LAUNCHER_HOME`, `XDG_DATA_HOME`,
   `XDG_CONFIG_HOME`, `HORIZON_LAUNCHER_*`, `HORIZON_LAUNCHER_NO_SELF_UPDATE=1`,
   `HORIZON_NO_AUDIO=1`. Nothing under `~/.local/share/horizon*` or `~/.local/bin` is touched.
2. The jars are copied into `<game-dir>/config/horizon_java/java-mods`, then
   `tools/bake.py --probe <horizon-native-java> --game-dir <game-dir>` writes the content packages
   (see *There were no content jars to sample*).
3. The game starts in its own memory-capped systemd unit
   (`MemoryMax=10G`, `MemorySwapMax=0`) with `--game-dir <scratch> --offscreen --creative
   --control --control-socket <sock>`; one game build serves every batch.
4. `horizon-ctl wait-ready`, then the same actions in every batch: `setblock` dirt at feet + 2 and
   `break` it; `give minecraft:bread 2` + `select bread` + `use-item --hold 1.7`; `move forward 2`;
   one command for each jar that registered usages; `wait 600` (about 30 s of ticks); `quit`.
5. Results come from `<game-dir>/config/horizon_java/java-runtime.json` (written at server start and
   rewritten at `server_stopping` with the callback/command totals) and the game log.

Verdicts are the mod's own: `ran-clean` = every entrypoint stage ran with zero unmodelled host
calls; `ran-hollow` = every stage ran with at least one; `ran-partial` = at least one stage ran and
at least one failed, so the running stages' listeners stay active and `reason` names the failures;
`refused` = the jar could not run, or every stage failed, and `reason` says why.

## The sample

The census at `e0d0f1f` (`docs/unmodelled-calls.md`, rerun with `tools/unmodelled_calls.py`)
labelled **172 jars `ran-clean`**: 51 in `corpus-xl`, 84 in `mega`, 37 in `neoforge`. The sample was
drawn from that run. (Re-measured on 11 November 2026 with the interpreter-honesty fixes and every
server stage, the census labels **112** `ran-clean`: 31, 59 and 22; see
[docs/unmodelled-calls.md](unmodelled-calls.md#re-measured-11-november-2026).) The sample is:

- **All 22 distinct mods among the 172 that register commands or listeners** (26 rows
  collapse to 22 mods because some jars appear in two corpora). Every run-clean jar with an
  observable surface is included.
- **2 declared dependencies that are themselves mods** and are in the corpus: `konkrete` (for
  `fancymenu`, `ran-clean`) and `collective` (for `stackrefill`, `ran-hollow` with 67 unknown
  calls even in the census; it is here because it is a dependency, not because it is clean). `fabric-api` and `libjf` are excluded: the
  census labels them `skipped` (library, no server-side entrypoint), and the game does not link
  jars anyway.
- **16 control jars with no hooks at all**, spread over both loaders and all three corpora (server
  libraries such as `Bookshelf`, `cloth-config`, `yacl`, `PacketFixer`, server-side utilities such
  as `krypton`, `asynclogger`, `c2me`, `fast-ip-ping`), to see what "loaded without errors" is
  worth on its own.

The sample stops at 40; the other 130 run-clean jars register nothing and would only repeat the
control rows. Every jar is 26.3-era and was selected from the census, not from its name.

**Three dependencies added, 12 November 2026 (43 jars).** Survey #3 refused three sampled jars
for a dependency the sample left out. Each dependency is already in the corpus and now sits in
its dependent's batch:

| added | for | batch | corpus file | licence |
|---|---|---|---|---|
| `melody` | `fancymenu` (beside `konkrete`) | b1 | `neoforge/melody_neoforge_1.0.17_MC_26.3.jar` | MIT |
| `libjf` | `respackopts` (`libjf-base`, `libjf-data-manipulation-v0`, `libjf-config-core-v2` are its nested jars) | b4 | `corpus-xl/libjf-26.3.1.jar` | LGPL-3.0-or-later |
| `ResourcefulConfig` | `structure_layout_optimizer` | b10 | `corpus-xl/ResourcefulConfig-6.0.2.jar` | MIT |

The licences are the jars' own metadata, and Modrinth's project pages agree. The corpus is
never committed, so these are run locally only. `libjf` has no entrypoint.

### There were no content jars to sample

None of the 172 run-clean jars declares a block or an item through the runtime's typed registry
vocabulary: the census probe reports `content.blocks = 0` and `content.items = 0` for every one of
them. Registering content is itself behind calls the runtime does not model yet (see *Causes*), so
the "give one of its items" step could not be performed for any jar. The bake step still ran for
all 40 and produced 20 asset-only packages from 18 jars (`lang`, models, textures); the game loaded
them all (`Horizon: content from N mod(s)`), which is evidence that M3b's package route works, but
it says nothing about content declarations.

## Results

`native` is the census verdict (with declared dependencies, as the census links them) followed by
the same jar probed alone with no `--link` (as the game runs it). **Re-measured on 11 November 2026**
with the interpreter-honesty fixes (a call on `null` throws, bodiless and unmodelled instance calls
are counted, casts / `Enum.valueOf` / `Thread.start` are checked or counted; #37, #38, #41) and every
server stage the loader runs (`main`, then `server`): 21 of the 40 rows changed, and
20 are still `ran-clean` in the census (it was 39). The `in game` and later columns are the
batch runs above, from before these fixes; they are not re-run here. `content` is declared/skipped
typed declarations. `commands` is command usages translated/refused at registration. `listeners`
is callbacks that ran/failed over the batch's life. A dash means the jar produced nothing
observable after its entrypoint.

| jar | loader | native | in game | content | commands | listeners | what visibly worked | what failed, and why |
|---|---|---|---|---|---|---|---|---|
| `fancymenu_neoforge_3.9.14_MC_26.3.jar` | NE | refused / refused alone | **ran-clean** | 0/0 | 7 tr, 9 ref | 3/0 | 3 lifecycle listeners fired (server_started, player_join, server_stopping) | `/fmvariable get test` refused: `ServerPlayer#level()`; 9 usages refused (Entity and Bool arguments) |
| `konkrete_neoforge_1.11.1_MC_26.3.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `Jade-mc26.3-NeoForge-26.3.5.jar` | NE | ran-hollow (4) / ran-hollow alone (4) | **ran-clean** | 0/0 | 0 tr, 2 ref | 0/0 | command registration callback ran | both usages refused (Entity argument), so `/jade` is not a registered usage |
| `midnightlib-neoforge-1.10.0+26.3.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `carryon-neoforge-26.3-2.12.1.jar` | NE | ran-hollow (13) / ran-hollow alone (13) | **ran-clean** | 0/0 | 3 tr, 2 ref | 5/660 | 5 callbacks ran (server_started, block/attack/use listeners) | 660 tick callbacks failed on `Player#getData(Supplier)`; `/carryon debug` refused on the same call; 2 usages refused (Entity argument) |
| `fzzy_config-0.7.7+fix3+26.3+neoforge.jar` | NE | ran-hollow (8) / ran-hollow alone (8) | **refused** | 0/0 | 0 tr, 0 ref | 0/0 | — | refused: the client `@Mod` class `FzzyConfigNeoForgeClient` ran on the server and hit "recursion exceeded 12 nested jar frames" |
| `Iceberg-26.3-neoforge-1.4.3.2.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 1/1 | 1 listener ran (level load) | player_join listener failed on `ServerPlayer#level()` |
| `Searchables-neoforge-26.3-1.0.3.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `470193-8893473.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 10 tr, 1 ref | 642/0 | 642 server-tick callbacks ran; 10 usages registered | `/connectivity packetsSummary` answered "Incorrect argument for command"; 1 usage refused |
| `456640-8026052.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 1 tr, 1 ref | 642/0 | 642 server-tick callbacks ran; 1 usage registered | `/limitedchunks listnonplayerchunktickets` refused: `CommandSourceStack#getLevel()`; 1 usage refused |
| `326652-8890593.jar` | FA | ran-hollow (78) / ran-hollow alone (78) | **ran-clean** | 0/0 | 0 tr, 0 ref | 642/0 | 642 server-tick callbacks ran | nothing beyond the entrypoint line |
| `832882-8896904.jar` | FA | ran-hollow (198) / ran-hollow alone (1) | **ran-hollow** | 0/0 | 2 tr, 3 ref | 0/0 | 2 command usages registered | 3 usages refused (Resource arguments); entry hollow on its own `CommonConfiguration#disableLegacyRandomCrashes` field; `/structureessentials showGenerationTimes` is not a registered usage |
| `fc1f2f374c-Chunky-Fabric-1.5.3.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 35 tr, 7 ref | 1/1 | 1 lifecycle listener ran; 35 usages registered | server_started listener failed: interpreter `list index is out of bounds` in `Chunky#<init>`; `/chunky` refused on `ChunkyFabric#chunky()`; 6 usages refused (Dimension argument) and 1 (its permission predicate) |
| `respackopts-26.3.0.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/2 | 2 server_started callbacks attempted | both server_started listeners failed: `WeakSet` collection marker not modelled (`Set#isEmpty`) |
| `c661609de9-modelbrowser-1.5.1.jar` | FA | ran-hollow (3) / ran-hollow alone (3) | **ran-clean** | 0/0 | 3 tr, 3 ref | 0/0 | `/modelbrowser items_always_equippable` ran in Java and answered the player | the command replied with the raw translation key `commands.modelbrowser.gamerule.query`; 3 usages refused (Bool argument) |
| `baguettelib-26.3-Fabric-2.0.7.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/1 | join and leave listeners fired | player_join failed on `ServerPlayer#level()`, player_leave on `ServerPlayer#getUUID()` (the second after the report snapshot) |
| `361579-8918571.jar` | FA | ran-hollow (1) / ran-hollow alone (1) | **ran-clean** | 0/0 | 0 tr, 0 ref | 1/1 | 2 lifecycle callbacks attempted | server_starting listener failed: `MissingMethod("java/lang/Thread#getId()J")`; no command usage translated |
| `mcwifipnp-2.1.4-26.3-fabric.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 2/0 | 2 lifecycle callbacks ran | none of its command usages were translated |
| `1079f41d14-otterlib-0.4.0.2+26.3-fabric.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `bdc7223ca2-collective-26.3.0-8.67.jar` | FA | refused / refused alone | **ran-hollow** | 0/0 | 44 tr, 0 ref | 4350/3 | 4,350 callbacks over 9 events; 44 command usages registered | entry hollow: 67 unknown calls (`Items.*` statics, `RandomSource.create`, its own `EventPhaseData`); 3 listener failures (`MinecraftServer#getWorldPath` twice, `Level#registryAccess`) |
| `9f12a0dffd-stackrefill-26.3.0-5.1.jar` | FA | refused / ran-hollow alone (1) | **ran-hollow** | 0/0 | 0 tr, 0 ref | 0/0 | — | entry hollow: 1 unknown call, `collective`'s `ShouldLoadCheck#shouldLoad` (its dependency is not linked in the game) |
| `d198676d8b-calcmod-1.6.0+fabric.26.3.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | command registration callback attempted | command registration listener failed: unsupported `Locale` marker, so no command was registered |
| `09ee9afdd2-entity-view-distance-1.9.0+26.2.jar` | FA | ran-clean / clean alone | **refused** | 0/0 | 0 tr, 0 ref | 0/0 | — | refused: `Metadata("malformed java/util/Iterator marker")` — it needs its Fabric API dependency, which the game does not link |
| `1b0a4ed5f7-structure_pool_api-fabric-1.3.0+26.3.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 1/0 | 1 server_starting callback ran | nothing beyond one lifecycle callback |
| `supermartijn642configlib-1.1.8-neoforge-mc26.3.jar` | NE | ran-hollow (3) / ran-hollow alone (3) | **ran-clean** | 0/0 | 0 tr, 0 ref | 1/0 | 1 server_starting callback ran | nothing beyond one lifecycle callback |
| `Bookshelf-neoforge-MC26.3-26.3.0.4.jar` | NE | refused / refused alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `cloth-config-neoforge-26.3.159.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `geckolib-neoforge-26.3-5.5.7.jar` | NE | ran-hollow (2) / ran-hollow alone (2) | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `yet_another_config_lib_v3-3.9.7+26.3-neoforge.jar` | NE | ran-hollow (1) / ran-hollow alone (1) | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `ForgeConfigAPIPort-v26.3.1-mc26.3.x-NeoForge.jar` | NE | ran-hollow (1) / ran-hollow alone (1) | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `fast-ip-ping-v1.0.12-mc26.3.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `Clumps-neoforge-26.3-26.3.2.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `PacketFixer-neoforge-3.3.7.jar` | NE | ran-hollow (2) / ran-hollow alone (2) | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `NoChatReports-NEOFORGE-26.3-v2.21.0.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `MossyLib-1.6.0+26.3+fabric.jar` | FA | ran-hollow (1) / ran-hollow alone (1) | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `825dce0c0b-ResourcefulLib-6.0.1.jar` | FA | ran-hollow (2) / ran-hollow alone (2) | **ran-hollow** | 0/0 | 0 tr, 0 ref | 0/0 | — | entry hollow: 2 unknown calls, `Optionull#mapOrElse` |
| `krypton-0.3.2.jar` | FA | ran-partial / ran-partial alone | **ran-hollow** | 0/0 | 0 tr, 0 ref | 0/0 | — | entry hollow: 3 unknown calls (its own `Natives#cipher`/`compress`, `System#setProperty`) |
| `asynclogger-2.2.2+26.1.2-fabric.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `c2me-fabric-mc26.3-0.4.2-alpha.0.106.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `structure_layout_optimizer-1.1.4+26.3-fabric.jar` | FA | ran-hollow (1) / ran-hollow alone (1) | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |


Loader: `FA` = Fabric, `NE` = NeoForge.

## Totals

| Measure | Result |
|---|---|
| Jars | 40 (22 Fabric, 18 NeoForge; 13 from `corpus-xl`, 9 from `mega`, 18 from `neoforge`) |
| In-game verdicts | **33 `ran-clean`, 5 `ran-hollow`, 2 `refused`** |
| Unknown host calls (entrypoints) | **81** (67 `collective`, 7 the refused `fzzy_config`, 3 `krypton`, 2 `ResourcefulLib`, 1 `stackrefill`, 1 `structureessentials`) |
| Callbacks run / failed | **6,291 / 669** |
| Command usages translated / refused | **105 / 28** (of 133 built by the jars' registration callbacks) |
| Commands run / refused at runtime | **1 / 4** |
| Content declarations | **0 blocks, 0 items, 0 skipped** |
| Baked packages | 20 asset-only packages from 18 jars, all loaded by the game |
| Playability | 10/10 runs reached `wait-ready`; a block was placed and broken in all 10; the player moved 2.6–8.9 blocks per run |

### Working versus "loaded without errors"

These are deliberately different categories.

- **Observed in game (18 jars)**: something the jar does was seen in the run — a listener fired
  (counted and logged as an event with its callbacks), a Java command answered the player, or a
  listener/registration failure was logged against it. 12 jars had at least one listener run
  (`fancymenu`, `carryon`, `iceberg`, `connectivity`, `limitedchunks`, `cupboard`, `chunky`,
  `collective`, `spark`, `mcwifipnp`, `structure_pool_api`, `supermartijn642configlib`);
  `modelbrowser` ran one command; `jade`, `structureessentials`, `respackopts`,
  `baguettelib`, `calcmod` registered or attempted commands/listeners and are counted here only
  because their failure is the observed outcome.
- **Loaded without errors (20 jars)**: the entrypoint returned with no unmodelled call, but
  nothing the jar does was observed — no listener fired, no command was registered, no content
  existed. This is a weaker category and is not "working": the hook it would have used may not
  exist, or the event may simply not have happened during the run. Two of the 40 (`fzzy_config`,
  `entity-view-distance`) are refused and are in neither category.

## Top causes

### 1. The player/level reads (`arismoko/horizon#2747`, `Player#getData`, `ServerPlayer#getUUID`)

**662 of the 669 listener failures** and three of the four refused commands are one family: a
listener or command that reads something about the acting player, which the mod API's `PlayerRef`
does not carry.

- `Player#getData(Supplier)` — `carryon`'s `ServerTickEvent.Post` listener reads its attachment
  once per tick: 660 failures, all counted, 100 of them in the log (the game's logger writes at
  most 100 lines per 60 s, then only counts).
- `ServerPlayer#level()` — `iceberg`'s player_join (1 counted), `baguettelib`'s player_join
  (`getUUID` on player_leave is logged after the report snapshot), `/fmvariable get test`
  (`fancymenu`), and `CommandSourceStack#getLevel()` for
  `/limitedchunks listnonplayerchunktickets`.
- `ServerPlayer#getUUID()` — `baguettelib`'s player_leave listener.

The seam is the one `docs/mod-plan.md` already names: no mod API read gives a player's dimension,
and the runtime counts the call rather than guessing the overworld. This is now the largest single
in-game failure family by two orders of magnitude.

### 2. The wrong entrypoint, and no cross-jar linking

Five jars' in-game verdict is worse than the census's. Two different loader choices explain all
five, and neither is a runtime gap:

| jar | census (kind, deps) | game | why |
|---|---|---|---|
| `entity-view-distance` | `server`, Fabric API | refused: `Metadata("malformed java/util/Iterator marker")` | the game runs its `main` entrypoint; `main` fails even **with** Fabric API linked, `server` is clean either way |
| `krypton` | `server`, none | `ran-hollow`, 3 | `main` (`KryptonSharedInitializer`) hits 3 unknowns; `server` (`KryptonServerInitializer`) is clean |
| `ResourcefulLib` | `server`, Fabric API | `ran-hollow`, 2 | `server` is clean with or without its dependency; `main` is not |
| `structureessentials` | `main`, `cupboard` | `ran-hollow`, 1 | with `cupboard` linked the same entrypoint is clean: the missing dependency |
| `stackrefill` | `main`, Fabric API + `collective` | `ran-hollow`, 1 | with `collective` linked it is clean: `ShouldLoadCheck#shouldLoad` is a cross-jar class |

The first three are the mod's entrypoint order: `SERVER_KINDS` is `main, server, neoforge`, so a
Fabric jar that has both a `main` and a `server` entrypoint runs `main`, while the census's
`entrypoint_kind` prefers `server`. `entity-view-distance` is the costly one: its `main` is broken
under the interpreter and its `server` entrypoint is fine.

The last two are the lack of cross-jar linking: the game runs jars one at a time, so a declared
dependency sits in `java-mods/` but not on the jar's class path. The census's "with declared
dependencies" number is therefore not a prediction of in-game behaviour until the mod links jars.

### 3. Client-only `@Mod` classes run on the server

`fzzy_config` is `ran-clean` under the probe and refused in game:

```
me.fzzyhmstrs.fzzy_config.FzzyConfigNeoForgeClient: native interpreter recursion exceeded 12
nested jar frames at .../validation/ValidatedField$Companion#<init>
```

The cause is in the mod, not the runtime: `load_jar` (`mod/src/lib.rs`) picks a kind with
`jar.discover_entrypoints(kind)`, which is the side-`BOTH` discovery, so a NeoForge `@Mod` marked
`dist = CLIENT` is collected and executed on a server. The probe uses
`discover_entrypoints_on(kind, "SERVER")` and never sees it. One line, one jar, a whole verdict.

### 4. The command bridge's argument vocabulary

133 usages were built; **28 were refused by kind**:

| argument kind | usages refused | jars |
|---|---:|---|
| `Entity` | 9 | `fancymenu` (5), `jade` (2), `carryon` (2) |
| `Bool` | 7 | `fancymenu` (4), `modelbrowser` (3) |
| `Dimension` | 6 | `chunky` |
| `Resource` (biome tag) | 3 | `structureessentials` |
| `BlockPos` | 1 | `limitedchunks` |
| `GameProfile` | 1 | `connectivity` |
| permission predicate | 1 | `chunky` (`ChunkyFabric#chunky()`) |

Add the four usages that did register but failed at runtime (`Player#getData`,
`ServerPlayer#level`, `CommandSourceStack#getLevel`, `ChunkyFabric#chunky()`) and one that the
game's parser rejected (`connectivity packetsSummary` → "Incorrect argument for command"), and
exactly one Java command of 133 ran end to end (`modelbrowser`). Three of the four runtime
failures are the player read of cause 1; the fourth is `chunky`'s own static.

### 5. Interpreter and library gaps behind the rest

- `Metadata("list index is out of bounds (at org/popcraft/chunky/Chunky#<init> +101)")` — `chunky`'s
  server_started listener.
- `Metadata("expected a native collection marker, got marker dev/jfronny/commons/ref/WeakSet#2
  (call java/util/Set#isEmpty()Z)")` — `respackopts`'s two server_started listeners.
- `Metadata("unsupported Locale marker (at net/jsa2025/calcmod/commands/CalcCommand#<clinit> +23)")`
  — `calcmod`'s command registration, which is why it offered no command.
- `MissingMethod("java/lang/Thread#getId()J")` — `spark`'s server_starting listener.
- `MinecraftServer#getWorldPath`, `Level#registryAccess` — `collective`'s lifecycle listeners.
- `Items.*` statics + `RandomSource.create` — `collective`'s 67 entrypoint unknowns (game-bound,
  the census's largest family).
- `Optionull#mapOrElse` — `ResourcefulLib`'s 2.
- `System#setProperty` / its own native loader — `krypton`'s 3.

### 6. Reply text loses its translation

`modelbrowser`'s command did run and reply, but the player saw
`commands.modelbrowser.gamerule.query`: the jar's command sends a translation key, the bridge
carries it as a key, and the key is not in a language the client resolved (the jar's own `lang`
file was baked into its package). The reply arrived; its text did not.

## What this survey does not show

- **The entrypoint verdict is not a game-host measurement.** The mod runs each jar with its own
  `StdHost`, exactly like the probe. The in-game verdict therefore matches the offline runtime
  run the same way (no `--link`, side `SERVER`) for 39 of 40 jars; only `fzzy_config` differs, for
  the discovery reason above. What the game adds is hooks, commands and content — not more host
  model for entrypoints.
- **The report is a snapshot at `server_stopping`.** Failures that happen during teardown (a
  player_leave listener, for one) appear in the log but not in the JSON row.
- **The log is flood-limited.** The game writes at most 100 identical warning lines per 60 s and
  then only counts, so `carryon`'s 660 counted failures show as 100 lines.
- **No content, no mixins.** Zero sampled jars declare blocks or items, and no jar's mixins were
  applied by the game; both are measured elsewhere.
- **One loader generation** (Fabric 26.3-era, NeoForge 26.3), one game build (dev profile), about 40 s of
  play, four jars per process.
- **Batches, not isolation.** Four jars share one process; the game runs each jar with its own
  host, but a jar's statics cannot see another's. `fzzy_config` reproduces its refusal alone.

## Reproduce

```sh
# the game, at the pin, unmodified (staged build from docs/mod-plan.md)
git -C <arismoko/horizon> worktree add --detach /tmp/game 1376e3bcb68b771373ea75637b0201381ccfa6bd
cp -r <horizon-java>/mod /tmp/game/native-api/horizon_java
sed -i 's|horizon-mod-api = { git = .*|horizon-mod-api = { path = "../../crates/horizon-mod-api" }|' \
  /tmp/game/native-api/horizon_java/Cargo.toml
# crates/native-game/{Cargo.toml,src/main.rs} as docs/mod-plan.md describes
CARGO_TARGET_DIR=/tmp/target cargo build -p horizon-native-game
cp -r /tmp/game/native-api/horizon_java /tmp/target/debug/horizon-native-mods/horizon_java

# one batch
python3 tools/bake.py --probe /tmp/target/debug/horizon-native-java --game-dir /tmp/run/game-dir
/tmp/target/debug/horizon-client --game-dir /tmp/run/game-dir --offscreen --creative \
  --control --control-socket /tmp/run/ctl.sock --resources <prepared>/26.3
/tmp/target/debug/horizon-ctl --socket /tmp/run/ctl.sock wait-ready 240
/tmp/target/debug/horizon-ctl --socket /tmp/run/ctl.sock break X Y Z
/tmp/target/debug/horizon-ctl --socket /tmp/run/ctl.sock command /<root>
```

The batch driver written for this survey is `tools/in_game_survey.py`; it takes the game, the
control tool, the probe and the jars, and writes one JSON per batch plus the reports and logs.

## Follow-up at the `cf0113879e` pin: entrypoint order

Re-measured 11 November 2026 on the game built from `arismoko/horizon`
`cf0113879ebee600bff86070466f75b2db90b40a` (unmodified; the log's own line is `Horizon 0.1.0`),
the pin `mod/horizon-mod.toml` now records; resources were prepared from the same checkout.
Fabric's `Hooks.startServer` invokes the `main` entrypoint stage and then `server`
(`Hooks.startClient`: `main`, then `client`), but `load_jar` stopped at the first kind that
declared anything.

| jar | before | after |
|---|---|---|
| `entity-view-distance` | refused: `Metadata("malformed java/util/Iterator marker (at eu/pb4/entityviewdistance/EVDMod#onInitialize +17)")` | **ran-partial**, 2 host calls: `main` failed, `server` ran; the row's `reason` names the failed stage. It is not clean: a real Fabric loader would have thrown |
| `krypton` | ran-hollow 3, 7 host calls (main only) | ran-hollow 3, 10 host calls (`server` ran too; `main`'s 3 unknowns remain) |
| `ResourcefulLib` | ran-hollow 2, 16 host calls (main only) | ran-hollow 2, 20 host calls (`server` ran too) |

The two hollow rows are unchanged because Fabric really does run `main` on a server; what changed
is that the side's own entrypoint runs after it. The departure from Fabric: this path records a
failed stage and continues, while Fabric throws after the `main` stage; the jar is refused only
when every stage failed, and a jar with any failed stage is `ran-partial`, never clean.

The run's own summary line read `1 jar(s) ...: 0 ran-clean, 0 ran-hollow, 1 ran-partial,
0 refused`, and the row's `reason` was `entrypoint(s) failed:
eu.pb4.entityviewdistance.EVDMod: Metadata("malformed java/util/Iterator marker ...")`.

**Interpreter bug for the next batch:** `entity-view-distance`'s `main` fails on
`Metadata("malformed java/util/Iterator marker (at .../EVDMod#onInitialize +17)")`. That is this
interpreter refusing the jar's iterator construction, not a mod error; it should become `ran-clean`
once the marker is modelled and the stage runs.

## Follow-up: cross-jar linking

Re-measured 11 November 2026 on the unmodified `cf0113879e` game (log line `Horizon 0.1.0`), with
both jars of each pair in `config/horizon_java/java-mods/`. Before, each jar had its own host and
the declared dependency was unreachable.

| jar(s) | before | after |
|---|---|---|
| `structureessentials` with `cupboard` | ran-hollow 1 | **ran-clean**, 0 unknown |
| `stackrefill` with `collective` | ran-hollow 1 | **ran-clean**, 0 unknown |
| `stackrefill` alone | ran-hollow 1 (dependency absent, silently) | **refused**: `requires collective (not in horizon_java/java-mods)` |
| `collective` | ran-hollow 67 | ran-hollow 67 (unchanged; it is the dependency) |

Jars now run dependencies before dependents (`cupboard` before `structureessentials`,
`collective` before `stackrefill`), and the missing-dependency refusal names every absent id.
Host-provided ids (`minecraft`, `java`, `fabricloader`, `fabric-api`, `neoforge`, `forge`) are not
required to be jars. Since 12 November 2026 the list also includes the Fabric API module ids whose
entry points the host's Fabric API surface answers: `fabric-api-base`,
`fabric-lifecycle-events-v1`, `fabric-command-api-v2` and `fabric-resource-loader-v1`
(`dependencies.rs` names each module's modelled classes). spark and libjf require exactly these
modules, and both were refused for them. Any other module id still has to be a jar.

## Follow-up: client `@Mod` classes on a server

Re-measured 11 November 2026 on the unmodified `cf0113879e` game (log line `Horizon 0.1.0`).
NeoForge constructs a `@Mod(dist = Dist.CLIENT)` class only on a client, and the probe's
`discover_entrypoints_on("neoforge", "SERVER")` always skipped it; the game used the side-agnostic
discovery and ran it.

| jar | before | after |
|---|---|---|
| `fzzy_config` | refused: `me.fzzyhmstrs.fzzy_config.FzzyConfigNeoForgeClient: Metadata("malformed java/lang/Boolean marker ...")` | **ran-clean**, 0 unknown host calls |

The fixture in `entrypoint_role.rs` pins the annotation shape javap shows
(`dist = [Lnet/neoforged/api/distmarker/Dist;.CLIENT]`) and asserts the SERVER discovery drops it
while the CLIENT discovery keeps it.

## Follow-up: player identity reads

Re-measured 11 November 2026 on the unmodified `cf0113879e` game (log line `Horizon 0.1.0`).
`player_logged_out` fires after the server removed the player from its list, so the façade had no
record to answer from even though the firing carries the player's UUID.

| jar | before | after |
|---|---|---|
| `baguettelib` | `player_leave` listener failed: `ServerPlayer#getUUID()` | no `getUUID` failure; the listener now proceeds to its next read, `ServerPlayer#level()` (`arismoko/horizon#2747`), and still fails on that counted read |
| `carryon` | 725 failed tick callbacks, all `Player#getData(Supplier)` | 707 failed tick callbacks on the same call, still counted |

`PlayerRef` at the pin exposes only `entity()` and `uuid()`, so only the UUID is answered; the
name, dimension and attachment reads keep their counted-refusal path. The attachment design note
is in `docs/mod-plan.md`.

## Follow-up: interpreter gaps

Measured 11 November 2026 with the native probe (`horizon-native-java --json <jar> [main|server]`)
and with a listener harness that loads the real jar, runs its server-side entrypoints and then
fires the survey's event against the runtime exactly as the game bridge does (one `EventFiring`
per event, a `game/server` façade and the `ServerFacts` snapshot). The in-game re-run on the
pinned build could not be made in this session: the machine stayed below the build's resource
floor (12 GB available, 40 GB free on `/`), so no claim below is an in-game verdict. The
reproduction command is the same as the survey's with this branch's `mod/`.

Each jar was first reproduced at `main`, then measured again with the fixes in this branch.

| jar | before | after |
|---|---|---|
| `entity-view-distance` | `main` failed: `Metadata("malformed java/util/Iterator marker (at eu/pb4/entityviewdistance/EVDMod#onInitialize +17)")`; probe verdict refused (or `ran-partial` with the entrypoint-order change) | probe `main` and `server`: `ok`, **0 unknown host calls**; the `server_started` listener runs clean. A registry marker now iterates the entries registered through the host (`registry_contents`); the game's own ~140 entity types are not materialised, so the mod's per-type loop is a no-op for them, which is the bound named in that module |
| `chunky` | `server_started` failed: `Metadata("list index is out of bounds (at org/popcraft/chunky/Chunky#<init> +101)")` | the failure is gone: `Iterable.forEach` now runs the jar's consumer, so `Server#getWorlds` collects the levels and `List.get(0)` has one. The listener then reaches `unknown native host call net/minecraft/server/level/ServerLevel#getWorldBorder()`, a game read the mod API does not carry (the same seam as `#2747`). Without the level ledger it would instead throw a real `IndexOutOfBoundsException`, because the game's `server_started` payload carries no level list; the runtime now records the levels the `LevelLoad` firings announced, and the bridge notes every firing on every jar's host, so the list is the server's own |
| `respackopts` | both `server_started` listeners failed: `expected a native collection marker, got marker "dev/jfronny/commons/ref/WeakSet#2" (call java/util/Set#isEmpty())` | both listeners run, **0 unknown host calls**, with its declared `libjf` dependency beside it in `java-mods`. Three gaps had to line up: a jar's bundled `META-INF/jars/*.jar` libraries are on its class path (and a bundle's mod ids answer its dependents' requirements), `java.lang.ref` references and `ReferenceQueue` are modelled, and `AbstractCollection.isEmpty` on a jar class runs its own `size()`. Alone (dependency absent) the jar still fails exactly as before, which is the missing-dependency refusal that `#33` reports |
| `calcmod` | command registration failed: `Metadata("unsupported Locale marker (at net/jsa2025/calcmod/commands/CalcCommand#<clinit> +23)")`, so it offered no command | the registration listener runs: `new Locale("en", "US")` is modelled (`java.util.Locale` constructor, fields and `BaseLocale` hash). It still counts **22** unknowns, all `java/text/NumberFormat#getInstance(Ljava/util/Locale;)`: the decimal-format model (`NumberFormat`/`DecimalFormat`) is a separate JDK gap, so `nf` is null and nothing that formats a result runs |
| `spark` | `server_starting` failed: `MissingMethod("java/lang/Thread#getId()J")` | `Thread#getId`/`threadId` answer the runtime's one thread (id 1) and a constructed `Thread` a stable distinct id, so `ThreadDumper$Specific` is built. The listener then stops at the debug-build interpreter frame bound (12 nested jar frames) inside adventure's `TextColor#red`; an optimised build's bound is 128. That bound is a stack-safety limit, not a missing model, and is not raised here |

One more JDK gap showed up behind respackopts and is fixed with it: `System.getLogger` and
`System.Logger.Level` answer the existing disabled-logger model instead of a null that turns into
an NPE in the jar's `DelegateLoggerPlus` initialiser.

What is left in this family, in the order the five jars meet it: the world border read on a level
(`chunky`), `java.text` decimal formatting (`calcmod`), the interpreter frame bound in
unoptimised builds (`spark`), and the game's own registry contents (`entity-view-distance`).

## Re-survey after the honesty fixes (11 November 2026, elysium)

The same 40 jars in the same 10 batches, plus the four own fixtures, re-run in game after the
interpreter-honesty fixes (#37, #38, #41: a call on `null` throws, bodiless and unmodelled
instance calls are counted, casts / `Enum.valueOf` / `Thread.start` are checked or counted), the
entrypoint-order change, cross-jar linking (#33), the client-`@Mod` filter, the player-identity
reads and the interpreter-gap fixes above. The `old` column is the first survey at the top of this
page (game `1376e3bcb6`, before every one of these changes); the follow-ups above measured a few of
these jars in game one at a time, and this is the first full in-game run since.

| | |
|---|---|
| Machine | elysium (x86_64, 24 cores, RTX 3090, Vulkan; NVIDIA 615.78.08), headless, so the game ran on a private `Xvfb :77` display (winit needs one even with `--offscreen`). cytonic could not free the memory for the build |
| Game source | `arismoko/horizon` `cf0113879ebee600bff86070466f75b2db90b40a` (the pin in `mod/horizon-mod.toml`), unmodified; the stage is a separate worktree of that commit with only the launcher's generated files added (`native-api/horizon_java`, `crates/native-game`, the two workspace members, and `Cargo.lock` gaining exactly those two packages) |
| Game's own version line | `Horizon 0.1.0` (first line of every batch log; the pinned game logs no commit field, so the commit above is the build's `HEAD`) |
| Mod and runtime | this branch at `ba4ece913d` (`docs/in-game-resurvey` merged with `main` `02c1d4e8ad`); `tools/vendor.py --check`: 61 vendored files match `crates/horizon-native-java/src` (`vendor.lock` still records `e42d6e9254`, a stale record, not a different runtime) |
| Build | dev profile (as the first survey), default features (elysium has the ALSA and udev headers), `HORIZON_NO_AUDIO=1` at run time; the generated `main.rs` binds `mod_0::hooks`, exactly as the launcher writes it, with no deviation |
| Driver | `tools/in_game_survey.py --sample`, then `--manifest` with the fixture batch; one fresh scratch tree per batch, 10 GiB systemd cap, 600 play ticks |

Two driver fixes came out of this run and are in this branch: the content-give step (one item per
content jar, read from its baked package), and a wait between `setblock` and `break`. The first fixture
run showed `break` answering `there is no block at …` because `setblock` runs on the server thread and
`break` checks the window's own world before the update arrives; the driver now polls `block` until
the client shows the placed block (two or three reads in every batch).

### The fixtures (batch `fx`)

| fixture | in game | what was observed |
|---|---|---|
| `m0_fixture` | **ran-clean**, 11 host calls, 0 unknown | entrypoint ran; no hooks to observe |
| `m2b_fixture` | **ran-clean**, 3 usages translated, 0 refused | `/javahello Viola` answered **`Hello, Viola!`** (1 command run) |
| `m2c_fixture` | **ran-clean**, 575 callbacks ran, 0 failed | `m2c: joined Player…`, `m2c: 100 server ticks`, and the stone break was cancelled: `m2c: kept stone at -26,67,10`, `m2c: break canceled at …` (`block_break_before` + `block_break_canceled`, 1 callback each); `horizon-ctl block` after the break still read `minecraft:stone` |
| `m3_fixture` | **ran-clean**, 9 host calls | the runtime skipped its typed declarations (`1 block(s), 1 item(s) in namespace m3_fixture not declared: not owned by horizon_java; run tools/bake.py`), the baked `m3_fixture` package supplied them, and `give m3_fixture:ruby_block 1` answered `Gave 1 [Block of Ruby]` |

### Per jar, old → new

`cb` is listener callbacks ran/failed over the batch; `usages` is command usages
translated/refused at registration; `cmd` is the survey's one command for the jar and its answer.
None of the 40 declares a block or an item through the typed vocabulary (0/0 everywhere, as in the
first survey), so no jar had content to give; 17 asset-only packages from 16 jars were baked and
loaded (`Horizon: content from N mod(s)`), against 20 from 18 the first time.

| jar | old in game | new in game | cb old → new | usages old → new | cmd: old → new | why it moved |
|---|---|---|---|---|---|---|
| `fancymenu` | ran-clean | **refused**: `requires melody (not in horizon_java/java-mods)` | 3/0 → 0/0 | 7/9 → 0/0 | `/fmvariable get test`: refused (`ServerPlayer#level`) → unknown command | cross-jar linking now refuses a missing declared dependency instead of running without it; `melody` was never in the sample |
| `konkrete` | ran-clean | ran-clean | 0/0 → 0/0 | 0/0 → 0/0 | — | — |
| `Jade` | ran-clean | **ran-hollow**, 4 unknown | 0/0 → 1/0 | 0/2 → 0/2 | `/jade`: not registered → not registered | `IEventBus#addListener(Class, Consumer)` ×3 and `(Consumer)` ×1 are now counted |
| `midnightlib` | ran-clean | ran-clean | 0/0 → 1/0 | 0/0 → 0/0 | — | one callback now observed |
| `carryon` | ran-clean | **ran-hollow**, 13 unknown | 5/660 → 7/616 | 3/2 → 3/2 | `/carryon debug`: refused → refused (`Player#getData(Supplier)`, "An unexpected error occurred") | entry unknowns now counted (`Field#canAccess` ×8, `IEventBus#addListener` ×2, `ModConfigSpec$Builder#build` ×2, `ModContainer#registerConfig`); 616 listener failures; every logged one (97 `ServerTickEvent.Post` lines and 1 `BreakBlockEvent`, the log is flood-limited) is `Player#getData(Supplier)` |
| `fzzy_config` | refused | **ran-hollow**, 8 unknown | 0/0 → 2/0 | 0/0 → 4/0 | `/configure_update test list`: — → "Incorrect argument for command" | the client `@Mod` class no longer runs on the server; the 8 are `IEventBus#addListener(Consumer)` |
| `Iceberg` | ran-clean | ran-clean, **2 listener failures** | 1/1 → 0/2 | 0/0 → 0/0 | — | `level_load`: `ExceptionInInitializerError` in `EventFactory#<clinit>` (NPE: `new MapMaker().weakKeys()` came back null, then `.makeMap()`), then `player_join`: `NoClassDefFoundError` for the same class. The level-load listener that "ran" before was running on a silent null |
| `Searchables` | ran-clean | ran-clean | 0/0 → 0/0 | 0/0 → 0/0 | — | — |
| `connectivity` | ran-clean | ran-clean | 642/0 → 632/0 | 10/1 → 10/1 | `/connectivity packetsSummary`: "Incorrect argument" → same | unchanged (tick count follows run length) |
| `limitedchunks` | ran-clean | ran-clean | 642/0 → 632/0 | 1/1 → 1/1 | `/limitedchunks listnonplayerchunktickets`: refused → refused (`CommandSourceStack#getLevel()`) | unchanged |
| `cupboard` | ran-clean | **ran-hollow**, 78 unknown | 642/0 → 631/0 | 0/0 → 0/0 | — | Gson instance calls now counted: `JsonObject#add` ×24, `#addProperty(String, Boolean)` ×24, `#addProperty(String, String)` ×24, `#get` ×4, `#asMap` ×2 |
| `structureessentials` | ran-hollow, 1 | ran-hollow, **198** | 0/0 → 1/0 | 2/3 → 2/3 | `/structureessentials showGenerationTimes`: not registered → same | with `cupboard` now linked it reaches `CupboardConfig` and cupboard's Gson config code (alone it stops at 1, `CupboardConfig#getCommonConfig`); the cross-jar follow-up's 0 was before the counting fixes |
| `Chunky` | ran-clean | ran-clean, **1 listener failure** | 1/1 → 2/1 | 35/7 → 35/7 | `/chunky`: refused → refused (`ChunkyFabric#chunky`) | `server_started` no longer dies on the list index; it now reaches `ServerLevel#getWorldBorder()`, a game read the mod API does not carry |
| `respackopts` | ran-clean (0/2) | **refused**: `requires libjf-base, libjf-config-core-v2, libjf-data-manipulation-v0, …` | 0/2 → 0/0 | 0/0 → 0/0 | — | missing-dependency refusal; `libjf` is excluded from the sample, so this is the honest verdict for the jar alone |
| `modelbrowser` | ran-clean | **ran-hollow**, 3 unknown | 0/0 → 1/0 | 3/3 → 3/3 | `/modelbrowser items_always_equippable`: **ran** → **ran**, reply still the raw key `commands.modelbrowser.gamerule.query` | `JsonObject#addProperty(String, Boolean)` ×3 now counted |
| `baguettelib` | ran-clean (0/1) | ran-clean | 0/1 → 1/0 | 0/0 → 0/0 | — | `player_join` runs and no longer fails; 2 unknown calls are counted inside the callback (the report does not name them) |
| `spark` | ran-clean (1/1) | **refused**: `requires fabric-api-base, fabric-command-api-v2, fabric-lifecycle-events-v1` | 1/1 → 0/0 | 0/0 → 0/0 | `/spark`: not registered → unknown command | missing-dependency refusal: Fabric API's module ids are not among the host-provided ids, and Fabric API is excluded from the sample |
| `mcwifipnp` | ran-clean (2/0) | ran-clean, **1 listener failure** | 2/0 → 2/1 | 0/0 → **6/2** | `/uuidfixer list`: not translated → refused (`com/google/gson/JsonObject#entrySet()`) | its usages now translate; `server_stopping` fails on its own `IUPnPProvider#getUPnPInstance()` |
| `stackrefill` | ran-hollow, 1 | **refused**: NPE `ModOrigin.toString()` at `collective`'s `FabricBundleJarJarCheck#isModJarJard +41` | 0/0 → 0/0 | 0/0 → 0/0 | — | `collective` is linked now, so the call reaches its jar-in-jar check, where Fabric's `ModContainer#getOrigin()` came back null and the next call threw |
| `collective` | ran-hollow, 67 (4,350/3) | **refused**, 112 unknown: `ExceptionInInitializerError` in `EventFactoryImpl#<clinit> +10` (NPE `MapMaker.makeMap()`) | 4,350/3 → 0/0 | 44/0 → 0/0 | — | the same Guava `MapMaker` null as `Iceberg`; before, the entrypoint carried on past it and registered 44 usages and 9 events' listeners on null state |
| `otterlib` | ran-clean | ran-clean | 0/0 → 2/0 | 0/0 → 0/0 | — | two callbacks now observed |
| `calcmod` | ran-clean (0/1) | ran-clean | 0/1 → 1/0 | 0/0 → **51/6** | — (no survey command) | `new Locale("en", "US")` is modelled, so registration runs and offers 51 usages; inside it 22 counted `NumberFormat#getInstance(Locale)` |
| `entity-view-distance` | refused | **ran-clean** | 0/0 → 2/0 | 0/0 → 1/2 | — | `main` no longer fails on the iterator marker and `server` runs after it; 2 usages refused on its permission predicate |
| `structure_pool_api` | ran-clean (1/0) | ran-clean, **1 listener failure** | 1/0 → 0/1 | 0/0 → 0/0 | — | `server_starting`: `ExceptionInInitializerError` in `StructurePoolAPI#<clinit> +44`, the `MapMaker` null again |
| `supermartijn642configlib` | ran-clean (1/0) | **ran-hollow**, 3 unknown | 1/0 → 1/0 | 0/0 → 0/0 | — | `IEventBus#addListener(Consumer)` ×3 now counted |
| `Bookshelf` | ran-clean | **refused**: NPE `Class.getCanonicalName()` at `Services#lambda$loadMany$0 +4` | 0/0 → 0/0 | 0/0 → 0/0 | — | `obj.getClass()` on a service-loader result came back null; the call on it now throws |
| `cloth-config` | ran-clean | ran-clean | 0/0 → 0/0 | 0/0 → 0/0 | — | — |
| `geckolib` | ran-clean | **ran-hollow**, 2 unknown | 0/0 → 0/0 | 0/0 → 0/0 | — | `IEventBus#addListener(Class, Consumer)`, `DeferredRegister$DataComponents#registerComponentType` now counted |
| `yacl` | ran-clean | **ran-hollow**, 1 unknown | 0/0 → 0/0 | 0/0 → 0/0 | — | `IEventBus#addListener(Class, Consumer)` |
| `ForgeConfigAPIPort` | ran-clean | **ran-hollow**, 1 unknown | 0/0 → 0/0 | 0/0 → 0/0 | — | `IEventBus#addListener(Consumer)` |
| `fast-ip-ping` | ran-clean | ran-clean | 0/0 → 0/0 | 0/0 → 0/0 | — | — |
| `Clumps` | ran-clean | ran-clean | 0/0 → 0/0 | 0/0 → 0/0 | — | — |
| `PacketFixer` | ran-clean | **ran-hollow**, 2 unknown | 0/0 → 0/0 | 0/0 → 0/0 | — | `java/util/Properties#setProperty` ×2 |
| `NoChatReports` | ran-clean | ran-clean | 0/0 → 0/0 | 0/0 → 0/0 | — | — |
| `MossyLib` | ran-clean | **ran-hollow**, 1 unknown | 0/0 → 0/0 | 0/0 → 0/0 | — | `String#formatted` |
| `ResourcefulLib` | ran-hollow, 2 | ran-hollow, 2 | 0/0 → 0/0 | 0/0 → 0/0 | — | unchanged: `Optionull#mapOrElse` ×2 in `main`; `server` is clean |
| `krypton` | ran-hollow, 3 | **ran-partial**, 2 unknown | 0/0 → 0/0 | 0/0 → 0/0 | — | `main` now throws: `ExceptionInInitializerError` in velocity-natives' `NativeConstraints#<clinit> +37` (NPE `ByteBuf.release()` on a Netty buffer the runtime does not model); `server` runs |
| `asynclogger` | ran-clean | ran-clean | 0/0 → 0/0 | 0/0 → 0/0 | — | — |
| `c2me` | ran-clean | ran-clean | 0/0 → 0/0 | 0/0 → 0/0 | — | — |
| `structure_layout_optimizer` | ran-clean | **refused**: `requires resourcefulconfig (not in horizon_java/java-mods)` | 0/0 → 0/0 | 0/0 → 0/0 | — | missing-dependency refusal; `resourcefulconfig` is not in the sample |

The unknown-call names come from the jar's own `--json` probe run on the same vendored runtime
(`horizon-native-java --json <jar> neoforge|main|server`); the game report carries only the counts,
and every count matched the probe's for the same stage.

### Totals, old → new

| Measure | First survey (`1376e3bcb6`) | Re-survey (`cf0113879e`) |
|---|---|---|
| In-game verdicts | 33 `ran-clean`, 5 `ran-hollow`, 0 `ran-partial`, 2 `refused` | **19 `ran-clean`** (4 of them with listener failures), **13 `ran-hollow`**, **1 `ran-partial`**, **7 `refused`** |
| Unknown host calls (entrypoints) | 81 | **431** (198 `structureessentials`, 112 `collective`, 78 `cupboard`, 13 `carryon`, 8 `fzzy_config`, 22 more over 11 jars) |
| Unknown host calls inside callbacks | not reported | 36 (22 `calcmod`, 8 `chunky`, 2 `baguettelib`, 1 each `Iceberg`, `mcwifipnp`, `entity-view-distance`, `structure_pool_api`) |
| Callbacks ran / failed | 6,291 / 669 | **1,919 / 621** |
| Command usages translated / refused | 105 / 28 | **116 / 29** |
| Commands run / refused at runtime | 1 / 4 | **1 / 4** (plus the fixture's `/javahello Viola`) |
| Content declarations | 0 blocks, 0 items | 0 blocks, 0 items from the 40; the `m3` fixture's 1 block + 1 item through its baked package, given in game |
| Baked packages | 20 asset-only from 18 jars | 17 asset-only from 16 jars, all loaded |
| Playability | 10/10 `wait-ready`, a block placed and broken in 10/10 | 10/10 `wait-ready` (732–787 ticks each); a block placed and broken in 9/10, and in `b6` the break was refused by the control tool because an oak leaf blocked the view (spawned under a tree, not a mod effect); moved 2.2–9.4 blocks |

The 29 refused usages by argument kind: `Dimension` 6 (`chunky`), `BlockPos` 5 (`calcmod` 4,
`limitedchunks`), `Bool` 5 (`modelbrowser` 3, `mcwifipnp` 2), `Entity` 4 (`Jade` 2, `carryon` 2),
`Resource` biome tag 3 (`structureessentials`), `Identifier` 2 (`calcmod`), a permission
predicate 3 (`entity-view-distance` 2, `chunky`), `GameProfile` 1 (`connectivity`).

### Reading the movements

Most of the drop from 33 clean to 19 clean is the measurement getting more accurate, not jars
getting worse:

- **10 clean → hollow** because unmodelled instance calls are now counted instead of answered
  silently (#38): every one of them was already doing what it does now; the first survey just
  could not see it. Nothing they do in game changed.
- **7 refused now (5 were clean, 2 hollow)**, because a NullPointerException now surfaces (#37)
  or the dependency check now names what is missing (#33). `Bookshelf`, `collective` and `stackrefill` were running on a
  `null` the runtime had invented where a real JVM has a real object (Guava, Fabric Loader and the
  JDK are all there on a real server); that null now throws at the first call on it instead of
  being carried along. `fancymenu`, `respackopts`, `spark` and `structure_layout_optimizer` really
  declare dependencies the sample leaves out; before #33 they ran without them, silently.
- **The callback total fell by 4,372** almost entirely because `collective` is refused: its 4,350
  callbacks the first time were listeners registered by an entrypoint that had already skipped
  past the `MapMaker` failure.
- The movements that are improvements, `fzzy_config` refused → running, `entity-view-distance`
  refused → clean, `calcmod` and `mcwifipnp` now offering 51 and 6 usages, and `chunky`'s and
  `respackopts`' listener gaps closing, are the follow-ups above, now confirmed in game.

### Top in-game failure causes now

1. **`Player#getData(Supplier)`**, the attachment read (the `arismoko/horizon#2747` family):
   616 of the 621 listener failures (all `carryon`: its `ServerTickEvent.Post` listener and one
   `BreakBlockEvent`) and `/carryon debug`. The rest of the player/level reads are the same seam:
   `CommandSourceStack#getLevel()` (`/limitedchunks`), `ServerLevel#getWorldBorder()` (`chunky`'s
   `server_started`).
2. **NeoForge's mod-bus `IEventBus#addListener`** (`(Consumer)` and `(Class, Consumer)`) is not
   modelled: 20 counted calls, making 7 NeoForge jars hollow (`Jade`, `carryon`, `fzzy_config`,
   `supermartijn642configlib`, `ForgeConfigAPIPort`, `yacl`, `geckolib`). Whatever those
   listeners would have done on the mod bus never happens.
3. **Missing declared dependencies**, refused by #33: 4 jars (`melody`, `libjf`, Fabric API
   modules, `resourcefulconfig`). This is the sample's choice, not a runtime gap: a follow-up
   survey should add those jars to the same batches.
4. **A host library call that answers `null`, then a call on it** (now a real NPE): Guava
   `MapMaker#weakKeys()` in `Iceberg`'s `EventFactory#<clinit> +10`, `collective`'s
   `EventFactoryImpl#<clinit> +10` and `StructurePoolAPI#<clinit> +44`; Fabric's
   `ModContainer#getOrigin()` (`stackrefill` through `collective`); `Object#getClass()` on a
   service-loader result (`Bookshelf`); a Netty `ByteBuf` (`krypton`). Three refusals, one
   `ran-partial` and three listener failures.
5. **Gson `JsonObject` instance calls**: 78 in `cupboard` (whose config code `structureessentials`
   reaches once cupboard is linked, for 198), 3 in `modelbrowser`, and `JsonObject#entrySet()`, which refuses
   `/uuidfixer list`. Gson ships with the game, so a real loader always has it.

Then the command bridge's argument kinds (29 usages, above), and `modelbrowser`'s reply still
arriving as its raw translation key.

### What this run does not show

- The game logs no commit; `Horizon 0.1.0` plus the stage's `HEAD` is the whole provenance.
- Dev profile again, so the interpreter's frame bound is the debug one (12), as before.
- One earlier `python3 -m unittest discover -s tests` run on elysium showed one error that three
  reruns did not reproduce; its output was not kept.
- The run logs, the per-batch JSON and the probe outputs stay on elysium under
  `~/ava/results/resurvey-2026-11-11/` and are not committed.

## Attachments first slice: carryon before/after (11 November 2026, elysium)

The re-survey above put 616 of its 621 listener failures on one call, carryon's
`Player#getData(Supplier)`. This run measures what the NeoForge data attachments first slice
(#58 runtime, #59 mod bridge; `docs/attachments-design.md` §4, §7) did for that jar, following
the in-game plan in `docs/attachments-design.md` §5.2. It is measurement only: nothing was changed
in the game, the mod or the runtime.

| | |
|---|---|
| Machine | elysium, headless, the game on a private `Xvfb :79`; load average 37–56 from other users during the runs |
| Game source | `arismoko/horizon` `cf0113879ebee600bff86070466f75b2db90b40a` (the pin in `mod/horizon-mod.toml`), unmodified, the re-survey's stage (`HEAD` `cf01138`; only the launcher's generated `native-api/horizon_java`, `crates/native-game`, their two workspace members and their two `Cargo.lock` packages added) |
| Game's own version line | `Horizon 0.1.0` (both runs) |
| **before** | horizon-java `22dff61c29` (main just before #58 merged: the #57 design merge). The stage's `native-api/horizon_java` is that commit's `mod/`, its vendored runtime included; the bake probe is that commit's `horizon-native-java` |
| **after** | horizon-java `9768f5dd6e` (main with #58 and #59), the same way |
| Build | dev profile, default features, as both surveys; only `horizon-java-mod` and `horizon-native-game` recompiled between the two |
| Driver | each commit's own `tools/in_game_survey.py --sample --only b2` (the tool is identical at both): carryon's re-survey batch, `carryon` + `fzzy_config` + `Iceberg` (NeoForge) + `Searchables`, command `/carryon debug`. Carryon declares no dependencies, so the batch is the whole set. Join, place and break a dirt block, give and use bread, walk 2 s, run the commands, 600 play ticks (~30 s), clean `quit`. 10 GiB systemd cap, fresh scratch tree per run |
| World | `--world B2 --creative` with no `--seed`, exactly the re-survey's invocation: `world_gen_settings.dat` reads seed `0` in both runs. The spawn point still differed (`-38.5, 71, 0.5` before, `-25.5, 71, 0.5` after) |

**Runs thrown away.** I first pinned the seed with `--seed 20261111` (two runs) and `--seed 1`
(one run), on the `before` build. All three ended early: the pinned game's server thread hit a
debug assertion, `entity residency operation did not finish` (`join_seq/residency/table.rs:188`,
`UnbalancedPass { extractions: 9 | 2, lifts: 0 }`), after 151–243 server ticks. That is the game's own end-of-tick
residency check, and the Java hooks around it were the same `entity_load`/`entity_unload` and
`server_tick_end` firings as in every run. Without `--seed` it did not happen, in the two measured
runs, the two relaunches below, or any of the re-survey's 11 batches. That is a candidate fork issue,
not filed here. The three logs are kept with the results.

*Since (reproduced on elysium, no Java jars loaded, idle after `wait-ready`):* the pinned game
panics the same way with no mod content at all: `--seed 20261111` (`extractions: 9`, ~200 ticks) and
`--seed 1` (`extractions: 2`), while no seed (seed `0`) runs 1,891 ticks clean. Fork main
`d3e649f849` runs 2,205 and 2,295 ticks (seed 20261111, twice) and 2,374 (seed 1) without it. A
first-parent bisect over the 73 merges between the two names the merge of `arismoko/horizon#2855`
(`34724f7686`; commit `aa8209dbcc`, "Retire terminal falling blocks from entity residency"), which
closes `arismoko/horizon#2845`: `tick_one_falling` extracted a falling block and never retired it
when it landed, dropped or was gone, and these seeds have gravity blocks near spawn. Nothing to
file; the next pin bump past `34724f7686` takes the fix.

### carryon, before → after

| | before (`22dff61`) | after (`9768f5d`) |
|---|---|---|
| Verdict | `ran-hollow, 535 listener failures` | **`ran-hollow`** |
| Server ticks played (`level.dat` `Time`) | 534 | 539 |
| Listener callbacks ran / failed | 2 / **535** (534 `server_tick_end` + 1 `block_break_before`) | **542 / 0** (the 2 that ran before, plus 539 ticks and the break) |
| Which calls fail | every logged one (99 `ServerTickEvent$Post` lines, as the log is flood-limited, the `BreakBlockEvent` line and the command) is `Player#getData(Ljava/util/function/Supplier;)Ljava/lang/Object;` | none |
| `/carryon debug` | refused (`Player#getData(Supplier)`), chat "An unexpected error occurred while trying to execute that command" | **ran**, returned 0 with no messages, which is what the jar does for a player carrying nothing (`CommandCarryOn`'s debug handler falls through its three `isCarrying` checks to `return 0`, @254–274) |
| Command usages translated / refused at registration | 3 / 2 | 3 / 2 (unchanged: `/carryon clear` and `/carryon place` take an `Entity` `target`) |
| Unknown host calls, entrypoints | 13 | 13, the same ones (below) |
| Unknown host calls, inside callbacks | 0 | 1: the `neoforge:sync_attachments (not sent)` count for the one default creation (the report's `sync_not_sent: 1`), in the first `server_tick_end` |
| Attachment report (after only) | — | `types 1`, `live 1`, `defaults_created 1`, `decoded 0`, `encodes 1615` over `batches 1620`, `encode_failures 0`, `refusals 0`, `tainted 0`, `sync_not_sent 1` |

The 13 entrypoint unknowns, from each commit's own probe (`horizon-native-java --json
carryon-neoforge-26.3-2.12.1.jar neoforge`), are identical before and after:
`java/lang/reflect/Field#canAccess(Object)` ×8, `IEventBus#addListener(Consumer)` ×2,
`ModConfigSpec$Builder#build()` ×2, `ModContainer#registerConfig(ModConfig$Type, IConfigSpec,
String)` ×1. The other three jars in the batch did not move between the two runs (`Iceberg`
ran-clean 2/0, `Searchables` ran-clean 0/0, `fzzy_config` ran-hollow 2/0 with 8 unknown).

So the slice turned every carryon listener failure in this script into a run. Carryon no longer
fails on attachments here: no holder, serializer or hook was missing on the idle path the survey
drives.

**Cost (R1).** One player, one live value: 1,615 encodes in 1,620 batches over 539 ticks, about
three `CompoundTag.CODEC` encodes of an empty compound per tick, one per batch that ran any of
carryon's Java. No wall-clock cost was measured; the slice has no per-batch timing report.

### Saved data

After the clean `quit`, the player's file (`saves/B2/players/data/<uuid>.dat`, read on elysium
with a small NBT reader and not committed) holds:

```
horizon:attachments: { carryon: { carry_on_data: { value: { CarryOnData: {} } } } }
```

`CarryOnData` is an empty `TAG_Compound`, and there is no `copy_on_death` and no `sync` byte. That
is §5.2's expectation exactly: the JVM oracle's empty-compound encoding inside `value`. The
`before` file has no `horizon:attachments` key. Both runs also leave a second player file,
`a01e3843-….dat`, with the same UUID in both, no `Pos` and no attachments: the game writes it, not the
mod, and it is not the player who joined.

The §5.2 follow-ups on the `after` build, each relaunching a copy of the same world with
`horizon-ctl`:

- **Reload** (relaunch, 100 ticks, `quit`): no `Failed to deserialize` line; the report shows
  `decoded 1`, `defaults_created 0`, `decode_failures 0`; carryon's callbacks 150 ran / 0 failed;
  the saved entry is the same after the second quit.
- **Death** (relaunch, `/kill @s`, `respawn`, 40 ticks, `quit`): `decoded 1` (the saved value at
  login), then `defaults_created 1` after the respawn and `sync_not_sent 2`, so the non-`copyOnDeath`
  value was dropped at death and a fresh default made at the next tick, as NeoForge does. The saved
  entry after `quit` is the default again. But this step found **one new failure**: carryon's
  `PlayerEvent$Clone` listener (`CommonEvents#onClone`) fails at `@1` on
  `PlayerEvent$Clone#getOriginal()` (callbacks 113 ran / **1 failed**).

### What is left for carryon, and what each needs

1. **`PlayerEvent$Clone#getOriginal()`**, seen only on death. This is not an attachment holder
   or serializer gap; the event object is not modelled. `onClone` reads `getOriginal().level().isClientSide()`,
   then calls `PlacementHandler.placeCarriedOnDeath(original, getEntity(), isWasDeath())`, whose
   first call is `getCarryData(original)`. Making it real needs:
   - the Clone accessors (`getOriginal`, `getEntity`, `isWasDeath`);
   - an original player that is a distinct holder still carrying its pre-death attachments. The fork's
     `PlayerCloneEvent` names one player (`event.player`, `was_death`), and by the time
     `player_clone` fires the store has already kept only `copy_on_death` envelopes (the bridge's
     own note on `guest_respawn` order). On NeoForge the old `ServerPlayer` still holds its
     attachments when `Clone` fires. So this needs either a fork-side original-player view or the
     bridge holding the runtime's pre-death objects until the Clone listeners have run.
   - Then, from the bytecode reading (not run): `ServerPlayer#level()` (`arismoko/horizon#2747`),
     `ServerLevel#getGameRules()` + `GameRules#get`, and `Inventory#getSelectedSlot`/`setSelectedSlot`.

   *Since:* the runtime answers `PlayerEvent$Clone#isWasDeath()` (the hook's `was_death`, as
   NeoForge's `restoreFrom` posts it) and `getEntity()` already was. `getOriginal()` stays refused:
   the fork's hook names one player at pin and fork main alike, and the store is already the
   `copy_on_death` subset when it fires. The fork issue text is in `docs/mod-plan.md`,
   "Fork seams needed" ("The old player on `player_clone`"), not filed.
2. **The 13 entrypoint unknowns** keep carryon `ran-hollow`: NeoForge config
   (`ModConfigSpec$Builder#build`, `ModContainer#registerConfig`), the mod-bus
   `IEventBus#addListener(Consumer)` (the re-survey's cause 2) and `Field#canAccess` in its config
   reflection.
3. **Two command usages** (`/carryon clear`, `/carryon place`) need the bridge's `Entity` argument
   kind.
   *Since:* the bridge carries `EntityArgument.players()` as the mod API's `player` kind (one
   player; `docs/mod-plan.md`, "Departures"), so both usages now translate. Not re-run in game.
4. **Sync** is counted, never sent: there is no client bridge, so the client's copy of
   `CarryOnData` is never set. That is harmless while idle; it matters once carrying works.
5. **Not exercised by this script:** picking up and placing (`docs/attachments-design.md` §4 lists the
   non-attachment calls the carry path makes), multiplayer guests, an End return, and client-side
   `getData`.

Results (both batch JSONs, game logs, `java-runtime.json` reports, player files, probe outputs, the
three discarded runs and the scripts) stay on elysium under
`~/ava/results/attachments-carryon-2026-11-11/` and are not committed.

## Third survey: after attachments, config and reflection (12 November 2026, elysium)

The same 40 jars in the same 10 batches plus the fixture batch, re-run on current `main`. The `#56`
column is the re-survey above. Its build was horizon-java `ba4ece913d`, whose runtime is `main` at
`02c1d4e8ad`. That is **before** #49–#53 too: those five merged into the `docs/unmodelled-census`
branch and reached `main` with #55, after the re-survey's build. So this delta covers #49–#53, #55,
#58/#59, #61, #62, #65–#68, #70, #71, #73, #74 and #75.

| | |
|---|---|
| Machine | elysium (x86_64, 24 cores, RTX 3090, NVIDIA 615.78.08), headless, the game on a private `Xvfb :83`; nice 10, `CARGO_BUILD_JOBS=12` |
| Game source | `arismoko/horizon` `cf0113879ebee600bff86070466f75b2db90b40a` (still the pin in `mod/horizon-mod.toml`), unmodified; the re-survey's stage worktree, with `native-api/horizon_java` replaced by each measured commit's `mod/` (the `horizon-mod-api` path edit only) |
| Game's own version line | `Horizon 0.1.0`: the first line of all 22 batch logs. The pinned game logs no commit field, so the stage's `HEAD` above is the build |
| Measured | **horizon-java `9eb57c547f`** (`main` with #75), and **`e912e9f117`** (`main` with #74, before #75 merged) as a separate full run. `tools/vendor.py --check`: 77 vendored files match `crates/horizon-native-java/src` at both (`vendor.lock` records an older commit, a stale record, not a different runtime) |
| Build | dev profile, default features, rust 1.99.1, a fresh target dir; `HORIZON_NO_AUDIO=1` at run time; the generated `main.rs` binds `mod_0::hooks` as the launcher writes it |
| Driver | `tools/in_game_survey.py --sample`, then `--manifest` with the fixture batch, unchanged since the re-survey: fresh scratch tree per batch under `~/ava/env`, 10 GiB systemd cap, 600 play ticks, `--world B<n> --creative` with no `--seed` |

**#75 changed nothing in this sample.** The `e912e9f` and `9eb57c5` runs agree on every jar's
verdict, unknown count, host-call count, usages, commands and failures; only tick-driven callback
totals differ, because run lengths differ. The table below is the `9eb57c5` run.

**Fixtures:** all four `ran-clean` again. `/javahello Viola` answered `Hello, Viola!`, and
`give m3_fixture:ruby_block 1` gave one.

**Playability:** 11/11 runs reached `wait-ready` (732–773 play ticks), placed and broke a block in all
11 (the re-survey's `b6` leaf miss did not recur), moved 3.4–14.1 blocks, and quit cleanly.

### Per jar, #56 → now

Only jars where something moved are listed. The other 33 are unchanged in verdict, unknown count,
callbacks ran/failed, usages and command answer, apart from tick-driven callback counts
(`connectivity`, `limitedchunks`, `cupboard` 631–632 → 669–670).

| jar | #56 in game | now in game | cb ran/failed | usages tr/ref | survey command | cause |
|---|---|---|---|---|---|---|
| `carryon` | ran-hollow, 13 unknown | **ran-hollow, 2 unknown** | 7/616 → **671/0** | 3/2 → **5/0** | `/carryon debug`: "An unexpected error occurred" (`Player#getData`) → **ran**, no reply (returns 0, as the jar does for a player carrying nothing) | listeners and the command: **#58/#59** (attachments). Usages: **#61** (`EntityArgument.players()` carried as the player kind). Entry unknowns, from a probe at each merge: **#67** removed `ModConfigSpec$Builder#build` ×2 (and so reached a second `ModContainer#registerConfig`, 1 → 2); **#68** removed `Field#canAccess` ×8, which uncovered 27 `Field#getBoolean`/`getDouble`/`getInt`; **#71** removed those 27; **#73** removed `registerConfig` ×2. The 2 left are `IEventBus#addListener(Consumer)` |
| `Jade` | ran-hollow, 4 | ran-hollow, 4 | 1/0 → 1/0 | 0/2 → **2/0** | `/jade`: unknown command → same | usages: **#61**. `/jade` is still not a registered root; the 4 unknowns are unchanged (`IEventBus#addListener`) |
| `Iceberg` | ran-clean, **2 listener failures** | ran-clean | 0/2 → **2/0** | 0/0 | — | **#51** (Guava `MapMaker`): `EventFactory#<clinit>` no longer dies on `makeMap()`, so `level_load` and `player_join` run. One counted unknown remains inside the `level_load` callback (the report does not name it) |
| `structure_pool_api` | ran-clean, **1 listener failure** | ran-clean | 0/1 → **1/0** | 0/0 | — | **#51**: `StructurePoolAPI#<clinit> +44` no longer dies on the `MapMaker` null |
| `collective` | refused, 112 unknown | refused, **80** unknown | 0/0 | 0/0 | — | **#51** moved the failure. The `EventFactoryImpl#<clinit>` `MapMaker` NPE is gone, and the entrypoint now throws `NullPointerException: Cannot read field "listeners"`, reported at `ArrayBackedEvent#register +53`. That offset is the `synchronized` block's rethrow, so the first throw is inside the locked region (`getOrCreatePhase` → `EventPhaseData#addListener` → `rebuildInvoker`) and is not pinned down yet. The count is what was counted before the throw, so it follows where the throw lands |
| `stackrefill` (with `collective`) | refused, 1 unknown | refused, **5** unknown | 0/0 | 0/0 | — | **#50** (Fabric `ModOrigin`): `FabricBundleJarJarCheck#isModJarJard +41` no longer NPEs. The next stop is collective's `RegisterMod#register +30`, `String.replace` on null. Its receiver is `arg.replaceAll("\\[", "")` (+18–23), so `String#replaceAll` answered null |
| `NoChatReports` | ran-clean | **ran-hollow, 1 unknown** | 0/0 | 0/0 | — | **#49** (honest Gson): `Gson#toJson(Object, Appendable)` is now counted instead of answered as if written. At `02c1d4e` the probe reports 0 unknowns; at #49's merge (`535c990`) it reports this 1. Its config file write never happened before either; the label now says so |

Totals, using the unknown-call names from the jars' own `--json` probe on `9eb57c5`'s runtime (`neoforge`/`main` stage, matching the game's counts):

| Measure | Re-survey #56 (`ba4ece913d`) | Now (`9eb57c547f`) |
|---|---|---|
| In-game verdicts | 19 `ran-clean` (4 with listener failures), 13 `ran-hollow`, 1 `ran-partial`, 7 `refused` | **18 `ran-clean`** (2 with listener failures), **14 `ran-hollow`**, 1 `ran-partial`, 7 `refused` |
| Unknown host calls (entrypoints) | 431 | **393**: `carryon` −11, `collective` −32, `stackrefill` +4, `NoChatReports` +1 |
| Unknown host calls inside callbacks | 36 | 36 (22 `calcmod`, 8 `chunky`, 2 `baguettelib`, 1 each `Iceberg`, `carryon` (`sync_not_sent`), `mcwifipnp`, `entity-view-distance`) |
| Callbacks ran / failed | 1,919 / 621 | **2,700 / 2** |
| Listener failures by jar | `carryon` 616, `Iceberg` 2, `chunky` 1, `mcwifipnp` 1, `structure_pool_api` 1 | `chunky` 1 (`ServerLevel#getWorldBorder()`), `mcwifipnp` 1 (its own `IUPnPProvider#getUPnPInstance()`) |
| Command usages translated / refused | 116 / 29 | **120 / 25** (`carryon` and `Jade` +2/−2 each, #61) |
| Survey commands run / refused at runtime | 1 / 4 | **2 / 3**: `modelbrowser` and `carryon` run; `limitedchunks` (`CommandSourceStack#getLevel()`), `chunky` (`ChunkyFabric#chunky()`) and `mcwifipnp` (`JsonObject#entrySet()`) are refused |
| Content declarations | 0 blocks, 0 items from the 40 | the same; the `m3` fixture's block was given through its baked package |

**Regressions: one label, no behaviour.** `NoChatReports` went `ran-clean` → `ran-hollow`. That is
#49 counting a Gson write it used to pretend happened; the jar does exactly what it did before.
Nothing else got worse. No jar went from running to refused, no listener or command that worked in
#56 fails now, and the fixtures did not move. #75 had no effect here, for better or worse.

**Still not working, even where the label is good.** `carryon` runs all its listeners and
its debug command, but it is `ran-hollow`: its two mod-bus listeners are never registered, and
carrying itself was not exercised. 22 of the 40 jars remain `ran-hollow` (14), `ran-partial` (1)
or `refused` (7).

### Top in-game causes now

1. **Gson instance calls**: 280 of the 393 entrypoint unknowns. `JsonObject#add`/`addProperty`/`get`/
   `asMap` and `JsonArray#add` account for `structureessentials` 198 and `cupboard` 78, `JsonObject#addProperty`
   for `modelbrowser` 3, and `Gson#toJson(Object, Appendable)` for `NoChatReports` 1. `JsonObject#entrySet()`
   also refuses `/uuidfixer list` (`mcwifipnp`). Four jars hollow, one command refused.
2. **NeoForge mod-bus `IEventBus#addListener`** (`(Consumer)` and `(Class, Consumer)`): 20 calls in 7
   NeoForge jars: the only remaining unknowns in 6 of them (`fzzy_config` 8, `Jade` 4,
   `supermartijn642configlib` 3, `carryon` 2, `ForgeConfigAPIPort` 1, `yacl` 1), and one of
   `geckolib`'s 2 (the other is `DeferredRegister$DataComponents#registerComponentType`). This is
   now the whole gap between `carryon` and `ran-clean`.
3. **Refusals: missing dependencies (4) and host `null`s that a call then hits (3 + 1 partial).**
   `fancymenu` (`melody`), `respackopts` (`libjf`), `spark` (Fabric API modules) and
   `structure_layout_optimizer` (`resourcefulconfig`) declare dependencies the sample leaves out.
   `collective` (the `listeners` NPE above), `stackrefill` (`String#replaceAll` answering null),
   `Bookshelf` (`Class#getCanonicalName()` on a null `getClass()` at `Services#lambda$loadMany$0 +4`)
   and `krypton`'s `main` (a Netty `ByteBuf` in `NativeConstraints#<clinit> +37`) are the runtime's
   own gaps.

Then the single-call hollows: `Properties#setProperty` ×2 (`PacketFixer`) and `String#formatted` ×1
(`MossyLib`), both in open #63; `Optionull#mapOrElse` ×2 (`ResourcefulLib`). Then the game reads the
mod API does not carry: `CommandSourceStack#getLevel()`, `ServerLevel#getWorldBorder()`. Then the
command bridge's remaining 25 refused usages (`Dimension` 6, `BlockPos` 5, `Bool` 5, `Resource` 3,
`Identifier` 2, permission predicates 3, `GameProfile` 1). `modelbrowser`'s reply still arrives as
its raw translation key.

### How the causes were found

A native probe was built at every first-parent merge from `02c1d4e` to `9eb57c5`, including the
merges inside #55's branch (`8742db2`, `5ff89dd` #50, `882a216` #51, `06be281` #52, `f64dfa5` #53,
`535c990` #49). It was run on `NoChatReports`, `carryon`, `Jade`, `collective`, `stackrefill --link
collective`, `cupboard` and `structureessentials --link cupboard`, and the first merge where each
result changed is the cause named above. Listener and command changes are not visible to the
probe. They are attributed from the failure the re-survey logged (`Player#getData` → #58/#59, the
`MapMaker` null → #51, the `Entity` argument kind → #61), and that failure is gone in this run.

Results (all 22 batch JSONs, game logs, `java-runtime.json` reports, probe and sweep outputs, the
build logs) stay on elysium under `~/ava/results/survey3-2026-11-12/` and are not committed.

## Fourth survey: Gson, regex, ServiceLoader and the three dependencies (13 November 2026, elysium)

The 43-jar sample (the 40 jars plus `melody`, `libjf` and `ResourcefulConfig`, #84) in the same 10
batches, plus the fixture batch, re-run on current `main`. The `#76` column is survey #3 above
(`9eb57c547f`). The delta covers #77/#78 (Gson tree and parser), #79 (`getClass`/`instanceof`
counting, native-library refusal, counts kept on error), #80 (`ServiceLoader`), #81/#82 (exact
`java.util.regex`, `String`'s regex methods), #83 (sequenced lists, `FMLModContainer`) and #84 (the
three dependency jars in their dependents' batches; Fabric API module ids the host models count as
present).

| | |
|---|---|
| Machine | elysium (x86_64, 24 cores, RTX 3090), headless, the game on a private `Xvfb :84`; nice 10, `CARGO_BUILD_JOBS=12` |
| Game source | `arismoko/horizon` `cf0113879ebee600bff86070466f75b2db90b40a` (still the pin in `mod/horizon-mod.toml`), unmodified; the same stage worktree as surveys 2 and 3, with `native-api/horizon_java` replaced by `1c3f1b3`'s `mod/` (the `horizon-mod-api` path edit only). `Cargo.lock` gains exactly the two generated packages; `horizon-java-mod` now also lists `libc` (#81's `mod/Cargo.toml`) |
| Game's own version line | `Horizon 0.1.0`: the first line of all 11 batch logs. The pinned game logs no commit field, so the stage's `HEAD` above is the build |
| Measured | **horizon-java `1c3f1b3931`** (`main` with #82). `tools/vendor.py --check`: 88 vendored files match `crates/horizon-native-java/src` (`vendor.lock` records `29db910`, a stale record, not a different runtime) |
| Build | dev profile, default features, rust 1.99.1, a fresh target dir (deleted after the run); `HORIZON_NO_AUDIO=1` at run time; the generated `main.rs` binds `mod_0::hooks` as the launcher writes it |
| Driver | `tools/in_game_survey.py --sample` (now 43 jars), then `--manifest` with the fixture batch: fresh scratch tree per batch under `~/ava/env`, 10 GiB systemd cap, 600 play ticks, `--world B<n> --creative` with no `--seed` |

**Fixtures:** all four `ran-clean` again. `/javahello Viola` answered `Hello, Viola!`, and
`give m3_fixture:ruby_block 1` gave one.

**Playability:** 11/11 runs reached `wait-ready` (735–789 play ticks), placed and broke a block in
all 11, moved 1.7–8.6 blocks, and quit cleanly.

### Per jar, #76 → now

Every row of the sample. `unk` is the entrypoint unknown host-call count; `cb` is listener
callbacks ran/failed with the unknown calls counted inside them in brackets; usages are command
usages translated/refused at registration; the command column is the survey's one command per jar
(ran/refused at runtime, and what the chat showed). Rows in **bold** moved.

| batch | jar | #76 (`9eb57c5`) | now (`1c3f1b3`) | cb #76 → now | usages | survey command, #76 → now |
|---|---|---|---|---|---|---|
| b1 | `jade` | ran-hollow, 4 unk | ran-hollow, 4 unk | 1/0 (0) → 1/0 (0) | 2/0 → 2/0 | `/jade`: unknown command → same |
| b1 | `konkrete` | ran-clean | ran-clean | 0/0 → 0/0 | — | — |
| b1 | **`fancymenu`** | refused | **ran-hollow, 2 unk** | 0/0 → 4/0 (3) | 0/0 → **12/4** | `/fmvariable get test`: unknown command → **ran** (returned 1, no message) |
| b1 | `midnightlib` | ran-clean | ran-clean | 1/0 → 1/0 | — | — |
| b2 | `iceberg_nf` | ran-clean | ran-clean | 2/0 (1) → 2/0 (1) | — | — |
| b2 | `searchables` | ran-clean | ran-clean | 0/0 → 0/0 | — | — |
| b2 | `carryon` | ran-hollow, 2 unk | ran-hollow, 2 unk | 671/0 (1) → 672/0 (**2**) | 5/0 → 5/0 | `/carryon debug`: ran, no reply → same |
| b2 | `fzzy_config` | ran-hollow, 8 unk | ran-hollow, 8 unk | 2/0 → 2/0 | 4/0 → 4/0 | incorrect argument → same |
| b3 | **`cupboard`** | ran-hollow, 78 unk | **ran-clean** | 669/0 → 701/0 | — | — |
| b3 | `limitedchunks` | ran-clean | ran-clean | 670/0 → 702/0 | 1/1 → 1/1 | refused (`CommandSourceStack#getLevel()`) → same |
| b3 | `connectivity` | ran-clean | ran-clean | 670/0 → 702/0 | 10/1 → 10/1 | incorrect argument → same |
| b3 | **`structureessentials`** | ran-hollow, 198 unk | **ran-clean** | 1/0 → 1/0 | 2/3 → 2/3 | unknown command → same |
| b4 | `baguettelib` | ran-clean | ran-clean | 1/0 (2) → 1/0 (2) | — | — |
| b4 | **`modelbrowser`** | ran-hollow, 3 unk | **ran-clean** | 1/0 → 1/0 | 3/3 → 3/3 | ran, reply is the raw key `commands.modelbrowser.gamerule.query` → same |
| b4 | `chunky` | ran-clean, 1 listener failure | ran-clean, 1 listener failure | 2/1 (8) → 2/1 (8) | 35/7 → 35/7 | refused (`ChunkyFabric#chunky()`) → same |
| b4 | `respackopts` | refused | refused | — | — | — |
| b5 | **`spark`** | refused | **ran-hollow, 1 listener failure, 1 unk** | 0/0 → 2/1 (8) | 0/0 → 0/0 | `/spark`: unknown command → same |
| b5 | **`mcwifipnp`** | ran-clean, 1 listener failure | ran-clean, 1 listener failure | 2/1 (1) → 2/1 (1) | 6/2 → 6/2 | refused (`JsonObject#entrySet()`) → **ran**, reply is the raw key `mcwifipnp.commands.uuidfixer.default.show` |
| b5 | **`stackrefill`** | refused, 5 unk | **ran-hollow, 1 listener failure, 3 unk** | 0/0 → 1370/1 (4) | — | — |
| b5 | **`collective`** | refused, 80 unk | **ran-hollow, 3 listener failures, 474 unk** | 0/0 → 5020/3 (951) | 0/0 → **44/0** | — |
| b6 | `otterlib` | ran-clean | ran-clean | 2/0 → 2/0 | — | — |
| b6 | `calcmod` | ran-clean | ran-clean | 1/0 (22) → 1/0 (22) | 51/6 → 51/6 | — |
| b6 | `entity-view-distance` | ran-clean | ran-clean | 2/0 (1) → 2/0 (1) | 1/2 → 1/2 | — |
| b6 | `structure_pool_api` | ran-clean | ran-clean | 1/0 → 1/0 | — | — |
| b7 | `supermartijn642configlib` | ran-hollow, 3 unk | ran-hollow, 3 unk | 1/0 → 1/0 | — | — |
| b7 | **`bookshelf`** | refused | **ran-hollow, 1 listener failure, 18 unk** | 0/0 → 0/1 (10) | — | — |
| b7 | `cloth-config` | ran-clean | ran-clean | 0/0 → 0/0 | — | — |
| b7 | `geckolib` | ran-hollow, 2 unk | ran-hollow, 2 unk | 0/0 → 0/0 | — | — |
| b8 | `yacl` | ran-hollow, 1 unk | ran-hollow, 1 unk | 0/0 → 0/0 | — | — |
| b8 | `forgeconfigapiport` | ran-hollow, 1 unk | ran-hollow, 1 unk | 0/0 → 0/0 | — | — |
| b8 | `fast-ip-ping` | ran-clean | ran-clean | 0/0 → 0/0 | — | — |
| b8 | `clumps` | ran-clean | ran-clean | 0/0 → 0/0 | — | — |
| b9 | `packetfixer` | ran-hollow, 2 unk | ran-hollow, 2 unk | 0/0 → 0/0 | — | — |
| b9 | `nochatreports` | ran-hollow, 1 unk | ran-hollow, 1 unk | 0/0 → 0/0 | — | — |
| b9 | `mossylib` | ran-hollow, 1 unk | ran-hollow, 1 unk | 0/0 → 0/0 | — | — |
| b9 | `resourcefullib` | ran-hollow, 2 unk | ran-hollow, 2 unk | 0/0 → 0/0 | — | — |
| b10 | `krypton` | ran-partial, 2 unk | ran-partial, 2 unk | 0/0 → 0/0 | — | — |
| b10 | `asynclogger` | ran-clean | ran-clean | 0/0 → 0/0 | — | — |
| b10 | `c2me` | ran-clean | ran-clean | 0/0 → 0/0 | — | — |
| b10 | `structure_layout_optimizer` | refused | refused | — | — | — |
| b1 | *`melody`* (new) | — | ran-clean | 0/0 | — | — |
| b4 | *`libjf`* (new) | — | refused | — | — | — |
| b10 | *`ResourcefulConfig`* (new) | — | ran-partial, 8 unk | 1/0 | — | — |

Callback counts on the tick-driven listeners (`cupboard`, `connectivity`, `limitedchunks`,
`carryon`) differ with run length. Six unchanged rows report one or two more *host* calls than in
#76 (`konkrete` 19→21, `geckolib` 23→25, `carryon` 1562→1564, `baguettelib`, `forgeconfigapiport`,
`packetfixer` +1); no verdict, unknown or listener changed with them, and they are not attributed.

### What moved, and the PR behind each

A native probe was built at every first-parent merge from `9eb57c5` to `1c3f1b3` (`9aba82e` #77,
`6e3684b` #78, `3d9ad15` #79, `b03c11c` #80, `a862a72` #83, `e02154f` #84, `fe2d950` #81, `1c3f1b3`
#82) and run on every mover with its dependencies linked, as the game links them; the first merge
where a result changed is named. Listener and command changes are not visible to the probe and are
attributed from the failure survey #3 logged, which is gone here.

| jar | before → after | cause |
|---|---|---|
| `cupboard` | ran-hollow 78 → **ran-clean** | **#77** (78 → 2: the tree calls; `Gson#fromJson` left), **#78** (→ 4 `String#replaceFirst`), **#82** (→ 0) |
| `structureessentials` | ran-hollow 198 → **ran-clean** | **#77** (198 → 2), **#78** (→ 9 `String#replaceFirst`), **#82** (→ 0). Its survey command is still not a registered root |
| `modelbrowser` | ran-hollow 3 → **ran-clean** | **#77** (`JsonObject#addProperty(String, Boolean)`). The reply is still a raw translation key |
| `mcwifipnp` | `/uuidfixer list` refused → **runs** | **#77**: `JsonObject#entrySet()` is in the tree model. Reply is a raw translation key; the `IUPnPProvider` listener failure is unchanged |
| `collective` | refused 80 → **ran-hollow, 474, 3 listener failures** | **#79** kept the counts on error (41 at its merge); **#83** moved the failure from `Cannot read field "listeners"` to `String.replace` on null at `RegisterMod#register +30` (`List#getFirst`, counted at #79's merge, is answered from #83's); **#82** made `String#replaceAll` answer, and the entrypoint runs. Running further it counts 474 (below) |
| `stackrefill` (with `collective`) | refused 5 → **ran-hollow 3, 1 listener failure** | **#82**: the same `RegisterMod#register +30` null, from `String#replaceAll`. Its `use_item` listener now runs and fails on `Player#getItemInHand(InteractionHand)` |
| `bookshelf` | refused → **ran-hollow 18, 1 listener failure** | **#79** counted the `getClass()` behind the null; **#80** built the `Services` providers, moving the failure to `Mod 'bookshelf' is not an FML mod!` (`NeoForgeRegistryHelper#getModBus +70`); **#83** (`FMLModContainer`) let the entrypoint finish. Its `RegisterCommandsEvent` listener then dies: `ExceptionInInitializerError` at `MapCodecs#getEnumsByName +23`, `Class net.minecraft.world.item.Rarity is not an enum!` |
| `spark` | refused (`requires fabric-api-base, fabric-command-api-v2, fabric-lifecycle-events-v1`) → **ran-hollow 1, 1 listener failure** | **#84**: the host now provides the Fabric API module ids it models. The probe is the same at every merge (1 unknown, `MixinEnvironment#getActiveTransformer()`). Its `SERVER_STARTING` listener throws an NPE on `HttpClient$Builder.connectTimeout` (`BytesocksClientJdk#<init> +61`); `/spark` registers no usages |
| `fancymenu` | refused (`requires melody`) → **ran-hollow 2** | **#84**: `melody` in its batch. The probe gives the same 2 unknowns (`IEventBus#addListener`, `IEventBus#register`) with and without `melody` at every merge: the in-game refusal was the dependency check alone. `/fmvariable get test` now runs (returns 1, sends nothing) |
| `respackopts` | refused → refused | **#84** added `libjf`: the missing list shrank from six ids to three, but `libjf_base`, `libjf_config_core_v2`, `libjf_data_manipulation_v0` (the ids of `libjf`'s nested jars) still count as absent. The probe with `--link libjf` runs it with 0 unknowns at every merge, so this is the game's dependency check, not the interpreter |
| `structure_layout_optimizer` | refused (`requires resourcefulconfig`) → refused (`native interpreter recursion exceeded 12 nested jar frames at …resourcefulconfig…TranslatableValue#<init>`) | **#84** supplied `ResourcefulConfig`, then **#80** (see *Regressions*) |
| `carryon` | 1 → 2 unknowns inside callbacks | **not attributed.** The report does not name callback calls. In `b2` the `living_allow_death` callback now counts 1 where survey #3's two counted 0. Entrypoint unknowns, listeners and the command are unchanged |

### Totals, #76 → now

The original 40 jars (comparable with survey #3), then the three new ones alone:

| Measure | #76 (`9eb57c547f`), 40 jars | Now (`1c3f1b3931`), same 40 | The 3 new jars |
|---|---|---|---|
| In-game verdicts | 18 `ran-clean` (2 with listener failures), 14 `ran-hollow`, 1 `ran-partial`, 7 `refused` | **21 `ran-clean`** (2 with listener failures), **16 `ran-hollow`** (4 with listener failures), 1 `ran-partial`, **2 `refused`** | 1 `ran-clean` (`melody`), 1 `ran-partial` (`ResourcefulConfig`), 1 `refused` (`libjf`) |
| Unknown host calls (entrypoints) | 393 | **527**: `collective` +394, `bookshelf` +18, `fancymenu` +2, `spark` +1, `stackrefill` −2, `modelbrowser` −3, `cupboard` −78, `structureessentials` −198 | 8 (`ResourcefulConfig`) |
| Unknown host calls inside callbacks | 36 | **1,013**: `collective` 951, `calcmod` 22, `bookshelf` 10, `chunky` 8, `spark` 8, `stackrefill` 4, `fancymenu` 3, `carryon` 2, `baguettelib` 2, one each `Iceberg`, `mcwifipnp`, `entity-view-distance` | 0 |
| Callbacks ran / failed | 2,700 / 2 | **9,193 / 8** (`collective` 5,020 and `stackrefill` 1,370 of the runs) | 1 / 0 |
| Listener failures by jar | `chunky` 1, `mcwifipnp` 1 | `chunky` 1, `mcwifipnp` 1, **`collective` 3** (`MinecraftServer#getWorldPath` ×2, `Level#registryAccess`), **`stackrefill` 1** (`Player#getItemInHand`), **`spark` 1** (the `HttpClient$Builder` NPE), **`bookshelf` 1** (`Rarity is not an enum`) | 0 |
| Command usages translated / refused | 120 / 25 | **176 / 29** (`collective` +44/0, `fancymenu` +12/+4: four `Bool` arguments) | 0 / 0 |
| Survey commands run / refused at runtime | 2 / 3 | **4 / 2**: `modelbrowser`, `carryon`, **`fancymenu`**, **`mcwifipnp`** run; `limitedchunks` (`CommandSourceStack#getLevel()`) and `chunky` (`ChunkyFabric#chunky()`) are refused | — |
| Content declarations | 0 blocks, 0 items | the same | the same |

With the three new jars, the 43 are 22 `ran-clean`, 16 `ran-hollow`, 2 `ran-partial`, 3 `refused`.

The unknown-call totals went **up** because jars that were refused now run: `collective` alone
counts 394 more than it did when it died early. That is more work done and counted, not less
correctness. Five jars left `refused`, all for `ran-hollow`, and four of the five have a listener
that throws on every run (`collective`, `stackrefill`, `spark`, `bookshelf`): running, not
working.

### Regressions

**No jar got a worse label, and nothing that ran in #76 fails now.** No listener or command that
worked in #76 fails, and the fixtures did not move.

**One hidden regression in reach: `structure_layout_optimizer`.** It is `refused` in both runs, so
the label hides it. In #76 it was refused for the missing `resourcefulconfig`. With
`ResourcefulConfig` linked, the probe at `9eb57c5`, `9aba82e`, `6e3684b` and `3d9ad15` **runs it**
(`ok`, 1 unknown: `ServiceLoader#forEach`, counted and skipped). From **#80** on, `ServiceLoader`
builds the providers as JDK 27 does, and the run now dies on the interpreter's 12-nested-jar-frame
bound inside `ResourcefulConfig`'s `TranslatableValue#<init>`. So #84 alone would have made this jar
`ran-hollow` with one unknown, and with #80 it is `refused`. #80 does the honest thing (the
providers are real work the jar needs); the frame bound is what now stops it.

**Smaller:** `carryon` counts one more unknown inside a callback (2, was 1), not attributed (see
the table above); its behaviour is unchanged.

### Top in-game causes now

1. **NeoForge mod-bus `IEventBus#addListener` / `IEventBus#register`**: 26 calls in 9 jars, and the
   *only* unknowns in 7 of the 16 hollow jars (`fzzy_config` 8, `Jade` 4, `supermartijn642configlib`
   3, `carryon` 2, `fancymenu` 2, `forgeconfigapiport` 1, `yacl` 1), plus 4 of `bookshelf`'s 18 and 1
   of `geckolib`'s 2. Still the biggest single lever on the label.
2. **Game reads the mod API does not carry, and the JDK basics `collective` reaches.** Five of the
   eight listener failures and one refused command are host reads: `MinecraftServer#getWorldPath`
   ×2 and `Level#registryAccess` (`collective`), `Player#getItemInHand` (`stackrefill`),
   `ServerLevel#getWorldBorder()` (`chunky`), `CommandSourceStack#getLevel()` (`/limitedchunks`).
   `collective`'s 474 entry unknowns are mostly plain JDK: `StringBuilder#charAt` ×344,
   `StringBuilder#setCharAt` ×12, `Character#toUpperCase` ×12 (368 of 474), then
   `Gson#toJsonTree` ×12, `Class#isEnum` ×12, four `CustomPacketPayload#codec`, `Blocks#GRAVEL`
   ×2 and single `Collections`/`CountDownLatch`/`ThreadPoolExecutor` calls. Its `entity_load`
   listeners count 2 unknowns on each of 471 entity loads (942 of its 951 callback unknowns).
3. **Mojang codecs and dependency linking: the rest of the refusals.** DataFixerUpper
   (`RecordCodecBuilder#mapCodec`/`create`, `Codec#xmap`/`dispatch`/`listOf`/`encodeStart`,
   `JsonOps#INSTANCE`, `Identifier#CODEC`, `UUIDUtil#AUTHLIB_CODEC`) is 13 of `bookshelf`'s
   unknowns and all 8 of `ResourcefulConfig`'s, and `ResourcefulConfig`'s `WebServer` entrypoint
   dies on the null `DataResult` that `encodeStart` answered (`WebServer#loadConfig +39`); host
   enums (`Rarity`) are not seen as enums. `respackopts` is refused because the game's dependency
   check does not count `libjf`'s nested-jar mod ids; `structure_layout_optimizer` hits the 12-frame
   bound; `libjf` itself is `refused` with `no main, server or neoforge entrypoint` (a library with
   nothing to run, which the label cannot tell apart from a failure).

Then the single-call hollows, unchanged since #76: `Properties#setProperty` ×2 (`PacketFixer`),
`String#formatted` ×1 (`MossyLib`), `Optionull#mapOrElse` ×2 (`ResourcefulLib`),
`Gson#toJson(Object, Appendable)` ×1 (`NoChatReports`; #77's writer does not cover the reflective
form), `DeferredRegister$DataComponents#registerComponentType` (`geckolib`), and `krypton`'s Netty
`ByteBuf` in `NativeConstraints#<clinit>`. The command bridge refuses 29 usages (now including
`fancymenu`'s four `Bool` arguments), and `modelbrowser`'s and `mcwifipnp`'s replies arrive as raw
translation keys.

Results (all 11 batch JSONs, game logs, `java-runtime.json` reports, the per-merge probe sweep, the
probes of every hollow and partial jar, the build log and the scripts) stay on elysium under
`~/ava/results/survey4-2026-11-13/` and are not committed.
