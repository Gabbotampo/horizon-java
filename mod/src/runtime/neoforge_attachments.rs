//! NeoForge data attachments on players, bridged onto the game's attachment store
//! (`docs/attachments-design.md`, slice 1).
//!
//! # What runs here
//!
//! * **Types.** `AttachmentType.builder(Supplier|Function)` and the builder's `serialize`,
//!   `copyOnDeath`, `copyHandler`, `sync` and `build`, with the checks and messages of
//!   `AttachmentType$Builder` in `neoforge-26.3.0.101-universal.jar` (`serialize` @9–21 "Serializer
//!   already set", `copyOnDeath` @1–16 "copyOnDeath requires a serializer", `copyHandler` @5–21
//!   "copyHandler requires a serializer", `requireNonNull` on every handler).
//! * **Registration.** `DeferredRegister.create(NeoForgeRegistries.ATTACHMENT_TYPES, ns)` names the
//!   registry `neoforge:attachment_types` (the bytecode's key; the runtime used to say
//!   `attachment_type`). `register(name, Supplier)` keeps its supplier, and
//!   [`NativeJar::register_attachment_types`] runs them at NeoForge's moment, the `RegisterEvent`
//!   for that registry: after every entrypoint stage, in registration order, only for registers
//!   whose `register(IEventBus)` ran. Before that a `DeferredHolder.get()` is NeoForge's
//!   `NullPointerException("Trying to access unbound value: ResourceKey[neoforge:attachment_types /
//!   ns:path]")` (`DeferredHolder.value` @0–31).
//! * **Holder calls on a player**, both overloads of `getData`, `setData`, `hasData`,
//!   `getExistingData`, `getExistingDataOrNull`, `removeData` and `syncData`, as
//!   `AttachmentHolder` and `IAttachmentHolder` do them: `getData` makes and keeps the default and
//!   answers the live object; `setData` rejects null (`requireNonNull` @5) and answers the previous
//!   value; `removeData` answers null without a sync when the holder's map was never made.
//! * **Saving** is the game's: each value is stored in the game's store by its registered id
//!   (`<ns>`/`<path>`), with the bytes the mod's own codec writes, run here
//!   ([`crate::runtime::attachment_codec`]). Only `serialize(MapCodec[, Predicate])` over carryon's three
//!   codec nodes runs; every other serializer is a counted refusal.
//!
//! # The bridge: a snapshot in, a journal out
//!
//! The runtime never holds the game's host. Before a batch of Java calls in a hook that has a
//! `ServerHost`, the mod hands the jar's host a snapshot of each online player's store entries for
//! the jar's types ([`StdHost::open_attachment_batch`]): absent, one of ours (by token), or a saved
//! compound not decoded yet. Holder calls read and update that view, and every store change is
//! journalled; after the batch the mod applies the journal to the game's store
//! ([`StdHost::close_attachment_batch`]). Nothing else runs on the server thread between the two,
//! so the view is the store. A hook without a `ServerHost` opens no batch, and holder calls there
//! stay the counted unknowns they were.
//!
//! # Departures from NeoForge, named
//!
//! * **D1** the saved envelope is the game's `horizon:attachments.<ns>.<path>.value`, not
//!   `neoforge:attachments.<ns:path>`; the per-type compound inside is the same bytes.
//! * **D2** saved data of a removed jar is kept by the game's store, where NeoForge drops it.
//! * **D3** a saved value decodes at first access, plus an eager pass when the player joins
//!   ([`NativeJar::attachment_player_joined`]), not inside `Entity#load`.
//! * **D4** the R1 cadence: every live value is re-encoded after each batch
//!   ([`NativeJar::attachment_batch_end`]), not once per save. Exact for a pure serializer.
//! * **D5** sync is not sent: every point NeoForge would enter `AttachmentSync` for a synced type is
//!   counted as the unknown `neoforge:sync_attachments (not sent)`, and values are stored with
//!   `sync: false`.
//! * **D6** an End return round-trips each value through its codec at `player_clone`, after the
//!   game kept the map, instead of inside `restoreFrom`.
//!
//! Also named (the PR lists them): a codec failure at save stores the empty child `{}` NeoForge's
//! output keeps (`TagValueOutput.child` puts it before `write` runs; the design table said "left
//! out"); a failure during an End-return or death copy, where NeoForge throws out of the respawn,
//! drops that value instead; a value whose batch made an unmodelled host call is *tainted* and its
//! last exact bytes stay stored, so taint follows the objects that were live during the call, not
//! data a later clean batch copies out of them.

use std::collections::{BTreeMap, BTreeSet};

use crate::runtime::attachment_codec::{Codecs, Coded, READ_FAILED};
use crate::runtime::java_nbt::NbtCompound;
use crate::runtime::{
    EMPTY_OPTIONAL, ExecutionBudget, Host, MethodRef, NativeError, NativeJar, Runtime, StdHost,
    Value, expect_ref,
};

/// The registry key NeoForge registers attachment types in (`NeoForgeRegistries.Keys`; the oracle
/// prints `ResourceKey[minecraft:root / neoforge:attachment_types]`).
pub const ATTACHMENT_TYPES_REGISTRY: &str = "neoforge:attachment_types";
/// What `DeferredRegister.create` names a register over that registry.
const REGISTER_PREFIX: &str = "deferred-register/registry/neoforge:attachment_types/";
const BUILDER: &str = "net/neoforged/neoforge/attachment/AttachmentType$Builder";
const TYPE: &str = "net/neoforged/neoforge/attachment/AttachmentType";
const DEFERRED_REGISTER: &str = "net/neoforged/neoforge/registries/DeferredRegister";
const DEFERRED_HOLDER: &str = "net/neoforged/neoforge/registries/DeferredHolder";
/// The unknown every sync NeoForge would send is counted under (design §3.3).
pub const SYNC_NOT_SENT: &str = "neoforge:sync_attachments (not sent)";
/// The holder classes a player's attachment call names in bytecode.
const HOLDER_OWNERS: [&str; 6] = [
    "net/minecraft/world/entity/player/Player",
    "net/minecraft/server/level/ServerPlayer",
    "net/minecraft/world/entity/LivingEntity",
    "net/minecraft/world/entity/Entity",
    "net/neoforged/neoforge/attachment/AttachmentHolder",
    "net/neoforged/neoforge/attachment/IAttachmentHolder",
];
/// A façade player's marker (`events::player_marker`).
const PLAYER_PREFIX: &str = "game/player/";
/// Types and builders one jar may make; past this the builder surface stays unmodelled.
const MAX_TYPES: usize = 4096;
const TYPE_ARG: &str = "Lnet/neoforged/neoforge/attachment/AttachmentType;";
const SUPPLIER_ARG: &str = "Ljava/util/function/Supplier;";

/// One store entry, as the mod read it from the game's store for this batch.
#[derive(Clone, Debug, PartialEq)]
pub enum StoreEntry {
    /// Nothing is stored under the key.
    Absent,
    /// A value this jar stored, by the token it was stored with.
    Ours(u64),
    /// A saved value not decoded yet: the per-type compound inside the envelope's `value`.
    Raw(NbtCompound),
    /// A value of another mod under the same key.
    Foreign,
    /// A saved value the bridge cannot represent exactly (a string that is not valid UTF-16, an
    /// envelope whose `value` is not a compound), and why. Its holder calls are refused; the
    /// store keeps it.
    Unreadable(String),
}

/// One change for the mod to make to the game's store after a batch.
#[derive(Clone, Debug, PartialEq)]
pub enum StoreWrite {
    /// `AttachmentStore::store` of this jar's value `token`, with its saved form and policy. `sync`
    /// is always false (D5).
    Store {
        player: u128,
        id: String,
        token: u64,
        encoded: Option<NbtCompound>,
        copy_on_death: bool,
        write: StoreWriteKind,
    },
    /// `AttachmentStore::discard`, without a sync.
    Discard { player: u128, id: String },
}

/// Why a value is stored: the game's `AttachmentWrite`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreWriteKind {
    Decoded,
    Default,
    Set,
}

/// What the bridge did, for the jar's report row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttachmentStats {
    /// Types registered, and the ones refused because their serializer or copy handler is not
    /// modelled.
    pub types: usize,
    pub refused_types: usize,
    /// Values live now.
    pub live: usize,
    pub defaults_created: usize,
    pub decoded: usize,
    pub decode_failures: usize,
    /// Encodes run (at `getData`/`setData` and by R1), and the stores R1 made because the bytes
    /// changed.
    pub encodes: usize,
    pub re_encode_stores: usize,
    pub encode_failures: usize,
    /// Encodes that made an unmodelled call: their bytes were not stored.
    pub encodes_withheld: usize,
    /// Values marked tainted by a batch that made an unmodelled host call.
    pub tainted: usize,
    pub sync_not_sent: usize,
    /// Holder calls refused by name.
    pub refusals: usize,
    pub batches: usize,
}

/// A built `AttachmentType`, or the builder that will make one.
#[derive(Clone, Debug)]
struct TypeSpec {
    /// The default: a `Supplier` (`holder_arg == false`) or a `Function<IAttachmentHolder, T>`.
    default: Value,
    holder_arg: bool,
    serializer: Serializer,
    copy_on_death: bool,
    copy_handler: Option<Value>,
    sync: Option<Value>,
}

#[derive(Clone, Debug)]
enum Serializer {
    None,
    /// `serialize(MapCodec)` (`predicate: None`, Guava's `alwaysTrue`) or
    /// `serialize(MapCodec, Predicate)`.
    MapCodec {
        codec: String,
        predicate: Option<Value>,
    },
    /// A serializer this slice does not run, and why.
    Unmodelled(String),
}

impl TypeSpec {
    /// Why calls on this type are refused, or `None` when the bridge runs it.
    fn unbridgeable(&self) -> Option<String> {
        if let Serializer::Unmodelled(why) = &self.serializer {
            return Some(format!("serializer not modelled: {why}"));
        }
        self.copy_handler
            .as_ref()
            .map(|_| "custom copyHandler not modelled".to_owned())
    }

    fn serializable(&self) -> bool {
        !matches!(self.serializer, Serializer::None)
    }
}

/// One `DeferredRegister` over the attachment registry.
#[derive(Clone, Debug)]
struct Register {
    marker: String,
    namespace: String,
    bus_registered: bool,
    /// `(path, supplier, holder key)` in registration order.
    entries: Vec<(String, Value, String)>,
}

/// A value the jar holds for one player and type.
#[derive(Clone, Debug)]
struct Live {
    value: Value,
    token: u64,
    /// The saved form last stored for it: `None` when nothing exact was stored yet.
    last: Option<Option<NbtCompound>>,
    tainted: bool,
    /// The last encode failed (logged once until it succeeds again).
    failing: bool,
}

/// One jar's attachment types and live values (kept on its [`StdHost`]).
#[derive(Clone, Debug, Default)]
pub struct AttachmentState {
    codecs: Codecs,
    builders: BTreeMap<String, TypeSpec>,
    types: BTreeMap<String, TypeSpec>,
    next_marker: usize,
    registers: Vec<Register>,
    /// Holder key → the type its supplier made, once registered.
    bound: BTreeMap<String, String>,
    /// Type marker → registered id (`ns:path`).
    ids: BTreeMap<String, String>,
    registration_done: bool,
    live: BTreeMap<(u128, String), Live>,
    /// Players whose NeoForge attachment map exists (`getAttachmentMap` ran).
    allocated: BTreeSet<u128>,
    next_token: u64,
    /// The open batch's store view: per online player, each key's entry, or why the player's
    /// store could not be read.
    view: Option<BTreeMap<u128, Result<BTreeMap<String, StoreEntry>, String>>>,
    journal: Vec<StoreWrite>,
    log: Vec<String>,
    stats: AttachmentStats,
    /// `SYNC_NOT_SENT` counts made in the open batch: accounting, not a neutral answer, so they
    /// do not taint.
    sync_in_batch: usize,
}

impl AttachmentState {
    /// Every value this state holds that names a jar object: roots of the heap collection.
    pub(crate) fn root_texts<'a>(&'a self, out: &mut Vec<&'a str>) {
        fn value<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
            if let Value::Ref(Some(text)) = value {
                out.push(text);
            }
        }
        self.codecs.root_texts(out);
        for spec in self.builders.values().chain(self.types.values()) {
            value(&spec.default, out);
            if let Serializer::MapCodec {
                predicate: Some(predicate),
                ..
            } = &spec.serializer
            {
                value(predicate, out);
            }
            spec.copy_handler.iter().for_each(|v| value(v, out));
            spec.sync.iter().for_each(|v| value(v, out));
        }
        for register in &self.registers {
            for (_, supplier, _) in &register.entries {
                value(supplier, out);
            }
        }
        for live in self.live.values() {
            value(&live.value, out);
        }
    }

    /// The registered ids the bridge runs, with each one's type marker.
    fn bridged(&self) -> impl Iterator<Item = (&String, &String, &TypeSpec)> {
        self.ids.iter().filter_map(|(marker, id)| {
            let spec = self.types.get(marker)?;
            spec.unbridgeable().is_none().then_some((marker, id, spec))
        })
    }

    fn entry(&self, player: u128, id: &str) -> Result<StoreEntry, String> {
        match self.view.as_ref() {
            None => Err("no ServerHost in this hook".into()),
            Some(view) => match view.get(&player) {
                None => Err("the player is not online".into()),
                Some(Err(why)) => Err(why.clone()),
                Some(Ok(entries)) => Ok(entries.get(id).cloned().unwrap_or(StoreEntry::Absent)),
            },
        }
    }

    fn set_entry(&mut self, player: u128, id: &str, entry: StoreEntry) {
        if let Some(Ok(entries)) = self.view.as_mut().and_then(|view| view.get_mut(&player)) {
            entries.insert(id.to_owned(), entry);
        }
    }

    /// Whether the player's NeoForge map exists: something was put in it this session, or the
    /// store holds one of this jar's types for them (a loaded or copied value: `Entity#load` and
    /// the death copy both allocate it).
    fn is_allocated(&self, player: u128) -> bool {
        self.allocated.contains(&player)
            || matches!(
                self.view.as_ref().and_then(|view| view.get(&player)),
                Some(Ok(entries)) if entries.values().any(|e| *e != StoreEntry::Absent)
            )
    }

    fn new_marker(&mut self, class: &str) -> String {
        let marker = format!("nf-attachment/{class}/{}", self.next_marker);
        self.next_marker += 1;
        marker
    }

    fn drop_player(&mut self, player: u128) {
        self.live.retain(|(uuid, _), _| *uuid != player);
        self.allocated.remove(&player);
    }
}

impl StdHost {
    /// The ids (`ns:path`) of this jar's registered types the bridge runs: the keys the mod reads
    /// into each batch's snapshot.
    pub fn attachment_store_ids(&self) -> Vec<String> {
        self.neoforge_attachments
            .bridged()
            .map(|(_, id, _)| id.clone())
            .collect()
    }

    /// Opens a batch: `view` is each online player's store entries for
    /// [`Self::attachment_store_ids`], or why that player's store could not be read.
    pub fn open_attachment_batch(
        &mut self,
        view: BTreeMap<u128, Result<BTreeMap<String, StoreEntry>, String>>,
    ) {
        let state = &mut self.neoforge_attachments;
        state.view = Some(view);
        state.journal.clear();
        state.sync_in_batch = 0;
        state.stats.batches += 1;
    }

    /// Closes the batch and hands back the store changes to apply, in order.
    pub fn close_attachment_batch(&mut self) -> Vec<StoreWrite> {
        let state = &mut self.neoforge_attachments;
        state.view = None;
        state.stats.live = state.live.len();
        std::mem::take(&mut state.journal)
    }

    /// NeoForge's own log lines this jar's attachments produced since the last call.
    pub fn take_attachment_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.neoforge_attachments.log)
    }

    pub fn attachment_stats(&self) -> AttachmentStats {
        let state = &self.neoforge_attachments;
        let mut stats = state.stats.clone();
        stats.live = state.live.len();
        stats.types = state.ids.len();
        stats.refused_types = state
            .ids
            .keys()
            .filter(|marker| {
                state
                    .types
                    .get(*marker)
                    .is_some_and(|spec| spec.unbridgeable().is_some())
            })
            .count();
        stats
    }

    /// A player left: NeoForge's player object, and every value on it, is gone. The game saved
    /// the last bytes R1 stored before this.
    pub fn attachment_player_left(&mut self, player: u128) {
        self.neoforge_attachments.drop_player(player);
    }

    /// A new server: no player values carry over (types and registration stay, as they do in one
    /// NeoForge process).
    pub fn reset_attachment_values(&mut self) {
        let state = &mut self.neoforge_attachments;
        state.live.clear();
        state.allocated.clear();
        state.view = None;
        state.journal.clear();
    }

    /// The initial syncs NeoForge sends a player about itself (`PlayerList#placeNewPlayer` @740,
    /// `PlayerList#respawn` @524, `ServerPlayer#teleport` @387): one per synced type with a value,
    /// counted as not sent (D5). Needs the open batch's view.
    pub fn count_attachment_initial_sync(&mut self, player: u128) {
        let state = &self.neoforge_attachments;
        let synced = state
            .bridged()
            .filter(|(_, id, spec)| {
                spec.sync.is_some()
                    && matches!(
                        state.entry(player, id),
                        Ok(StoreEntry::Ours(_) | StoreEntry::Raw(_))
                    )
            })
            .count();
        for _ in 0..synced {
            self.count_sync_not_sent();
        }
    }

    fn count_sync_not_sent(&mut self) {
        *self
            .unknown_host_calls
            .entry(SYNC_NOT_SENT.to_owned())
            .or_default() += 1;
        self.neoforge_attachments.stats.sync_not_sent += 1;
        self.neoforge_attachments.sync_in_batch += 1;
    }
}

/// Which holder call a method reference is, and whether it takes a `Supplier`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HolderOp {
    Get,
    Set,
    Has,
    Existing,
    ExistingOrNull,
    Remove,
    Sync,
}

fn holder_op(r: &MethodRef) -> Option<(HolderOp, bool)> {
    if !HOLDER_OWNERS.contains(&r.owner.as_str()) {
        return None;
    }
    let by_supplier = r.descriptor.starts_with(&format!("({SUPPLIER_ARG}"));
    let first = if by_supplier { SUPPLIER_ARG } else { TYPE_ARG };
    let op = match (
        r.name.as_str(),
        r.descriptor.strip_prefix(&format!("({first}"))?,
    ) {
        ("getData", ")Ljava/lang/Object;") => HolderOp::Get,
        ("setData", "Ljava/lang/Object;)Ljava/lang/Object;") => HolderOp::Set,
        ("hasData", ")Z") => HolderOp::Has,
        ("getExistingData", ")Ljava/util/Optional;") => HolderOp::Existing,
        ("getExistingDataOrNull", ")Ljava/lang/Object;") => HolderOp::ExistingOrNull,
        ("removeData", ")Ljava/lang/Object;") => HolderOp::Remove,
        ("syncData", ")V") => HolderOp::Sync,
        _ => return None,
    };
    Some((op, by_supplier))
}

fn player_uuid(receiver: Option<&Value>) -> Option<u128> {
    let hex = receiver.and_then(expect_ref)?.strip_prefix(PLAYER_PREFIX)?;
    (hex.len() == 32).then(|| u128::from_str_radix(hex, 16).ok())?
}

fn npe(message: Option<String>) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message,
        frame: None,
    }
}

fn illegal_state(message: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalStateException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

/// What a holder's entry is, after the lazy decode.
enum Current {
    Value(Value),
    Absent,
    Refused(String),
}

impl<H: Host> Runtime<'_, H> {
    fn attachments(&mut self) -> Option<&mut AttachmentState> {
        self.host.neoforge_attachments().map(|(state, _)| state)
    }

    /// Every unmodelled host call the jar's host has counted so far.
    pub(crate) fn attachment_unknown_total(&mut self) -> usize {
        self.host
            .neoforge_attachments()
            .map_or(0, |(_, unknown)| unknown.values().sum())
    }

    /// Counts a holder call refused by name and answers what an unmodelled call answers.
    fn attachment_refusal(&mut self, r: &MethodRef, subject: &str, why: &str) -> Value {
        if let Some((state, unknown)) = self.host.neoforge_attachments() {
            state.stats.refusals += 1;
            *unknown
                .entry(format!(
                    "{}#{}{} (attachment {subject}: {why})",
                    r.owner, r.name, r.descriptor
                ))
                .or_default() += 1;
        }
        Value::default_for(&r.descriptor)
    }

    fn count_sync(&mut self, spec: &TypeSpec) {
        if spec.sync.is_none() {
            return;
        }
        if let Some((state, unknown)) = self.host.neoforge_attachments() {
            *unknown.entry(SYNC_NOT_SENT.to_owned()).or_default() += 1;
            state.stats.sync_not_sent += 1;
            state.sync_in_batch += 1;
        }
    }

    pub(crate) fn codec_node(
        &mut self,
        marker: &str,
    ) -> Option<crate::runtime::attachment_codec::CodecNode> {
        self.attachments()?.codecs.node(marker).cloned()
    }

    /// The attachment surface: the codec nodes, the builder, the attachment register and the
    /// holder calls on a player. `None` leaves every other call to the rest of the chain.
    pub(crate) fn invoke_neoforge_attachment(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(state) = self.attachments() else {
            return Ok(None);
        };
        let mut codecs = std::mem::take(&mut state.codecs);
        let coded = self.invoke_attachment_codec(&mut codecs, r, receiver, args);
        if let Some(state) = self.attachments() {
            state.codecs = codecs;
        }
        if coded.is_some() {
            return Ok(coded);
        }
        if let Some(value) = self.attachment_builder_call(r, receiver, args)? {
            return Ok(Some(value));
        }
        if let Some(value) = self.attachment_register_call(r, receiver, args)? {
            return Ok(Some(value));
        }
        self.attachment_holder_call(r, receiver, args)
    }

    fn attachment_builder_call(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(state) = self.attachments() else {
            return Ok(None);
        };
        if r.owner == TYPE {
            let spec = match (r.name.as_str(), r.descriptor.as_str()) {
                (
                    "builder",
                    "(Ljava/util/function/Supplier;)Lnet/neoforged/neoforge/attachment/AttachmentType$Builder;",
                ) => (false, None),
                (
                    "builder",
                    "(Ljava/util/function/Function;)Lnet/neoforged/neoforge/attachment/AttachmentType$Builder;",
                ) => (true, None),
                ("serializable", descriptor)
                    if descriptor.ends_with(
                        ")Lnet/neoforged/neoforge/attachment/AttachmentType$Builder;",
                    ) =>
                {
                    (
                        descriptor.starts_with("(Ljava/util/function/Function;"),
                        Some("serializable(..) / ValueIOSerializable".to_owned()),
                    )
                }
                _ => return Ok(None),
            };
            if state.builders.len() + state.types.len() >= MAX_TYPES {
                return Ok(None);
            }
            let marker = state.new_marker(BUILDER);
            state.builders.insert(
                marker.clone(),
                TypeSpec {
                    default: args.first().cloned().unwrap_or(Value::Ref(None)),
                    holder_arg: spec.0,
                    serializer: spec.1.map_or(Serializer::None, Serializer::Unmodelled),
                    copy_on_death: false,
                    copy_handler: None,
                    sync: None,
                },
            );
            return Ok(Some(Value::Ref(Some(marker))));
        }
        if r.owner != BUILDER {
            return Ok(None);
        }
        let Some(builder) = receiver.and_then(expect_ref).cloned() else {
            return Ok(None);
        };
        let Some(spec) = state.builders.get(&builder).cloned() else {
            return Ok(None);
        };
        let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Ref(None));
        let is_null = |v: &Value| matches!(v, Value::Ref(None));
        let mut spec = spec;
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("build", "()Lnet/neoforged/neoforge/attachment/AttachmentType;") => {
                let marker = state.new_marker(TYPE);
                state.types.insert(marker.clone(), spec);
                return Ok(Some(Value::Ref(Some(marker))));
            }
            (
                "serialize",
                "(Lnet/neoforged/neoforge/attachment/IAttachmentSerializer;)Lnet/neoforged/neoforge/attachment/AttachmentType$Builder;",
            ) => {
                if is_null(&arg(0)) {
                    return Err(npe(None));
                }
                if spec.serializable() {
                    return Err(illegal_state("Serializer already set"));
                }
                let class = expect_ref(&arg(0))
                    .map(|m| m.split('#').next().unwrap_or(m).to_owned())
                    .unwrap_or_default();
                spec.serializer =
                    Serializer::Unmodelled(format!("custom IAttachmentSerializer {class}"));
            }
            ("serialize", descriptor)
                if descriptor.starts_with("(Lcom/mojang/serialization/MapCodec;")
                    && descriptor.ends_with(
                        ")Lnet/neoforged/neoforge/attachment/AttachmentType$Builder;",
                    ) =>
            {
                let predicate = match descriptor {
                    "(Lcom/mojang/serialization/MapCodec;)Lnet/neoforged/neoforge/attachment/AttachmentType$Builder;" => {
                        None
                    }
                    "(Lcom/mojang/serialization/MapCodec;Ljava/util/function/Predicate;)Lnet/neoforged/neoforge/attachment/AttachmentType$Builder;" => {
                        Some(arg(1))
                    }
                    _ => return Ok(None),
                };
                // `serialize(codec, predicate)` @0: `requireNonNull(codec)` before the
                // already-set check in `serialize(IAttachmentSerializer)`.
                if is_null(&arg(0)) {
                    return Err(npe(None));
                }
                if spec.serializable() {
                    return Err(illegal_state("Serializer already set"));
                }
                let codec = expect_ref(&arg(0)).cloned().unwrap_or_default();
                spec.serializer = if state.codecs.is_map_codec(&codec) {
                    Serializer::MapCodec { codec, predicate }
                } else {
                    Serializer::Unmodelled(format!(
                        "a MapCodec not built from CompoundTag.CODEC, flatXmap and fieldOf ({codec})"
                    ))
                };
            }
            ("copyOnDeath", "()Lnet/neoforged/neoforge/attachment/AttachmentType$Builder;") => {
                if !spec.serializable() {
                    return Err(illegal_state("copyOnDeath requires a serializer"));
                }
                spec.copy_on_death = true;
            }
            (
                "copyHandler",
                "(Lnet/neoforged/neoforge/attachment/IAttachmentCopyHandler;)Lnet/neoforged/neoforge/attachment/AttachmentType$Builder;",
            ) => {
                if is_null(&arg(0)) {
                    return Err(npe(None));
                }
                if !spec.serializable() {
                    return Err(illegal_state("copyHandler requires a serializer"));
                }
                spec.copy_handler = Some(arg(0));
            }
            (
                "sync",
                "(Lnet/neoforged/neoforge/attachment/AttachmentSyncHandler;)Lnet/neoforged/neoforge/attachment/AttachmentType$Builder;",
            )
            | (
                "sync",
                "(Lnet/minecraft/network/codec/StreamCodec;)Lnet/neoforged/neoforge/attachment/AttachmentType$Builder;",
            ) => {
                if is_null(&arg(0)) {
                    return Err(npe(None));
                }
                spec.sync = Some(arg(0));
            }
            (
                "sync",
                "(Ljava/util/function/BiPredicate;Lnet/minecraft/network/codec/StreamCodec;)Lnet/neoforged/neoforge/attachment/AttachmentType$Builder;",
            ) => {
                if is_null(&arg(0)) || is_null(&arg(1)) {
                    return Err(npe(None));
                }
                spec.sync = Some(arg(1));
            }
            _ => return Ok(None),
        }
        state.builders.insert(builder.clone(), spec);
        Ok(Some(Value::Ref(Some(builder))))
    }

    fn attachment_register_call(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(marker) = receiver.and_then(expect_ref).cloned() else {
            return Ok(None);
        };
        // `DeferredHolder.get()`/`value()`, and `get()` through the `Supplier` a mod usually types
        // its holder field as.
        if ((r.owner == DEFERRED_HOLDER && matches!(r.name.as_str(), "get" | "value"))
            || (r.owner == "java/util/function/Supplier" && r.name == "get"))
            && r.descriptor == "()Ljava/lang/Object;"
        {
            let Some(key) = marker.strip_prefix("holder/") else {
                return Ok(None);
            };
            return self.attachment_holder_value(key);
        }
        if r.owner != DEFERRED_REGISTER {
            return Ok(None);
        }
        let Some(namespace) = marker.strip_prefix(REGISTER_PREFIX) else {
            return Ok(None);
        };
        let namespace = namespace.to_owned();
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("register", "(Lnet/neoforged/bus/api/IEventBus;)V") => {
                let Some(state) = self.attachments() else {
                    return Ok(None);
                };
                register_entry(state, &marker, &namespace).bus_registered = true;
                Ok(Some(Value::Void))
            }
            (
                "register",
                "(Ljava/lang/String;Ljava/util/function/Supplier;)Lnet/neoforged/neoforge/registries/DeferredHolder;",
            ) => {
                // The host keeps the registration row and the holder as it always has; this keeps
                // the supplier the row drops, to run at the registration point.
                let holder =
                    self.host
                        .invoke_instance(r, &Value::Ref(Some(marker.clone())), args)?;
                let (Some(path), Some(key)) = (
                    args.first().and_then(expect_ref).cloned(),
                    expect_ref(&holder)
                        .and_then(|h| h.strip_prefix("holder/"))
                        .map(str::to_owned),
                ) else {
                    return Ok(Some(holder));
                };
                let supplier = args.get(1).cloned().unwrap_or(Value::Ref(None));
                if let Some(state) = self.attachments() {
                    register_entry(state, &marker, &namespace)
                        .entries
                        .push((path, supplier, key));
                }
                Ok(Some(holder))
            }
            _ => Ok(None),
        }
    }

    /// `DeferredHolder.get()` on an attachment holder: the registered type, or before the
    /// registration point NeoForge's unbound-value NPE. `Ok(None)` for a holder of another
    /// registry.
    fn attachment_holder_value(&mut self, key: &str) -> Result<Option<Value>, NativeError> {
        let Some(state) = self.attachments() else {
            return Ok(None);
        };
        let Some((namespace, path)) = state.registers.iter().find_map(|register| {
            register
                .entries
                .iter()
                .find(|(_, _, holder)| holder == key)
                .map(|(path, _, _)| (register.namespace.clone(), path.clone()))
        }) else {
            return Ok(None);
        };
        match state.bound.get(key) {
            Some(marker) => Ok(Some(Value::Ref(Some(marker.clone())))),
            None => Err(npe(Some(format!(
                "Trying to access unbound value: ResourceKey[{ATTACHMENT_TYPES_REGISTRY} / {namespace}:{path}]"
            )))),
        }
    }

    /// The type a holder call names: `Ok(None)` when it is not one this runtime built (the call
    /// stays the host's).
    fn attachment_type_arg(
        &mut self,
        arg: &Value,
        by_supplier: bool,
    ) -> Result<Option<String>, NativeError> {
        let mut value = arg.clone();
        if by_supplier {
            // `IAttachmentHolder`'s default overloads: `supplier.get()`, then the typed call.
            let Some(marker) = expect_ref(arg).cloned() else {
                return Err(crate::runtime::null_checks::invoke_on_null(&MethodRef {
                    owner: "java/util/function/Supplier".into(),
                    name: "get".into(),
                    descriptor: "()Ljava/lang/Object;".into(),
                }));
            };
            value = if let Some(key) = marker.strip_prefix("holder/") {
                match self.attachment_holder_value(key)? {
                    Some(value) => value,
                    None => return Ok(None),
                }
            } else {
                match self.invoke_callback(arg, &[])? {
                    Some(value) => value,
                    None => return Ok(None),
                }
            };
        }
        // `validateAttachmentType` @0: `requireNonNull(type)`. In production (`FMLEnvironment`
        // answers `isProduction() == true` here) it checks nothing else (@5–11).
        let Some(marker) = expect_ref(&value).cloned() else {
            return Err(npe(None));
        };
        let Some(state) = self.attachments() else {
            return Ok(None);
        };
        Ok(state.types.contains_key(&marker).then_some(marker))
    }

    fn attachment_holder_call(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some((op, by_supplier)) = holder_op(r) else {
            return Ok(None);
        };
        let Some(player) = player_uuid(receiver) else {
            return Ok(None);
        };
        let Some(type_marker) =
            self.attachment_type_arg(args.first().unwrap_or(&Value::Ref(None)), by_supplier)?
        else {
            return Ok(None);
        };
        let Some(state) = self.attachments() else {
            return Ok(None);
        };
        let spec = state
            .types
            .get(&type_marker)
            .cloned()
            .expect("type checked above");
        let id = state.ids.get(&type_marker).cloned();
        let subject = id.clone().unwrap_or_else(|| type_marker.clone());
        if let Some(why) = spec.unbridgeable() {
            return Ok(Some(self.attachment_refusal(r, &subject, &why)));
        }
        let Some(id) = id else {
            // In production NeoForge keeps an unregistered type in memory but cannot save it (a
            // null registry id fails `serializeAttachments`); the game's store keys by id.
            return Ok(Some(self.attachment_refusal(
                r,
                &subject,
                "type not registered",
            )));
        };
        if let Err(why) = state.entry(player, &id) {
            return Ok(Some(self.attachment_refusal(r, &subject, &why)));
        }
        let holder = receiver.cloned().unwrap_or(Value::Ref(None));
        let value = match op {
            HolderOp::Sync => {
                // `Entity#syncData` → `AttachmentSync.syncEntityUpdate`: nothing without a sync
                // handler.
                self.count_sync(&spec);
                Value::Void
            }
            HolderOp::Get => {
                match self.attachment_current(player, &id, &spec)? {
                    // A null value in the map makes `getData` run the default again (@16–17).
                    Current::Value(value) if value != Value::Ref(None) => value,
                    Current::Refused(why) => {
                        return Ok(Some(self.attachment_refusal(r, &subject, &why)));
                    }
                    _ => match self.attachment_default(player, &id, &spec, &holder)? {
                        Ok(value) => value,
                        Err(why) => {
                            self.attachment_refusal(r, &subject, &why);
                            return Err(NativeError::Metadata(format!(
                                "attachment {id}: {why}; nothing was inserted"
                            )));
                        }
                    },
                }
            }
            HolderOp::Set => {
                let data = args.get(1).cloned().unwrap_or(Value::Ref(None));
                if data == Value::Ref(None) {
                    return Err(npe(None));
                }
                let previous = match self.attachment_current(player, &id, &spec)? {
                    Current::Value(value) => value,
                    Current::Absent => Value::Ref(None),
                    Current::Refused(why) => {
                        return Ok(Some(self.attachment_refusal(r, &subject, &why)));
                    }
                };
                self.attachment_put(player, &id, &spec, data, StoreWriteKind::Set);
                self.count_sync(&spec);
                previous
            }
            HolderOp::Has => match self.attachment_current(player, &id, &spec)? {
                Current::Value(_) => Value::Int(1),
                Current::Absent => Value::Int(0),
                Current::Refused(why) => {
                    return Ok(Some(self.attachment_refusal(r, &subject, &why)));
                }
            },
            HolderOp::ExistingOrNull | HolderOp::Existing => {
                let value = match self.attachment_current(player, &id, &spec)? {
                    Current::Value(value) => value,
                    Current::Absent => Value::Ref(None),
                    Current::Refused(why) => {
                        return Ok(Some(self.attachment_refusal(r, &subject, &why)));
                    }
                };
                match (op, value) {
                    (HolderOp::Existing, Value::Ref(Some(marker))) => {
                        Value::Ref(Some(format!("optional/{marker}")))
                    }
                    (HolderOp::Existing, _) => Value::Ref(Some(EMPTY_OPTIONAL.into())),
                    (_, value) => value,
                }
            }
            HolderOp::Remove => {
                let allocated = self.attachments().is_some_and(|s| s.is_allocated(player));
                // @9–13: no map, no removal and no sync.
                if !allocated {
                    Value::Ref(None)
                } else {
                    let previous = match self.attachment_current(player, &id, &spec)? {
                        Current::Value(value) => {
                            if let Some(state) = self.attachments() {
                                state.live.remove(&(player, id.clone()));
                                state.set_entry(player, &id, StoreEntry::Absent);
                                state.journal.push(StoreWrite::Discard {
                                    player,
                                    id: id.clone(),
                                });
                            }
                            value
                        }
                        Current::Absent => Value::Ref(None),
                        Current::Refused(why) => {
                            return Ok(Some(self.attachment_refusal(r, &subject, &why)));
                        }
                    };
                    // @25: `syncData` even when nothing was there.
                    self.count_sync(&spec);
                    previous
                }
            }
        };
        Ok(Some(value))
    }

    /// The holder's value now, decoding a saved one on first access (D3).
    fn attachment_current(
        &mut self,
        player: u128,
        id: &str,
        spec: &TypeSpec,
    ) -> Result<Current, NativeError> {
        let Some(state) = self.attachments() else {
            return Ok(Current::Refused("no attachment state".into()));
        };
        let key = (player, id.to_owned());
        match state.entry(player, id) {
            Err(why) => Ok(Current::Refused(why)),
            Ok(StoreEntry::Absent) => {
                state.live.remove(&key);
                Ok(Current::Absent)
            }
            Ok(StoreEntry::Foreign) => Ok(Current::Refused(
                "the game's store holds another mod's value under this key".into(),
            )),
            Ok(StoreEntry::Unreadable(why)) => {
                Ok(Current::Refused(format!("saved value not readable: {why}")))
            }
            Ok(StoreEntry::Ours(token)) => Ok(match state.live.get(&key) {
                Some(live) if live.token == token => Current::Value(live.value.clone()),
                _ => Current::Refused("the store holds a value this runtime no longer has".into()),
            }),
            Ok(StoreEntry::Raw(saved)) => {
                let Serializer::MapCodec { codec, .. } = &spec.serializer else {
                    // A saved entry for a type with no serializer: NeoForge's load skips it as
                    // "unknown or non-serializable" and it is dropped at the next save.
                    state.set_entry(player, id, StoreEntry::Absent);
                    state.journal.push(StoreWrite::Discard {
                        player,
                        id: id.to_owned(),
                    });
                    state.log.push(format!(
                        "Encountered unknown or non-serializable data attachment {id}. Skipping."
                    ));
                    return Ok(Current::Absent);
                };
                let codec = codec.clone();
                match self.decode_map_codec(&codec, &saved) {
                    Coded::Ok(value) => {
                        self.attachment_store(
                            player,
                            id,
                            spec,
                            value.clone(),
                            Some(Some(saved)),
                            StoreWriteKind::Decoded,
                        );
                        if let Some(state) = self.attachments() {
                            state.stats.decoded += 1;
                        }
                        Ok(Current::Value(value))
                    }
                    Coded::Failed(why) => {
                        if let Some(state) = self.attachments() {
                            state.stats.decode_failures += 1;
                            state.set_entry(player, id, StoreEntry::Absent);
                            state.live.remove(&key);
                            state.journal.push(StoreWrite::Discard {
                                player,
                                id: id.to_owned(),
                            });
                            state.log.push(format!(
                                "Failed to deserialize data attachment {id}. Skipping. ({READ_FAILED} {why})"
                            ));
                        }
                        Ok(Current::Absent)
                    }
                    Coded::Unmodelled(why) => {
                        Ok(Current::Refused(format!("saved value not decoded: {why}")))
                    }
                }
            }
        }
    }

    /// Records `value` as the holder's live value with a new token and journals its store.
    /// `known` is the saved form when the caller already has it (a decode); otherwise the value is
    /// encoded now.
    fn attachment_store(
        &mut self,
        player: u128,
        id: &str,
        spec: &TypeSpec,
        value: Value,
        known: Option<Option<NbtCompound>>,
        write: StoreWriteKind,
    ) -> u64 {
        let (last, failing) = match known {
            Some(saved) => (Some(saved), false),
            None => match self.attachment_encode(spec, &value) {
                Coded::Ok(saved) => (Some(saved), false),
                Coded::Failed(why) => {
                    self.attachment_encode_failed(id, &why);
                    (Some(Some(NbtCompound::new())), true)
                }
                Coded::Unmodelled(_) => {
                    if let Some(state) = self.attachments() {
                        state.stats.encodes_withheld += 1;
                    }
                    (None, false)
                }
            },
        };
        let Some(state) = self.attachments() else {
            return 0;
        };
        state.next_token += 1;
        let token = state.next_token;
        let encoded = last.clone().flatten();
        state.journal.push(StoreWrite::Store {
            player,
            id: id.to_owned(),
            token,
            // A failed encode keeps NeoForge's empty child, but is never copied on death.
            copy_on_death: spec.copy_on_death && encoded.is_some() && !failing,
            encoded,
            write,
        });
        state.set_entry(player, id, StoreEntry::Ours(token));
        state.allocated.insert(player);
        state.live.insert(
            (player, id.to_owned()),
            Live {
                value,
                token,
                last,
                tainted: false,
                failing,
            },
        );
        token
    }

    fn attachment_put(
        &mut self,
        player: u128,
        id: &str,
        spec: &TypeSpec,
        value: Value,
        write: StoreWriteKind,
    ) {
        self.attachment_store(player, id, spec, value, None, write);
    }

    /// `getData`'s default: the type's supplier (or function of the holder) runs, its value is put
    /// and stored. `Ok(Err(why))` when the supplier made an unmodelled call: nothing is put.
    fn attachment_default(
        &mut self,
        player: u128,
        id: &str,
        spec: &TypeSpec,
        holder: &Value,
    ) -> Result<Result<Value, String>, NativeError> {
        let before = self.attachment_unknown_total();
        let args = if spec.holder_arg {
            vec![holder.clone()]
        } else {
            Vec::new()
        };
        // A Java exception from the supplier reaches the caller, as it does on NeoForge.
        let Some(value) = self.invoke_callback(&spec.default, &args)? else {
            return Ok(Err("the default supplier cannot run in this runtime".into()));
        };
        if self.attachment_unknown_total() > before {
            return Ok(Err(
                "the default supplier made an unmodelled host call".into()
            ));
        }
        self.attachment_put(player, id, spec, value.clone(), StoreWriteKind::Default);
        if let Some(state) = self.attachments() {
            state.stats.defaults_created += 1;
        }
        self.count_sync(spec);
        Ok(Ok(value))
    }

    /// What NeoForge's `serializer.write(value, child)` leaves in the type's child: `Ok(None)` for
    /// a type with no serializer or a `write` that answered false (the child is discarded), the
    /// fields otherwise.
    fn attachment_encode(&mut self, spec: &TypeSpec, value: &Value) -> Coded<Option<NbtCompound>> {
        let Serializer::MapCodec { codec, predicate } = &spec.serializer else {
            return Coded::Ok(None);
        };
        if let Some(state) = self.attachments() {
            state.stats.encodes += 1;
        }
        if let Some(predicate) = predicate {
            // `Builder$1.write` @0–14: `shouldSerialize.test(value)`, false omits the entry.
            let before = self.attachment_unknown_total();
            let tested = self.invoke_callback(predicate, std::slice::from_ref(value));
            if self.attachment_unknown_total() > before {
                return Coded::Unmodelled("the predicate made an unmodelled host call".into());
            }
            match tested {
                Ok(Some(Value::Int(0))) => return Coded::Ok(None),
                Ok(Some(Value::Int(_))) => {}
                // A null predicate is an NPE inside `write`, which `serializeAttachments` catches.
                Err(NativeError::UncaughtException { class, .. }) => return Coded::Failed(class),
                Ok(None) if *predicate == Value::Ref(None) => {
                    return Coded::Failed("java.lang.NullPointerException".into());
                }
                Ok(_) => return Coded::Unmodelled("a predicate the runtime cannot run".into()),
                Err(other) => return Coded::Unmodelled(other.to_string()),
            }
        }
        let codec = codec.clone();
        match self.encode_map_codec(&codec, value) {
            Coded::Ok(fields) => Coded::Ok(Some(fields)),
            Coded::Failed(why) => Coded::Failed(why),
            Coded::Unmodelled(why) => Coded::Unmodelled(why),
        }
    }

    fn attachment_encode_failed(&mut self, id: &str, why: &str) {
        if let Some(state) = self.attachments() {
            state.stats.encode_failures += 1;
            state.log.push(format!(
                "Failed to serialize data attachment {id}. Skipping. ({why})"
            ));
        }
    }
}

fn register_entry<'a>(
    state: &'a mut AttachmentState,
    marker: &str,
    namespace: &str,
) -> &'a mut Register {
    if let Some(index) = state.registers.iter().position(|r| r.marker == marker) {
        return &mut state.registers[index];
    }
    state.registers.push(Register {
        marker: marker.to_owned(),
        namespace: namespace.to_owned(),
        bus_registered: false,
        entries: Vec::new(),
    });
    state.registers.last_mut().expect("just pushed")
}

impl NativeJar {
    /// Runs `body` on a runtime over this jar's resident heap, as a listener's would, and keeps
    /// what it changed.
    pub(super) fn with_resident_runtime<R>(
        &self,
        host: &mut StdHost,
        budget: &ExecutionBudget,
        body: impl FnOnce(&mut Runtime<'_, StdHost>) -> R,
    ) -> Result<R, NativeError> {
        let (static_fields, initialized_classes) = host.persistent_class_state();
        let mut callback_infos = crate::runtime::CallbackInfos::new();
        let mut runtime = Runtime {
            classes: &self.classes,
            linked: host.linked_class_table().cloned().unwrap_or_default(),
            host,
            calls: 0,
            fields: BTreeMap::new(),
            static_fields,
            initialized_classes,
            arrays: BTreeMap::new(),
            next_array: 0,
            next_object: 0,
            streams: BTreeMap::new(),
            next_stream: 0,
            enum_constants: BTreeMap::new(),
            string_builders: BTreeMap::new(),
            string_readers: BTreeMap::new(),
            io_writers: BTreeMap::new(),
            io_paths: BTreeMap::new(),
            io_files: BTreeMap::new(),
            io_dirs: BTreeSet::new(),
            watch: Default::default(),
            annotations: BTreeMap::new(),
            next_annotation: 0,
            callback_infos: &mut callback_infos,
            invoke_hooks: None,
            anchor_hooks: None,
            modify_hooks: None,
            pending_skipped_new: Vec::new(),
            depth: 0,
            budget,
        };
        let resident = runtime.adopt_resident_heap();
        let result = body(&mut runtime);
        let persistence = runtime.persist_class_state();
        if resident {
            runtime.persist_resident_heap()?;
        }
        persistence?;
        Ok(result)
    }

    /// NeoForge's `RegisterEvent` for `neoforge:attachment_types`: every supplier of every
    /// attachment register whose `register(IEventBus)` ran, in registration order. Answers one
    /// line per supplier that did not make a type (it threw, or answered something else); its
    /// holder stays unbound, so a later `DeferredHolder.get()` is NeoForge's unbound-value NPE.
    pub fn register_attachment_types(
        &self,
        host: &mut StdHost,
        budget: &ExecutionBudget,
    ) -> Vec<String> {
        if host.neoforge_attachments.registration_done {
            return Vec::new();
        }
        host.neoforge_attachments.registration_done = true;
        let entries: Vec<(String, String, Value, String)> = host
            .neoforge_attachments
            .registers
            .iter()
            .filter(|register| register.bus_registered)
            .flat_map(|register| {
                register.entries.iter().map(|(path, supplier, key)| {
                    (
                        register.namespace.clone(),
                        path.clone(),
                        supplier.clone(),
                        key.clone(),
                    )
                })
            })
            .collect();
        let mut problems = Vec::new();
        for (namespace, path, supplier, key) in entries {
            let id = format!("{namespace}:{path}");
            // The supplier's unmodelled calls are counted like any other. They cannot make a
            // type look bridgeable: the type is whatever the modelled builder recorded, and a
            // serializer built on a neutral answer is refused by name on every holder call.
            let ran = self.with_resident_runtime(host, budget, |runtime| {
                runtime.invoke_callback(&supplier, &[])
            });
            let made = match ran {
                Ok(Ok(Some(Value::Ref(Some(marker)))))
                    if host.neoforge_attachments.types.contains_key(&marker) =>
                {
                    marker
                }
                Ok(Ok(other)) => {
                    problems.push(format!(
                        "{id}: the supplier answered {other:?}, not an AttachmentType"
                    ));
                    continue;
                }
                Ok(Err(error)) | Err(error) => {
                    problems.push(format!("{id}: {error}"));
                    continue;
                }
            };
            let state = &mut host.neoforge_attachments;
            state.bound.insert(key.clone(), made.clone());
            state.ids.insert(made.clone(), id);
            // `DeferredHolder.get()` through the host's own path answers the type too.
            host.holders.insert(key, Value::Ref(Some(made)));
        }
        problems
    }

    /// R1 (D4), after a batch that ran Java: every live value whose store entry is still this
    /// jar's is encoded again, and stored when its bytes changed. `batch_unknown` is the unknown
    /// host calls the batch counted; when any of them is a neutral answer (not a
    /// [`SYNC_NOT_SENT`] count), every live value is tainted and keeps its last exact bytes.
    pub fn attachment_batch_end(
        &self,
        host: &mut StdHost,
        budget: &ExecutionBudget,
        batch_unknown: usize,
    ) {
        let state = &mut host.neoforge_attachments;
        if state.view.is_none() || state.live.is_empty() {
            return;
        }
        if batch_unknown > state.sync_in_batch {
            for live in state.live.values_mut() {
                if !live.tainted {
                    live.tainted = true;
                    state.stats.tainted += 1;
                }
            }
        }
        let due: Vec<((u128, String), Value, String)> = state
            .live
            .iter()
            .filter(|((player, id), live)| {
                !live.tainted && state.entry(*player, id) == Ok(StoreEntry::Ours(live.token))
            })
            .filter_map(|(key, live)| {
                let marker = state.ids.iter().find(|(_, id)| **id == key.1)?.0.clone();
                Some((key.clone(), live.value.clone(), marker))
            })
            .collect();
        let _ = self.with_resident_runtime(host, budget, |runtime| {
            for ((player, id), value, marker) in due {
                let Some(spec) = runtime
                    .attachments()
                    .and_then(|s| s.types.get(&marker).cloned())
                else {
                    continue;
                };
                let coded = runtime.attachment_encode(&spec, &value);
                let (saved, failed) = match coded {
                    Coded::Ok(saved) => (saved, None),
                    Coded::Failed(why) => (Some(NbtCompound::new()), Some(why)),
                    Coded::Unmodelled(_) => {
                        if let Some(state) = runtime.attachments() {
                            state.stats.encodes_withheld += 1;
                        }
                        continue;
                    }
                };
                let Some(state) = runtime.attachments() else {
                    continue;
                };
                let Some(live) = state.live.get_mut(&(player, id.clone())) else {
                    continue;
                };
                let was_failing = live.failing;
                live.failing = failed.is_some();
                if live.last.as_ref() == Some(&saved) {
                    continue;
                }
                live.last = Some(saved.clone());
                let token = live.token;
                state.stats.re_encode_stores += 1;
                state.journal.push(StoreWrite::Store {
                    player,
                    id: id.clone(),
                    token,
                    copy_on_death: spec.copy_on_death && saved.is_some() && failed.is_none(),
                    encoded: saved,
                    write: StoreWriteKind::Set,
                });
                if let Some(why) = failed
                    && !was_failing
                {
                    runtime.attachment_encode_failed(&id, &why);
                }
            }
        });
    }

    /// D3's eager pass: when a player joins, every saved value of this jar's types decodes now,
    /// as `Entity#load` would have done, and the player's initial self-sync is counted (D5). Call
    /// it inside the join batch, before the join listeners.
    pub fn attachment_player_joined(
        &self,
        host: &mut StdHost,
        budget: &ExecutionBudget,
        player: u128,
    ) {
        let raw: Vec<(String, String)> = host
            .neoforge_attachments
            .bridged()
            .filter(|(_, id, _)| {
                matches!(
                    host.neoforge_attachments.entry(player, id),
                    Ok(StoreEntry::Raw(_))
                )
            })
            .map(|(marker, id, _)| (marker.clone(), id.clone()))
            .collect();
        if !raw.is_empty() {
            let _ = self.with_resident_runtime(host, budget, |runtime| {
                for (marker, id) in raw {
                    let Some(spec) = runtime
                        .attachments()
                        .and_then(|s| s.types.get(&marker).cloned())
                    else {
                        continue;
                    };
                    let _ = runtime.attachment_current(player, &id, &spec);
                }
            });
        }
        host.count_attachment_initial_sync(player);
    }

    /// `PlayerEvent.Clone` (`AttachmentInternals.onPlayerClone`), inside the clone batch and
    /// before the clone listeners.
    ///
    /// * A death (`was_death`): the game already kept only the `copy_on_death` envelopes
    ///   (`replace_contents`), so every object of the old player is dropped and a copied type
    ///   decodes from its bytes at the next access, a new object as NeoForge's copy handler makes.
    /// * An End return: NeoForge copies every serializable type through its default copy handler,
    ///   a write then a read into a new object, and drops every transient one. The game kept the
    ///   map as it was, so that happens here (D6). A copy that cannot be made exactly (a tainted
    ///   value, an unmodelled encode or decode) keeps the old object, counted; a codec failure,
    ///   where NeoForge throws out of the respawn, drops the value and is logged.
    pub fn attachment_player_cloned(
        &self,
        host: &mut StdHost,
        budget: &ExecutionBudget,
        player: u128,
        was_death: bool,
    ) {
        if was_death {
            host.neoforge_attachments.drop_player(player);
            return;
        }
        let entries: Vec<(String, String, Live)> = host
            .neoforge_attachments
            .live
            .iter()
            .filter(|((uuid, id), live)| {
                *uuid == player
                    && host.neoforge_attachments.entry(player, id)
                        == Ok(StoreEntry::Ours(live.token))
            })
            .filter_map(|((_, id), live)| {
                let state = &host.neoforge_attachments;
                let marker = state.ids.iter().find(|(_, known)| *known == id)?.0.clone();
                Some((marker, id.clone(), live.clone()))
            })
            .collect();
        let _ = self.with_resident_runtime(host, budget, |runtime| {
            for (marker, id, live) in entries {
                let Some(spec) = runtime
                    .attachments()
                    .and_then(|s| s.types.get(&marker).cloned())
                else {
                    continue;
                };
                let discard = |runtime: &mut Runtime<'_, StdHost>, why: Option<String>| {
                    if let Some(state) = runtime.attachments() {
                        state.live.remove(&(player, id.clone()));
                        state.set_entry(player, &id, StoreEntry::Absent);
                        state.journal.push(StoreWrite::Discard {
                            player,
                            id: id.clone(),
                        });
                        if let Some(why) = why {
                            state.log.push(format!(
                                "Attachment {id} not copied to the returning player: {why}"
                            ));
                        }
                    }
                };
                let Serializer::MapCodec { codec, .. } = spec.serializer.clone() else {
                    // `copyAttachments` @20: types without a serializer are never copied.
                    discard(runtime, None);
                    continue;
                };
                if live.tainted {
                    runtime.attachment_refusal_note(
                        &id,
                        "End-return copy of a tainted value: the old object is kept",
                    );
                    continue;
                }
                match runtime.attachment_encode(&spec, &live.value) {
                    // `defaultCopyHandler` @26–30: a `write` that answers false copies nothing.
                    Coded::Ok(None) => discard(runtime, None),
                    Coded::Ok(Some(saved)) => match runtime.decode_map_codec(&codec, &saved) {
                        Coded::Ok(copy) => {
                            runtime.attachment_store(
                                player,
                                &id,
                                &spec,
                                copy,
                                Some(Some(saved)),
                                StoreWriteKind::Set,
                            );
                        }
                        Coded::Failed(why) => discard(
                            runtime,
                            Some(format!(
                                "Attachment failed to deserialise during copy: {why}"
                            )),
                        ),
                        Coded::Unmodelled(why) => runtime.attachment_refusal_note(
                            &id,
                            &format!(
                                "End-return copy not modelled ({why}): the old object is kept"
                            ),
                        ),
                    },
                    Coded::Failed(why) => discard(
                        runtime,
                        Some(format!("Attachment failed to serialise during copy: {why}")),
                    ),
                    Coded::Unmodelled(why) => runtime.attachment_refusal_note(
                        &id,
                        &format!("End-return copy not modelled ({why}): the old object is kept"),
                    ),
                }
            }
        });
    }
}

impl<H: Host> Runtime<'_, H> {
    /// A lifecycle step the bridge could not make exactly, counted under the attachment's id.
    fn attachment_refusal_note(&mut self, id: &str, why: &str) {
        if let Some((state, unknown)) = self.host.neoforge_attachments() {
            state.stats.refusals += 1;
            *unknown
                .entry(format!("neoforge attachment {id}: {why}"))
                .or_default() += 1;
        }
    }
}

#[cfg(test)]
#[path = "neoforge_attachments_tests.rs"]
pub(crate) mod tests;
