# NeoForge `native-gap` rows: causes and fixes

The greedy 85-jar NeoForge 26.3 corpus (`tools/real_mod_fixtures/neoforge_corpus.lock.json`)
has 14 `recommend=native-gap` rows in
`tools/real_mod_fixtures/LAST_SERVER_LEDGER_NEOFORGE.txt`. This page records, per row, the
failing call, the root cause from `javap -c -p` on the jar, whether this batch fixed it, and the
measured before/after.

Repro, one jar at a time (the probe binary is built with `cargo build --profile fast -p
horizon-native-java`; `--game-version 26.3` supplies the release only the host can know):

```sh
target/fast/horizon-native-java --json --game-version 26.3 \
  ~/horizon-corpus/neoforge/<jar> neoforge
```

"Ran" below always means the `@Mod` constructor returned and the probe's JSON says
`"status":"ok"`. It does **not** mean `ran-clean`: every remaining neutral host answer is in
`unknown_host_calls`, and only YACL has none.

## Cause table

| Jar | Failing call in the ledger | Root cause | This batch | After |
|---|---|---|---|---|
| BiomesOPlenty | with glitchcore: `ExceptionInInitializerError` in `glitchcore/util/Environment#getConfigPath`; alone: `Map#containsKey` on null in `biomesoplenty/init/ModConfig#isBiomeEnabled` | Two chains. GlitchCore's `Environment` methods always throw and are replaced by the in-jar `@Overwrite` `glitchcore.neoforge.mixin.impl.MixinEnvironment`; that overwrite needs `FMLPaths.CONFIGDIR.get()`, which the host answered null. Alone, the null came from `JsonUtil` -> `FileUtils.readFileToString` -> `Gson.fromJson`, and `NightConfig.getOrElse` returned null instead of the caller's default | Fixed: loader statics (`FMLPaths`/`Dist`), NightConfig `getOrElse`, Commons IO text files | `ok`, 221 neutral calls (glitchcore/glitchcore-adjacent surfaces remain unmodelled). The in-jar overwrite is applied by the game's load-time weave, not by the entrypoint probe; with the weave applied the same jar reaches the same `ok` |
| EuphoriaPatcher | `MissingMethod net/neoforged/fml/loading/FMLEnvironment#getDist` | `Class.getMethod("getDist").invoke(null)`; the reflection dispatcher only ran methods present in the jar, so a real loader static was a missing member | Fixed: the reflected method list names the loader statics it can answer, and the host answers `getDist` from its own role | `ok`, 1 neutral call (`System.err` stream) |
| KotlinLangForge | `MissingMethod dev/nyon/klf/mv/TypeAliasesKt#<init>` | Not a mod class: `TypeAliasesKt` only contains the `@Mod` **descriptor string** inside `kotlin.Metadata`; the entrypoint scan matched any pool string, then tried to construct a class with no constructor. KLF has no `@Mod` class at all (it is a `modLoader="klf"` language adapter) | Fixed: entrypoints are read from parsed annotations | Refused as `no entrypoint` - correct for a language adapter; the old row was a false entrypoint. Reclassify as library/needs-loader, not a native gap |
| RoughlyEnoughItems | `NoSuchElementException` in `PrimitivePlatformAdapterImpl#getMinecraftVersion` | `ModList.get()` is a static **method** (not a field), so it was unmodelled and returned null; then `ModList.getModContainerById("minecraft")` was empty because the game id was never in the seeded mod map | Fixed: `ModList.get`, always-loaded ids, and a caller-seeded game version | `ok` with `--game-version 26.3`; 58 neutral calls (Guava/architectury surfaces). Without a seeded version the host still answers an empty Optional, as it should |
| TerraBlender | `Metadata malformed java/lang/Integer marker` in `TerraBlenderConfig#load` | `Config.addNumber` -> NightConfig `CommentedConfig.getOrElse(key, default)` answered null, so `Integer.intValue` saw no integer | Fixed: `getOrElse` returns its default for the empty config the runtime holds | `ok`, 4 neutral calls (NightConfig `TomlFormat`/`WritingMode` values) |
| almanac | `List#iterator` on null in `ItemNBTUtil#resetCache` (with pack: NPE at +90) | `AlmanacConfig.BLACKLIST_ITEMS` is a `dungeonconfig.ConfigEntry` from the **JarJar-nested** `META-INF/jarjar/DungeonConfig-*.jar`. The nested jar is reported but never put on the class path, so `ConfigEntry.get()` has no body and answers neutral null | **Out of scope**: needs nested-jar (JarJar) class linking, a larger feature than this batch | Refused, same message. Dependency chain: almanac -> dungeonconfig (nested). Linking `dungeonconfig` as a sibling jar would also work and is the cheap follow-up |
| architectury | `IllegalStateException Mod 'architectury' is not available!` | `EventBusesHooks.whenAvailable` maps `ModList.getModContainerById` to `ModContainer.getEventBus`; `ModList.get()` was null and the container had no event bus | Fixed: both loader methods are modelled | `ok`, 7 neutral calls (Guava lists, payload codecs, `NeoForge.EVENT_BUS`) |
| balm | `MissingMethod ...NeoForgeBalmCapabilities$Registrations#<init>(String)` | `ModBusEventRegisters.getRegistrations` asks for `Registrations(String)`, catches `NoSuchMethodException`, and uses the no-arg constructor. The runtime invented the requested signature, so the fallback never fired; the real call then ran with a `String` against a class with no such constructor | Fixed: `Class.getConstructor` answers only declared constructors and raises a catchable `NoSuchMethodException` | Advanced past the old failure; now refuses at `NeoForgeBalmCapabilities#addExistingType` because `Capabilities.Item.BLOCK` (NeoForge API class) is not modelled, so the capability is null. **Out of scope**: needs the NeoForge capability classes, whose type/context classes cannot be guessed |
| fzzy_config | `ExceptionInInitializerError` in `ContextInput.<clinit>`, `fromEnum(...) must not be null` | `StringRepresentable.fromEnum(Supplier)` returns an `EnumCodec`; the host answered null and Kotlin's `checkNotNullExpressionValue` raised | Fixed: the factory answers an opaque codec object | `ok`, 4 neutral calls (`NeoForge.EVENT_BUS`) |
| puzzleslib | `NullPointerException mod list is null` in `NeoForgeModContainerHelper#getOptionalModContainer` | Same static-method gap as REI: `ModList.get()` was null | Fixed: `ModList.get` | Advanced past the old failure; now `NullPointerException mod id context is null` in `NeoForgeEventInvokerRegistryImpl$NeoForgeEventInvoker#getEventBus` - its event registry was built with a null context/event-bus. **Out of scope** here: needs the mod-bus context dispatch, not a one-call model |
| veinminer-client | `NullPointerException get(...) must not be null` in `VeinminerClient.<init>` | The class is `@Mod(dist = CLIENT)`; a dedicated server never loads it | Fixed (discovery): `dist` is honoured, so a server run has no entrypoint | Refused as `no entrypoint`; reclassify as `client-only-untested`, not a native gap |
| veinminer | `NullPointerException name(...) must not be null` in `Veinminer.<init>` | `DetectedVersion.tryDetectVersion().name()` - the game version, which the host had no channel for | Fixed: `DetectedVersion`/`WorldVersion` answer the seeded version | Advanced past the old failure; now `BaseConfigManager.<init>` gets a null `jsonModule` from the unbundled `kotlinx.serialization` runtime. **Out of scope**: needs the Kotlin serialization runtime |
| YACL | `ExceptionInInitializerError` in `YACLPlatform$1.<clinit>` | Already fixed on `main` before this batch (the ledger predates it); no code change here | None | `ok`, 0 neutral calls - the only `ran-clean` row |
| zoomify | `ExceptionInInitializerError` in `NeoforgeZoomifyPlatform#getConfigDir`, `get(...) must not be null` | `FMLPaths.CONFIGDIR.get()` answered null | Fixed: `FMLPaths` | Advanced past the old failure; now `Zoomify.<clinit>` calls client `KeyMapping.Category.register` and gets null. **Out of scope** for a server ledger: that is the client key-mapping surface |

## What changed

- `@Mod` discovery reads the parsed annotation, not any copy of its descriptor, and honours the
  annotation's `dist` member (`discover_entrypoints_on`).
- `neoforge_loader.rs`: `FMLEnvironment` (`getDist`, `dist`, `isProduction`), `Dist`,
  `FMLPaths` (`GAMEDIR`/`CONFIGDIR`/`MODSDIR`), `ModList.get`, always-loaded mod ids,
  `ModContainer.getEventBus`/`getModInfo`, `IModInfo.getVersion`, `ArtifactVersion.toString`,
  `DetectedVersion`/`WorldVersion`, and `StdHost::with_game_version`.
- `night_config.rs`: `UnmodifiableConfig`/`CommentedConfig#getOrElse` answers its default.
- `commons_io.rs`: `FileUtils.write`/`readFileToString` over the runtime's virtual files.
- `game_codecs.rs`: `StringRepresentable.fromEnum` answers a non-null opaque codec.
- Reflection: a modelled loader static can be selected by `Class.getMethod`, and
  `Class.getConstructor` refuses signatures the class does not declare (catchable
  `NoSuchMethodException`).

## Known left as neutral, not fixed

No jar here is `ran-clean` except YACL. The largest remaining families are Guava factories
(`Lists.newArrayList`, `Maps.newTreeMap`, `HashBasedTable.create`), NeoForge's `NeoForge.EVENT_BUS`
static, the payload codecs, and the client key-mapping surface. None of them refused the jar in
this batch; they are the `ran-hollow` remainder and stay visible in `unknown_host_calls`.
