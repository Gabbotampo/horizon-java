# Unmodelled host calls: the `ran-hollow` census

`ran-clean` is the only label that says a jar ran without leaning on the host. A
`ran-hollow` run reached the end of its entrypoint, but at least one call crossed the
`Host` boundary without a model and was answered with the neutral value for its
descriptor (`null`, `0`, `false`). This page ranks those calls over the three measured
corpora so fixes target the jars they actually hold back.

Measured on 10 November 2026 at commit `784429b` with the native probe
(`crates/horizon-native-java`), one process per jar, each jar with its declared
dependency closure on the same class path.

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
  ~/horizon-corpus/corpus-xl/<jar>.jar server
```

The NeoForge corpus is the 85 files pinned by
`tools/real_mod_fixtures/neoforge_corpus.lock.json`; NeoForge dependencies resolve
inside that corpus only, because the Fabric corpora carry Fabric builds of the same
libraries (TerraBlender, GlitchCore) and linking one beside a NeoForge mod measures the
wrong class path.

## Verdicts

| Verdict | Meaning |
|---|---|
| `ran-clean` | the entrypoint returned and the probe counted zero unknown host calls |
| `ran-hollow` | the entrypoint returned with at least one neutral answer |
| `refused` | the probe stopped: metadata, discovery or an uncaught exception |
| `skipped` | no server-side entrypoint (client-only or a library, out of scope for a server) |

These are the probe's numbers, not the committed server ledgers'. The ledgers predate
the host-call count and read `ran-hollow` with `unknown_source=none` for every row that
ran; re-running the same jars through a game host would model more of the game and count
fewer calls, so the probe is a consistent, conservative ceiling. A jar only moves to
`ran-clean` in this table when the probe reaches zero, which is the gate this page
measures.

## Baseline

| Corpus | Jars | `ran-clean` | `ran-hollow` | `refused` | `skipped` | Unknown calls |
|---|---:|---:|---:|---:|---:|---:|
| wide (`corpus-xl`) | 223 | 34 | 72 | 10 | 107 | 1,273 |
| mega (500 unique) | 500 | 73 | 144 | 35 | 248 | 11,018 |
| NeoForge 26.3 | 85 | 24 | 34 | 27 | 0 | 693 |
| **total** | **808** | **131** | **250** | **72** | **355** | **12,984** |

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
| 1 | `java/io/File#separatorLjava/lang/String;` | 19 | 54 | 8 | JDK |
| 2 | `net/minecraft/network/codec/StreamCodec#ofMember(Lnet/minecraft/network/codec/StreamMemberEncoder;Lnet/minecraft/network/codec/StreamDecoder;)Lnet/minecraft/network/codec/StreamCodec;` | 15 | 127 | 5 | game-bound |
| 3 | `net/neoforged/neoforge/common/NeoForge#EVENT_BUSLnet/neoforged/bus/api/IEventBus;` | 19 | 29 | 4 | loader |
| 4 | `com/ezzenix/emlib/config/EmConfig#init(Ljava/lang/String;Ljava/lang/Class;)V` | 5 | 5 | 4 | mod-owned |
| 5 | `net/minecraft/SharedConstants#getCurrentVersion()Lnet/minecraft/WorldVersion;` | 8 | 8 | 3 | game-bound |
| 6 | `java/lang/System#errLjava/io/PrintStream;` | 6 | 10 | 3 | JDK |
| 7 | `java/lang/System#outLjava/io/PrintStream;` | 5 | 5 | 3 | JDK |
| 8 | `net/caffeinemc/mods/lithium/common/config/Option#enabledZ` | 4 | 8 | 3 | mod-owned |
| 9 | `java/util/regex/Pattern#compile(Ljava/lang/String;)Ljava/util/regex/Pattern;` | 26 | 51 | 2 | JDK |
| 10 | `net/minecraft/network/protocol/common/custom/CustomPacketPayload#codec(Lnet/minecraft/network/codec/StreamMemberEncoder;Lnet/minecraft/network/codec/StreamDecoder;)Lnet/minecraft/network/codec/StreamCodec;` | 18 | 146 | 2 | game-bound |
| 11 | `net/fabricmc/fabric/api/resource/v1/ResourceLoader#get(Lnet/minecraft/server/packs/PackType;)Lnet/fabricmc/fabric/api/resource/v1/ResourceLoader;` | 4 | 6 | 2 | loader |
| 12 | `java/lang/Runtime#getRuntime()Ljava/lang/Runtime;` | 2 | 2 | 2 | JDK |
| 13 | `kotlin/collections/CollectionsKt#sortedWith(Ljava/lang/Iterable;Ljava/util/Comparator;)Ljava/util/List;` | 2 | 2 | 2 | mod-owned |
| 14 | `one/pkg/config/metadata/ConfigMeta#of(Ljava/lang/Class;Ljava/nio/file/Path;)Lone/pkg/config/metadata/ConfigMeta;` | 2 | 2 | 2 | mod-owned |
| 15 | `net/minecraft/network/codec/StreamCodec#of(Lnet/minecraft/network/codec/StreamEncoder;Lnet/minecraft/network/codec/StreamDecoder;)Lnet/minecraft/network/codec/StreamCodec;` | 15 | 90 | 1 | game-bound |
| 16 | `net/neoforged/fml/config/ModConfig$Type#CLIENTLnet/neoforged/fml/config/ModConfig$Type;` | 9 | 9 | 1 | loader |
| 17 | `net/neoforged/fml/loading/FMLLoader#getCurrent()Lnet/neoforged/fml/loading/FMLLoader;` | 8 | 21 | 1 | loader |
| 18 | `net/minecraft/client/Minecraft#getInstance()Lnet/minecraft/client/Minecraft;` | 5 | 8 | 1 | game-bound |
| 19 | `eu/midnightdust/lib/config/MidnightConfig#init(Ljava/lang/String;Ljava/lang/Class;)V` | 2 | 2 | 1 | mod-owned |
| 20 | `folk/sisby/kaleido/api/ReflectiveConfig#createToml(Ljava/nio/file/Path;Ljava/lang/String;Ljava/lang/String;Ljava/lang/Class;)Lfolk/sisby/kaleido/api/ReflectiveConfig;` | 2 | 2 | 1 | mod-owned |
| 21 | `net/neoforged/neoforge/registries/DeferredRegister#createDataComponents(Lnet/minecraft/resources/ResourceKey;Ljava/lang/String;)Lnet/neoforged/neoforge/registries/DeferredRegister$DataComponents;` | 2 | 2 | 1 | loader |
| 22 | `com/google/common/collect/Maps#newConcurrentMap()Ljava/util/concurrent/ConcurrentMap;` | 1 | 2 | 1 | library |
| 23 | `io/netty/util/internal/PlatformDependent#isOsx()Z` | 1 | 2 | 1 | library |
| 24 | `com/jahirtrap/configlib/TXFConfig#init(Ljava/lang/String;Ljava/lang/Class;)V` | 1 | 1 | 1 | mod-owned |
| 25 | `java/util/concurrent/CompletableFuture#runAsync(Ljava/lang/Runnable;)Ljava/util/concurrent/CompletableFuture;` | 1 | 1 | 1 | JDK |
| 26 | `me/shedaniel/autoconfig/serializer/PartitioningSerializer#wrap(Lme/shedaniel/autoconfig/serializer/ConfigSerializer$Factory;)Lme/shedaniel/autoconfig/serializer/ConfigSerializer$Factory;` | 1 | 1 | 1 | mod-owned |
| 27 | `net/neoforged/neoforge/registries/NeoForgeRegistries#ATTACHMENT_TYPESLnet/minecraft/core/Registry;` | 1 | 1 | 1 | loader |
| 28 | `org/polyfrost/oneconfig/api/config/v1/Config$Category#QOLLorg/polyfrost/oneconfig/api/config/v1/Config$Category;` | 1 | 1 | 1 | mod-owned |
| 29 | `net/minecraft/network/codec/StreamCodec#composite(Lnet/minecraft/network/codec/StreamCodec;Ljava/util/function/Function;Lnet/minecraft/network/codec/StreamCodec;Ljava/util/function/Function;Ljava/util/function/BiFunction;)Lnet/minecraft/network/codec/StreamCodec;` | 33 | 95 | 0 | game-bound |
| 30 | `net/minecraft/network/codec/StreamCodec#composite(Lnet/minecraft/network/codec/StreamCodec;Ljava/util/function/Function;Ljava/util/function/Function;)Lnet/minecraft/network/codec/StreamCodec;` | 30 | 95 | 0 | game-bound |
| 31 | `net/minecraft/network/codec/ByteBufCodecs#BOOLLnet/minecraft/network/codec/StreamCodec;` | 28 | 121 | 0 | game-bound |
| 32 | `net/minecraft/network/codec/ByteBufCodecs#STRING_UTF8Lnet/minecraft/network/codec/StreamCodec;` | 25 | 145 | 0 | game-bound |
| 33 | `net/minecraft/network/codec/ByteBufCodecs#VAR_INTLnet/minecraft/network/codec/StreamCodec;` | 20 | 128 | 0 | game-bound |
| 34 | `net/minecraft/network/codec/StreamCodec#composite(Lnet/minecraft/network/codec/StreamCodec;Ljava/util/function/Function;Lnet/minecraft/network/codec/StreamCodec;Ljava/util/function/Function;Lnet/minecraft/network/codec/StreamCodec;Ljava/util/function/Function;Lcom/mojang/datafixers/util/Function3;)Lnet/minecraft/network/codec/StreamCodec;` | 20 | 58 | 0 | game-bound |
| 35 | `com/mojang/serialization/codecs/RecordCodecBuilder#create(Ljava/util/function/Function;)Lcom/mojang/serialization/Codec;` | 18 | 66 | 0 | game-bound |
| 36 | `com/mojang/serialization/codecs/RecordCodecBuilder#mapCodec(Ljava/util/function/Function;)Lcom/mojang/serialization/MapCodec;` | 16 | 224 | 0 | game-bound |
| 37 | `java/util/Collections#synchronizedMap(Ljava/util/Map;)Ljava/util/Map;` | 16 | 17 | 0 | JDK |
| 38 | `com/google/common/collect/ImmutableMap#builder()Lcom/google/common/collect/ImmutableMap$Builder;` | 15 | 15 | 0 | library |
| 39 | `net/minecraft/network/codec/ByteBufCodecs#INTLnet/minecraft/network/codec/StreamCodec;` | 14 | 53 | 0 | game-bound |
| 40 | `com/google/common/collect/Maps#immutableEnumMap(Ljava/util/Map;)Ljava/util/Map;` | 14 | 14 | 0 | library |

### Baskets the `sole` column hides

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

## After the fixes

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

## Known limits of this census

- The probe executes a jar as shipped. A jar whose entrypoint only works with its own
  `@Overwrite` mixin applied (BiomesOPlenty's NeoForge build refuses in
  `glitchcore/util/Environment#getConfigPath`) is `refused` here even though the game's
  load-time weave makes it run; that jar's 221 neutral calls are not in these tables.
- `StdHost::unknown_host_calls` keeps at most 64 distinct keys per run. A jar that hits
  more distinct calls reports 64 of them; its label is still `ran-hollow`, and it is no
  jar's `sole` row because `sole` requires exactly one distinct call.
- The probe's host is smaller than the game's, so a jar that is `ran-clean` here is
  `ran-clean` under the game too; a `ran-hollow` jar here may be clean in game.
