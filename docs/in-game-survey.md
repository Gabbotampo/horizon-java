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
labels **172 jars `ran-clean`**: 51 in `corpus-xl`, 84 in `mega`, 37 in `neoforge`. The sample is:

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
the same jar probed alone with no `--link` (as the game runs it). `content` is declared/skipped
typed declarations. `commands` is command usages translated/refused at registration. `listeners`
is callbacks that ran/failed over the batch's life. A dash means the jar produced nothing
observable after its entrypoint.

| jar | loader | native | in game | content | commands | listeners | what visibly worked | what failed, and why |
|---|---|---|---|---|---|---|---|---|
| `fancymenu_neoforge_3.9.14_MC_26.3.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 7 tr, 9 ref | 3/0 | 3 lifecycle listeners fired (server_started, player_join, server_stopping) | `/fmvariable get test` refused: `ServerPlayer#level()`; 9 usages refused (Entity and Bool arguments) |
| `konkrete_neoforge_1.11.1_MC_26.3.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `Jade-mc26.3-NeoForge-26.3.5.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 2 ref | 0/0 | command registration callback ran | both usages refused (Entity argument), so `/jade` is not a registered usage |
| `midnightlib-neoforge-1.10.0+26.3.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `carryon-neoforge-26.3-2.12.1.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 3 tr, 2 ref | 5/660 | 5 callbacks ran (server_started, block/attack/use listeners) | 660 tick callbacks failed on `Player#getData(Supplier)`; `/carryon debug` refused on the same call; 2 usages refused (Entity argument) |
| `fzzy_config-0.7.7+fix3+26.3+neoforge.jar` | NE | ran-clean / clean alone | **refused** | 0/0 | 0 tr, 0 ref | 0/0 | — | refused: the client `@Mod` class `FzzyConfigNeoForgeClient` ran on the server and hit "recursion exceeded 12 nested jar frames" |
| `Iceberg-26.3-neoforge-1.4.3.2.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 1/1 | 1 listener ran (level load) | player_join listener failed on `ServerPlayer#level()` |
| `Searchables-neoforge-26.3-1.0.3.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `470193-8893473.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 10 tr, 1 ref | 642/0 | 642 server-tick callbacks ran; 10 usages registered | `/connectivity packetsSummary` answered "Incorrect argument for command"; 1 usage refused |
| `456640-8026052.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 1 tr, 1 ref | 642/0 | 642 server-tick callbacks ran; 1 usage registered | `/limitedchunks listnonplayerchunktickets` refused: `CommandSourceStack#getLevel()`; 1 usage refused |
| `326652-8890593.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 642/0 | 642 server-tick callbacks ran | nothing beyond the entrypoint line |
| `832882-8896904.jar` | FA | ran-clean / ran-hollow alone (1) | **ran-hollow** | 0/0 | 2 tr, 3 ref | 0/0 | 2 command usages registered | 3 usages refused (Resource arguments); entry hollow on its own `CommonConfiguration#disableLegacyRandomCrashes` field; `/structureessentials showGenerationTimes` is not a registered usage |
| `fc1f2f374c-Chunky-Fabric-1.5.3.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 35 tr, 7 ref | 1/1 | 1 lifecycle listener ran; 35 usages registered | server_started listener failed: interpreter `list index is out of bounds` in `Chunky#<init>`; `/chunky` refused on `ChunkyFabric#chunky()`; 6 usages refused (Dimension argument) and 1 (its permission predicate) |
| `respackopts-26.3.0.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/2 | 2 server_started callbacks attempted | both server_started listeners failed: `WeakSet` collection marker not modelled (`Set#isEmpty`) |
| `c661609de9-modelbrowser-1.5.1.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 3 tr, 3 ref | 0/0 | `/modelbrowser items_always_equippable` ran in Java and answered the player | the command replied with the raw translation key `commands.modelbrowser.gamerule.query`; 3 usages refused (Bool argument) |
| `baguettelib-26.3-Fabric-2.0.7.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/1 | join and leave listeners fired | player_join failed on `ServerPlayer#level()`, player_leave on `ServerPlayer#getUUID()` (the second after the report snapshot) |
| `361579-8918571.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 1/1 | 2 lifecycle callbacks attempted | server_starting listener failed: `MissingMethod("java/lang/Thread#getId()J")`; no command usage translated |
| `mcwifipnp-2.1.4-26.3-fabric.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 2/0 | 2 lifecycle callbacks ran | none of its command usages were translated |
| `1079f41d14-otterlib-0.4.0.2+26.3-fabric.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `bdc7223ca2-collective-26.3.0-8.67.jar` | FA | ran-hollow (67) | **ran-hollow** | 0/0 | 44 tr, 0 ref | 4350/3 | 4,350 callbacks over 9 events; 44 command usages registered | entry hollow: 67 unknown calls (`Items.*` statics, `RandomSource.create`, its own `EventPhaseData`); 3 listener failures (`MinecraftServer#getWorldPath` twice, `Level#registryAccess`) |
| `9f12a0dffd-stackrefill-26.3.0-5.1.jar` | FA | ran-clean / ran-hollow alone (1) | **ran-hollow** | 0/0 | 0 tr, 0 ref | 0/0 | — | entry hollow: 1 unknown call, `collective`'s `ShouldLoadCheck#shouldLoad` (its dependency is not linked in the game) |
| `d198676d8b-calcmod-1.6.0+fabric.26.3.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | command registration callback attempted | command registration listener failed: unsupported `Locale` marker, so no command was registered |
| `09ee9afdd2-entity-view-distance-1.9.0+26.2.jar` | FA | ran-clean / refused alone | **refused** | 0/0 | 0 tr, 0 ref | 0/0 | — | refused: `Metadata("malformed java/util/Iterator marker")` — it needs its Fabric API dependency, which the game does not link |
| `1b0a4ed5f7-structure_pool_api-fabric-1.3.0+26.3.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 1/0 | 1 server_starting callback ran | nothing beyond one lifecycle callback |
| `supermartijn642configlib-1.1.8-neoforge-mc26.3.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 1/0 | 1 server_starting callback ran | nothing beyond one lifecycle callback |
| `Bookshelf-neoforge-MC26.3-26.3.0.4.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `cloth-config-neoforge-26.3.159.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `geckolib-neoforge-26.3-5.5.7.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `yet_another_config_lib_v3-3.9.7+26.3-neoforge.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `ForgeConfigAPIPort-v26.3.1-mc26.3.x-NeoForge.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `fast-ip-ping-v1.0.12-mc26.3.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `Clumps-neoforge-26.3-26.3.2.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `PacketFixer-neoforge-3.3.7.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `NoChatReports-NEOFORGE-26.3-v2.21.0.jar` | NE | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `MossyLib-1.6.0+26.3+fabric.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `825dce0c0b-ResourcefulLib-6.0.1.jar` | FA | ran-clean / ran-hollow alone (2) | **ran-hollow** | 0/0 | 0 tr, 0 ref | 0/0 | — | entry hollow: 2 unknown calls, `Optionull#mapOrElse` |
| `krypton-0.3.2.jar` | FA | ran-clean / ran-hollow alone (3) | **ran-hollow** | 0/0 | 0 tr, 0 ref | 0/0 | — | entry hollow: 3 unknown calls (its own `Natives#cipher`/`compress`, `System#setProperty`) |
| `asynclogger-2.2.2+26.1.2-fabric.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `c2me-fabric-mc26.3-0.4.2-alpha.0.106.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |
| `structure_layout_optimizer-1.1.4+26.3-fabric.jar` | FA | ran-clean / clean alone | **ran-clean** | 0/0 | 0 tr, 0 ref | 0/0 | — | — |


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

**Interpreter bug for the next batch:** `entity-view-distance`'s `main` fails on
`Metadata("malformed java/util/Iterator marker (at .../EVDMod#onInitialize +17)")`. That is this
interpreter refusing the jar's iterator construction, not a mod error; it should become `ran-clean`
once the marker is modelled and the stage runs.
