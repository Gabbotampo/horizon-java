# Unmodelled host calls: the `ran-hollow` census

`ran-clean` is the only label that says a jar ran without leaning on the host. A
`ran-hollow` run reached the end of its entrypoint, but at least one call crossed the
`Host` boundary without a model and was answered with the neutral value for its
descriptor (`null`, `0`, `false`). This page ranks those calls over the three measured
corpora so fixes target the jars they actually hold back.

Measured on 11 November 2026 at `b45a717` (main) plus the four interpreter-honesty
PRs: null receivers and empty optionals (#37), bodiless and instance calls counted
(#38), checkcast / `Enum.valueOf` / `Thread.start` (#41), the census-stages PR that
carried this page, and the unmodelled-answer batch: Gson's builder and writer (#49),
Fabric `ModOrigin` (#50), Guava `MapMaker` (#51), executors (#52) and host method
references (#53). With the native probe (`crates/horizon-native-java`), one process per
jar, each jar with its declared dependency closure on the same class path and **every
server stage the loader runs** (`main`, then `server`). The honesty fixes first made the
numbers go **down**, and that was the correct outcome: every jar that left `ran-clean`
had been reading clean only because the interpreter answered a call on `null`, a bodiless
method or an unmodelled instance call silently. The answer batch then moves the jars that
stopped on those invented `null`s forward. See
[Re-measured](#re-measured-11-november-2026) for old → new.

```sh
cargo build --profile fast -p horizon-native-java

python3 tools/unmodelled_calls.py \
  --probe target/fast/horizon-native-java \
  --json /tmp/census-before.json
```

`tools/unmodelled_calls.py` prints and can save, per corpus, one row per jar
(`verdict`, `unknown`, `calls`) and the ranked table. A single jar can be reproduced by
hand:

```sh
target/fast/horizon-native-java --json --game-version 26.3 \
  --link ~/horizon-corpus/neoforge/GlitchCore-neoforge-26.3-26.3.0.0.5.jar \
  ~/horizon-corpus/neoforge/<jar>.jar neoforge
target/fast/horizon-native-java --json --game-version 26.3 \
  --link ~/horizon-corpus/mega/2e364989e3-fabric-api-0.165.2+26.3.jar \
  ~/horizon-corpus/corpus-xl/<jar>.jar main,server   # the stages the jar declares, in loader order
```

The NeoForge corpus is the 85 files pinned by
`tools/real_mod_fixtures/neoforge_corpus.lock.json`; NeoForge dependencies resolve
inside that corpus only, because the Fabric corpora carry Fabric builds of the same
libraries (TerraBlender, GlitchCore) and linking one beside a NeoForge mod measures the
wrong class path.

## Verdicts

A Fabric jar runs every server stage it declares, in Fabric Loader's order
(`Hooks.startServer`: `main`, then `server`; the probe takes them as `main,server`); a
NeoForge jar runs its `@Mod` classes. A failing entrypoint is recorded and the rest still
run, and the label is the game's own `verdict()` (`crates/horizon-native-java/src/verdict.rs`):

| Verdict | Meaning |
|---|---|
| `ran-clean` | every stage returned and the probe counted zero unknown host calls |
| `ran-hollow` | every stage returned, with at least one counted unknown (a neutral answer, an unchecked cast, a bodiless call) |
| `ran-partial` | at least one stage ran and at least one failed (a real loader would have thrown) |
| `refused` | nothing ran: metadata, discovery or an uncaught exception (a `NullPointerException` on a `null` the host answered is the common one now) |
| `skipped` | no server-side entrypoint (client-only or a library, out of scope for a server) |

These are the probe's numbers, not the committed server ledgers'. The ledgers predate
the host-call count and read `ran-hollow` with `unknown_source=none` for every row that
ran; re-running the same jars through a game host would model more of the game and count
fewer calls, so the probe is a consistent, conservative ceiling. A jar only moves to
`ran-clean` in this table when the probe reaches zero, which is the gate this page
measures.

## Current numbers

| Corpus | Jars | `ran-clean` | `ran-hollow` | `ran-partial` | `refused` | `skipped` | Unknown calls (in runs that finished) |
|---|---:|---:|---:|---:|---:|---:|---:|
| wide (`corpus-xl`) | 223 | 31 | 41 | 4 | 40 | 107 | 1,186 |
| mega (500 unique) | 500 | 59 | 84 | 4 | 105 | 248 | 2,458 |
| NeoForge 26.3 | 85 | 21 | 22 | 0 | 42 | 0 | 234 |
| **total** | **808** | **111** | **147** | **8** | **187** | **355** | **3,878** |

The unknown-call total is close to the honesty-era figure even though fifteen jars now run
further: those runs used to stop at the first invented `null` they dereferenced (`refused`),
and a refused run's calls are not summed. The batch below is the first one that moves jars
the *other* way — fifteen leave `refused`. Two rows leave `ran-clean` at the same time,
because their `Gson` registers an adapter or a custom strategy and the runtime now counts
that write instead of emitting text that ignored it.

## Ranked calls

`jars` is how many jars hit the call at least once, `calls` the total invocations, and
`sole` the jars where that call is the only *distinct* unknown left, so modelling it
alone moves them to `ran-clean`. `surface` says what a bounded runtime could do:

- **JDK**: model to `java`'s own behaviour.
- **loader**: Fabric/NeoForge/FML surface, model from the loader's API.
- **library**: a library the platform ships (Guava, Gson, Netty, log4j, NightConfig);
  model the call, not the whole library.
- **game-bound**: `net/minecraft`/`com/mojang` or a client surface. A guessed answer is
  worse than a counted unknown, so these stay out of a JDK/loader batch (see below).
- **mod-owned**: the call names a class the jar or a missing dependency owns; a host
  model would invent the mod's own code.

| # | call | jars | calls | sole | surface |
|---:|---|---:|---:|---:|---|
| 1 | `java/lang/String#formatted([Ljava/lang/Object;)Ljava/lang/String;` | 8 | 14 | 7 | JDK |
| 2 | `net/neoforged/bus/api/IEventBus#addListener(Ljava/util/function/Consumer;)V` | 13 | 30 | 5 | loader |
| 3 | `java/lang/StackWalker$Option#RETAIN_CLASS_REFERENCELjava/lang/StackWalker$Option;` | 10 | 10 | 5 | JDK |
| 4 | `java/util/Properties#setProperty(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/Object;` | 7 | 28 | 5 | JDK |
| 5 | `net/fabricmc/loader/api/FabricLoader#getAllMods()Ljava/util/Collection;` | 9 | 28 | 4 | loader |
| 6 | `net/minecraft/network/codec/StreamCodec#ofMember(Lnet/minecraft/network/codec/StreamMemberEncoder;Lnet/minecraft/network/codec/StreamDecoder;)Lnet/minecraft/network/codec/StreamCodec;` | 7 | 44 | 4 | game-bound |
| 7 | `com/google/common/collect/Maps#newHashMap()Ljava/util/HashMap;` | 5 | 5 | 4 | library |
| 8 | `java/util/ServiceLoader#forEach(Ljava/util/function/Consumer;)V` | 3 | 3 | 3 | JDK |
| 9 | `net/minecraft/network/protocol/common/custom/CustomPacketPayload#codec(Lnet/minecraft/network/codec/StreamMemberEncoder;Lnet/minecraft/network/codec/StreamDecoder;)Lnet/minecraft/network/codec/StreamCodec;` | 16 | 68 | 2 | game-bound |
| 10 | `com/google/gson/Gson#toJson(Ljava/lang/Object;Ljava/lang/Appendable;)V` | 7 | 7 | 2 | library |
| 11 | `org/spongepowered/asm/mixin/MixinEnvironment#getActiveTransformer()Ljava/lang/Object;` | 2 | 2 | 2 | loader |
| 12 | `net/minecraft/network/codec/StreamCodec#of(Lnet/minecraft/network/codec/StreamEncoder;Lnet/minecraft/network/codec/StreamDecoder;)Lnet/minecraft/network/codec/StreamCodec;` | 6 | 23 | 1 | game-bound |
| 13 | `org/spongepowered/asm/service/MixinService#getService()Lorg/spongepowered/asm/service/IMixinService;` | 5 | 68 | 1 | loader |
| 14 | `com/google/gson/JsonObject#addProperty(Ljava/lang/String;Ljava/lang/Boolean;)V` | 4 | 64 | 1 | library |
| 15 | `net/neoforged/bus/api/IEventBus#addListener(Ljava/lang/Class;Ljava/util/function/Consumer;)V` | 4 | 6 | 1 | loader |
| 16 | `net/minecraft/Optionull#mapOrElse(Ljava/lang/Object;Ljava/util/function/Function;Ljava/util/function/Supplier;)Ljava/lang/Object;` | 2 | 4 | 1 | game-bound |
| 17 | `java/util/Collections#synchronizedMap(Ljava/util/Map;)Ljava/util/Map;` | 2 | 2 | 1 | JDK |
| 18 | `net/minecraft/client/Minecraft#getInstance()Lnet/minecraft/client/Minecraft;` | 2 | 2 | 1 | game-bound |
| 19 | `java/util/List#getFirst()Ljava/lang/Object;` | 1 | 2 | 1 | JDK |
| 20 | `java/util/Base64#getUrlEncoder()Ljava/util/Base64$Encoder;` | 1 | 1 | 1 | JDK |
| 21 | `java/util/concurrent/CompletableFuture#runAsync(Ljava/lang/Runnable;Ljava/util/concurrent/Executor;)Ljava/util/concurrent/CompletableFuture;` | 1 | 1 | 1 | JDK |
| 22 | `net/fabricmc/loader/api/FabricLoader#invokeEntrypoints(Ljava/lang/String;Ljava/lang/Class;Ljava/util/function/Consumer;)V` | 1 | 1 | 1 | loader |
| 23 | `net/minecraft/network/codec/StreamCodec#composite(Lnet/minecraft/network/codec/StreamCodec;Ljava/util/function/Function;Lnet/minecraft/network/codec/StreamCodec;Ljava/util/function/Function;Ljava/util/function/BiFunction;)Lnet/minecraft/network/codec/StreamCodec;` | 18 | 44 | 0 | game-bound |
| 24 | `net/minecraft/network/codec/StreamCodec#composite(Lnet/minecraft/network/codec/StreamCodec;Ljava/util/function/Function;Ljava/util/function/Function;)Lnet/minecraft/network/codec/StreamCodec;` | 14 | 55 | 0 | game-bound |
| 25 | `net/minecraft/network/codec/StreamCodec#composite(Lnet/minecraft/network/codec/StreamCodec;Ljava/util/function/Function;Lnet/minecraft/network/codec/StreamCodec;Ljava/util/function/Function;Lnet/minecraft/network/codec/StreamCodec;Ljava/util/function/Function;Lcom/mojang/datafixers/util/Function3;)Lnet/minecraft/network/codec/StreamCodec;` | 12 | 45 | 0 | game-bound |
| 26 | `com/mojang/serialization/codecs/RecordCodecBuilder#create(Ljava/util/function/Function;)Lcom/mojang/serialization/Codec;` | 12 | 20 | 0 | game-bound |
| 27 | `net/minecraft/network/codec/ByteBufCodecs#STRING_UTF8Lnet/minecraft/network/codec/StreamCodec;` | 11 | 106 | 0 | game-bound |
| 28 | `net/minecraft/network/codec/ByteBufCodecs#BOOLLnet/minecraft/network/codec/StreamCodec;` | 11 | 46 | 0 | game-bound |
| 29 | `net/minecraft/network/codec/ByteBufCodecs#INTLnet/minecraft/network/codec/StreamCodec;` | 10 | 46 | 0 | game-bound |
| 30 | `net/minecraft/network/codec/StreamCodec#map(Ljava/util/function/Function;Ljava/util/function/Function;)Lnet/minecraft/network/codec/StreamCodec;` | 8 | 8 | 0 | game-bound |
| 31 | `xaero/lib/common/config/option/ConfigOptionManager#frozenZ` | 7 | 592 | 0 | mod-owned |
| 32 | `java/lang/Math#pow(DD)D` | 7 | 238 | 0 | JDK |
| 33 | `java/lang/Math#floor(D)D` | 7 | 108 | 0 | JDK |
| 34 | `java/lang/reflect/Field#setAccessible(Z)V` | 7 | 74 | 0 | JDK |
| 35 | `net/minecraft/world/effect/MobEffectCategory#HARMFULLnet/minecraft/world/effect/MobEffectCategory;` | 7 | 60 | 0 | game-bound |
| 36 | `net/minecraft/core/Registry#registerForHolder(Lnet/minecraft/core/Registry;Lnet/minecraft/resources/Identifier;Ljava/lang/Object;)Lnet/minecraft/core/Holder$Reference;` | 7 | 40 | 0 | game-bound |
| 37 | `java/lang/Math#ceil(D)D` | 7 | 27 | 0 | JDK |
| 38 | `java/lang/Math#min(DD)D` | 7 | 27 | 0 | JDK |
| 39 | `net/minecraft/world/effect/MobEffectCategory#NEUTRALLnet/minecraft/world/effect/MobEffectCategory;` | 7 | 20 | 0 | game-bound |
| 40 | `java/io/FileOutputStream#close()V` | 7 | 14 | 0 | JDK |

`<checkcast>` rows are casts the runtime could not check (a game object cast to a jar
interface a Mixin may add, an opaque marker); they are counted, not passed.

### Baskets the `sole` column hides (10 November, `784429b`)

A jar goes clean only when *every* distinct unknown is answered, so a call with
`sole = 0` can still be the last piece of a multi-call basket. The census's per-jar rows
make those baskets visible; four accounted for most of the early runs:

| Basket | Calls | Jars blocked by exactly that basket |
|---|---|---:|
| NIO watch registration | `FileSystems.getDefault` + `StandardWatchEventKinds.ENTRY_MODIFY` (then the chain the mod runs on them) | 7 |
| NeoForge config registration | `ConfigTracker.INSTANCE` + `ModConfig$Type.SERVER` | 1 (2 corpus copies) |
| Trade-security config | `NeoForge.EVENT_BUS` + `FMLLoader.getCurrent` + `Pattern.compile` | 1 |
| Dataset codes | `ByteBufCodecs` + `StreamCodec` + `CustomPacketPayload.codec` | many (game-bound) |

## What stays game-bound

The largest families by volume are the game's own codec and constant surface, which a
JDK/loader batch must not guess:

- `net/minecraft/network/codec/{StreamCodec,ByteBufCodecs}` (`ofMember`, `of`,
  `composite`, `unit`, `map`, `collection`, and the `StreamCodec` constants): 33 jars
  for `composite` alone. A neutral `null` codec is wrong; the real answer builds a
  codec over the game's buffers.
- `CustomPacketPayload.codec` and `DataComponentPatch.STREAM_CODEC`,
  `ItemStack.STREAM_CODEC`, `GameType` constants: same family.
- `com/mojang/serialization/codecs/RecordCodecBuilder` and
  `com/mojang/serialization/JsonOps.INSTANCE`: DFU, game-bound by the same rule.
- `SharedConstants.getCurrentVersion`, `Blocks.*`, `Items.*`, `Minecraft.getInstance`:
  named in PR #7's merged catch-all; the census still counts them because the probe's
  bare host has no game, and modelling them outside a game is exactly the invented
  answer the rule forbids.

They stay listed in every census run; they are not a target until the game host is in
the loop.

## Overlap with the open community PRs

PRs #4–#7 (Gabbotampo) touch: Java `float` max/min and `StandardCharsets`;
`ItemLike.asItem` over game superclasses; `String` constructors from `byte`/`char`
arrays; and the vanilla block-class / game-constant `get_static` catch-all. The batches
below do not touch any of those surfaces: the JDK batch adds only `java.io.File`,
`System`, `Runtime`, `Pattern.compile` and NIO watch statics; the loader batch adds only
`net/neoforged`/`net/fabricmc` loader members. No line of #4–#7's hunks is in the same
`match` arm as these.

## After the fixes (10 November, `784429b`, single-stage probe)

Three batches landed against this table. Each was measured on its own branch (from the
same `784429b` base) with the command above, so the per-batch delta is what that batch
alone moved; the cumulative row is the three branches merged and probed once. A jar
counts as fixed only when its row moves to `ran-clean`, not when its call count drops.

| Batch | Corpus | `ran-clean` | `ran-hollow` | `refused` | Unknown calls |
|---|---|---:|---:|---:|---:|
| JDK statics (#16: `File`, `System`, `Runtime`, `Pattern`, watch service) | wide | 34 → 48 | 72 → 58 | 10 → 10 | 1,273 → 1,211 |
| | mega | 73 → 81 | 144 → 136 | 35 → 35 | 11,018 → 10,962 |
| | NeoForge | 24 → 26 | 34 → 32 | 27 → 27 | 693 → 620 |
| NeoForge loader statics (#17: `EVENT_BUS`, config, `FMLLoader`, registries) | wide | 34 → 35 | 72 → 71 | 10 → 10 | 1,273 → 1,262 |
| | mega | 73 → 74 | 144 → 143 | 35 → 35 | 11,018 → 10,999 |
| | NeoForge | 24 → 34 | 34 → 25 | 27 → 26 | 693 → 640 |
| Platform libraries (#18: `ResourceLoader`, `Maps`, `PlatformDependent`) | wide | 34 → 36 | 72 → 70 | 10 → 10 | 1,273 → 1,268 |
| | mega | 73 → 75 | 144 → 142 | 35 → 35 | 11,018 → 11,013 |
| | NeoForge | 24 → 24 | 34 → 34 | 27 → 27 | 693 → 693 |
| **All three merged** | wide | **34 → 51** | 72 → 55 | 10 → 10 | **1,273 → 1,195** |
| | mega | **73 → 84** | 144 → 133 | 35 → 35 | **11,018 → 10,938** |
| | NeoForge | **24 → 37** | 34 → 22 | **27 → 26** | **693 → 567** |

41 jars move from `ran-hollow` to `ran-clean`: 24 from the JDK statics, 12 from the
loader batch, 4 from the platform libraries, and voicechat from the JDK and loader
batches together. PuzzlesLib stops being refused (its old `NeoForge.EVENT_BUS` NPE) and
runs hollow with 18 calls. No jar regresses and none newly refuses. Total unknown calls:
12,984 → 12,700.

What is left is the game-bound list above — `StreamCodec`/`ByteBufCodecs` and
`CustomPacketPayload` still dominate the remaining `ran-hollow` rows (33 jars on one
`composite` overload), and they need the game host, not a guessed codec.

## Re-measured (11 November 2026)

Old is `1894d35` (main, with #45) with its own single-stage census; new is the same commit with the
four interpreter-honesty PRs and the stage runner. Same corpora, deps and command.

| Corpus | `ran-clean` | `ran-hollow` | `ran-partial` | `refused` |
|---|---:|---:|---:|---:|
| wide (`corpus-xl`) | 52 → **31** | 52 → 34 | 0 → 8 | 12 → 43 |
| mega | 86 → **59** | 136 → 67 | 0 → 10 | 30 → 116 |
| NeoForge | 36 → **22** | 21 → 20 | 0 → 0 | 28 → 43 |
| **total** | **174 → 112** | 209 → 121 | 0 → 18 | 70 → 202 |

Where the 62 jars that left `ran-clean` went: 34 to `ran-hollow` (an unmodelled instance
call or bodiless method is now counted), 22 to `refused` (a call on `null` now throws), 6
to `ran-partial` (a second stage the old census never ran fails). Of the old `ran-hollow`
jars, 110 are now `refused` and 12 `ran-partial`. Every newly refused run stops on a
`NullPointerException` (83 of them inside a class initializer, so it surfaces as
`ExceptionInInitializerError`), on a `null` that an unmodelled call answered; the most
common receivers are `ModOrigin.toString()` (12 jars), `GsonBuilder` fluent calls (16),
`MapMaker.makeMap()` (9) and `ExecutorService.execute` (7). 26 Fabric jars declare both
`main` and `server` and now run both; 18 rows are `ran-partial`.

Per PR on its own against `0c45845` (ran-clean total 174): #37 → 156, #38 → 118, #41 →
174 (no corpus jar depended on an unchecked cast, an invalid `valueOf` name or
`Thread.start`; the new counts landed in jars already hollow).

## Re-measured (11 November 2026, after the unmodelled-answer batch)

Old is `b45a717` (main) and new is the same commit with #49–#53: Gson's fluent builder,
`FormattingStyle` and writer (#49), Fabric `ModOrigin` (#50), Guava `MapMaker` (#51),
executors (#52) and host method references (#53). Same corpora, deps and command.

| Corpus | `ran-clean` | `ran-hollow` | `ran-partial` | `refused` |
|---|---:|---:|---:|---:|
| wide (`corpus-xl`) | 31 → **31** | 34 → 41 | 8 → 4 | 43 → 40 |
| mega | 59 → **59** | 67 → 84 | 10 → 4 | 116 → 105 |
| NeoForge | 22 → **21** | 20 → 22 | 0 → 0 | 43 → 42 |
| **total** | **112 → 111** | 121 → 147 | 18 → 8 | 202 → 187 |

Fifteen jars leave `refused`; ten `ran-partial` rows become `ran-hollow` (their second stage
used to fail on one of those `null`s and now runs). Two `ran-clean` rows leave: their `Gson`
registers a type adapter or a custom naming strategy, and #49 now counts that write instead of
emitting text that ignored the option — the honesty rule the whole page is about. Unknown calls
counted: 3,703 → 3,878.

What each fix moved, measured on its own branch:

| PR | Fixed call | Effect |
|---|---|---|
| #49 | `GsonBuilder` fluent calls (16 jars) | 12 leave `refused`; naming policy, `excludeFieldsWithModifiers`, non-executable prefix, special floats and long policy are applied; a custom strategy/adapter is counted, which costs two `ran-clean` rows |
| #50 | `ModOrigin.toString()` (12 jars) | all 12 get past it; they stop on the next unmodelled answer |
| #51 | `MapMaker.makeMap()` (18 jars reached) | 3 leave `refused`; 15 stop later |
| #52 | `ExecutorService.execute` (7 jars) | all 7 get past it; they stop in `CommentedProperties#save` on the bounded regex subset |
| #53 | method references to host methods | no label moves (a dropped reference was a silent wrong answer, not a stop); pinned by fixtures |

What is left where those runs now stop: `BigDecimal.stripTrailingZeros()` and
`String.replace` on a null argument (Serilum's shared init), a Balm event's `listeners`
field, `EnumConfigProperty`'s `getEnumConstants`, `FabricBalmCapabilities`, and
`CommentedProperties#save`'s `Pattern` outside the native regex subset. The game-bound
list below is unchanged.

## Re-measured (12 November 2026, Gson's JSON tree)

Old is `9eb57c5` (main) and new is the same commit with Gson's JSON tree modelled from
gson-2.14.0 (`gson_tree.rs`): `JsonObject`, `JsonArray`, `JsonPrimitive`, `JsonNull` and
`LazilyParsedNumber`, `JsonElement.toString` and `Gson.toJson` of a tree. Reading text into a
tree (`JsonParser`, `Gson.fromJson` into a tree class) is counted, and comes in the stacked
parser PR. Same corpora, deps and command (release-built probes).

| Corpus | `ran-clean` | `ran-hollow` | `ran-partial` | `refused` | unknown calls |
|---|---:|---:|---:|---:|---:|
| wide (`corpus-xl`) | 31 → 31 | 42 → 42 | 4 → 4 | 39 → 39 | 1,164 → **888** |
| mega | 59 → **60** | 88 → 87 | 4 → 4 | 101 → 101 | 2,473 → **2,470** |
| NeoForge | 21 → 21 | 25 → 25 | 0 → 0 | 39 → 39 | 255 → 255 |

Every `JsonObject` and `JsonArray` call left the ranking (the table above predates it;
`JsonObject#addProperty(String, Boolean)` was 64 calls in 4 jars). Movers: `cupboard`
(`326652-8890593.jar`) 78 → 2 and `structureessentials` (`832882-8896904.jar`) 198 → 2, both
still `ran-hollow` on `Gson#fromJson(String, Class)` into `JsonObject` (Cupboard's
`CupboardConfig#writeConfig`), now counted where the general `fromJson` path used to answer a map
marker typed as a `JsonObject`; `modelbrowser` 3 → 0, now `ran-clean`; `takeitout` 53 → 49. No
verdict got worse.

With the parser on top (`gson_tree/parse.rs`: `JsonParser`, `Gson.fromJson` into a tree class),
against the same `9eb57c5`:

| Corpus | `ran-clean` | `ran-hollow` | `ran-partial` | `refused` | unknown calls |
|---|---:|---:|---:|---:|---:|
| wide (`corpus-xl`) | 31 → 31 | 42 → 42 | 4 → 4 | 39 → 39 | 1,164 → **896** |
| mega | 59 → **60** | 88 → 87 | 4 → 4 | 101 → 101 | 2,473 → **2,468** |
| NeoForge | 21 → 21 | 25 → 25 | 0 → 0 | 39 → 39 | 255 → 255 |

Every `JsonParser` call left the ranking too, and the counted `fromJson` into `JsonObject` is
read. `cupboard` is 78 → 4 and `structureessentials` 198 → 9: both now get through Cupboard's
config write and stop on `String#replaceFirst` in `CupboardConfig#splitLongDesc`, which they now
reach (0 → 13 calls, the only call whose count rose). `PatPat` 14 → 13 and both `clientcommands`
builds 2 → 1. Corpus-xl's total is 8 above the tree-only run because those two jars run further.
No verdict got worse. The Gson calls still counted are `Gson` writes through an adapter or a
custom strategy (#49's rule), `toJsonTree`, `toJson(JsonElement, JsonWriter)`,
`TypeToken#getType` and two `GsonBuilder#setPrettyPrinting` calls, all unchanged by this model.

## Re-measured (12 November 2026, sequenced lists and `FMLModContainer`)

JDK 21's `getFirst`/`getLast`/`removeFirst`/`removeLast`/`addFirst`/`addLast`/`reversed` now
answer on lists, not only on queues (`sequenced.rs`). A `javafml` mod's container is an
`FMLModContainer`, and its `getEventBus()` returns the mod bus. Against `6e3684b`:

| Corpus | `ran-clean` | `ran-hollow` | `ran-partial` | `refused` | unknown calls |
|---|---:|---:|---:|---:|---:|
| wide (`corpus-xl`) | 31 → 31 | 42 → **43** | 4 → 4 | 39 → **38** | 896 → 909 |
| mega | 60 → 60 | 87 → 87 | 4 → 4 | 101 → 101 | 2,468 → 2,468 |
| NeoForge | 21 → 21 | 25 → 25 | 0 → 0 | 39 → 39 | 255 → 257 |

- `241160-8893517` (Balm's netherportalfix) goes from refused to `ran-hollow` (13).
- `ImmediatelyFast-NeoForge` gets its two `List#getFirst` answered and runs on (2 → 4).
- Every `collective` build, and `fullbrightnesstoggle` through it, gets past
  `ArrayBackedEvent#rebuildInvoker` and now stops at the `String#replaceAll` null in
  `RegisterMod`.

## Known limits of this census

- The probe executes a jar as shipped. A jar whose entrypoint only works with its own
  `@Overwrite` mixin applied (BiomesOPlenty's NeoForge build refuses in
  `glitchcore/util/Environment#getConfigPath`) is `refused` here even though the game's
  load-time weave makes it run; that jar's 221 neutral calls are not in these tables.
- Every unknown call is counted (the old 64-distinct-key cap is gone, #38).
- The probe runs entrypoints only. Listeners and commands a jar registers do not run
  here, so a `ran-clean` jar can still fail in game when its listeners fire (the in-game
  survey has several), and a `ran-hollow` jar here may be clean in game, whose host
  models more of the game.
- A counted neutral `null` that the jar then dereferences throws
  `NullPointerException`, as Java would on a `null`; that `null` is still the runtime's
  invention, so such a `refused` row says "stopped on an unmodelled answer", not "the
  jar is broken".
