# Java mod compatibility: status

> **Extracted repository.** Game-side paths in this page (`horizon-client`, `.local/`,
> `target/fast/horizon-client`, the full `tools/lab.py`) refer to the Horizon branch
> `port/native-java-on-main` of [github.com/Solenopsisbot/horizon](https://github.com/Solenopsisbot/horizon);
> they are not part of this repository. The runtime crates and corpus tools are here.

Status: **ported onto the main tree** (branch `port/native-java-on-main`). Measured 6 November 2026 with
binaries built from this branch at 19:02. The design, tier model and phase plan are
[java-mod-compat.md](java-mod-compat.md); this page is the maintained progress view: what runs today, what the
corpus and the clock measured, and what is left.

A jar runs only when it is explicitly named with `--java-mod PATH` (repeatable) on `horizon-client`. Horizon never
scans a folder for Java jars, and a run without the flag starts no Java runtime. A window runs its jars in the client
role, its server thread in the integrated-server role, and `--dedicated` runs them before the content root is built.
A jar the runtime refuses is logged and skipped, and `HORIZON_JAVA_STRICT=1` makes the dedicated server stop instead
of skipping it. A jar that panics in its entrypoint is isolated; the remaining jars still run.

## The goal: two phases to native-only

The project has one destination and one checkpoint on the way. Both phases are gated on the measured corpus, not on a
date:

| Phase | Goal | Exit criteria |
|---|---|---|
| **1. Every corpus jar works on a tier** | no measured jar is left without a path: it runs natively, or through the loader tier, and its label says why | **0 `native-gap`** rows in both committed ledgers; **every `needs-loader` jar proven on the loader tier** — a real Fabric-loader run reaches `Done (`, lists the jar's mod id and names no exception against it; and a hybrid pack session completes both halves inside its work bound |
| **2. Native-only** | the same corpus runs on `horizon-native-java` alone; the loader tier is retired and no game path starts a JVM | 0 `needs-loader` and 0 `native-gap`; every exercised mixin plan is applied by the native weaver or bound to a named host hook; `backends.jvm` stays zero because nothing has a JVM to fall back to |

Phase 1 is a checkpoint, not the product: a `needs-loader` jar is proven, not ported. Skipped client-only jars and
libraries count as `client-only-untested` and `library`: the phase gate treats them as non-gaps, but the ledger never
claims they work. `ran-hollow` says the server ran the jar while falling through calls the host does not model, so
it is not `ran-clean` either. Phase 2 starts from phase 1's zero-refusal baseline and adds what the loader was
carrying: game-class mixins, event callbacks and the client surface. The design and tier model are
[java-mod-compat.md](java-mod-compat.md#native-first-strategy-and-tiers).

### Entrypoint census, re-measured (11 November 2026)

The ledgers above run each jar on a real dedicated server; the entrypoint census
([unmodelled-calls.md](unmodelled-calls.md)) runs the native probe over the same corpora, entrypoints only,
with every server stage the loader runs (`main`, then `server`) and the game's own verdict. Re-measured at
`1894d35` plus the interpreter-honesty PRs (#37: a call on `null` throws `NullPointerException`, empty optionals
are objects; #38: bodiless and unmodelled instance calls are counted, no 64-key cap; #41: casts to jar classes,
`Enum.valueOf` and `Thread.start` are checked or counted), old → new:

| Corpus | `ran-clean` | `ran-hollow` | `ran-partial` | `refused` |
|---|---:|---:|---:|---:|
| wide (`corpus-xl`) | 52 → **31** | 52 → 34 | 0 → 8 | 12 → 43 |
| mega | 86 → **59** | 136 → 67 | 0 → 10 | 30 → 116 |
| NeoForge 26.3 | 36 → **22** | 21 → 20 | 0 → 0 | 28 → 43 |

The numbers going down is the correct outcome: 62 jars had read `ran-clean` only because the interpreter carried
on through a call on `null`, answered a bodiless or unmodelled instance call without counting it, or never ran the
jar's second stage. The newly refused rows stop on a `NullPointerException` on a `null` an unmodelled call answered:
the runtime stopping where Java would on that `null`, not proof the jar is broken.

### Coverage of the two gates (8 November 2026, commit `8a3c8dc59`)

The committed ledgers predate the per-run unmodelled-host-call count, so every row that ran reads `ran-hollow` with
`unknown_source=none`; no row is called `ran-clean` on evidence nobody collected. The next measurement fills
`unknown`, `unknown_source` and the three most frequent calls, and a run that measured zero becomes `ran-clean`.

| Corpus | Jars | `ran-clean` | `ran-hollow` | `client-only-untested` | `library` | `needs-loader` | `native-gap` | with declared dependencies | Phase 1 met? |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| wide (`corpus-xl`) | 223 | 0 | 58 | 103 | 21 | 38 (17%) | 3 (1%) | 3 | no |
| mega (500 unique) | 500 | 0 | 116 | 262 | 38 | 73 (15%) | 11 (2%) | 6 | no |

`ran-clean` + `ran-hollow` + `client-only-untested` + `library` is the old `native-ok` column (182 and 416); the
summary's `native_ok=` field is that sum, so the phase gate still reads one number, and it still requires
0 `native-gap` and every `needs-loader` row proven.

The label columns are the one-jar server ledgers (one fresh dedicated server per jar, 60 s, frozen binaries).
"With declared dependencies" reruns the `native-gap` rows with each jar's required mods beside it
(`java_mod_server_ledger.py --rerun --with-deps`): emotecraft ×2, archers, wizards and paladins run with their
libraries. The 6 November ledgers had 27 and 68 `native-gap`. Against them, `--compare-to` flags one jar that got
worse: beautify (`809311-8919465.jar`) now fails in `BookStack.<clinit>` with an `ExceptionInInitializerError`.

"Proven" comes from the two loader proof ledgers (`LAST_LOADER_PROOF_223.txt`, `LAST_LOADER_PROOF_500.txt`): one
real Fabric launch per jar's dependency closure that lists the jar's mod id, reaches `Done (`, and names the mod in
no failure line. Over the union of both corpora, 116 of 118 `needs-loader` rows are proven, and the loader also
lists 13 of the native refusals. The full per-row result is the generated
[tier coverage](#tier-coverage-over-the-whole-corpus).

The distance to phase 2: the one-jar runs weave no game-class mixins (4,891 wide and 10,099 mega plans skipped,
overwhelmingly `target-outside-jar`); the opt-in game-hook tier binds plans to hook sites instead, and the phase-2
score below is measured with it on. 96 wide and 189 mega jars ran a server entrypoint to `Done`.

### Blockers

**Phase 1:**

1. **Native refusals (27 wide, 68 mega).** 9 wide / 29 mega fail decoding class metadata; 18 / 34 fail with an
   uncaught exception in the entrypoint; mega adds one verifier refusal, two nested-jar-frame cases, one duplicate
   declaration and one unreadable metadata file. The gate is 0, so every row needs either a fix or a reclassification
   backed by a loader proof. Re-measured 7 November on the 38 metadata rows of both ledgers (one fresh dedicated
   server per jar): 15 → 22 run, 6 are client-only and now skipped, and 7 metadata refusals remain, each a jar
   that needs something outside itself (below, under *Close metadata refusals*).
2. **Five loader rows are listed but not proven.** 169 of the 174 `needs-loader` rows of the tier coverage reach
   `Done (` with no failure naming them. Four ship client code the server role loads (CreativeCore 2.14.16 twice,
   sway, combat-hitboxes; a plain Fabric server fails on sway and CreativeCore too), so they need a client-role
   loader proof, which needs a client launch the server corpus does not have. moderner_beta boots but logs an
   empty-registry ERROR against itself.
3. **The hybrid session completes on one pack, the server role only.** The 112-jar Helix pack now runs both
   halves inside the 180 s bound with 0 static conflicts (71 native jars, 43 loader jars, the loader half at
   `Done (` in 15 s; [Hybrid sessions](#hybrid-sessions-a-native-half-and-a-loader-half)). Sway stays out of
   it (client code), seven Helix jars have no ledger row, and no client-role hybrid has been measured.
4. **NeoForge is measured, not gated.** An 85-jar NeoForge 26.3 corpus now has a server ledger (40 `ran-hollow`,
   6 `client-only-untested`, 5 `library`, 20 `needs-loader`, 14 `native-gap`;
   [NeoForge corpus](#neoforge-corpus-8-november-2026)) and both server roles have
   the opt-in FML preflight, but phase 1's gate still counts only the two Fabric corpora: the NeoForge loader tier
   has no `Done (` proof (the FML probe loads and constructs mods without opening a world) and no hybrid session.
   Remapping is not a blocker: 26.3 ships unobfuscated and NeoForge jars name official classes.

**Phase 2:**

1. **Game-class mixins mostly do not apply.** The load-time weaver only rewrites same-jar targets. The opt-in hook
   tier (`HORIZON_JAVA_GAME_HOOKS=1`) now fires its 19 sites from the game, but they cover a small share of the
   game-class plans a `needs-loader` jar carries, and a callback runs only as far as the hook tier's object model
   reaches. Closing this is most of the distance for the 45 / 86 `needs-loader` jars.
2. **Callbacks and the client surface.** Only entrypoints and received-payload handlers execute; event listeners are
   recorded but never dispatched, and there is no compiled HUD, key-binding, tooltip, screen, renderer or model
   façade, so client jars register content but do not draw.
3. **Content consumption.** Recipes and block entity types reach the content root only when everything they name is
   declared (824 of 3,192 mod-namespace recipe files in the 25 mega jars that ship them); 12 of 40 data-component
   registrations are still opaque and every `BlockEntityType` builder is; ingredients have no consumer; a guest
   without the jars is refused by the content identity check before the manifest helps it; there is no mod hot
   reload.
4. **No fallback by definition.** Phase 2 keeps phase 1's zero-refusal baseline and moves the 45 / 86
   `needs-loader` jars onto the native engine; the opt-in FML preflight and `horizon-jvm` remain development tools,
   never a quiet fallback, and the NeoForge lifecycle needs native coverage if the corpus widens.

### Measuring the gates

```sh
# the label gates, from the committed ledgers (no jars, no servers)
python3 tools/java_mod_server_ledger.py --from tools/real_mod_fixtures/LAST_SERVER_LEDGER_223.txt | tail -1
python3 tools/java_mod_server_ledger.py --from tools/real_mod_fixtures/LAST_SERVER_LEDGER_500.txt | tail -1

# a fresh corpus through the real dedicated server (one fresh server per jar, 60 s bound)
scratch=$(mktemp -d)
TMPDIR=$scratch HORIZON_NO_AUDIO=1 python3 tools/java_mod_server_ledger.py ~/horizon-corpus/corpus-xl \
  --binary target/fast/horizon-client --native-binary target/fast/horizon-native-java \
  --resources .local/horizon-client/26.3 --timeout 60 --json --output ledger.txt > ledger.json

# the same corpus's native-gap rows again, each jar with its declared dependency closure
TMPDIR=$scratch HORIZON_NO_AUDIO=1 python3 tools/java_mod_server_ledger.py ~/horizon-corpus/corpus-xl --with-deps \
  --rerun tools/real_mod_fixtures/LAST_SERVER_LEDGER_223.txt --binary target/fast/horizon-client \
  --native-binary target/fast/horizon-native-java --resources .local/horizon-client/26.3 --output deps-ledger.txt

# loader-tier proof (needs the pinned game jar, loader and libraries)
python3 tools/java_mod_loader_ledger.py --mods-dir ~/horizon-corpus/corpus-xl \
  --binary target/fast/horizon-jvm --output tools/real_mod_fixtures/LAST_LOADER_LEDGER.txt

# the hybrid session, both halves, on the measured pack
python3 tools/hybrid_pack.py ~/horizon-corpus/corpus-helix \
  --ledger tools/real_mod_fixtures/LAST_SERVER_LEDGER_223.txt \
  --ledger tools/real_mod_fixtures/LAST_SERVER_LEDGER_500.txt \
  --loader-ledger tools/real_mod_fixtures/LAST_LOADER_PROOF_223.txt \
  --loader-ledger tools/real_mod_fixtures/LAST_LOADER_PROOF_500.txt --match-by-hash --measure
```

## What is ported

### `horizon-native-java` (the runtime)

- **Metadata.** Fabric `fabric.mod.json`, NeoForge `neoforge.mods.toml` and legacy Forge `mods.toml`, with identity,
  versions, dependencies, client-only markers and declared entrypoint kinds. Nested JiJ/JarJar jars are reported
  (the corpus probe extracts and probes them) but the game does not descend into them.
- **Entrypoints.** Fabric `main`, `client` and `server` (string, object and array forms) and NeoForge `@Mod` classes.
  The loader runs the first kind it finds for its role: `client`/`main` on a client and `server`/`main` on a server
  for a jar with `fabric.mod.json`, `neoforge` (its `@Mod` classes) for any other jar. A universal jar's `@Mod` class
  never runs in place of Fabric entrypoints it lacks, as on Fabric Loader, which never reads `@Mod`
  (`entrypoint_role.rs`). The Kotlin adapter is resolved to the default invocation contract (its
  classes compile to ordinary JVM bytecode), and an adapter it does not model is refused. `preLaunch` entrypoints
  are not run.
- **Execution.** A bounded classfile interpreter with exact `long`/`float`/`double` arithmetic, exceptions, records,
  bounded reflection, collections and strings, behind an explicit `Host` boundary. A call the host does not model
  returns a neutral value by default and is counted; the probe fixtures can make it a hard error
  (`with_strict_unknown_host_calls`). The whole run is isolated per jar (`catch_unwind`).
- **Declarations.** Typed `Registry.register`/`DeferredRegister` rows become blocks, items, mob effects, entities,
  menus, block entities, recipes, ingredients, data components and attachments, with item/block property inference
  from ordinary Java builders, data-component flags from `DataComponentType.builder()` chains, and Fabric
  `AttachmentRegistry` rows (`declarations_content.rs`). A jar's `data/<ns>/recipe/*.json` files are recipe rows
  beside its metadata. `META-INF/horizon-native.json` remains the explicit override.
- **Mixin.** Annotation-based plan discovery (config lists, side, priority, selectors) and a classfile writer/weaver
  that rewrites classes whose target is in the same jar. All eleven fixture families weave: `HEAD`, cancellable
  `HEAD`, `RETURN`, `TAIL`, `INVOKE`, `FIELD`, `NEW`, `JUMP`, `@Overwrite`, `@ModifyArg` and `@ModifyVariable`,
  with `CallbackInfo`/`CallbackInfoReturnable`, instance callbacks through a generated bridge, condy and stack-map
  frames. A plan the weaver cannot carry stays skipped with a fixed label (`target-outside-jar`, `non-head-anchor`,
  `unsupported-family`, `cancellable`, `selector-modifier`, `not-in-config`, `other-side`, `config-plugin`,
  `constructor-target`, `unresolved`, `target-missing`, `ambiguous-target`, `listed-without-plan`, `weaver-rejected`).
  The game uses the load-time weave only: a game-class target still has no class to rewrite, so a real jar reads
  `native-qualified`. With `HORIZON_JAVA_GAME_HOOKS=1` the loader plans with `plan_load_time_weave_with_game_hooks`
  instead, binds an `@Inject` for a game class to one of the 19 `GAME_HOOK_SITES` rows, and the game fires the
  callback at that host hook point ([the hook tier](java-mod-compat.md#game-class-hook-tier-experiment)).

  The fixture family matrix is **11 applied / 0 missing**: every family above weaves and runs with its own argument
  at its own position (`crates/horizon-native-java/tests/fixture_mixin_family_matrix.rs`). This is the exact
  load-time path the game calls, not a claim that game-class mixins work: those plans still need the loader or the
  opt-in hook tier.
- **Payloads.** Declared payload channels, codecs (`minecraft:raw`/`byte_array`, `minecraft:string`,
  `minecraft:empty`, opaque fallback), sender/receiver hosts and dispatch of a received frame to the jar's handler.
- **Game events** (`src/events.rs`). Each listener a jar registers gets a stable identity: the Fabric `Event` field
  (22 server-side fields: `ServerLifecycleEvents` start/started/stopping/stopped, `ServerTickEvents` server and
  level start/end, `ServerLevelEvents` load/unload, `ServerPlayConnectionEvents` and `ServerPlayerEvents`
  join/leave, `ServerEntityEvents.ENTITY_LOAD`, `ServerLivingEntityEvents` allow/after death, `UseBlock`/`UseItem`/
  `AttackEntityCallback`, `PlayerBlockBreakEvents` before/after) or the NeoForge/Forge event class (`addListener`
  lambdas and method references, `register(Object|Class)` and `@EventBusSubscriber` `@SubscribeEvent` methods,
  Forge EventBus 7's per-event `BUS`). `NativeJar::run_game_listener` runs one with the jar's own statics, under a
  time budget, against bounded façade markers (server, player list, level, player, join handler, NeoForge event
  object; world path, game rules, registry access, level clock and `ServerPlayNetworking.canSend` answer from the
  server's own state through `facade_server.rs`); a façade call outside that surface refuses the listener by name.
  Fabric `boolean`/`InteractionResult` callbacks stop at the first deciding answer, NeoForge `setCanceled` skips
  later listeners without `receiveCanceled`. Listeners run in the loaders' order across every jar of an event
  (`src/events/order.rs`): Fabric phases (`register(phase, listener)`, `addPhaseOrdering`, sorted as
  `NodeSorting` does, ties by `Identifier.compareTo`), NeoForge/Forge `EventPriority` (from `addListener` or
  `@SubscribeEvent(priority)`), then load and registration order; Forge EventBus 7's byte priorities are read as
  `NORMAL`. The rest of the server's player and entity events are named too (`AttackBlockCallback`,
  `UseEntityCallback`, `PlayerBlockBreakEvents.CANCELED`, `ENTITY_UNLOAD`, `ALLOW_DAMAGE`/`AFTER_DAMAGE`,
  `AFTER_RESPAWN`/`COPY_FROM`, `AFTER_PLAYER_CHANGE_LEVEL`, and NeoForge's `LeftClickBlock`, `EntityInteract`,
  `EntityLeaveLevelEvent`, `LivingIncomingDamageEvent`, `LivingDamageEvent.Post`, `PlayerRespawnEvent`, `Clone`,
  `PlayerChangedDimensionEvent`); a firing hands the block as a `block-pos` value, the face as a `Direction`,
  the hand, and a `BlockHitResult` that answers its block and face. Chat (`ServerMessageEvents`, `ServerChatEvent`)
  is not named yet. Over the 723-jar corpus, 83 of the 499 jars
  that bootstrap natively register 257 listeners the game can fire (`server_started` 38 jars, `server_starting`
  35, `player_join` 34, `server_stopped` 22, `player_leave` 21, `server_stopping` 19, `server_tick_end` 17); the
  largest unfired family was client ticks. Command registration is a listener of its own
  (`GameEvent::RegisterCommands`), fired at the server's command-tree build (next item).
- **Commands** (`src/brigadier.rs`). A bounded Brigadier façade: `Commands.literal`/`argument` and the
  `LiteralArgumentBuilder`/`RequiredArgumentBuilder` statics, `then`/`executes`/`requires`/`redirect`/`fork`/
  `suggests`/`build`, `CommandDispatcher.register`/`getRoot` and `CommandNode` lookups, recorded on an arena with
  Brigadier's own semantics (`then` builds the child at that call, `addChild` merges a same-named node with the
  newer command winning, `register` returns the built node). `StdHost::command_tree` snapshots a typed tree:
  literals; arguments of 25 modelled kinds (`Integer`/`Long`/`Float`/`Double`/`Bool` with bounds, word/quoted/
  greedy strings, entity/player selectors, game profile, block/column pos, vec2/vec3, identifier, UUID,
  dimension, item, block state, component, message, game mode, time, `resource`/`resourceOrTag` with their
  registry, hex color), anything else `Unmodelled` by class; `executes`/`suggests` kept as callback markers.
  `requires` keeps a level (`Commands.hasPermission(Commands.LEVEL_*)`, fabric-permissions-api's `require`) or a
  lambda, which `requirement_level` classifies by running it against probe players of levels 0-4.
  `NativeJar::run_command` runs one `Command.run` strictly (any unmodelled member refuses the run) with a
  `CommandContext` whose `getArgument` and the type getters (`getInteger`, `getString`, `EntityArgument.getPlayer`,
  `getBlockPos`, `getId`...) answer the game's parsed values, and a `CommandSourceStack` stand-in:
  `sendSuccess` (its supplier runs after the command), `sendFailure`, `sendSystemMessage`, a player's
  `sendSystemMessage`, `permissions()` (26.3's level-based `PermissionSet`, `Permissions.COMMANDS_*`,
  `Permission.HasCommandLevel`), `getPlayer`/`getPlayerOrException` (vanilla's `permissions.requires.player`
  `CommandSyntaxException` from the console), `getServer`/`getLevel` (the event façade's markers),
  `registryAccess` (the server façade's). A `CommandSyntaxException` the jar throws (`SimpleCommandExceptionType`)
  keeps its text. A message keeps vanilla's component JSON (`facade_text.rs`): `Component.translatable(key,
  args...)` with its arguments, `withStyle(ChatFormatting...)`, `withColor`, `append` siblings, `copy`/`plainCopy`;
  the game decodes it as a server's chat component and sends it to vanilla clients unchanged, so feedback is
  coloured and translated with its `%s` filled. A `Style`/`TextColor` object is not modelled yet. Over the corpus (81 jars naming a command registration; 69 bootstrap natively alone), **55**
  register a server command callback and **42** of them build a tree when it fires (48 root commands, 1,135
  executable nodes, no argument of an unmodelled type once `resource`/`resourceOrTag`/`hexColor` were added);
  6 register nothing on their own (Balm and JamLib forward to their own buses; spark), 7 refuse (Architectury's
  event loop ×3, one unremapped 1.21.1 jar, one missing a sibling jar, two interpreter markers). Before this
  change none reached Horizon's tree.
- **Probe.** `horizon-native-java [--json] [--mixin-plans] [--weave <out.jar>] <jar> [kind] [class]` reports
  entrypoints, host calls, registrations, plans and skips, and can write a woven jar.

### `horizon-client` (the game)

- Java rows are converted into the current content model and layered into the content root: the window's client role
  before the root is built, the integrated server's rows published from the server thread and merged on the next
  rebuild, and the dedicated role before resources. A sorted typed registry snapshot is published beside the root
  and coalesces equal rows; two rows that disagree are refused, not first-wins. Resolvable recipes become generated
  `data/<ns>/recipe/` files the crafting loader and the recipe book read, block entity types are numbered after the
  per-block ones, and one namespace declared by two jars joins rather than clashes. Each jar logs what it adds and
  the first reason for what it leaves out.
- Declared Java content travels to LAN guests: a bounded, versioned `horizon:java_content` manifest (v3 header with
  epoch and content hash, eight registry id sections, block-entity/menu/entity template rows) is sent at login and
  again when the declaration set changes. A mismatched v3 hash is refused rather than replacing local content.
- Java payload frames travel both ways over LAN on `horizon:payload`, registration on `horizon:payload_channels`;
  frames above the custom-payload envelope are chunked, direction and codec are validated, and `horizon-ctl`'s
  `payload send` injects a declared frame into a hosted world. Each role (client, integrated server, dedicated)
  has its own handler host and queues.
- Java event listeners run on servers (`game/java_events.rs`): the batch keeps every jar that registered a listener
  the game fires, with the host its entrypoints ran on, per role (such a jar is no longer replayed from the content
  cache). The integrated and dedicated servers fire server starting/started/stopping/stopped, server tick
  start/end, level tick start/end, player join/leave and a living entity's death (`ALLOW_DEATH`/`LivingDeathEvent`
  can cancel it, then `AFTER_DEATH`). With no listener, each point costs one atomic load. A listener that fails is
  disabled and logged once with the cause; a server stop logs kept/disabled/run/refused counts. The servers also
  fire the player and entity events (`game/java_events/player.rs`), each from the point the loader's own hook
  uses, beside the native mods' hook: use on a block (`useItemOn`'s start), use of an item (`useItem`'s start),
  use and attack of an entity, a dig's start, a break (before, cancelled, after), an incoming hit
  (`hurtServer`), an entity joining and leaving a level, a respawn, the copy to the new player, a change of level
  and a level's load and unload. A non-`PASS` result, a `false` answer or a NeoForge cancel stops the vanilla
  action there (`SUCCESS_SERVER` swings the hand). Not fired yet: `AFTER_DAMAGE`, `AFTER_RESPAWN` for the End's
  exit, a player's own `ENTITY_LOAD`. `java-runtime.json` has a `listeners` section, jar → event → kept,
  dispatched, refused and disabled counts, rewritten at a server's stop. Measured on `--dedicated` with bots (8
  November 2026, fast profile, flat world, two `flood-use` bots for 25 s, one bot attacking a summoned pig for
  12 s): with a source-built jar whose use, dig and attack listeners answer `FAIL`, the bots placed 0 stone and
  broke 0 grass blocks (248 and 53 without it) and the pig kept its 10 health after 30 attacks (killed without
  it); the jar's listeners ran 510 use, 510 dig and 127 attack firings with no refusal. In a run with three
  fight bots (normal difficulty) and 15 corpus jars that listen to these events (Carry On, Farmer's Delight, Connectible Chains, collective, Architectury...),
  before this change 0 of these listeners were fired; now use-block (4 listeners), dig, attack, break-before
  and one of two damage listeners are fired and refused at their first unmodelled call (`Player#isSpectator`,
  `getItemInHand`, Fabric data attachments, `DamageSource#getEntity`), while Carry On's `ALLOW_DAMAGE` (172
  runs), collective's and Icy's Better Horses' `ENTITY_LOAD` (765) and `ENTITY_UNLOAD` (116) and the level
  loads run; no corpus listener reached a deciding answer, so the bots' blocks matched the run without jars.
  Use of an item and of an entity were not exercised by the bots. Measured on `--dedicated` (fast
  profile, flat world, no players): Cupboard alone ran its `SERVER_STARTED` and `END_SERVER_TICK` listeners 557 times
  in 28 s with no refusal; the 48 corpus jars with server listeners kept 138 listeners from 42 jars, of which 105
  were still enabled at stop after 13,865 runs in 45 s. Join, leave and death were covered by tests only, not by a
  run with a connected player.
- Java mods' commands join the server's command tree (`game/java_commands.rs`): the batch keeps every jar with a
  command registration on its resident host (the one its listeners and payload handlers share). The first
  command-tree build (`register_native_commands`, the native mods' extension point) runs the registrations, merges
  the jars' trees the way `CommandNode.addChild` does (a later jar wins a command, children join), leaves out a
  root vanilla or a native mod already has and an argument of an unmodelled type (each with a log line), and adds
  the result to the native mods' root, so every player's `ClientboundCommandsPacket` carries the Java commands
  their level allows and the client parses, colours and completes them. `Game::run_command` (players, the console,
  command blocks) asks the Java tree first: the line is parsed with a Horizon dispatcher built from the same
  tree, `requires` lambdas on the path run with the real source, selectors resolve to players (a non-player
  selection refuses), and the jar's callback runs; its feedback goes where a vanilla command's does
  (`send_command_feedback`, the console, operators for a broadcast). A refused run answers vanilla's
  `command.failed` and logs the cause. Custom `SuggestionProvider`s do not run (clients complete from the argument
  type), a redirect that changes the source (`fork`, a modifier) is refused, and `execute ... run` cannot reach a
  Java command. A `requires` lambda that no probe player passes is owners-only in the tree and decided by the real
  source when run. Measured on `--dedicated` (fast profile, flat world, console source, one jar per server): the
  34 registering jars with a console-typable line ran 83 sampled lines; 3 jars did not start in game; of the
  rest, **23 lines in 10 jars returned** (Pingwheel's `/pingwheel:server rate_limit 7` sets the value and the
  read-back answers through the mod's own feedback callback), 16 lines in 6 more jars ended in the mod's own
  `CommandSyntaxException` (mostly "A player is required to run this command here", correct for a console), 18
  stopped at a `requires` that is false for the console, and 26 were refused (`CommandSourceStack#getPosition`,
  a field a refused `SERVER_STARTED` listener never set, `Locale.ROOT`, `RegistryAccess#lookupOrThrow`). A
  command run by a connected player was covered by tests only.
- An opt-in NeoForge loader preflight: when `HORIZON_NEOFORGE_SERVER` (an installed server) and `HORIZON_JVM_BIN` are
  set, the integrated and the dedicated server run `horizon-jvm --real-loader --entry-kind neoforge` once before
  their native batch in a bounded child process and report what FancyModLoader saw (completion, mod count, sorted
  ids, first error with FML's `- Mod x requires y` items). With no `--java-mod` the run loads the install's own
  `minecraft` and `neoforge` (the runner used to stop that at its usage line, so a jar-less preflight always
  failed). The result is in `server-status` and in `java-runtime.json`'s `fml` section; a role with no jar writes
  that file with the section alone. A missing binary or server is a report line, not a failure; only
  `HORIZON_NEOFORGE_STRICT=1` stops startup. Checked on a real NeoForge 26.3.0.101 server: a jar-less dedicated
  start lists 2 mods, and Waystones with Balm and Shogi lists 7 (`kuma_api` and `shogi_api` are JarJar modules).
- `server-status` carries bounded `java` and `fml` sections: per-jar loader, engine, backend, weave label, executed
  entrypoints, registrations, opaque rows and isolation failures, `payload_channels`, and the preflight's
  status/role/jars/mods/ids/error. Refused jars are rows too (listed first, up to 8, `engine = "refused"`, the
  runner's whole cause in `refusal`), and `jars`/`started`/`refused` and the engine counts include them. The dedicated
  log prints one line per started jar and one bounded `skipping` line. `java-runtime.json` (schema v1, 256 jars,
  256 KiB cap) is written under `HORIZON_HOME` for launcher and support tooling: one row per jar given, refused ones
  with `refusal`, started ones with the first 64 skipped `plans` (mixin, target, method, injector, reason; the
  loader-relevant ones first), `skipped_by_reason` counts and the jar's cache `sha256`. Plan lists shrink before rows
  are dropped when the file would pass its cap. The launcher's runtime panel shows all of it per mod, with a "Clear
  runtime cache" action that removes that jar's `java-content-cache/<sha256>-<role>.json` entries.

### `horizon-jvm` (the reference runner, not the native path)

A standalone JVM runner for the same jars: real Fabric Loader discovery/freeze/entrypoints, an opt-in Knot class
loader (classpath unlock, transformer chain, `--launch-game` against a real 26.3 server), a real
FancyModLoader/NeoForge lifecycle (`--real-loader`, `--list-fml-mods`, `--fml-report`), datagen entrypoints, bounded
mixin-config registration and `--measure`. It is development/reference tooling: no game path runs a world or a jar's
gameplay through it (the only in-game use is the opt-in FML preflight child above), so `backends.jvm` in
`server-status` is always zero and a refused jar is skipped rather than handed to it. A Knot-backed run hands Fabric
a copy of any supplied jar that shades the platform logging classes (Axiom's slf4j 1.x), without the classes the
game's logging jars already provide, so the game's copy wins as in Fabric's own class path order.

### Tools and tests

`tools/real_mod_corpus.py` (outer + nested probe with a verdict per jar), `tools/java_mod_server_ledger.py` (one real
dedicated server per jar, verdict from its log, `java-runtime.json`, the plan report and the probe's
`unknown_host_call_count` as `ran-clean`/`ran-hollow`/`client-only-untested`/`library`/`needs-loader`/`native-gap`;
`--with-deps` runs a jar with its declared closure and labels an unresolvable one `missing-dependency`; `--docs`
regenerates the
table below), `tools/java_mod_loader_ledger.py` (real Fabric loader),
`tools/real_mod_fixtures/build.py` (source-only Fabric/NeoForge/legacy-Forge fixtures), the
`real-mod-corpus` workflow (fixtures and harness self-test only, never third-party jars), the fixture suites under
`crates/horizon-native-java/tests/` and `crates/horizon-jvm/tests/` (the latter skipped unless a Minecraft jar,
loader and libraries are supplied), `tools/native_java_smoke.py`, and the efficiency harness (`tools/efficiency/`,
[efficiency.md](efficiency.md)) behind the numbers below.

## Measured corpus

Two Fabric 26.3 corpora, each jar run once through the real dedicated server
(`horizon-client --java-mod <jar> --dedicated`, fresh scratch home, flat world, `online-mode=false`, 60 s bound, one
process group killed after `Done (`). The [nightly Java corpus workflow](../.github/workflows/nightly-java-corpus.yml)
runs this same probe and ledger over jars already held by its runner. Raw reports:
`~/horizon-corpus/{ledger-wide,ledger-mega}(-classified).{txt,json}`, analysed in `ledger-wide-report.md` and
`mega-report.md`; the committed ledgers are `tools/real_mod_fixtures/LAST_SERVER_LEDGER_{223,500}.txt`.

| Corpus | Jars | Ran | Refused (native gap) | Skipped | Wall to `Done` per jar |
|---|---|---:|---:|---:|---|
| wide (`corpus-xl`: 6 packs, SHA-256 deduped) | 223 | 94 (42%) | 27 (12%) | 102 (46%) | mean 2.42 s, max 5.66 s |
| mega (500 unique jars from 50 Fabric 26.3 packs) | 500 | 197 (39%) | 68 (14%) | 235 (47%) | mean 2.26 s, max 8.75 s |

Skipped jars are out of scope for a server, not failures: 69 are client-only and 33 are libraries in the wide
corpus, 180 and 55 in mega. Of the jars that ran, 13 (wide) and 37 (mega) ran with engine `native` (no skipped plan);
the rest ran `native-qualified`. **No mixin was woven in either corpus**: 2,944 plans were skipped in the wide corpus
and 7,742 in mega. Every `ran` row therefore has `woven=0`: the load-time weaver only rewrites targets found in the
same jar, while real mods' plans target `net/minecraft` classes and Horizon has no Java class for those targets. The
dominant skip reason is `target-outside-jar`; 45 (wide) and 86 (mega) of the running jars have at least half of
their skipped plans aimed at the game and are labelled `needs-loader`.

Refusals split by cause (the ledger's own categories): 9 wide / 29 mega fail decoding class metadata (bounded
collection markers, `ServiceLoader` and stream markers, `String.split`/`Map#keySet` null receivers, `Gson` needing an
`Appendable`, malformed `fabric.mod.json`), and 18 / 34 fail with an uncaught exception in the entrypoint (a null
config or listener phase, reflection on a null class, a mixed-loader guard, a mixin plugin that did not initialise).
Mega adds one verifier refusal (`StackUnderflow` in spell_engine's datagen), two jars that exceed the interpreter's
128 nested-jar-frame bound, one duplicate native item declaration, and one jar whose metadata cannot be read at all.
The Kotlin entrypoint adapter now runs (`fzzy_config` starts), so no refusal is attributed to it. The ledger marks
27 / 68 rows as `native-gap`, 45 / 86 as `needs-loader`, and the remainder, 151 / 346, as non-gaps — today's
labels split those into `ran-hollow` (a run whose host-call count nobody recorded), `client-only-untested` and
`library`.

The role-agnostic probe over the same 500 jars (`real_mod_corpus.py`) reports **167 ok (33.4%), 303 fallback (60.6%),
30 unsupported (6.0%)**, with the top non-ok groups being "no main entrypoint but needs the loader/embedded JVM",
client-only entrypoints, and malformed metadata.

### NeoForge corpus (8 November 2026)

`tools/neoforge_corpus.py lock --limit 80` took the 80 most-downloaded Modrinth mods tagged NeoForge and 26.3; 79
publish a NeoForge 26.3 jar, and their required Modrinth dependencies add 6 (85 jars, 211 MB, every file checked
against Modrinth's SHA-512; the lock is `tools/real_mod_fixtures/neoforge_corpus.lock.json`, the jars stay in
`~/horizon-corpus/neoforge/`). 70 carry only `neoforge.mods.toml`, 4 also a legacy `mods.toml`, 8 also a
`fabric.mod.json`, and 3 have no root metadata (Essential, Kotlin for Forge, mezz_config's JarJar carrier).

Native, each jar with its declared closure through the real dedicated server
(`LAST_SERVER_LEDGER_NEOFORGE.txt`, fast build of 40db5fe5b plus the wave's uncommitted runtime work, 60 s bound):

| Label | Jars | What |
|---|---:|---|
| `ran-clean` | 0 | no committed row recorded a host-call count |
| `ran-hollow` | 40 | ran; the host-call count was not recorded (`unknown_source=none`) |
| `client-only-untested` | 6 | the server skipped them; nothing says they work |
| `library` | 5 | libraries with nothing for the server to run |
| `needs-loader` | 20 | ran, with server mixin plans on game classes the weave skips |
| `native-gap` | 14 | 9 uncaught exceptions, 3 missing members, 1 metadata, 1 no entrypoint |
| `missing-dependency` | 0 | (6 before the reader fixes below) |

The real FancyModLoader of an installed NeoForge 26.3.0.101 server (`horizon-jvm --real-loader --entry-kind
neoforge`, `Dist.DEDICATED_SERVER`, one run per jar with the same closure; it bootstraps the game and constructs
every mod, but opens no world) loads and initializes 76 of 85: 48 of the 51 non-gap native rows (Essential misses
commons-codec, ImmediatelyFast reaches LWJGL, Cubes Without Borders fails its own init), 18 of the 20 `needs-loader`
(sodium-extra needs client Sodium; Carry On asks for `javafml` 26.3.0.37-beta) and 10 of the 14 `native-gap`. The
other 4 gaps fail under FML too: fzzy_config's init, and veinminer, veinminer-client and zoomify need the `klf`
language provider, which the probe does not pick up from Kotlin for Forge's jar. So 28 rows have a real-loader
listing (not a `Done (` proof) and 6 have no tier.

Three reader bugs the corpus found, fixed in `tools/hybrid_pack.py` (the ledger's declaration reader): FML 12
reads only a dependency's `type` and defaults it to required (BiomesOPlenty writes `required=true`), NeoForge's
template comments (`modId = "almanac" #mandatory`) were kept in the value, and JarJar modules
(`META-INF/jarjar/`, JEI's mezz_config, Xaero's xaerolib) were not counted as provided ids.

## What runs native, what needs the loader, why

Every ledger row ends in one of six labels (seven with `--with-deps`), so a verdict says what to do instead of
counting a refusal:

- `ran-clean`: the dedicated server ran its entrypoint to `Done (`, no server-side plan targets a game class, and
  the run fell through no unmodelled host call. Only a measured zero earns this label.
- `ran-hollow`: it ran to `Done (`, but at least one call the host does not model returned a neutral value instead of
  refusing (the row records the count, its source and the three most frequent calls), or nobody measured the count
  (`unknown_source=none`). The old `native-ok` label hid this.
- `client-only-untested`: the server skipped it because it is client-only. This ledger says nothing about whether the
  mod works; a client-role measurement has to answer that.
- `library`: the server skipped it because it has no entrypoint of its own.
- `needs-loader`: it runs, but at least half of its skipped mixin plans target game classes (`target-outside-jar`), so
  only a real loader applies them.
- `native-gap`: native execution refuses it yet; [Known gaps](#known-gaps) and the row's reason say where.
- `missing-dependency` (`--with-deps` only): the jar's declared closure needs a mod id no corpus jar provides, so no
  loader would start it either; the ledger does not run it.

The in-game mod's report adds `ran-partial`: at least one entrypoint stage ran and at least one failed, so the
listeners from the running stages stay active and the row's reason names the failed stages. It is never counted as
`ran-clean` or `ran-hollow`. The ledger above does not emit it, because it runs one stage per invocation; the
entrypoint census ([unmodelled-calls.md](unmodelled-calls.md)) runs every server stage and uses the same verdict.

The default ledger starts one server per jar, so a jar that declares a dependency (archers on `spell_engine`, zoomify
on YACL) runs without it, which a real loader never does. `--with-deps` runs each jar with its declared closure
(`fabric.mod.json` `depends`, `mods.toml`/`neoforge.mods.toml` required dependencies, resolved through `provides`
aliases and jar-in-jar modules against the corpora under `~/horizon-corpus`); the row stays the jar's own, and
`deps=`, `dep_refused=` (a dependency whose entrypoint was refused; libraries with nothing to run are not counted)
and `blocked_in=` (the dependency whose class the refusal stopped in) say what the closure did. When a jar is refused
both alone and with the pack class path and the two causes differ, the game reports
`with the pack class path: <cause>; alone: <cause>`: the linked attempt runs the same code until the first class
alone lacked, so its cause is the blocker the pack hits, and the ledger categorises the row by it.

Measured on 7 November 2026 (one build, the 95 `native-gap` rows of the two committed ledgers): one jar per server,
15 stay `native-gap` and 80 leave it from native fixes landed since those ledgers (68 run: 31 non-gaps, 37
`needs-loader`; 12 are now read as client-only, `client-only-untested`); with the closure, 14 stay `native-gap`, 1 more runs
(archers, `needs-loader`) and none is `missing-dependency`. The closure moves the blocker more than the label:
wizards and paladins now stop in `spell_power`'s `SpellSchools.createMagic`, zoomify in YACL's
`JsonFileCodecConfig.saveToFile`. The native host already answers a required mod's presence, so most dependents
ran alone too.

The block below is generated from the wide corpus's committed ledger, and the tool rewrites it in place
(`--docs`), so it cannot drift from the ledger format:

<!-- java-mod-ledger:start -->
_Generated by `tools/java_mod_server_ledger.py --docs docs/java-mod-compat-status.md` from `tools/real_mod_fixtures/LAST_SERVER_LEDGER_223.txt`; edit the tool, not this block._

**223 jars: 58 `ran-hollow`, 38 `needs-loader`, 103 `client-only-untested`, 21 `library`, 3 `native-gap`.**

| Label | Jars | What it means | Why |
|---|---:|---|---|
| `ran-hollow` | 58 | it ran to `Done (`, but calls the host does not model returned neutral values | 58 ran to Done; the unmodelled-host-call count was not recorded |
| `needs-loader` | 38 | it runs, but its plans target the game, so only a real loader applies them | 38 ran, but their skipped plans target the game |
| `client-only-untested` | 103 | client-only: the server skipped it, so this ledger says nothing about it working | 103 client-only (the server role never ran it) |
| `library` | 21 | no server entrypoint of its own: the server skipped it | 21 libraries (nothing for the server to execute) |
| `native-gap` | 3 | native execution cannot carry it yet | 2 refused by an uncaught exception in its entrypoint; 1 refused while decoding metadata |

**Jars that did not run clean (99).**  103 `client-only-untested` and 21 `library` row(s) ran nothing and are counted in the table above, not listed here.

| Jar | Label | Why |
|---|---|---|
| `809311-8919465.jar` | `native-gap` | refused by an uncaught exception: `uncaught exception java/lang/ExceptionInInitializerError (at io/github/suel_ki/…` |
| `Legacy4J-26.3-1.9.1-fabric.jar` | `native-gap` | refused while decoding metadata: `with the pack class path: Metadata("malformed java/util/Iterator marker (at wil…` |
| `servercore-fabric-1.5.20+26.3.jar` | `native-gap` | refused by an uncaught exception: `with the pack class path: uncaught exception java/lang/ExceptionInInitializerEr…` |
| `1399257-8910835.jar` | `needs-loader` | ran; 14 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `241160-8893517.jar` | `needs-loader` | ran; 2 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `248787-8907912.jar` | `needs-loader` | ran; 11 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `326652-8890593.jar` | `needs-loader` | ran; 9 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `342584-8921261.jar` | `needs-loader` | ran; 158 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `348831-8926733.jar` | `needs-loader` | ran; 18 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `410295-8911219.jar` | `needs-loader` | ran; 8 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `419699-8915075.jar` | `needs-loader` | ran; 131 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `456640-8026052.jar` | `needs-loader` | ran; 1 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `511733-8929530.jar` | `needs-loader` | ran; 10 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `526901-8925000.jar` | `needs-loader` | ran; 1 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `608235-8905060.jar` | `needs-loader` | ran; 15 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `623764-8914234.jar` | `needs-loader` | ran; 21 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `734339-8931641.jar` | `needs-loader` | ran; 4 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `832882-8896904.jar` | `needs-loader` | ran; 31 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `882495-8910991.jar` | `needs-loader` | ran; 2 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `888790-8002356.jar` | `needs-loader` | ran; 5 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `BedrockSkins-26.3-2.1.1.jar` | `needs-loader` | ran; 90 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `CustomPlayerModels-Fabric-26.3-0.6.27e.jar` | `needs-loader` | ran; 62 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `MossyLib-1.6.0+26.3+fabric.jar` | `needs-loader` | ran; 14 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `StringDupersReturn-1.0.0-Fabric-26.3.jar` | `needs-loader` | ran; 1 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `advancementscreenshot-26.3.0-5.3.jar` | `needs-loader` | ran; 6 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `architectury-fabric-22.0.4.jar` | `needs-loader` | ran; 131 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `armor-stand-poses-fabric-26.3-1.4.0.jar` | `needs-loader` | ran; 6 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `baguettelib-26.3-Fabric-2.0.7.jar` | `needs-loader` | ran; 19 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `bannerpoint-fabric-1.1.2+26.3.jar` | `needs-loader` | ran; 17 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `collective-26.3.0-8.42.jar` | `needs-loader` | ran; 160 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `collective-26.3.0-8.46.jar` | `needs-loader` | ran; 159 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `fix-keyboard-on-linux-1.0.2.jar` | `needs-loader` | ran; 2 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `jei-26.3-fabric-31.16.0.83.jar` | `needs-loader` | ran; 25 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `mcwifipnp-2.1.4-26.3-fabric.jar` | `needs-loader` | ran; 4 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `modernregsyncfix-2.0.0.jar` | `needs-loader` | ran; 5 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `naturally_trimmed-3.6.1+26.3-fabric.jar` | `needs-loader` | ran; 7 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `pickupnotifications-fabric-3.0.1+26.3.jar` | `needs-loader` | ran; 20 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `simplemenu-26.3.0-2.3.jar` | `needs-loader` | ran; 39 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `swinginglanterns-fabric-26.3-2.0.3.jar` | `needs-loader` | ran; 38 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `takeitout-1.1.28-26.3.jar` | `needs-loader` | ran; 20 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `xaeroworldmap-fabric-26.3-1.47.0.jar` | `needs-loader` | ran; 45 plan(s) skipped, at least half aimed at the game (`target-outside-jar`) |
| `1005914-8891048.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`) |
| `241895-8893531.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`) |
| `257814-8894326.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 24 plan(s) skipped, none aimed at the game |
| `258587-8894337.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 24 plan(s) skipped, none aimed at the game |
| `361579-8918571.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 8 plan(s) skipped, none aimed at the game |
| `367706-8917645.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 369 plan(s) skipped, none aimed at the game |
| `368825-8901843.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 13 plan(s) skipped, none aimed at the game |
| `411813-8925261.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`) |
| `452834-8914313.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 1 plan(s) skipped, none aimed at the game |
| `470193-8893473.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 45 plan(s) skipped, none aimed at the game |
| `531761-8889669.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 98 plan(s) skipped, none aimed at the game |
| `535489-8909383.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 24 plan(s) skipped, none aimed at the game |
| `547434-8889946.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`) |
| `582327-7898857.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 13 plan(s) skipped, none aimed at the game |
| `639842-8906536.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 41 plan(s) skipped, none aimed at the game |
| `689467-8889627.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 22 plan(s) skipped, none aimed at the game |
| `714059-8925906.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 6 plan(s) skipped, none aimed at the game |
| `938643-8912168.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`); 6 plan(s) skipped, none aimed at the game |
| `ConfigManager-fabric-26.1_26.2-1.1.3.jar` | `ran-hollow` | ran to Done, but the unmodelled-host-call count was not recorded (`unknown_source=none`) |

_39 more row(s) are in the ledger, not this table._
<!-- java-mod-ledger:end -->

After a ledger run, regenerate the block without touching a jar set again:

```sh
python3 tools/java_mod_server_ledger.py --from tools/real_mod_fixtures/LAST_SERVER_LEDGER_223.txt \
  --docs docs/java-mod-compat-status.md
```

`--table` prints the same block without writing it, and `--table-limit N` sets how many per-jar rows it keeps.

### Tier coverage over the whole corpus

Every jar in both corpora gets exactly one tier. `native-ok` means the dedicated server did not refuse it; the
ledger's label says what that run was: `ran-clean` (measured clean), `ran-hollow` (ran, but leaned on unmodelled
host calls or nobody counted), `client-only-untested`, or `library` (the server role had nothing to run).
`needs-loader` means native execution cannot
carry it but a real Fabric loader launch of its pack lists it; the row also says whether that launch reached
`Done (`. `native-gap` means neither tier carries it, with the cause. The block below is generated from the
two committed server ledgers and the two committed loader proof ledgers:

<!-- java-mod-coverage:start -->
_Generated by `tools/hybrid_pack.py --coverage --docs docs/java-mod-compat-status.md` from `LAST_SERVER_LEDGER_223.txt`, `LAST_SERVER_LEDGER_500.txt`, `LAST_LOADER_PROOF_223.txt`, `LAST_LOADER_PROOF_500.txt`; edit the tool, not this block._

**599 unique jars (723 rows across 2 corpora): 486 `native-ok` (0 `ran-clean`, 138 `ran-hollow`, 297 `client-only-untested`, 51 `library`), 107 `needs-loader`, 6 `native-gap`, 0 out of scope.**

`native-ok` is the phase-gate tier: the jar is not a gap. Only `ran-clean` says the server ran its entrypoint to `Done (` with no unmodelled host call. `ran-hollow` ran, but a call the host does not model returned a neutral value (or no ledger recorded the count: `unknown_source=none`). `client-only-untested` and `library` were skipped on the server, so this report says nothing about whether they work.

| Corpus | Jars | `native-ok` | `needs-loader` | `native-gap` | out of scope |
|---|---:|---:|---:|---:|---:|
| `corpus-xl` | 223 | 182 | 36 | 5 | 0 |
| `mega` | 500 | 416 | 82 | 2 | 0 |
| union (SHA-256) | 599 | 486 | 107 | 6 | 0 |

| Corpus | `ran-clean` | `ran-hollow` | `client-only-untested` | `library` |
|---|---:|---:|---:|---:|
| `corpus-xl` | 0 | 58 | 103 | 21 |
| `mega` | 0 | 116 | 262 | 38 |
| union (SHA-256) | 0 | 138 | 297 | 51 |

**Loader proof.** 116 of 118 `needs-loader` rows are proven: a real Fabric loader launch lists the jar, reaches `Done (` and no failure line of that run names its mod id. 2 are listed without that proof (below), and 0 have no loader evidence.

**Native refusals absorbed by the loader tier (13 rows).** They are `native-gap` in the server ledger; the real loader lists them, so the tier coverage counts them as `needs-loader`.

| Cause | Rows |
|---|---:|
| metadata | 6 |
| runtime | 4 |
| other | 3 |

**Uncovered rows (7).**

| Jar | Corpus | Cause |
|---|---|---|
| `1399257-8910835.jar` | `corpus-xl` | the server runs it; no loader run covered it |
| `241160-8893517.jar` | `corpus-xl` | the server runs it; no loader run covered it |
| `48facdb59d-MossyLib-1.6.0+26.3+fabric.jar` | `mega` | the server runs it; no loader run covered it |
| `5ecc428a87-easyanvils-v26.3.1-mc26.3.x+fabric.jar` | `mega` | the server runs it; no loader run covered it |
| `809311-8919465.jar` | `corpus-xl` | native refused (runtime): uncaught exception java/lang/ExceptionInInitializerError (at io/github/suel_ki/beautify/common/block/BookStack#<clinit> +198; caused by java.lang.NullPointerEx…; no loader r… |
| `MossyLib-1.6.0+26.3+fabric.jar` | `corpus-xl` | the server runs it; no loader run covered it |
| `StringDupersReturn-1.0.0-Fabric-26.3.jar` | `corpus-xl` | the server runs it; no loader run covered it |

**Listed but not proven (2).** The loader discovered these jars, but the launch did not reach `Done (` or a failure line names them.

| Jar | Corpus | Loader id | Why |
|---|---|---|---|
| `3c1a43daba-sway-2.4.5-fabric+26.3.jar` | `mega` | `sway` | the real Fabric loader lists it but the run did not boot (client-only code on a server) |
| `4ed60c0212-combat-hitboxes-1.0.11.jar` | `mega` | `combat-hitboxes` | the real Fabric loader lists it but the run did not boot (client-only code on a server) |
<!-- java-mod-coverage:end -->

A row whose reason says `requires minecraft …` is out of scope: the jar was built for another game version
(1.21, 26.2), so no 26.3 tier can carry it and it belongs out of the corpus rather than in a compatibility
fix. Of the five `listed but not proven` rows, four ship client code the server role loads: CreativeCore 2.14.16
(both copies) and sway put client mixins (`GameRenderer`, `LevelRenderer`) in a common config, and
combat-hitboxes' `main` entrypoint reaches client classes. A plain `KnotServer` launch of sway or CreativeCore
with Fabric API, outside Horizon, fails the same way, so this is the jar's side, not the runner. They stay
`needs-loader` because a client pack uses the loader half; proving them needs a client launch, which the
server corpus does not carry. The fifth, moderner_beta, reaches `Done (` but logs
`Registry 'moderner_beta:blocksource' was empty after loading` at ERROR, which the proof does not wave through.

Re-proved 7 November 2026 with every row launched again (`--reprove`): the previous ledgers had counted
corpus-xl's CreativeCore as booted from a run that applied no mixins, and both Axiom versions as not booted.
Axiom shades slf4j 1.x, which muted the launched server's log (fixed in `horizon-jvm`, see
[Hybrid sessions](#hybrid-sessions-a-native-half-and-a-loader-half)); a launch now records the first failure
line naming each mod (`exception=`), and a row with one is not proven.

Regenerate it without starting a game:

```sh
python3 tools/hybrid_pack.py --coverage \
  --corpus corpus-xl=~/horizon-corpus/corpus-xl --corpus mega=~/horizon-corpus/mega \
  --ledger tools/real_mod_fixtures/LAST_SERVER_LEDGER_223.txt \
  --ledger tools/real_mod_fixtures/LAST_SERVER_LEDGER_500.txt \
  --loader-ledger tools/real_mod_fixtures/LAST_LOADER_PROOF_223.txt \
  --loader-ledger tools/real_mod_fixtures/LAST_LOADER_PROOF_500.txt \
  --docs docs/java-mod-compat-status.md
```

`--measure-proof` fills the proof ledgers: for every row without a proof it launches the real loader with
the jar's declared dependency closure (`provides` aliases included), splits a batch that cannot boot, gives an
unproven target one run of its own, and retries with each `--proof-loader`. `--reprove` launches every row
again instead of trusting the committed listings, and `--proof-ledger NAME=PATH` writes a corpus's proof
ledger in the committed format. The sweep that wrote the committed ledgers (about 190 launches, one JVM at a
time, the runner's default `-Xmx1g`):

```sh
python3 tools/hybrid_pack.py --coverage \
  --corpus corpus-xl=~/horizon-corpus/corpus-xl --corpus mega=~/horizon-corpus/mega \
  --ledger tools/real_mod_fixtures/LAST_SERVER_LEDGER_223.txt \
  --ledger tools/real_mod_fixtures/LAST_SERVER_LEDGER_500.txt \
  --loader-ledger tools/real_mod_fixtures/LAST_LOADER_PROOF_223.txt \
  --loader-ledger tools/real_mod_fixtures/LAST_LOADER_PROOF_500.txt \
  --measure-proof --reprove --proof-universe ~/horizon-corpus/mega-deps \
  --proof-loader ~/horizon-corpus/fabric-libs-98/net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar \
  --proof-loader ~/horizon-corpus/fabric-loader-0.19.5.jar \
  --proof-ledger corpus-xl=tools/real_mod_fixtures/LAST_LOADER_PROOF_223.txt \
  --proof-ledger mega=tools/real_mod_fixtures/LAST_LOADER_PROOF_500.txt
```

`--strict` exits non-zero while a `native-gap` or unproven row remains.

### Phase 2 score: native-only over the loader tier

The phase-1 table says which jars the loader carries. Phase 2 asks the other question of the same rows: with
the loader gone, does a fresh native-only dedicated run carry the jar on its own?
`tools/hybrid_pack.py --native-score` runs each unique `needs-loader` jar exactly like the committed server
ledger — one `horizon-client --java-mod <jar> --dedicated` process, flat world, 60 s bound — reads its
`java-runtime.json` row and its plan report, and calls it `loader-free` only when the run reaches `Done (`,
executes a server entrypoint and has no server-side plan whose target lives outside the jar. A jar
that still has such a plan is `loader`; a refusal or a run that never reaches `Done (` is `refused`. Client-only
mixins do not count against the server role. The same bytes in both corpora run once and answer both rows. The
block below is generated, and the trend keeps one point per measurement so a fix that moves a jar is visible:

<!-- java-mod-native-score:start -->
_Generated by `tools/hybrid_pack.py --native-score --docs docs/java-mod-compat-status.md` from `LAST_SERVER_LEDGER_223.txt`, `LAST_SERVER_LEDGER_500.txt`, `LAST_LOADER_PROOF_223.txt`, `LAST_LOADER_PROOF_500.txt`; edit the tool, not this block._

**Phase 2 score: 157 of 174 `needs-loader` rows (150 unique jars) run natively on a fresh native-only run; 33 are loader-free, 124 still need the loader, 17 refused, 0 skipped, 0 missing.**

| Status | Rows | What it means |
|---|---:|---|
| `loader-free` | 33 | runs, and no server-side plan targets a class outside the jar |
| `loader` | 124 | runs, but still skips at least one server-side game-class plan |
| `refused` | 17 | native execution refuses it or the run did not reach `Done (` |

**Trend.** One point per measurement: a rerun with the same commit, binary and counts replaces the last row instead of adding a duplicate.

| Date (UTC) | Commit | Binary | Hooks | Rows | Unique | `loader-free` | still need loader | refused | plans skipped | game plans |
|---|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| 2026-11-06T12:08:39Z | `efca454f4` | 20261106-2255 | off | 174 | 150 | 30 | 94 | 50 | 5028 | 1285 |
| 2026-11-08T01:26:07Z | `8a3c8dc59` | 20261108-1200 | on | 174 | 150 | 33 | 124 | 17 | 6753 | 1786 |

**Loader-free (33).** The run reached `Done (` and no server-side plan targets a class outside the jar.

| Jar | Corpus | Engine | Skipped | Server plans |
|---|---|---|---:|---:|
| `368825-8901843.jar` | `corpus-xl` | native-qualified | 13 | 0 |
| `410295-8911219.jar` | `corpus-xl` | native-qualified | 8 | 0 |
| `456640-8026052.jar` | `corpus-xl` | native | 0 | 1 |
| `526901-8925000.jar` | `corpus-xl` | native-qualified | 1 | 0 |
| `734339-8931641.jar` | `corpus-xl` | native-qualified | 4 | 0 |
| `882495-8910991.jar` | `corpus-xl` | native-qualified | 2 | 0 |
| `BedrockSkins-26.3-2.1.1.jar` | `corpus-xl` | native-qualified | 90 | 0 |
| `PatPat-1.3.2+26.3+fabric.jar` | `corpus-xl` | native-qualified | 35 | 0 |
| `advancementscreenshot-26.3.0-5.3.jar` | `corpus-xl` | native-qualified | 6 | 0 |
| `betterstats-5.6.0-beta.2+fn-26.3.jar` | `corpus-xl` | native-qualified | 1 | 0 |
| `betterstats-5.7.0+fn-26.3.jar` | `corpus-xl` | native-qualified | 1 | 0 |
| `clientsort-fabric-3.104.1+26.3.jar` | `corpus-xl` | native-qualified | 31 | 0 |
| `fix-keyboard-on-linux-1.0.2.jar` | `corpus-xl` | native-qualified | 2 | 0 |
| `jei-26.3-fabric-31.16.0.83.jar` | `corpus-xl` | native-qualified | 25 | 0 |
| `simplemenu-26.3.0-2.3.jar` | `corpus-xl` | native-qualified | 39 | 0 |
| `smooth_f5-fabric-1.3.2.jar` | `corpus-xl` | native-qualified | 8 | 0 |
| `0da0ce012a-betterstats-5.7.0+fn-26.3.jar` | `mega` | native-qualified | 1 | 0 |
| `26baec9303-NoChatRestrictions-Fabric-MC26.3-v1.3.0.jar` | `mega` | native-qualified | 5 | 0 |
| `2c0e2ba8a9-midnightlib-fabric-1.9.3+26.3-rc-2.jar` | `mega` | native-qualified | 3 | 0 |
| `3b90f638d6-jei-26.3-fabric-31.9.0.61.jar` | `mega` | native-qualified | 15 | 0 |
| `4ed60c0212-combat-hitboxes-1.0.11.jar` | `mega` | native-qualified | 3 | 0 |
| `53d859f95b-betterstats-5.6.0-beta.2+fn-26.3.jar` | `mega` | native-qualified | 1 | 0 |
| `5a2f06f60a-betterstats-5.8.0+fn-26.3.jar` | `mega` | native-qualified | 1 | 0 |
| `61412064ba-konkrete_fabric_1.11.1_MC_26.3.jar` | `mega` | native-qualified | 8 | 0 |
| `7751ee3925-locator-heads-2.5.0.jar` | `mega` | native-qualified | 7 | 0 |
| `7b1bf05fde-runes-fabric-1.3.2+26.3.jar` | `mega` | native-qualified | 1 | 0 |
| `96cbf8e292-clientsort-fabric-3.104.1+26.3.jar` | `mega` | native-qualified | 31 | 0 |
| `9c77a694f8-bkupsV3-fabric-mod-26.3.jar` | `mega` | native-qualified | 5 | 0 |
| `9ce277e691-particular-26.3-Fabric-1.5.7.jar` | `mega` | native-qualified | 36 | 0 |
| `cc53b1cd2b-fullbrightnesstoggle-26.3.0-4.9.jar` | `mega` | native-qualified | 6 | 0 |
| `dea53a10e5-CrashAssistant-fabric-26.2-26.3-1.11.14.jar` | `mega` | native-qualified | 7 | 0 |
| `e2f129702a-jei-26.3-fabric-31.9.0.56.jar` | `mega` | native-qualified | 14 | 0 |
| `f0b01c55aa-midnightlib-fabric-1.10.0+26.3.jar` | `mega` | native-qualified | 3 | 0 |

**Still needs the loader (124).**

| Jar | Corpus | Server game plans | Server plans |
|---|---|---:|---:|
| `248787-8907912.jar` | `corpus-xl` | 2 | 2 |
| `257814-8894326.jar` | `corpus-xl` | 5 | 5 |
| `326652-8890593.jar` | `corpus-xl` | 4 | 4 |
| `342584-8921261.jar` | `corpus-xl` | 79 | 79 |
| `348831-8926733.jar` | `corpus-xl` | 15 | 15 |
| `367706-8917645.jar` | `corpus-xl` | 3 | 3 |
| `419699-8915075.jar` | `corpus-xl` | 45 | 45 |
| `511733-8929530.jar` | `corpus-xl` | 2 | 2 |
| `531761-8889669.jar` | `corpus-xl` | 26 | 26 |
| `608235-8905060.jar` | `corpus-xl` | 4 | 4 |
| `623764-8914234.jar` | `corpus-xl` | 6 | 6 |
| `639842-8906536.jar` | `corpus-xl` | 6 | 6 |
| `832882-8896904.jar` | `corpus-xl` | 16 | 16 |
| `888790-8002356.jar` | `corpus-xl` | 1 | 1 |
| `CustomPlayerModels-Fabric-26.3-0.6.27e.jar` | `corpus-xl` | 3 | 3 |
| `Jade-mc26.3-Fabric-26.3.5.jar` | `corpus-xl` | 2 | 2 |
| `architectury-fabric-22.0.4.jar` | `corpus-xl` | 45 | 45 |
| `armor-stand-poses-fabric-26.3-1.4.0.jar` | `corpus-xl` | 5 | 5 |
| `baguettelib-26.3-Fabric-2.0.7.jar` | `corpus-xl` | 12 | 12 |
| `bannerpoint-fabric-1.1.2+26.3.jar` | `corpus-xl` | 10 | 10 |
| `collective-26.3.0-8.42.jar` | `corpus-xl` | 81 | 81 |
| `collective-26.3.0-8.46.jar` | `corpus-xl` | 81 | 81 |
| `cropsloverain-3.2.1.jar` | `corpus-xl` | 8 | 8 |
| `kreno_fpatcher-26.2.0+26.1.jar` | `corpus-xl` | 2 | 2 |
| `mcwifipnp-2.1.4-26.3-fabric.jar` | `corpus-xl` | 1 | 1 |
| `moderner-beta-fabric-5.0.0-alpha.4+26.3.jar` | `corpus-xl` | 14 | 14 |
| `modernregsyncfix-2.0.0.jar` | `corpus-xl` | 1 | 1 |
| `naturally_trimmed-3.6.1+26.3-fabric.jar` | `corpus-xl` | 4 | 4 |
| `peek-fabric-1.6.0+26.3.jar` | `corpus-xl` | 1 | 1 |
| `pickupnotifications-fabric-3.0.1+26.3.jar` | `corpus-xl` | 4 | 4 |
| `puzzleslib-v26.3.12-mc26.3.x+fabric.jar` | `corpus-xl` | 49 | 49 |
| `puzzleslib-v26.3.13-mc26.3.x+fabric.jar` | `corpus-xl` | 49 | 49 |
| `puzzleslib-v26.3.14-mc26.3.x+fabric.jar` | `corpus-xl` | 49 | 49 |
| `swinginglanterns-fabric-26.3-2.0.3.jar` | `corpus-xl` | 6 | 6 |
| `takeitout-1.1.28-26.3.jar` | `corpus-xl` | 4 | 4 |
| `tcdcommons-5.6.0-beta.2+fn-26.3.jar` | `corpus-xl` | 2 | 2 |
| `tcdcommons-5.7.0+fn-26.3.jar` | `corpus-xl` | 2 | 2 |
| `whiteout-0.21.0.jar` | `corpus-xl` | 25 | 25 |
| `xaeroworldmap-fabric-26.3-1.47.0.jar` | `corpus-xl` | 6 | 6 |
| `02d17d2f1c-Axiom-6.1.5-for-MC26.3.jar` | `mega` | 24 | 24 |

_84 more row(s) are in the score ledger._

**Not runnable natively (17).**

| Jar | Corpus | Status | Why |
|---|---|---|---|
| `1283899-8898481.jar` | `corpus-xl` | `refused` | no native entrypoint |
| `Legacy4J-26.3-1.9.1-fabric.jar` | `corpus-xl` | `refused` | with the pack class path: Metadata("malformed java/util/Iterator marker (at wily/legacy/block/LegacyBlockBehaviors#setup +24)"); alone: Metadata("malformed java/lang/Boolean marker (at wily/legacy/in… |
| `servercore-fabric-1.5.20+26.3.jar` | `corpus-xl` | `refused` | with the pack class path: uncaught exception java/lang/ExceptionInInitializerError (at space/arim/dazzleconf/internal/DefinitionReader#readAndSortEntries +48; caused by space.arim.dazzleconf.error.Il… |
| `zoomify-2.17.0+mc26.3-universal.jar` | `corpus-xl` | `refused` | no native entrypoint |
| `2280c8ca8d-shulkerboxtooltip-fabric-5.4.2+26.3.jar` | `mega` | `refused` | Metadata("java/lang/String#charAt character index out of bounds (at com/misterpemodder/shulkerboxtooltip/shadowed/blue/endless/jankson/api/Escaper#escapeString +295)") |
| `286a800d23-emotecraft-for-MC26.3-3.5.0-b.build.179.jar` | `mega` | `refused` | Metadata("expected a native stream marker (at io/github/kosmx/emotes/server/serializer/UniversalEmoteSerializer#<clinit> +5)") |
| `4881abae4b-archers-fabric-3.1.4+26.3.jar` | `mega` | `refused` | uncaught exception java/lang/NullPointerException (at net/archers/entity/ArcherEntities#register +76) |
| `7b10411d20-worldedit-7.4.6-beta-02.jar` | `mega` | `refused` | Metadata("expected a native stream marker (at org/enginehub/piston/util/CaseHelper#titleToSpacedLower +15)") |
| `7f18090fea-zoomify-2.17.0+mc26.3-universal.jar` | `mega` | `refused` | no native entrypoint |
| `8f7962199e-worldedit-mod-7.4.6.jar` | `mega` | `refused` | Metadata("expected a native stream marker (at org/enginehub/piston/util/CaseHelper#titleToSpacedLower +15)") |
| `92815a8e4c-emotecraft-for-MC26.3-3.5.0-a.build.171.jar` | `mega` | `refused` | Metadata("expected a native stream marker (at io/github/kosmx/emotes/server/serializer/UniversalEmoteSerializer#<clinit> +5)") |
| `9cc655e54a-cristellib-fabric-26.3-3.1.13.jar` | `mega` | `refused` | Metadata("class de.cristelknight.cristellib.CristelLib failed to initialize earlier: Metadata(\"game hook callback calls net/minecraft/SharedConstants#getCurrentVersion()Lnet/minecraft/WorldVersion;,… |
| `d068d42039-wizards-fabric-3.1.4+26.3.jar` | `mega` | `refused` | uncaught exception java/lang/NullPointerException (at net/wizards/entity/WizardEntities#register +76) |
| `e81054001c-shulkerboxtooltip-fabric-5.4.3+26.3.jar` | `mega` | `refused` | Metadata("java/lang/String#charAt character index out of bounds (at com/misterpemodder/shulkerboxtooltip/shadowed/blue/endless/jankson/api/Escaper#escapeString +295)") |
| `e878fad023-PlayerAnimationLibMerged-1.2.8+mc.26.3.jar` | `mega` | `refused` | no native entrypoint |
| `ec5fdf4798-servercore-fabric-1.5.20+26.3.jar` | `mega` | `refused` | with the pack class path: uncaught exception java/lang/ExceptionInInitializerError (at space/arim/dazzleconf/internal/DefinitionReader#readAndSortEntries +48; caused by space.arim.dazzleconf.error.Il… |
| `fca854f8ee-paladins-fabric-3.1.4.1+26.3.jar` | `mega` | `refused` | uncaught exception java/lang/ExceptionInInitializerError (at net/paladins/item/PaladinShields$1#<clinit> +19; caused by java.lang.ArrayIndexOutOfBoundsException: Index 0 out of bounds for length 0) |
<!-- java-mod-native-score:end -->

Regenerate it with about 150 dedicated runs:

```sh
python3 tools/hybrid_pack.py --native-score --docs docs/java-mod-compat-status.md
```

`--score-limit N` runs a prefix as a smoke test, `--strict` exits 1 while a row still needs the loader,
`--score-game-hooks` measures the opt-in `HORIZON_JAVA_GAME_HOOKS` tier instead of the default weave, and
`--plan-probe` points at the `horizon-native-java` used for the server-side plan count.

### Hybrid sessions: a native half and a loader half

The label decides where a jar runs: the four `native-ok`-tier labels (`ran-clean`, `ran-hollow`,
`client-only-untested`, `library`) in one `horizon-client --java-mod` process, `needs-loader` in
`horizon-jvm --real-loader --launch-game`, and `native-gap` in neither unless the loader proof lists it
(`--loader-ledger`, a listing that reached `Done (`) or `--gap-to-loader` routes every refusal there.
`tools/hybrid_pack.py` partitions a pack by these ledgers (falling back to SHA-256 when the pack's filenames differ
from the corpus), pulls every dependency of a loader jar into the loader half, moves a native jar whose dependency
only the loader half runs into the loader half too (`follows-dependency`, repeated until no native jar is
stranded), checks the pack's own declarations, and `--measure` runs both halves. The design and the two launch
commands are in [java-mod-compat.md](java-mod-compat.md#hybrid-sessions-a-native-half-and-a-loader-half).

Measured 7 November 2026 on the 112-jar Helix pack, matched to the two committed server ledgers by content hash
and to the two proof ledgers (horizon-client and horizon-jvm built from this branch that day; server role, flat
world, each half stops at `Done (`, 180 s bound):

| Half | Jars | Executed | Mixin plans applied | Wall |
|---|---:|---:|---:|---:|
| native-only (native half) | 71 | 11 entrypoints | 0 (891 skipped) | 3.6 s |
| hybrid — native half | 71 | 11 entrypoints | 0 (891 skipped) | 4.3 s |
| hybrid — loader half | 43 | 74 mods loaded, `Done (` | 477 (53 configs, 254 targets, 0 failed) | 15.3 s |

Both halves complete. The loader half carries 21 `needs-loader` jars, 8 `native-gap` jars the loader proof lists
(nochatreports, clientsort, collective, modernfix, puzzleslib, swinginglanterns, worldedit, xaeroworldmap), 10
native-ok-tier libraries shared with the native half, and 4 non-gap jars that follow their dependency
(litematica-printer, pickupnotifier, stackrefill, wakes). The static check reports 0 conflicts (was 5
`cross-partition-dependency`) and 42 warnings: 34 `declared-break` version ranges it does not evaluate, 7 jars with
no ledger row, and the excluded sway. The loader half's log has one ERROR line, vanilla's
`No key layers in MapLike[{}]` from the flat world, and no exception. The native half is the same process as before
and still applies no game mixins; its entrypoint count moved from 18 to 11 because the four add-ons left it and the
binary changed under concurrent work, so the two native rows are this build's, not a trend.

The 6 November run that "timed out at 180 s while transforming `NaturalSpawner`" was not a hang. Axiom shades
slf4j 1.x's `org/slf4j/LoggerFactory`; the launch kept the game's logging jars parent-visible only, so Knot's
child-first lookup resolved Minecraft's logger from Axiom, SLF4J bound its NOP logger and every vanilla line,
`Done (` included, vanished while the server ran (a thread dump showed it waiting for its next tick, and it wrote
region files). Fabric's own launch adds the game's log jars to Knot before any mod, so the game's copy wins;
`horizon-jvm` now hands Fabric a copy of such a jar without the classes a logging jar on the class path already
provides. Axiom is the only corpus jar that shades them.

What is not shown: a client-role hybrid (the native half as the window, the loader half as a client), a pack with
players connected, and any pack but Helix. Sway stays out because it cannot boot a server under any loader.

## Measured efficiency

Host: 16 CPUs, load average 9.6–13 during the runs (shared box). Medians; client figures are medians of three
interleaved runs in rotated order, server figures median of six. The rerun recipe is [efficiency.md](efficiency.md)
(`tools/efficiency/`); this run's raw reports are in `~/horizon-corpus/` (`EFFICIENCY.md`, `efficiency-*.json`, and
the `efficiency-scripts/` the suite was written from).

### Dedicated server, vanilla 26.3 vs Horizon (no java mods), same `server.properties`

| Metric (median) | Vanilla 26.3 | Horizon | Difference |
|---|---:|---:|---:|
| Wall to listening port | 7.09 s | 3.22 s | **2.2× faster** |
| Wall to `Done (` | 7.86 s | 3.22 s | **2.4× faster** |
| Server's own `Done (x.xxxs)` timer | 0.61 s | 3.20 s | 5.2× slower, different span* |
| RSS at ready | 1312 MiB | 248 MiB | **5.3× less** |
| Threads at ready | 102 | 21 | **4.9× fewer** |

\* Vanilla's timer starts after ~7.3 s of JVM startup, registries, datafixers and its 68 MiB library bundle; Horizon's
3.20 s is effectively the whole process (resources 1.9 s, worldgen data 0.39 s, level prep). Wall clock is the honest
comparison, where Horizon wins by 4.6 s. Horizon opens the listening socket at `Done` (0.04 ms gap), so a client
never connects to a world that is not ready.

### Client scaling with native java mods (offscreen, render distance 8, `maxFps` 260)

0/3/20/74 jars are filename-sorted slices of the natively-runnable set, not a curated pack.

| Jars | Effective client mods | Startup | RSS | avg fps | p50 | p95 | low 1% |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 | 0 | 4080 ms | 1179 MiB | 1556 | 0.62 ms | 0.89 ms | 494 |
| 3 | 3 | 4150 ms | 1229 MiB | 1714 | 0.58 ms | 0.64 ms | 543 |
| 20 | 19 | 5150 ms | 1698 MiB | 1529 | 0.62 ms | 0.82 ms | 452 |
| 74 | 65 | 8410 ms | 2089 MiB | 1629 | 0.60 ms | 0.67 ms | 564 |

Frame time is mod-insensitive in this fixture: tick and update stay ~0.01 ms and render ~0.2 ms per frame; the mod
tax is startup (the Java entrypoint plus its share of resource work, ~60 ms per effective mod) and memory
(~14 MiB per mod). Threads stay at 61 regardless of jar count.

### Horizon dedicated startup with native jars

| Jars passed | Effective server mods | Time to listening port/`Done` | RSS | Threads |
|---:|---:|---:|---:|---:|
| 0 | 0 | 2.39 s | 265 MiB | 21 |
| 25 | 25 | 3.26 s | 544 MiB | 21 |
| 50 | 50 | 3.82 s | 700 MiB | 21 |
| 74 | 51 | 4.39 s | 695 MiB | 21 |
| 89 (all non-gap) | 52 | 5.22 s | 689 MiB | 21 |

Only 52 of the 89 natively-runnable jars declare a dedicated-server entrypoint, so the 74-jar point is 51 executed
mods and time/RSS flatten past 50. There were zero fallbacks to the JVM and zero startup failures in any run.

### 500-jar native profile

Measured on 2026-11-06 (16 CPUs) over the same 500 jars, one flat-world dedicated run per point, `Done (N.NNNs)!`
and peak RSS; a 0-jar run takes 2.2-2.4 s, so "jar work" is wall time over that same binary's baseline. Both rows
use **one binary**, and only `HORIZON_WORKER_THREADS` differs (`1` against the machine's 16): the difference is the
bootstrap pool, not another change landing between runs.

| 500 jars | Wall to `Done` | Jar work | Peak RSS | Started |
|---|---:|---:|---:|---:|
| serial (`HORIZON_WORKER_THREADS=1`) | 15.90 s | 13.5 s | 828 MiB | 191 |
| parallel (default pool) | **10.46 s** | 8.2 s | 859 MiB | 191 |
| bootstrap batch, serial | 10.22 s | — | — | 191 |
| bootstrap batch, parallel | **5.14 s** | — | — | 191 |

The batch is most of the run and the pool takes 5.1 s off it (10.22 -> 5.14); the rest of startup is unchanged, so
the whole run drops 5.4 s. Peak RSS grows 31 MiB, because up to eight jars are parsed and bootstrapped at once.
**191 of the 500 started in that profile run**; the 19:02 binary's ledger starts 197 and skips 7,742 plans, so the
profile remains a parser/loader measurement, not a claim that 500 effective server mods run.

Caveats: the host was loaded (absolute fps is depressed and noisy); vanilla ran `-Xmx2G` and RSS is measured at
`Done`; the client ran offscreen with `maxFps=260` because the default 120 fps cap hides all frame differences.

### Bootstrap content cache (AOT)

What a jar declares depends on its bytes, this build's engine and its role, so a jar that started once is stored and
replayed instead of parsed and executed again (`game/java_cache.rs`). Measured 6 November 2026 (16 CPUs, load
average about 10) on the first 100 of the 197 jars that start natively from the 500-jar corpus: one flat-world
dedicated run per point, one game folder, `HORIZON_HOME` and world reused, the cache cleared before each cold run,
eight cold and eight warm starts alternating.

| Start | Wall to `Done` (8 runs) | Median | Peak RSS (median) | Reused |
|---|---|---:|---:|---:|
| cold (empty cache) | 2.88–4.34 s | 3.29 s | 821 MiB | 0 of 100 |
| warm (filled cache) | 2.82–3.73 s | 3.45 s | 722 MiB | 86 of 100 |

On this loaded host the wall difference is inside the run-to-run noise: the eight pairs average 0.02 s in the
warm start's favour and range from 0.58 s against it to 0.85 s for it (an earlier, quieter run of the same suite
measured 2.48 s cold against 2.41 s warm). What is stable is memory: the warm start's median peak RSS is ~100 MiB
lower because 86 jars are replayed rather than parsed at once, and its `java-runtime.json` is byte-identical to the
cold run's. The fourteen jars not reused declare payload channels, whose handlers need the bootstrapped host rather
than rows; the cache held 86 entries in 436 KiB.

## Known gaps

1. **Game-class mixins do not apply.** The load weave only rewrites classes inside the jar; a plan whose target is a
   Minecraft class is skipped. That is every plan that matters (0 woven; 2,944 and 7,742 skipped), so `ran` means
   "entrypoint executed, mixins skipped, vanilla server reached `Done`", not "the mod works". The opt-in hook tier
   (`HORIZON_JAVA_GAME_HOOKS=1`) fires its 19 public sites and 22 internal Java-tier ones (value-returning
   `getDestroySpeed`, `growCrop` and `hurtServer`, `PlayerList#remove`, `stopServer`'s tail, the player-tick heads,
   `Player#tick`, the server and level ticks, the server's constructor tails, `Bootstrap`, `Block#playerDestroy`,
   `ServerExplosion#explode`, `dropAllDeathLoot` and `LivingEntity#die`'s tail) from the game's own points; on a
   live dedicated server a probe jar's cancels and overrides held at `hurtServer`, `playerDestroy`,
   `getDestroySpeed` and `growCrop`, while the corpus callbacks there ran or refused as listed in
   [java-mod-compat.md](java-mod-compat.md). It runs a config's `IMixinConfigPlugin` the way Mixin
   does and binds MixinExtras' `@ModifyReturnValue`. Statically counted on 7 November 2026 (`plugin_census` in
   `mixin_plugins.rs`): of the loader tier's 1,285 server-side game plans, 224 name a site in a served shape and
   190 bind (was 167 and 51; 155 of the 190 pass a plugin's session, and plans stopped at `config-plugin` fell
   from 144 to 42); across both corpora (599 unique jars) 222 bind (was 87). The plugins left are led by ModernFix's
   config (`org.objectweb.asm.Type#getDescriptor`, 116 questions in 2 jars), plugin classes that live in another
   jar (`xaero.lib`'s, Collective's, MixinConstraints'; a lone jar cannot load them), and plugins extending a base
   class outside the jar. MixinExtras is mostly mid-method: across both corpora 424 `@WrapOperation` and 366
   `@ModifyExpressionValue` server-side game plans have no hook point that brackets their instruction, 72
   `@WrapMethod` need the original callable, and 4 of 133 `@ModifyReturnValue` bind. A bound callback still stops
   at the first member the hook tier does not model. On a
   dedicated server with two bot players and eight of those jars, 4 jars' callbacks ran (appleskin and syncmatica
   `player_logged_in` 2 each, shogi `player_tick` 1,457, servux `server_stopping` 1) and the rest refused and were
   disabled (`ServerPlayNetworking.canSend`, `Connection#getRemoteAddress`, a `@Shadow` field, Guava's `Ordering`).
   The objects an entrypoint builds now outlive it (`heap.rs`, a resident heap per jar and role shared by its payload
   handlers, listeners and hooks): measured 7 November 2026 on 172 corpus jars with server listeners or bound hooks
   (four dedicated batches, four `horizon-bots` players each, the same binary with the heap switched off and on),
   refused listeners fell from 62 to 57 of 325, failed game hooks from 28 to 23, and refusals on a lost collection
   (`got null`) from 11 to 3. servux keeps its `ServerInitHandler` list (one `ServuxInitHandler`), but its
   `server_stopping` callback still walks an empty `ServerHandler` list: the plan that fills it injects at
   `DedicatedServer.<init>` `TAIL` (`java_dedicated_server_constructed`), which fires but refuses at `RegistryAccess.EMPTY`. A `<clinit>` that a strict hook firing refused no longer
   leaves its class half-initialised (jamlib's `Event#listeners`): the class is erroneous and every later access is
   refused with the same cause (`class_init.rs`).
2. **Refusals.** The committed ledgers (an older binary) count 27 of 121 wide and 68 of 265 mega jars that declare a
   server entrypoint as failing (22%/26%). A fresh dedicated-server re-measure on 7 November 2026 (all workstreams'
   fixes, one server per jar, 60 s) counts **5 wide and 15 mega `native-gap` rows**. The entrypoint-exception bucket
   (54 rows across both ledgers: null-config/listener NPEs, `Unexpected subclass`, loader-detection guards,
   `AssertionError`s, `No value present`, invalid ids, the nested-frame bound, `StackUnderflow`, the duplicate item)
   fell from 22 still failing to 4: static Mixin accessors run their generated contract, reflection answers the
   class file (superclass, assignability, enums, constructors, nested simple names, annotations with defaults,
   `Modifier`), JDK method references keep their arguments, concrete virtual calls reach overrides, pattern-matching
   `switch` sites answer, `String.matches` has a bounded regex, `StackWalker` sees jar frames, and game codecs,
   builders, registry keys, `BlockItemId`/`ColorCollection` ids and mod registries are never null. The four left:
   archers and wizards read `spell_engine`'s config classes, which a lone jar does not have (both reach `Done` with
   `spell_engine` and `structure_pool_api` beside them; a real loader would refuse them alone for the missing
   dependency, so they need a dependency-aware ledger row, not an interpreter fix); both WorldEdit builds need a
   Guava collector in `Stream.collect` (stream modelling). Paladins fails the same way as archers. Jars whose
   `fabric.mod.json` says `"environment": "client"` are now client-only skips on a server, as on a real Fabric
   server, including a multi-loader jar's NeoForge `@Mod` class: 16 wide and 54 mega rows that used to "run" there
   moved to `skipped`. The raw log line is bounded at 160 characters, so a human sees a clipped cause; the ledger
   recovers the skip class from the jar's own metadata. The JDK model is bounded, not a JDK: a `subList` is a copy,
   a `Map.Entry` keeps its old value after `setValue` (the map is written), loggers drop everything,
   `String.format` refuses argument indexes and the `+ , # (` flags, and streams are eager with at most 4,096
   elements from a spliterator, range or file walk.
3. **Nested and non-server entrypoints.** Nested JiJ/JarJar jars are probed but never bootstrapped in-game (`preLaunch`
   is not run either); the two mega jars that hit the 128-nested-jar-frame bound (ShulkerBoxTooltip) now run, the
   bound itself is unchanged. The Kotlin entrypoint
   adapter runs, but an adapter the runtime still does not model is refused.
4. **Callbacks.** Entrypoints, received-payload handlers and server-side event listeners execute. Servers fire
   lifecycle, server/level tick, player join/leave and living-death listeners (Fabric, NeoForge, Forge EventBus 7),
   but the façade a listener sees is small: on a dedicated run of 48 corpus jars with server listeners, 42 jars
   kept 138 listeners and 33 were refused at their first unmodelled call, mostly `MinecraftServer#getWorldPath`
   (6), `registryAccess` (6 with the level's), `getResourceManager` (2), and interpreter collection/stream markers.
   Use, attack, dig, break, damage, entity load/unload, respawn, copy, level change and level load/unload
   listeners fire too, in Fabric phase and NeoForge priority order, and a deciding answer stops the vanilla
   action (proven with bots); the corpus listeners for them mostly stop at an unmodelled façade call. Commands registered through Brigadier reach the server's tree and run
   (above), but through the same small façade: of 83 sampled console lines, 23 returned and 26 were refused.
   *Client surface (first slice).* The window fires client tick, level tick, started/stopping and join/disconnect
   listeners (Fabric `ClientTickEvents`/`ClientLifecycleEvents`/`ClientPlayConnectionEvents`, NeoForge
   `ClientTickEvent`/`ClientPlayerNetworkEvent`), lists key mappings (`KeyMappingHelper`, `RegisterKeyMappingsEvent`)
   in Controls under the mod's own lang names and `key_<name>` in `options.txt`, answers `consumeClick`/`isDown`
   from real input, and draws HUD elements (`HudElementRegistry`, `RegisterGuiLayersEvent`) through a bounded
   `GuiGraphicsExtractor` (`text`, `centeredText`, `fill`, `fillGradient`, `outline`, pose translate/scale,
   `Font.width`). On both corpora through the real client batch (723 jars, 480 started in the client role, 7 November
   2026), 152 jars keep a client surface: 286 client listeners (139 `END_CLIENT_TICK` in 94 jars), 219 key mappings
   in 68 jars, 30 HUD elements in 21 jars. Fired in a synthetic world frame (three ticks, three HUD frames), 78 of
   the 316 callbacks refused (91 before the screen/server-data members), led by `getTextureManager` (5),
   `getSingleplayerServer`, `Player#getItemBySlot` (4 each), `Minecraft#execute`, `gameRenderer`, `getActiveEffects`
   (3 each) and interpreter Boolean/stream markers; **no corpus HUD element recorded a draw** there (they gate on
   config objects, items or refuse first). In an offscreen client, light-overlay's three mappings appear in Controls,
   a rebind persists, and its key shows its own "Light Overlay toggled on" in the action bar; DayCount's element
   runs but its cloth-config `AutoConfig` gate reads disabled, so it draws nothing.
   *Client surface (second slice).* The frame also carries the player's inventory, armour, off hand, effects, food,
   air, experience, camera, GUI ticks, render distance and level height: `ItemStack`/`Item`/`Inventory`/
   `MobEffectInstance` are slot markers (`client_items.rs`), `Items`/`Blocks`/`MobEffects` constants answer inside a
   firing so `stack.getItem() == Items.ELYTRA` holds, `Minecraft.execute` queues tasks the window runs after the
   firing, `getSingleplayerServer` is null on a server connection and a bounded stand-in in single player, and
   `getTextureManager` names textures but refuses `register`. `GuiGraphicsExtractor` adds `item`, `fakeItem`,
   `itemDecorations` (drawn by Horizon's item renderer from the real slot stack), `blitSprite` of GUI-atlas sprites
   and the line fills. Styled components are the interpreter's component JSON (`facade_text.rs`), decoded by the
   game's own component decoder; `Style`/`TextColor` objects and `java.awt.Color` constants answer on the client
   (`client_text.rs`). Config libraries answer their defaults (`client_config.rs`): cloth-config `AutoConfig`, YACL
   `ConfigClassHandler` and SuperMartijn642's config lib construct or keep the default values; a saved config file
   is not read yet. Fabric `ItemTooltipCallback` and NeoForge `ItemTooltipEvent` edit Horizon's item tooltips (the
   game's lines keep their style; added lines are the mod's components). A HUD element's first three frames get
   100 ms (class initialisation) and only eight overruns of 4 ms in a row disable it.
   Measured the same way (8 November 2026; the synthetic player holds a damaged pickaxe, tooltip listeners describe
   a damaged diamond sword), 175 jars keep a surface and 82 of 382 callbacks refuse (93 of 366 before this slice);
   **three jars' HUD elements draw** (DayCount, both durabilityviewer copies; slowest frame 1.2 ms), 11 jars keep a
   tooltip listener and Durability Tooltip adds its line. In an offscreen client, DayCount draws "Day: 3" and
   durabilityviewer draws the held pickaxe, armour and free-slot chest; its durability numbers are missing because
   the interpreter resolves an inherited static field through superclasses but not superinterfaces
   (`ItemIndicator.color_green` reads 0, so the text has no alpha); Durability Tooltip's line shows in the
   inventory tooltip. Remaining refusals lead with `getResourceManager`, `MultiPlayerGameMode#canHurtPlayer`,
   `ItemStack#get(DataComponentType)` (4 each), `ClientPacketListener#registryAccess`,
   `IntegratedServer#getWorldPath` (3 each) and interpreter Boolean/stream markers. Screens, renderers and models
   have no façade, and the old façade sources stay in commit `be8567b9a`.
5. **Content consumption.** Blocks, items, effects, entities, menus, block entity types and recipes reach the content
   model; data components and attachments are typed declaration rows that travel in the snapshot and the manifest.
   What is left:
   - *Recipes* load from a jar's `data/<ns>/recipe/*.json` and from typed declarations, but only when the type is a
     vanilla one and every item they name is vanilla or declared by the same jar. On the 25 mega jars that ship mod
     recipes, one dedicated server applies **824** and logs **1,212** left out with a reason (most name an item the
     jar registers opaquely, or use the mod's own serializer); before this change 0 applied. A recipe naming another
     jar's item is left out, since each jar is checked alone.
   - *Block entity types* come only from `META-INF/horizon-native.json`: the 15 corpus `block_entity_type`
     registrations go through `FabricBlockEntityTypeBuilder`/`BlockEntityType` builders with a mod class and no vanilla
     template, so they stay opaque. A declared type keeps its template id; the game gives the block its own
     template's behaviour, not the declared type's.
   - *Data components*: 28 of 40 corpus registrations are typed (17/29 mega, 11/11 wide; 0 before), with
     `persistent`/`network`/`cache_encoding` and a codec label that is only the codec's declared type. *Attachments*:
     21 Fabric `AttachmentRegistry` rows (16 mega, 5 wide; none recorded before). NeoForge's
     `DeferredRegister.DataComponents` and `AttachmentType` register through suppliers the deferred register never
     runs, so they stay opaque; no component value is encoded from a row.
   - *Ingredients* are kept as declarations: no vanilla data folder reads them.
   - *Guests*: an applied manifest's item, block, effect, entity, data-component, menu and block-entity ids join the
     guest's command registries (parsing and suggestions). A guest without the jars is still refused by the content
     identity check, and its creative tab and item tables stay its own root's.
   - There is no mod hot reload (content rows arriving late rebuild the root once).
6. **No JVM fallback in the game.** `horizon-jvm` is a standalone reference runner; a jar the native runtime refuses
   is skipped, so the `jvm`/`sidecar` counts in `server-status` are structurally zero.
7. **Diagnostics.** `server-status`, `java-runtime.json`, the launcher's runtime panel and the server ledger now carry
   refused jars with the runner's whole cause (bounded only at 2,048 characters; the log line still stops at 160) and
   each started jar's skipped plans and counts by reason. What is left: a refused row names no loader (the failure
   carries only the path and cause), and the launcher matches report rows to installed mods by file name, so a jar
   run outside the browser's `horizon-mods` folder has no panel row.
8. **NeoForge.** Native metadata and `@Mod` bootstrap work, and the 85-jar NeoForge corpus runs 51 jars natively
   (14 refused, 20 need the loader). The real FML lifecycle is a bounded, opt-in child process (`horizon-jvm
   --real-loader`) used as a preflight on both server roles, not the path a mod's code takes; there is no in-game
   loader selection and no NeoForge hybrid session. The refusals are mostly NeoForge's loader environment, which the
   runtime does not model: `FMLEnvironment#getDist`, `ModList`/`ModContainer` lookups that return null ("mod list is
   null", "Mod 'architectury' is not available!"), the platform version query REI makes, config bootstraps
   (TerraBlender, GlitchCore, YACL, fzzy_config), and Kotlin for Forge's language provider. A multi-loader jar with
   a `fabric.mod.json` runs its Fabric entrypoints, as Fabric Loader would; on a NeoForge server that is the wrong
   half (zoomify reads "no native entrypoint"). No remapping is needed: the 26.3 server jar holds official names
   (7,744 `net/minecraft/` classes), and the 4 corpus jars with intermediary-looking names carry them as strings in
   Fabric-compat code.
9. **The numbers are narrow.** Both corpora are filename-sorted slices of 26.3 Fabric jars, the client run is
   offscreen with no mod drawing, and the efficiency suite has no steady-state or playable-session figure.

## Planned work

1. **Make mixins count.** Widen the game-hook tier's object model from what the bound callbacks reach first
   (players, entities, levels, the server; Fabric networking, `Connection`, `@Shadow` fields next) and map the
   remaining game-class plans onto hook points, constructor tails such as servux's `DedicatedServer.<init>` first
   (or provide the target classes through the loader path). Entrypoint objects already outlive the entrypoint. Gate: `woven` or `run_game_hook` covered by a real-mod ledger, and every
   `native` label backed by zero skipped plans.
2. **Close metadata refusals.** Gate: metadata refusals 38 → 0 across both corpora. The JDK surface config
   bootstraps reach is modelled in `jdk_model.rs`/`jdk_stream.rs` (charsets, loggers, file walks, `Comparator`,
   the missing stream stages, `toMap`, `String.format` widths, JDK `instanceof`, interface-default and
   receiver dispatch; [java-mod-compat.md](java-mod-compat.md)). On the 38 rows, measured 7 November with a fresh
   dedicated server per jar: `malformed Boolean marker` 7 → 2, `malformed Integer marker` 3 → 0, `expected a native
   stream marker` 4 → 2, `ServiceLoader has no seeded providers` 2 → 0, `native collection marker, got null`
   2 → 2, `list index out of bounds` 1 → 1, and icy-better-horses' uncaught exception is gone. What is left needs
   more than the JDK: zoomify (×2) needs `yet_another_config_lib_v3`, whose config writer encodes through DFU's
   `JsonOps`; emotecraft (×2) needs `player_animation_library` and the service providers it ships; servercore (×2)
   builds its config through DazzleConf's reflection (`Method.getReturnType`, `getAnnotatedReturnType`, `Proxy`);
   voxy walks its own jar through `ModContainer.getRootPaths()`. A ledger run is one jar per server, so the first
   two cannot pass on their own, and the dedicated log reports the jar's alone-run refusal even when the pack class
   path got further. Library models (`reflect_generic.rs`, `jdk_reflect_proxy.rs`, `guava_model.rs`,
   `library_dispatch.rs`; [java-mod-compat.md](java-mod-compat.md)) then closed most of them; measured 7 November on
   the 15 `native-gap` rows of that day's fresh ledgers, one dedicated server per jar, 60 s: voxy and moderner_beta
   now run alone (15 → 13), and with their declared dependencies beside them emotecraft (×2, with
   `player_animation_library`), archers, wizards and paladins (with `spell_engine`, `spell_power` and the rest of
   their closure) reach `Done (` with their entrypoint run, so 8 rows are left. servercore (×2) now builds its
   DazzleConf config through generic reflection and proxies and stops at the game façade
   (`TextColor.fromLegacyFormat` answers null for a `@DefaultObject`). WorldEdit (×2) gets past Guava's
   `TypeToken`, caches and multimaps and, since `method_handles.rs`, past piston's `ArgumentConverters` (method
   handle combinators and a runtime `LambdaMetafactory`); re-measured 8 November it stops in piston's
   `CaseHelper.titleToSpacedLower` (Guava `Splitter.split` into `Streams.stream`, then `String.codePoints` and an
   `IntStream` collect), which is stream and Guava modelling. zoomify (×2) declares only client entrypoints in
   `fabric.mod.json`; the server no longer falls back to its NeoForge `@Mod` class (Fabric Loader never reads
   `@Mod`), so both rows are now client-only skips. cristellib's stage-1 init runs from a game-class mixin
   (`BootstrapMixin`, the `java_bootstrap_head` hook site, which fires only with the opt-in hook tier), so its
   entrypoint still finds its registry empty in the default ledger; whiteout's
   entrypoint now runs: its Mixin config plugin runs before it, as on a loader.
3. **Close runtime refusals.** The entrypoint-exception bucket is down to two missing-dependency rows (archers,
   wizards, and paladins in the same shape) that need the ledger to run a jar with its declared dependencies, and
   WorldEdit's Guava collector. Gate: refused share of in-scope server jars below 5% (committed ledgers 22%/26%;
   fresh re-measure 5 wide / 15 mega rows).
4. **Finish content.** Type `BlockEntityType` builders and the jar's opaque items so more recipes resolve (check
   recipes across the pack, not per jar); NeoForge data components and attachments through their deferred suppliers;
   let a guest without the jars build a root from the manifest; then mod hot reload.
5. **Event surface.** Grow the listener façade by the refusals the dedicated pack run logs. World path, registry
   access and game rules are modelled (`facade_server.rs`): measured 7 November on four dedicated servers with the
   172 corpus jars that listen or bind a hook (4 `fight` bots, 40 s each), they blocked 18, 11 and 4 callbacks
   before and none after; `canSend` and `Connection#getRemoteAddress` blocked 3 hook callbacks before and none
   after (servux's and both weaponmaster copies' `player_logged_in` now run to the end, 4 firings each). All
   refusals went 99 → 82 while the other workstreams added sites and listeners (listener runs 10,989 → 15,567),
   so that total is not this change's alone. Second round (7 November, 204 jars that listen, register a
   command or bind a hook, five dedicated servers, four fight bots for 40 s each): saved data
   (`getDataStorage` on the server 6 and a level 2), `getCommands` 3, `registries()` 2, a player's
   `permissions()` 2, `canSend(player.connection, type)` 2, `getRunningThread` 2 and `execute` 1 blocked 20
   callbacks before and none after (`facade_saved_data.rs`, `facade_server.rs`). All refusals went 94 → 91,
   because those callbacks now run on to the next member: a game rule a mod registers (graves), a registry the facts do
   not list, `getServerDirectory` (answered since), `ServerLevel#getChunkSource`. Command feedback keeps its
   style and translation arguments (`facade_text.rs`). Next by count: `getResourceManager` (3: the jars'
   `data/` files and a `Selector` callback), mod-registered game rules, saved data persisted through its
   `Codec`, payload sends from a callback (`PacketSender#sendPacket`, `ServerPlayNetworking.send`), the
   hook tier's `player.connection`. Then fire the identified use/attack/break/entity/level-load events from
   their game points. Commands next: the source's position and rotation, the jar's own `SuggestionProvider`s
   answering `ask_server`, a player-run sample. Gate: no façade refusal among the ten most
   common listener causes on the corpus pack run.
6. **Client surface.** Two slices done: client events, key mappings in Controls, HUD drawing with items, sprites
   and styled text, the player's items and effects, config defaults and item tooltips (`client_surface.rs`,
   `client_items.rs`, `client_text.rs`, `client_config.rs`, `game/java_client.rs`); the gate (a corpus HUD element
   draws in the measurement's frame) is met by DayCount and durabilityviewer. Next: data components on stacks
   (`ItemStack#get`), the resource manager, saved config files, then screens, renderers and models.
7. **NeoForge path.** Done: the dedicated role's preflight, its `java-runtime.json` section, and a reproducible
   NeoForge corpus (`tools/neoforge_corpus.py`, `LAST_SERVER_LEDGER_NEOFORGE.txt`); remapping checked and moot for
   26.3. Next: model FML's environment (`FMLEnvironment`, `ModList`, `ModContainer`) for the 14 refusals, give a
   multi-loader jar its NeoForge half on a NeoForge pack, add a `Done (` loader proof (a launched FML server) so the
   NeoForge loader tier can join the gate, then in-process loader selection.
8. **Diagnostics.** Done: weave counts, first skip reason, the per-plan list, refused rows with their whole cause in
   `server-status` and `java-runtime.json`, and the launcher's per-mod row with a runtime-cache clear. Next: name a
   refused jar's loader from its metadata, and show runtime rows for jars the launcher did not install.
9. **Keep the corpus honest.** Refresh `tools/real_mod_fixtures/LAST_SERVER_LEDGER_{223,500}.txt` per release, fail
   a run when `ran` regresses or a refused row loses its cause, and extend the fixture workflow (it deliberately never
   downloads third-party jars).
10. **Performance.** Rerun the efficiency suite with a curated pack and steady-state heap, and a playable client
   session with mods that actually draw.

## How to reproduce

```sh
cargo build --profile fast -p horizon-client -p horizon-native-java -p horizon-jvm
cargo test -p horizon-native-java
python3 tools/real_mod_corpus.py --self-test
python3 -m unittest tests.test_real_mod_corpus tests.test_real_mod_fixtures

# a corpus through the real dedicated server (one fresh server per jar; 60 s bound)
scratch=$(mktemp -d)
TMPDIR=$scratch HORIZON_NO_AUDIO=1 python3 tools/java_mod_server_ledger.py ~/horizon-corpus/corpus-xl \
  --binary target/fast/horizon-client --native-binary target/fast/horizon-native-java \
  --resources .local/horizon-client/26.3 --timeout 60 --json --output ledger.txt > ledger.json

# the NeoForge corpus: fetch the locked jars, then the same ledger with each jar's dependency closure
python3 tools/neoforge_corpus.py fetch
TMPDIR=$scratch HORIZON_NO_AUDIO=1 python3 tools/java_mod_server_ledger.py ~/horizon-corpus/neoforge --with-deps \
  --deps-dir ~/horizon-corpus/neoforge --binary target/fast/horizon-client \
  --resources .local/horizon-client/26.3 --timeout 60 --output ledger-neoforge.txt
# one jar (and its closure) through the real FML of an installed NeoForge 26.3 server
target/fast/horizon-jvm --real-loader --entry-kind neoforge --libs <server>/libraries <jar> <deps>...

# the same jars through the real Fabric loader (needs the game jar, loader and libraries)
python3 tools/java_mod_loader_ledger.py --mods-dir ~/horizon-corpus/corpus-xl \
  --binary target/fast/horizon-jvm --json
```

In the game: `horizon-client --java-mod mods/example.jar --resources .local/horizon-client/26.3 --world W`, add
`--java-mod` once per jar, and put `--java-mod` before `--dedicated` on a server. `HORIZON_JAVA_STRICT=1` turns a
refusal into a startup failure. The efficiency harness is in `tools/efficiency/` ([efficiency.md](efficiency.md)):
`python3 tools/efficiency/run_server.py --runs 3`,
`python3 tools/efficiency/run_client.py --counts 0,3,20,74 --max-fps 260 --warmup 5` and
`python3 tools/efficiency/run_startup.py --counts 0,25,50,74`; the run's own reports and its jar-selection probe stay
in `~/horizon-corpus/`.
