# Design: NeoForge data attachments on the fork's store

Status: **proposal, for review.** Nothing here is implemented. It replaces the "Mod-owned data
attachments" note in [mod-plan.md](mod-plan.md) (#34) once it is accepted.

**Why.** `Player#getData(Supplier)` is the largest single in-game failure: 616 of the 621 listener
failures in the [re-survey](in-game-survey.md#re-survey-after-the-honesty-fixes-11-november-2026-elysium),
all of them carryon, plus `/carryon debug`. Four other corpus NeoForge jars use the same API (balm, curios,
puzzleslib, CreativeCore; see [who uses it](#who-uses-it)). Today every such call is a counted unknown.

**Short answer.** Bridge Java attachment types onto the fork's erased `AttachmentStore` (option A). The
value's saved form comes from the mod's own codec, which runs in the runtime. Two things carryon needs today
would be modelled narrowly: `CompoundTag`, and the three codec nodes its serializer is built from. Values are
re-encoded after every Java call batch, so changes made in place reach disk without a fork change. Sync is
not sent and is counted. The first slice is players only. A pre-save fork hook (issue text in
[§6](#6-fork-issue-text-not-filed)) removes the re-encode departure later. Non-player holders need it.

Sources: bytecode of `neoforge-26.3.0.101-universal.jar`, NeoForge's patched server jar
`minecraft-server-patched-26.3.0.101.jar` (both under `~/horizon-corpus/nf263/libraries/`), and
`carryon-neoforge-26.3-2.12.1.jar` (`~/horizon-corpus/neoforge/`), read with `javap -c -p -v`. The fork
was read at the pin `cf0113879ebee600bff86070466f75b2db90b40a` (`mod/horizon-mod.toml`). Offsets are bytecode
offsets in the named method. Nothing decompiled is reproduced here.

## 1. NeoForge 26.3's contract (from bytecode)

### 1.1 `AttachmentType` and its builder

`AttachmentType` keeps five package-private fields:

- `defaultValueSupplier` (`Function<IAttachmentHolder,T>`)
- `serializer` (`IAttachmentSerializer`)
- `copyOnDeath` (`boolean`)
- `copyHandler` (`IAttachmentCopyHandler`)
- `syncHandler` (`AttachmentSyncHandler`, not final)

**Factories.**

- `builder(Supplier)` wraps the supplier as a function that ignores the holder, then calls `builder(Function)`,
  which is `new Builder(fn)`.
- `serializable(Supplier|Function)`, for `T extends ValueIOSerializable`, is `builder(fn).serialize(new
  AttachmentType$1(fn))`. That serializer's `read` makes a default value and calls `deserialize(ValueInput)`
  on it. Its `write` calls `serialize(ValueOutput)` and returns true.

**`Builder` methods.**

| method | effect | throws |
|---|---|---|
| `serialize(IAttachmentSerializer)` | sets the serializer | NPE on null; `IllegalStateException("Serializer already set")` if one is set (@9–21) |
| `serialize(MapCodec)` | `serialize(codec, Predicates.alwaysTrue())` | |
| `serialize(MapCodec, Predicate)` | `Builder$1`: `write` returns false when the predicate fails, else `out.store(codec, v)` and true (the map codec's fields go inline into the type's own child); `read` is `in.read(codec).orElseThrow(...)` | read: `IllegalStateException("Unable to read attachment due to an internal codec error.")` |
| `copyOnDeath()` | sets the flag | `IllegalStateException("copyOnDeath requires a serializer")` if no serializer **yet** (@1–16): order matters |
| `copyHandler(h)` | sets it | NPE; `IllegalStateException("copyHandler requires a serializer")` (@5–21) |
| `sync(AttachmentSyncHandler)` | sets it | NPE |
| `sync(StreamCodec)` / `sync(BiPredicate<IAttachmentHolder,ServerPlayer>, StreamCodec)` | `Builder$2`: the predicate is `sendToPlayer`, the stream codec writes and reads | |
| `build()` | `new AttachmentType(this)`, no further check | |

There is no `serialize(Codec)` overload in 26.3. Mods pass `codec.fieldOf(name)`, which is a `MapCodec`.

**Default copy handler.** The constructor (`AttachmentType.<init>(Builder)` @28–50) uses the builder's
`copyHandler` if one was set, otherwise `defaultCopyHandler(serializer)`.

- **With a serializer**, the default handler is a round trip:
  1. Write into a fresh `TagValueOutput.createWithContext(ProblemReporter$Collector, provider)`.
  2. If `write` returned false, the handler returns `null`, meaning "don't copy" (@26–30).
  3. If the reporter collected a problem, it throws `IllegalArgumentException("Attachment failed to serialise
     during copy: …")`.
  4. Otherwise it reads the result back through `TagValueInput.create(...)` and `serializer.read(newHolder,
     input)` (@83). A problem on the way back throws `"…deserialise during copy…"`.
- **Without a serializer**, the default handler throws `UnsupportedOperationException("Cannot copy
  non-serializable attachments")`. It is never reached, because the copy skips such types (§1.5).

### 1.2 The interfaces

26.3 uses `ValueInput`/`ValueOutput`, not `CompoundTag`.

- `IAttachmentSerializer<T>`:
  - `T read(IAttachmentHolder, ValueInput)`
  - `boolean write(T, ValueOutput)`: `false` omits the entry.
  - There is no `HolderLookup` argument; the provider lives inside the value I/O.
- `IAttachmentCopyHandler<T>`: `@Nullable T copy(T, IAttachmentHolder newHolder, HolderLookup.Provider)`.
- `AttachmentSyncHandler<T>`:
  - `boolean sendToPlayer(IAttachmentHolder, ServerPlayer)`: a default method that answers `true`.
  - `void write(RegistryFriendlyByteBuf, T, boolean initialSync)`
  - `@Nullable T read(IAttachmentHolder, RegistryFriendlyByteBuf, @Nullable T previous)`

### 1.3 `IAttachmentHolder` and `AttachmentHolder`

**The interface.**

- Abstract: `hasAttachments`, `hasData(AttachmentType)`, `getData(AttachmentType)`,
  `getExistingDataOrNull(AttachmentType)`, `setData(AttachmentType, T)`, `removeData(AttachmentType)`.
- Default methods:
  - Every `Supplier` overload is `supplier.get()`, then the typed call.
  - `getExistingData` is `Optional.ofNullable(getExistingDataOrNull(..))`.
  - `syncData(AttachmentType)` does nothing.

**`AttachmentHolder`, the base class.**

- **`getData` is a write** (`AttachmentHolder#getData` @0–52):
  1. `validateAttachmentType`.
  2. `map.get(type)`. If that is null: `defaultValueSupplier.apply(getExposedHolder())`, `map.put`,
     `syncData(type)`.
  3. Return the value. It is **the live object**, not a copy, and the default is not null-checked.
- **`setData`** (@5–28): `requireNonNull(data)` (so a null is an NPE), `put`, `syncData(type)`, returns the
  previous value.
- **`removeData`**: returns null when the map was never allocated. Otherwise it removes, calls `syncData` even
  when nothing was there, and returns the previous value.
- `hasData` and `getExistingDataOrNull` read without allocating.
- **Storage** is `IdentityHashMap(4)`, made lazily (`getAttachmentMap` @0–16), so iteration order is
  unspecified.
- **`validateAttachmentType`** always null-checks the type. Only in development
  (`!FMLEnvironment.isProduction()`) does it also require the type to be registered (@12–52). A production
  server, which is what a player runs, skips that check.

**Saving.** `serializeAttachments(ValueOutput, Predicate)` @0–161:

- For each type with a serializer that passes the predicate, it writes into `out.child(<registry id>)`.
- If `write` returns false, that child is discarded (@125–134).
- **Any exception** in @89–139 is logged as `Failed to serialize data attachment {}. Skipping.` and the entry
  is left out of that save. The value stays in memory.
- An unregistered type (null id) fails the same way.

**Loading.** `deserializeAttachments(ValueInput, Predicate)` @0–172:

- Walks the saved keys.
- An unparsable key logs `Encountered invalid data attachment key {}. Skipping.`
- An unknown or serializer-less id logs `Encountered unknown or non-serializable data attachment {}.
  Skipping.`, and **that data is dropped** at the next save.
- Otherwise it does `map.put(type, serializer.read(holder, child))` (@116–144). There is no `syncData`, and
  existing entries are not cleared.
- An exception logs `Failed to deserialize data attachment {}. Skipping.`

### 1.4 Holders, and when they save and load

| holder | saved under | save | load |
|---|---|---|---|
| `Entity` (players included) | `neoforge:attachments` → `<ns:path>` → the serializer's fields | `Entity#saveWithoutId` @414–442 (child dropped if empty) | `Entity#load` @582–597 |
| `BlockEntity` | same key | base `saveAdditional` @47–75; `setData`/`removeData` call `setChanged()` first | base `loadAdditional` @21–36 |
| `ChunkAccess` (an `AttachmentHolder.AsField`) | `neoforge:attachments` in the chunk | `SerializableChunkData#copyOf` @522–545 → `writeAttachmentsToNBT`; `write` @487–502 | `parse` @702–716, `read` @717–733 → `readAttachmentsFromNBT`; proto → full promotion copies through the copy handlers (`LevelChunk.<init>` @186–193) |
| `Level` (`ServerLevel`) | SavedData `neoforge:data_attachments`, entries at top level | its codec's encode → `serializeAttachments`; `isDirty()` is always true | `ServerLevel.<init>` @652 → `LevelAttachmentsSavedData.init` |
| server session (`MinecraftServer#getSessionData`, new in this line) | SavedData `neoforge:session_attachments` | as Level | `MinecraftServer.<init>` @647 |

### 1.5 Death, End return, conversion, dimension change

`AttachmentInternals#copyAttachments(provider, from, to, predicate)` @0–130:

- Iterates `from`'s entries and skips types with no serializer.
- For each type the predicate accepts, it calls `copyHandler.copy(value, to, provider)` and puts the
  non-null result straight into `to`'s map.
- It calls no `syncData` and catches nothing.

`copyEntityAttachments(from, to, isDeath)` (@0–26) uses the predicate `type.copyOnDeath` when `isDeath`, and
"always" otherwise. Where it runs:

- **Player clone.**
  - `ServerPlayer#restoreFrom(old, restoreAll)` @371–382 → `EventHooks.onPlayerClone(this, old,
    !restoreAll)` → `PlayerEvent.Clone` → `AttachmentInternals.onPlayerClone` → `copyAttachmentsFrom(original,
    wasDeath)`.
  - **After a death only `copyOnDeath` types are copied. After leaving the End (`restoreAll = true`) every
    serializable type is copied, as a new object made by the copy handler. Types without a serializer are
    never copied: they are gone in both cases**, because the player object is new
    (`PlayerList#respawn` @97).
- **Mob conversion.** `onLivingConvert` copies `copyOnDeath` types only. It fires from each conversion's
  lambda (`ConversionTracker`, `ZombieVillager`, `Villager.thunderHit`, ...).
- **Non-player dimension change.**
  - `Entity#teleportCrossDimension` calls `EntityType.create` then `restoreFrom(this)` @134.
  - `restoreFrom` is a full `saveWithoutId` → `load` round trip, so every serializable type is copied and the
    others are lost.
- **Player dimension change.** It is the same object (`ServerPlayer#teleport`): everything stays and is
  re-synced to the player.

### 1.6 Sync

- **Payload.** `SyncAttachmentsPayload`, id `neoforge:sync_attachments`. It carries a target (block entity,
  chunk, entity, level or session), the types by id in `neoforge:synced_attachment_types`, and one byte array.
- **Per-entry update.** `true` then `handler.write(buf, value, false)`, or `false` for a removal.
- **Who receives it.** A player gets it only if `sendToPlayer` passes and the connection
  `hasChannel(neoforge:sync_attachments)`. A client without NeoForge never receives it.
- **Triggers** (`AttachmentSync`):
  - every `syncData`: a `getData` that creates the default, `setData`, `removeData`;
  - an entity: its watchers (`ChunkMap#getPlayersWatching`), plus the player itself;
  - a chunk: `ChunkMap#getPlayers`;
  - a block entity: batched through `ChunkHolder#broadcastBlockEntity` @34;
  - a level: `level.players()`;
  - the session: every player.
- **Initial syncs.**
  - `ServerEntity#sendPairingData` @407, when another player starts tracking.
  - `PlayerList#placeNewPlayer` @736 (session) and @740 (the player itself).
  - `PlayerList#respawn` @524 and `ServerPlayer#teleport` @387 (the player itself).
  - `PlayerList#sendLevelInfo` @155 (level).
  - `ChunkWatchEvent.Sent` (a chunk and its block entities, in one bundle).
- **Types without a sync handler** are never sent.

### 1.7 Registration

- Types register in `NeoForgeRegistries.ATTACHMENT_TYPES`, registry key **`neoforge:attachment_types`**
  (plural), usually through `DeferredRegister.create(ATTACHMENT_TYPES, modid)`. The `register(name, Supplier)`
  supplier runs at the registry's `RegisterEvent`.
- An add-callback mirrors synced types into `neoforge:synced_attachment_types`.
- In production an unregistered type works in memory, but it cannot be saved (§1.3).

> The runtime answers `NeoForgeRegistries.ATTACHMENT_TYPES` as `registry/neoforge:attachment_type`
> (`neoforge_loader.rs`, singular). The bytecode's key is `neoforge:attachment_types`. Slice 1 fixes the
> name.

### 1.8 carryon's attachment, exactly

**The declaration.** `CarryOnNeoForge.<clinit>` registers `"carry_on_data"` on
`DeferredRegister.create(ATTACHMENT_TYPES, "carryon")`. The supplier `lambda$static$0` @0–34 builds:

```
AttachmentType.builder(() -> new CarryOnData(new CompoundTag()))   // lambda$static$1
    .sync(new CarryOnDataSyncHandler())
    .serialize(CarryOnData.CODEC.fieldOf("CarryOnData"))
    .build()
```

- There is **no `copyOnDeath` and no `copyHandler`**. Per §1.5 it is dropped on death and copied on an End
  return.
- `CarryOnDataSyncHandler` uses `STREAM_CODEC = ByteBufCodecs.fromCodecWithRegistries(CODEC)`. Its
  `sendToPlayer` casts the holder to `ServerPlayer` and also requires a live connection, both players alive,
  and `tickCount > 0`.

**`CarryOnData`.**

- `CODEC` (`<clinit>` @13–31) is `CompoundTag.CODEC.flatXmap(lambda$static$0, lambda$static$1)`:
  - decode is `DataResult.success(new CarryOnData(tag))`, with any `Exception` turned into
    `DataResult.error`;
  - encode is `DataResult.success(getNbt())`, with the same catch.
- `getNbt()` is `nbt.copy()`.
- **The object's whole state is its `nbt` compound.** Every setter writes the matching key into it:
  - `setKeyPressed` → `putBoolean("keyPressed")`
  - `setSelected` → `putInt("selected")`
  - `setTick` → `putInt("tick")`
  - `setBlock` → `putString("type")`, `put("block", NbtUtils.writeBlockState(..))` and `put("tile", ...)`
  - `clear()` → a new `CompoundTag`
- The constructor reads `type` (`getStringOr` + `CarryType.valueOf`, `INVALID` if absent), `keyPressed`
  (`getBooleanOr`) and `selected` (`getIntOr`). Only when an `activeScript` key is present does it also call
  `CarryOnScript.CODEC.parse(NbtOps.INSTANCE, ..)` and `getOrThrow`.

**Call sites.** There are two direct ones, both on `Player`, both in `NeoForgePlatformHelper`:
`getCarryData` (`getData(Supplier)`) and `setCarryData` (`setData(Supplier, Object)`). They are reached from
55 sites in 15 classes, some of them client-side.

**The failing listener.** `CommonEvents#onServerTick(ServerTickEvent$Post)` walks
`getPlayerList().getPlayers()` → `CarryOnCommon#onCarryTick(ServerPlayer)`:

1. @1: `getCarryData`.
2. @6: `isCarrying()`, a field compare against `INVALID`.
3. If not carrying, @9 `ifeq 108` returns.

So for a player who carries nothing, the listener needs exactly:

- `getData` with the default supplier;
- `new CompoundTag()`, `contains`, `getStringOr`, `getBooleanOr` and `getIntOr`;
- the field compare.

## 2. The fork's store at the pin

Read from:

- `crates/horizon-mod-runtime/src/attachments.rs` (re-exported as `mod_api::{AttachmentType, AttachmentStore,
  Holder, ...}`);
- `crates/horizon-mod-vocab/src/backend.rs` and `host/mod_state.rs`;
- `crates/horizon-client/src/game/modding/mod_api_host.rs` (`attachment_map`);
- `crates/horizon-client/src/game/network/lan_actions/respawn.rs`;
- `crates/horizon-client/src/lan/playerdata.rs`, `src/save.rs`;
- the chapter `docs/modding/src/concepts/trait-mods/028-server-attachments.md`.

**Two layers.**

- **The typed `AttachmentType<T>`** is a `static`:
  - `&'static str` namespace and name;
  - `default: fn() -> T`;
  - `codec: Option<AttachmentCodec<T>>`, with `encode: fn(&T) -> Result<Compound>` and `decode: fn(&Compound)
    -> Result<T>`;
  - two flags, `copy_on_death` and `sync`.

  It cannot be built from a jar at run time.
- **The erased `AttachmentStore`** is what the typed methods stand on:
  - methods: `holds`, `decoded`, `saved`, `store`, `discard`, `queue_sync`;
  - the key is two `&str`;
  - a value is `Arc<dyn Any + Send + Sync>`;
  - the saved form is `AttachmentForm { encoded: Option<Compound>, policy: { copy_on_death, sync }, write:
    Decoded | Default | Set }`.

  Every host trait is an `AttachmentStore` (`host_attachment_store!` in `backend.rs`). **A bridge can use the
  erased layer with keys made at run time; it needs no `static`.**

**Holders.**

- `Holder::{Player(uuid), Entity(id), BlockEntity([x,y,z]), Chunk([x,z]), Level}`, in the host's current
  dimension.
- A `Player(uuid)` resolves for:
  - the acting player;
  - any **online, placed** guest in any dimension (`hub.holder_guest`);
  - a non-online placed guest in the current dimension only.

  Anything else is `MissingHolder`. A guest whose state is swapped out is `MissingHolder`, and a holder in
  extraction is `HolderBusy`.
- Read-only level hosts have no attachments, and client hosts keep only the synced copy.

**Saving.**

- `Attachments::to_nbt` writes `horizon:attachments` → `<namespace>` → `<name>` → `{value: <compound>,
  copy_on_death: 1b, sync: 1b}` (a flag only when set).
- Players go into their `.dat`, entities and block entities into their own NBT, chunks into the chunk, and
  levels into `data/minecraft/horizon_attachments.dat`.
- **The saved `value` is the `encoded` compound captured when `store()` was called.** Nothing re-encodes at
  save time. A Rust value is cloned out, so a Rust mod must `set` to change it, and the cache is always
  current for Rust. A Java value is changed in place.
- An entry with `encoded: None` (a type with no codec) is not saved.
- Saved entries of unknown mods survive load and save undecoded.

**Loading.**

- A saved entry is `Raw` until the first typed read decodes it (`get_existing`).
- `holds` is true for a raw entry, even one that will not decode.

**Death.**

- `Attachments::copy_on_death` keeps the entries whose policy (or saved `copy_on_death` byte) is set, **as
  raw saved envelopes**, and drops the rest. `guest_respawn` then calls `replace_contents` with that copy.
- If any `copy_on_death` entry has no encoding, the whole call fails with `InvalidCopyPolicy`, and
  `respawn.rs` then **skips** the replacement, keeping every entry. Typed Rust callers can't reach this,
  because `validate_policy` forbids `copy_on_death` without a codec. An erased caller can, so the bridge must
  never store `copy_on_death: true` with `encoded: None`.
- An End return (`player_clone` with `was_death = false`) keeps the map as it is.
- Living-entity conversion is not supported.

**Sync.**

- `sync: true` needs a codec. Changes in one tick are sent once, at its end, on the custom payload
  `horizon:attachment_sync` (`horizon-protocol/src/attachment_sync.rs`).
- **Only Horizon clients** receive it. The wire form is the disk codec's compound, and there is no
  `sendToPlayer` predicate.
- Recipients are the holder's trackers (the player and its trackers, the level's players, and so on), and
  there are initial syncs when a client starts seeing a holder.

**What the 028 chapter itself says is not covered.** A holder-aware supplier, a custom copy handler,
conditional serialization, the `sendToPlayer` predicate, partial sync updates, and a wire codec apart from the
disk codec.

### 2.1 The exact gap

| NeoForge 26.3 | fork at the pin | consequence for a Java bridge |
|---|---|---|
| a type is a run-time object, built by mod bytecode | typed types are `static`; the erased store takes `&str` keys | use the erased store; keep the Java type in the runtime |
| default from `Function<IAttachmentHolder,T>` (mod bytecode) | `fn() -> T` | run the supplier in the runtime, and `store(.., Default)` the result |
| `getData` returns the live object; mods change it in place | values are cloned out; `encoded` is cached at `store()` | the bridge must re-encode (§3.1) |
| serializer = mod bytecode over `ValueOutput`, at save time | `fn` codec, at `store()` time | the runtime runs the codec; when it runs is §3.1 |
| on disk `neoforge:attachments.<ns:path>.<fields>` | `horizon:attachments.<ns>.<path>.value.<fields>` | the per-type compound is the same bytes; the envelope is not (departure D1) |
| unknown saved type: dropped with a log | kept, undecoded | departure D2 (keeps more than NeoForge) |
| load decodes eagerly in `Entity#load` | lazy, at first typed read | decode at first access, plus an eager pass at join (D3) |
| `hasData` after a failed decode: false | `holds`: true for any raw entry | the bridge decodes before answering `hasData` |
| death: `copyOnDeath` through the copy handler | `copy_on_death` raw envelope copy | the same for the default handler and a pure codec; a custom `copyHandler` needs the runtime at `player_clone` |
| End return: serializable types copied as new objects, transient ones dropped | map kept as is | the bridge round-trips and drops at `player_clone(was_death = false)` |
| conversion: `copyOnDeath` types copied | not supported | non-player holders are out of slice 1 |
| sync: StreamCodec bytes to NeoForge clients, `sendToPlayer` filter | disk-codec compound to Horizon clients, no filter | no consumer exists on a Horizon client; counted (§3.3) |
| holders: entity, block entity, chunk, level, session | player, entity, block entity, chunk, level | the server session has no holder (counted) |

## 3. Options

There are three ways to bridge, compared in §3.4. Three questions cut across all of them:

- when to encode, because Java changes values in place (§3.1);
- which codecs the runtime can run (§3.2);
- sync (§3.3).

### 3.1 Changed in place: when to re-encode

carryon's `CarryOnDataManager.setCarryData` calls `setData`, but `onCarryTick` and the packet handlers change
the object `getData` returned (`setKeyPressed`, `setSelected`, `setTick`). Other mods do the same. The fork
saves the encoding taken at `store()`, so without a re-encode those changes never reach disk.

**R1. Re-encode after every Java call batch.**

- Java code runs only inside this mod's hooks: listeners, commands, the content phase. So the heap cannot
  change between two batches.
- After a batch that ran any Java, for each **live serialized value** of that jar:
  1. Run its serializer.
  2. Compare the result to the last stored encoding.
  3. If it differs, `store(.., Set)` it with `sync: false`.
- *Exact:* for a **pure** serializer, the bytes on disk are the bytes NeoForge would serialize at the next
  save, since nothing can change between the batch and the save.
- *Departs (D4):*
  - The serializer runs once per batch instead of once per save. A serializer with side effects, or whose
    output depends on something other than the value (a clock, a counter), behaves differently.
  - A serializer that throws fails at the end of a tick instead of during a save. The outcome is the same:
    the entry is left out of the save (§3.5).
  - Cost is one interpreted encode per live value per batch. It is bounded and measured, and the runtime
    reports it.
- **Needs no fork change.** It only has to be every *live* value, not only the ones `getData` returned in
  this batch: a mod may keep the object in a static field and change it later.

**R2. A pre-save fork hook.**

- The fork asks the mod to refresh a holder's attachments **immediately before it captures them for saving**,
  on the server thread.
- NeoForge counterpart: `AttachmentHolder#serializeAttachments`, which runs inline in `Entity#saveWithoutId`
  @414–442, `BlockEntity#saveAdditional` @47–75, `SerializableChunkData#copyOf` @522–545 and the Level
  SavedData codec. NeoForge has no event for this: the serializer *is* the save.
- *Exact:* the same bytes, and the serializer runs once per save, so a side-effecting or throwing serializer
  behaves as on NeoForge.
- *Needs a fork seam:* the existing hooks are all **after** the write (`player_save_to_file` is documented as a
  later notification after an asynchronous write; `level_save` is "was saved"), so none can stand in.
  Issue text is in §6.
- It is also what non-player holders need. Re-encoding every loaded chunk's and block entity's attachments
  after every batch (R1) costs too much. NeoForge also does not mark a chunk unsaved for an in-place change
  (only `setData`/`removeData` call `markUnsaved`), and R1's `store` might, so for chunks R1 would save more
  than NeoForge.

**Decision.** R1 for slice 1, which is players only and carryon's serializer is pure (§1.8). R2 is filed for
later; nothing in slice 1 waits on it.

### 3.2 Which codecs can run: model narrowly or count

Today DFU codecs are opaque markers (`neutral_unmodelled_result` → `<type>#codec`). There is no
encode/decode model, and nothing models `CompoundTag`, `ValueInput` or `ValueOutput`. A type is only
**bridgeable** if the runtime can run its serializer exactly. Otherwise every call on it stays a counted
refusal, as today. No type is ever held in memory only because its serializer cannot run. That was #34's
objection, and it stands: it would silently lose data on restart.

**What carryon needs, and nothing more.**

1. **A modelled `CompoundTag`**, a runtime value backed by an NBT tree. The new module `java_nbt.rs` gives the
   runtime its own NBT enum. The runtime crate depends only on serde_json and zip, so the mod converts it to
   and from `mod_api::Compound`, keeping tag types exactly (byte/short/int/long/float/double, string, list
   element type, byte/int/long arrays).
   - Methods: `<init>()`, `copy()`, `contains(String)`, `get(String)`, `put(String, Tag)`,
     `putString/putBoolean/putInt`, `getStringOr/getBooleanOr/getIntOr`, `getCompoundOrEmpty`.
   - Each one's behaviour is read from the server jar's bytecode before it is written: for example, how
     `getBooleanOr` treats a non-byte tag, or `contains` on a key of another type.
   - Any other `CompoundTag`/`Tag` method stays an unmodelled, counted call.
2. **A codec algebra of three nodes**, evaluated by the runtime:
   - `CompoundTag.CODEC`: identity on a compound. Decoding a non-compound is an error.
   - `Codec#flatXmap(Function to, Function from)`: runs the two mod lambdas, which return `DataResult`, so
     model `DataResult.success(Object)` and `DataResult.error(Supplier)`; a `DataResult` from anywhere else
     stays opaque.
   - `Codec#fieldOf(String)` → a `MapCodec` of one key: encoding writes `{<name>: value}` inline into the
     type's child; decoding a missing key is an error.

   An error from any node, on encode or decode, is the error NeoForge's `Builder$1` would raise, and it is
   handled as §3.5 says.
3. **The serializer kind** `serialize(MapCodec)`, plus `serialize(MapCodec, Predicate)` (the predicate is mod
   bytecode run before `write`; false means "omit").

**Counted, not modelled, in slice 1.** A type is unbridgeable if anything in its serializer is outside the
above, for example:

- `RecordCodecBuilder`, primitive codecs (`Codec.INT`...), `xmap`, `optionalFieldOf`, `listOf`, `dispatch`;
- a custom `IAttachmentSerializer` class: mod bytecode over `ValueInput`/`ValueOutput`, which is slice 2;
- `serializable(..)`/`ValueIOSerializable`.

Calls on an unbridgeable type are refused by name: `IAttachmentHolder#getData (attachment carryon:x:
serializer not modelled: RecordCodecBuilder)`, counted as today. Types with no serializer (transient) are
bridgeable without any codec.

**Unknown calls inside a codec, supplier or copy handler.** An unmodelled call there is answered neutrally,
as everywhere in the runtime, but **a value or an encoding built on a neutral answer never reaches the
store**:

- an encode or decode that counted any unknown host call is treated as a failed encode or decode (§3.5);
- a default supplier that counted any unknown makes that `getData` throw a counted refusal, and nothing is
  inserted.

So the store holds only exact bytes. carryon's own constructor reaches `CarryOnScript.CODEC.parse` only when
an `activeScript` key is present. In slice 1 that is an opaque codec, so such a decode is refused, never
guessed.

### 3.3 Sync

The fork offers `sync: true`, which sends the disk codec's compound to Horizon clients on
`horizon:attachment_sync`. No Horizon client has a consumer for it: the Java mod's client side and its
`AttachmentSyncHandler#read` do not run there. NeoForge would send `STREAM_CODEC` bytes only to NeoForge
clients, and only past `sendToPlayer`.

So Java types are stored with **`sync: false`**. Every point where NeoForge would enter the sync path for a
synced type is **counted** as an unknown named `neoforge:sync_attachments (not sent)`:

- `getData` creating the default;
- `setData` and `removeData`;
- an explicit `syncData`;
- the initial syncs at join, respawn and dimension change.

This is counted without running `sendToPlayer`, so it counts attempts, not sends (named). Using the fork's sync
later needs a client-side Java runtime first. Even then the fork would lack the `sendToPlayer` predicate and a
wire codec separate from the disk codec. Those become issues when that work starts, not now.

### 3.4 The options

**(a) Bridge Java types onto the fork's store; the mod's codec runs in the runtime.**

- *Shape.*
  - The runtime keeps each jar's `JavaAttachmentType`: id, default supplier, serializer (codec tree, custom
    object, or none), `copyOnDeath`, copy handler, sync handler.
  - It keeps a live map `(holder, type) → heap value`.
  - The fork's store is authoritative for **existence and the saved bytes**. The bridge stores a marker
    `Arc<JavaAttachment { jar, token }>` as the value, with `encoded` = the codec's output (or `None` for a
    transient type) and `policy { copy_on_death: type.copyOnDeath && serializable, sync: false }`.
  - On every access the runtime's entry is checked against the store: if the store's decoded marker has the
    same token, the heap object is current. If the store holds a raw entry, decode it. If it holds nothing,
    take the default. That way a respawn, a logout or a restart that replaced the map on the fork side can
    never be answered from a stale heap object.
  - The live call goes through a new runtime trait `AttachmentBackend` (holds/saved/store/discard over the
    runtime's NBT type), which the mod implements over `host.attachments()`. Only firings whose hook has a
    `&mut dyn ServerHost` pass a real one; every other firing passes a refusing backend.
- *Exact:* the per-type compound on disk is the bytes NeoForge's serializer writes into its
  `<ns:path>` child, for every bridgeable type. In-memory semantics follow §1.3: the live object, default on
  first `getData`, `setData` returning the previous value and rejecting null, `removeData`. The death and End
  rules follow §1.5. The gaps are D1–D6 below.
- *Departs:*
  - **D1:** the envelope is `horizon:attachments.<ns>.<path>.value` rather than `neoforge:attachments.<ns:path>`,
    so a world moved between NeoForge and Horizon does not carry attachments.
  - **D2:** saved data of a removed jar is kept, where NeoForge drops it.
  - **D3:** decode happens at first access plus at join, instead of inside `Entity#load`, so a decoder with
    side effects runs at another moment.
  - **D4:** the R1 re-encode cadence.
  - **D5:** sync is not sent.
  - **D6:** End return round-trips through the codec at `player_clone`, after the fork kept the map. The
    result is the same; only the moment differs.
- *Fork seams:* none for slice 1. R2 (§6) is for non-player holders, and to remove D4.

**(b) A store held by the runtime, persisted as opaque bytes through the fork's API.**

- *Shape.*
  - The runtime is authoritative for values and existence.
  - It writes one fork entry per holder under the mod's own namespace (`horizon_java:<jar>`), holding a
    compound of `{<ns:path>: <serializer output>}` blobs, and reads it back at join.
- *Exact:* the bytes inside are the same as (a).
- *Departs:* everything in (a), and more:
  - existence lives in two places (the runtime map and the fork blob), so every fork-side event (respawn
    `replace_contents`, the copy-on-death policy, logout, holder unload, an entity's UUID-keyed save) needs
    the runtime to mirror it by hand;
  - one entry per jar means one undecodable type poisons the blob, and the fork's per-entry `copy_on_death`
    no longer applies: the bridge would reimplement death copying;
  - the fork's per-type query index and sync can never be used;
  - sizes are larger.
- *Fork seams:* the same as (a). It buys nothing over (a), because the erased layer already takes keys made at
  run time.

**(c) Transient only: no serializer runs.**

- Bridge only types with no serializer, with `encoded: None`.
- Exact for those: NeoForge never saves them and drops them at death and at End return.
- **It does not move carryon**, whose type is serialized and synced. Rejected as a first slice.

**(d) Hold serialized types in memory, and count the missing persistence.**

- `getData` works for every type; each save of a holder that carries a value counts "not persisted".
- This is #34's rejected option. carryon keeps the carried block and its block entity's NBT (a chest with
  items) in this attachment, so a restart while carrying would **delete items**. A count does not make that
  acceptable. Rejected.

### 3.5 Failure modes (option a)

| failure | NeoForge | bridge |
|---|---|---|
| serializer throws / `DataResult.error` / `write` returns false at encode | logged `Failed to serialize data attachment {}. Skipping.`, entry left out of that save, value stays in memory | same log once per type per batch, value kept, entry stored with `encoded: None` and `copy_on_death: false` so `to_nbt` leaves it out (never `copy_on_death` without bytes, §2); retried next batch; counted |
| decoder throws / error / missing field at load | logged `Failed to deserialize ... Skipping.`, entry dropped; next `getData` makes the default | same log; raw entry `discard`ed; `getData` then makes the default; `hasData` answers false |
| an unknown host call during encode/decode | (n/a: the JVM has the class) | treated as the matching failure above; the count names the call |
| default supplier throws | the exception reaches the caller | the Java exception reaches the caller (exact) |
| default supplier counts an unknown | (n/a) | `getData` throws a counted refusal; nothing inserted |
| a serializer, supplier or copy handler that never terminates | the server thread hangs | the jar's `ExecutionBudget` ends it; treated as a throw and reported by name (departure: NeoForge would hang) |
| `setData(type, null)` | NPE (`requireNonNull`, `AttachmentHolder#setData` @5) | NPE |
| fork `MissingHolder` / `HolderBusy` | (a holder always exists) | counted refusal naming the error |
| custom `copyHandler` throws at clone | propagates out of `restoreFrom` | slice 1 refuses types with a custom `copyHandler` (counted at registration and on each call) |

## 4. Recommendation and the first slice

**Option (a), with R1, the three-node codec algebra and a modelled `CompoundTag`, on players only.**

**Slice 1: what becomes real.**

- **Registration.**
  - `DeferredRegister.register(name, Supplier)` on the `ATTACHMENT_TYPES` register keeps its supplier. Today
    the supplier is dropped and `DeferredHolder.get()` answers an opaque `registered/...` marker.
  - The suppliers run at NeoForge's moment, the `RegisterEvent` for `neoforge:attachment_types`: once every
    entrypoint stage of every jar has run, in registration order, and only for registers whose
    `register(IEventBus)` was called. A `DeferredHolder.get()` before that is NeoForge's unbound-value NPE.
  - The builder chain is modelled with its exact checks (§1.1 table): `builder(Supplier|Function)`,
    `serialize(MapCodec[, Predicate])`, `copyOnDeath()`, `sync(..)` (recorded, not sent), `build()`.
    `serialize(IAttachmentSerializer)` and `copyHandler` are recorded as unbridgeable.
  - Registry name: `neoforge:attachment_types`.
- **Holder calls on `Player` / `ServerPlayer`** (both the `AttachmentType` and the `Supplier` overloads):
  - `getData`, `setData`, `hasData`, `getExistingData`, `getExistingDataOrNull`, `removeData`, `syncData`;
  - for bridgeable types, inside a firing whose hook gives a `&mut dyn ServerHost`.

  `server_tick` qualifies, and so do `player_logged_in` and `player_logged_out`, `player_respawned`,
  `player_clone`, `player_changed_dimension`, `break_block`, `attack_entity`, `living_death`, the server
  lifecycle hooks and `run_mod_command`. The rest pass the refusing backend.
- **Persistence:** R1 after every batch, and decode on first access plus an eager pass at
  `player_logged_in`.
- **Logout:** at `player_logged_out`, drop the runtime's entries for that UUID, after the fork has captured
  the last encoding.
- **Death** (`player_clone`, `was_death = true`):
  - the fork has already kept only `copy_on_death` envelopes;
  - the bridge drops every runtime object for that UUID;
  - `copyOnDeath` types decode from the copied bytes at the next access. That is exact for the default copy
    handler with a pure codec. carryon's type is simply gone, and its next `getData` makes a fresh default,
    as on NeoForge.
- **End return** (`player_clone`, `was_death = false`):
  - each serializable value is encoded and decoded into a **new** object (the default copy handler, §1.1);
  - a `write` that returns false drops the value;
  - every transient type is `discard`ed. The fork kept them, NeoForge does not.
- **Dimension change:** nothing moves (the same player). The initial sync to the player is counted as not
  sent.
- **`CompoundTag` and the codec algebra** as §3.2 lists.

**What stays counted after slice 1.**

- Every holder call on a non-player holder:
  - entities (`Entity#getData` on a mob, an item...);
  - block entities;
  - chunks (`ChunkAccess`/`LevelChunk`);
  - levels;
  - the server session (`MinecraftServer#getSessionData`, which has no fork holder).
- Any call on an unbridgeable type: a serializer outside the three nodes, a custom `IAttachmentSerializer`,
  `serializable(..)`, a custom `copyHandler`.
- Holder calls in a firing without a `ServerHost`: `level_tick_pre`, `right_click_block`, `right_click_item`,
  `living_damage_post`, `entity_interact`, and the other `HostSide` and level hooks.
- Every sync, as `neoforge:sync_attachments (not sent)`.
- Client-side `getData` (carryon's renderers and client events). There is no client bridge.
- The non-`CompoundTag` calls carryon makes while **carrying**: `NbtUtils.writeBlockState`,
  `TagValueOutput`, `BlockEntity#saveWithId`, `Inventory#setSelectedSlot`, `ServerPlayer#level()`
  (`arismoko/horizon#2747`), and so on. Slice 1 makes the idle path exact, not pick-up and place.
- Mob conversion and non-player dimension change copies, since there is no non-player holder.

**Expected effect on carryon, to be measured, not assumed.**

- The `ServerTickEvent.Post` listener for an idle player needs only §1.8's path, so its ~615 failures should
  become runs. Each player adds one counted `neoforge:sync_attachments (not sent)` when its default is
  created.
- The one `BreakBlockEvent` failure and `/carryon debug` get past `getData`. What they reach next is not
  predicted here; the run will say.
- carryon stays `ran-hollow` because of its 13 entrypoint unknowns.

**Slice 2, for orientation only:**

- a custom `IAttachmentSerializer` and `serializable(..)` over modelled `ValueInput`/`ValueOutput`;
- more codec nodes, chosen by a corpus count of what the bridgeable-type refusals name;
- custom `copyHandler`.

**Slice 3:** non-player holders, after the R2 seam.

## 5. Test plan

### 5.1 Unit tests (runtime and mod)

**Where expected values come from.** Expected values come from NeoForge's real behaviour, not from the new
code:

- **A JVM oracle**, a tiny Java program compiled against the patched server jar and the NeoForge universal
  jar and run with `java -cp`. Only its printed outputs are committed, as fixtures under
  `crates/horizon-native-java/tests/fixtures/attachments/`. It records:
  1. `TagValueOutput.createWithContext(ProblemReporter.DISCARDING, RegistryAccess.EMPTY)` →
     `store(CompoundTag.CODEC.flatXmap(f, g).fieldOf("CarryOnData"), v)` → `buildResult()`, as SNBT, for:
     - an empty compound;
     - one with `keyPressed: 1b, selected: 3, tick: 7`;
     - one with nested compounds and every numeric tag type.
  2. `TagValueInput.create(..).read(sameMapCodec)` on:
     - the same compounds (round trip);
     - a compound missing `CarryOnData`;
     - `CarryOnData` as an `IntTag`.

     It records `Optional` presence and the reporter's messages.
  3. The `CompoundTag` methods slice 1 models, on every tag type stored under the key:
     - `getBooleanOr("k", true)` on a byte 0, 1 and 2, a string and an int;
     - `getIntOr` on a long and a double;
     - `contains` on present and absent keys;
     - `copy()` independence;
     - `getCompoundOrEmpty` on a non-compound.
  4. The builder checks:
     - `builder(..).copyOnDeath()` with no serializer;
     - `serialize` twice;
     - `copyHandler` with no serializer.

     It records the exception classes and messages.
  5. If the oracle can construct an `AttachmentHolder` (a synthetic subclass), it also records `getData`
     identity across two calls, `setData` returning the previous value, `setData(null)`, `removeData` on an
     empty holder, and `hasData` after `getData`. **If it cannot** (static `FMLEnvironment` initialisation
     may need the loader), those expectations are the bytecode reading in §1.3, with the offsets in each
     test's doc, marked "bytecode-derived, not run".
- **The fork's real store.** The mod tests use `mod_api::Attachments` (a lone map answers for any holder),
  so `to_nbt`, `copy_on_death` and `replace_contents` are the fork's own code, not mocks.

**Behaviour tests.** Each uses a synthetic jar fixture built like the `two_stage_jar` helper in
`mod/src/lib.rs` (a carryon-shaped type: a supplier `new Data(new CompoundTag())`, mutators that
`put*`, `CODEC = CompoundTag.CODEC.flatXmap(..).fieldOf("Data")`):

- `getData` on an empty holder runs the supplier once; a second `getData` returns the same object, with the
  supplier not run again. The store's `to_nbt` then holds `horizon:attachments.<ns>.<path>.value` equal to
  oracle (1)'s empty case.
- In-place change, then the end of the batch: `to_nbt`'s value equals oracle (1)'s
  `keyPressed/selected/tick` case. With no change, nothing is stored. Assert that no `Set` happens, through
  the index's publication count.
- Load: a raw saved entry decodes at first `getData` to an object whose getters answer the saved values. A raw
  entry missing the field is discarded, `hasData` is false, and `getData` makes the default. The log line
  matches NeoForge's text.
- Death: a type with no `copyOnDeath` is gone after `copy_on_death` + `replace_contents` + the bridge's
  `player_clone(true)`, and the next `getData` runs the supplier again. A `copyOnDeath` type decodes the
  copied bytes into a new object (not identical to the old one).
- End return: a serializable value is a new object with equal encoding, and a transient value is gone.
- Failures: an encoder that throws leaves the value readable, while `to_nbt` omits it and has no
  `copy_on_death` byte. An encoder that makes an unmodelled call is treated the same and the call is named. A
  supplier that makes an unmodelled call refuses `getData` and inserts nothing. A non-terminating encoder is
  ended by the budget and reported. `setData(null)` throws NPE.
- A type with `RecordCodecBuilder` in its serializer is refused by name on every holder call.
- `getData` on a mob, from `living_damage_post` (no `ServerHost`), and on the client are refused and counted
  as before.
- Sync: a synced type counts one `neoforge:sync_attachments (not sent)` per default creation and per `setData`,
  and the store's `take_sync_changes` is empty.
- Registration: `DeferredHolder.get()` on an attachment holder before the registration point throws NPE; the
  registry key is `neoforge:attachment_types`.

**Gates.** The usual ones, plus `cargo test --manifest-path mod/Cargo.toml --lib` and mod clippy (CI's `mod`
job).

### 5.2 In game: carryon before and after (plan only; not run)

**Where and on what.**

- elysium, under `~/ava/` only. Run `ssh elysium`; the user is `infer`. It is a shared machine, so one run at
  a time, a 10 GiB `systemd-run --user --scope -p MemoryMax=10G` cap, and the scratch trees cleaned up after.
- The game is `arismoko/horizon` at the pin `cf0113879ebee600bff86070466f75b2db90b40a`, **unmodified**,
  staged exactly as the re-survey did: a worktree of that commit plus only the launcher's generated
  `native-api/horizon_java`, `crates/native-game` and their two workspace members. Dev profile, as in both
  surveys.
- Quote the game's own `Horizon 0.1.0` line and the stage's `HEAD` next to the numbers.

**Runs.** `before` is the mod built from `main` at the branch point; `after` is the mod built from the slice-1
branch. Each is a batch of carryon alone (`--manifest` with one row, command `/carryon debug`), 600 play
ticks:

```sh
ssh elysium
cd ~/ava/horizon-java && git fetch && git worktree add ~/ava/wt/att-before <main sha>   # and att-after <branch sha>
# stage and build the game at the pin with each mod (docs/in-game-survey.md "Reproduce", docs/mod-plan.md)
Xvfb :77 & export DISPLAY=:77 HORIZON_NO_AUDIO=1
systemd-run --user --scope -q -p MemoryMax=10G -p MemorySwapMax=0 \
  python3 tools/in_game_survey.py --game <stage>/target/debug/horizon-client --ctl <stage>/target/debug/horizon-ctl \
  --probe <stage>/target/debug/horizon-native-java --resources ~/ava/prepared/26.3 \
  --corpus ~/ava/corpus --manifest ~/ava/att/carryon.json --out ~/ava/results/attachments-<before|after>
```

**Record, before → after.**

- carryon's callbacks ran/failed for `server_tick`, `break_block` and the total.
- The distinct logged failure reasons. Before should be `Player#getData(Supplier)`; after, the next reason, if
  any, quoted.
- carryon's `unknown_host_call_count` and the new `neoforge:sync_attachments (not sent)` count.
- `/carryon debug`'s answer.

**After run only: persistence and death** (scripted with `horizon-ctl` in the same scratch tree):

1. Play 100 ticks, then `quit`. In `<game-dir>/saves/<world>/players/data/<uuid>.dat`, read
   `horizon:attachments.carryon.carry_on_data`. Expected: `{value: {CarryOnData: {}}}`, the oracle's
   empty-compound bytes inside `value`, with no `copy_on_death` byte and no `sync` byte. Read it with any NBT
   reader on elysium, never committing the file.
2. Relaunch the same world, play 100 ticks. Expected: no `Failed to deserialize` line, and the listener still
   0 failed.
3. `command /kill @s`, `respawn`, play 20 ticks. Expected: the entry is gone after respawn, then present again
   as the default after the next tick (a new default creation, so one more sync count).
4. A run with the slice's debug report (per-type live count, re-encodes, stores per batch) gives the R1 cost
   per tick for one player. Record it.

**Not covered by this plan.** Pick-up and place (they need the counted carry path), multiplayer guests (the
survey drives one player), and an End return. A follow-up `lan_e2e`-style run would cover guests.

## 6. Fork issue text (not filed)

R2 is needed before attachments on non-player holders, and to remove departure D4. Slice 1 does not wait
on it. Text for `arismoko/horizon`, in its template:

> 🤖 Filed by Solenopsisbot's AI agent (Claude Code, Claude Opus 5.5), not by Solenopsisbot personally.
>
> **Ask.** Let a server mod refresh a holder's attachment values immediately before the game captures that
> holder's attachments for saving: a server-thread hook (or an `AttachmentForm` encoder the store calls)
> that fires once per capture of a player, entity, block entity, chunk or level, naming the holder, with an
> `AttachmentStore` the mod may `store` new encodings into before `Attachments::to_nbt` runs. Done when a
> mod that changes a value's backing data without calling `set` sees that change in the saved NBT of the
> next save, with its encoder called exactly once per save, for each holder kind, including the autosave
> snapshot path and the closing-world save.
>
> **Background.** The store saves the `encoded` compound captured at `store()`. That is always current for
> a Rust value, which is cloned out and must be `set` to change. It is not current for a value whose
> state lives elsewhere and is changed in place: Horizon Java's NeoForge attachments
> (Solenopsisbot/horizon-java, `docs/attachments-design.md`) hold the live Java object in the bytecode
> runtime, and NeoForge mods change it in place (carryon's `CarryOnData` setters). Today the mod
> re-encodes every live value after each batch of Java calls. That is exact for a pure serializer, but it
> runs the serializer once per tick instead of once per save, surfaces serializer failures at the wrong
> moment, and is too costly for chunk and block-entity attachments. The existing hooks are all after the
> write: `player_save_to_file` is a later notification after an asynchronous write, `level_save` is "was
> saved", and `world_saved` is after the cycle. So none can stand in. Related: #857 (chunk attachment disk
> hooks on a worker), which this should share a capture point with rather than duplicate.
>
> **Hook or handle.** A new server hook `attachments_saving(holder)` (listen; fired at capture, before
> encoding), or a deferred-encode variant of `AttachmentForm.encoded` called by `to_nbt` on the thread
> that captures.
>
> **NeoForge 26.3 counterpart.** None as an event. NeoForge serializes inline at save:
> `AttachmentHolder#serializeAttachments(ValueOutput)` called from `Entity#saveWithoutId` (players
> included), `BlockEntity#saveAdditional`, `SerializableChunkData#copyOf` → `ChunkAccess#writeAttachmentsToNBT`,
> and the `neoforge:data_attachments` SavedData codec for levels. Each runs the mod's
> `IAttachmentSerializer#write` once per save.
>
> **Who uses it.** Horizon Java (NeoForge attachments on every holder kind: balm, curios, puzzleslib,
> CreativeCore and carryon in the corpus). Any native mod whose attachment value is a handle to state kept
> elsewhere.
>
> **Phase.** Host split

## Who uses it

From a constant-pool scan of the 85 NeoForge corpus jars. It finds the classes that reference
`AttachmentType`/`IAttachmentHolder`, not call counts, and misses calls made through `Player`/`Entity`
receivers:

| jar | classes | builder features seen |
|---|---|---|
| carryon 2.12.1 | 2 (+ `Player` call sites) | `serialize(MapCodec)`, `sync` |
| balm 26.3.0.3 | 2 | `serialize`, `copyOnDeath`, `sync`; `get/set/has/removeData`, `getExistingDataOrNull` |
| curios 17.0.0 | 5 | `serialize` ×2, `copyOnDeath` |
| puzzleslib 26.3.17 | 3 | `serialize`, `copyOnDeath`, `copyHandler`, `sync`; every holder call |
| CreativeCore 2.14.24 | 2 | `serialize`; `getData`, `setData` |

Before slice 2 is sized, a per-jar list of the codec nodes each serializer is built from (the
bridgeable-type refusal reasons, once slice 1 reports them) says which nodes to model next.
