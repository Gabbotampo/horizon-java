//! NeoForge data attachments on players, bridged onto the game's attachment store
//! (`docs/attachments-design.md`, slice 1; the runtime side is `runtime::neoforge_attachments`).
//!
//! The runtime keeps each jar's Java attachment types and the live Java objects, and runs the
//! mod's own codec. The game's store is authoritative for whether a player has a value and for
//! the bytes it saves. Around each batch of Java calls in a hook that has a `ServerHost`:
//!
//! 1. [`open`] reads every online player's entry for each of the jar's registered types into a
//!    snapshot the runtime answers holder calls from: absent, one of this jar's values (the
//!    [`JavaAttachment`] marker this bridge stored, by token), or a saved compound not decoded yet;
//! 2. the listeners (or the command) run;
//! 3. [`close`] lets the runtime re-encode every live value (R1, departure D4), then applies the
//!    runtime's journal of `store`/`discard` calls to the game's store.
//!
//! The marker stored as the value is an `Arc<JavaAttachment>`: the Java object itself never leaves
//! the runtime. Values are stored with `sync: false` (D5) and the per-type compound the codec
//! wrote as `encoded`, which the game saves under `horizon:attachments.<ns>.<path>.value` (D1).

use std::collections::BTreeMap;
use std::sync::Arc;

use mod_api::{
    AttachmentForm, AttachmentPolicy, AttachmentStore, AttachmentWrite, Compound, Holder,
    ServerHost, Tag,
};

use crate::runtime::neoforge_attachments::{StoreEntry, StoreWrite, StoreWriteKind};
use crate::runtime::{ExecutionBudget, NativeJar, Nbt, NbtCompound, StdHost};
use crate::{JAR_BUDGET, LOG};

/// The value this bridge stores in the game's store for one Java attachment value: which jar's
/// runtime holds the object, and the token it was stored with. A later batch whose snapshot finds
/// the same token knows the runtime's object is current; anything else (a respawn copy, a
/// restart's saved compound) is read from the store instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JavaAttachment {
    pub(crate) jar: String,
    pub(crate) token: u64,
}

/// A lifecycle step the runtime must take inside a batch, before the listeners run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lifecycle {
    None,
    /// A player joined: saved values decode now (D3) and the initial self-sync is counted.
    Joined(u128),
    /// A player's data was copied to their new player; `true` for a death.
    Cloned(u128, bool),
    /// A player respawned or changed dimension: NeoForge re-syncs the player to itself.
    Resynced(u128),
    /// A player left: the runtime's objects for them are dropped after the batch.
    Left(u128),
}

/// The game's `namespace`/`name` of a registered id.
fn split_id(id: &str) -> (&str, &str) {
    id.split_once(':').unwrap_or(("minecraft", id))
}

/// The game's compound as the runtime's tree. A string that is not valid UTF-16 (a Java string
/// with an unpaired surrogate) or an `End` tag cannot be represented exactly, so it is refused
/// rather than replaced.
pub(crate) fn to_runtime(compound: &Compound) -> Result<NbtCompound, String> {
    compound
        .iter()
        .map(|(key, tag)| {
            let key = String::from_utf16(&key.0).map_err(|_| "a key is not valid UTF-16")?;
            Ok((key, tag_to_runtime(tag, 0)?))
        })
        .collect()
}

fn tag_to_runtime(tag: &Tag, depth: usize) -> Result<Nbt, String> {
    if depth > crate::runtime::NBT_MAX_DEPTH {
        return Err("NBT nested too deep".into());
    }
    Ok(match tag {
        Tag::End => return Err("an End tag".into()),
        Tag::Byte(v) => Nbt::Byte(*v),
        Tag::Short(v) => Nbt::Short(*v),
        Tag::Int(v) => Nbt::Int(*v),
        Tag::Long(v) => Nbt::Long(*v),
        Tag::Float(bits) => Nbt::Float(*bits),
        Tag::Double(bits) => Nbt::Double(*bits),
        Tag::ByteArray(v) => Nbt::ByteArray(v.clone()),
        Tag::String(v) => {
            Nbt::String(String::from_utf16(&v.0).map_err(|_| "a string is not valid UTF-16")?)
        }
        Tag::List(elements) => Nbt::List(
            elements
                .iter()
                .map(|element| tag_to_runtime(element, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        Tag::Compound(entries) => Nbt::Compound(
            entries
                .iter()
                .map(|(key, tag)| {
                    let key =
                        String::from_utf16(&key.0).map_err(|_| "a key is not valid UTF-16")?;
                    Ok((key, tag_to_runtime(tag, depth + 1)?))
                })
                .collect::<Result<_, String>>()?,
        ),
        Tag::IntArray(v) => Nbt::IntArray(v.clone()),
        Tag::LongArray(v) => Nbt::LongArray(v.clone()),
    })
}

/// The runtime's tree as the game's compound, tag for tag.
pub(crate) fn to_game(compound: &NbtCompound) -> Compound {
    compound
        .iter()
        .map(|(key, tag)| (key.as_str().into(), tag_to_game(tag)))
        .collect()
}

fn tag_to_game(tag: &Nbt) -> Tag {
    match tag {
        Nbt::Byte(v) => Tag::Byte(*v),
        Nbt::Short(v) => Tag::Short(*v),
        Nbt::Int(v) => Tag::Int(*v),
        Nbt::Long(v) => Tag::Long(*v),
        Nbt::Float(bits) => Tag::Float(*bits),
        Nbt::Double(bits) => Tag::Double(*bits),
        Nbt::ByteArray(v) => Tag::ByteArray(v.clone()),
        Nbt::String(v) => Tag::String(v.as_str().into()),
        Nbt::List(elements) => Tag::List(elements.iter().map(tag_to_game).collect()),
        Nbt::Compound(entries) => Tag::Compound(to_game(entries)),
        Nbt::IntArray(v) => Tag::IntArray(v.clone()),
        Nbt::LongArray(v) => Tag::LongArray(v.clone()),
    }
}

/// Each player's entries for `ids`, read from the game's store. A player whose store cannot be
/// read (`MissingHolder`, `HolderBusy`) is an `Err` naming the store's answer, so the runtime
/// refuses their holder calls by name.
pub(crate) fn snapshot(
    store: &mut dyn AttachmentStore,
    jar: &str,
    ids: &[String],
    players: &[u128],
) -> BTreeMap<u128, Result<BTreeMap<String, StoreEntry>, String>> {
    players
        .iter()
        .map(|&uuid| {
            let holder = Holder::Player(uuid);
            let entries = ids
                .iter()
                .map(|id| {
                    let (namespace, name) = split_id(id);
                    let entry = match store.decoded(holder, namespace, name) {
                        Err(error) => return Err(format!("the game's store answered {error:?}")),
                        Ok(Some(value)) => match value.downcast_ref::<JavaAttachment>() {
                            Some(ours) if ours.jar == jar => StoreEntry::Ours(ours.token),
                            _ => StoreEntry::Foreign,
                        },
                        Ok(None) => match store.saved(holder, namespace, name) {
                            Ok(Some(saved)) => match to_runtime(&saved) {
                                Ok(saved) => StoreEntry::Raw(saved),
                                Err(why) => StoreEntry::Unreadable(why),
                            },
                            Ok(None) => StoreEntry::Absent,
                            Err(error) => StoreEntry::Unreadable(format!(
                                "the saved entry could not be read: {error:?}"
                            )),
                        },
                    };
                    Ok((id.clone(), entry))
                })
                .collect();
            (uuid, entries)
        })
        .collect()
}

/// Applies the runtime's journal to the game's store, in order, and answers a line for each
/// write the store refused.
pub(crate) fn apply(
    store: &mut dyn AttachmentStore,
    jar: &str,
    writes: Vec<StoreWrite>,
) -> Vec<String> {
    let mut refused = Vec::new();
    for write in writes {
        let (player, id, result) = match write {
            StoreWrite::Store {
                player,
                id,
                token,
                encoded,
                copy_on_death,
                write,
            } => {
                let (namespace, name) = split_id(&id);
                let form = AttachmentForm {
                    encoded: encoded.as_ref().map(to_game),
                    // Never `copy_on_death` without bytes: the game's death copy would fail as a
                    // whole (`InvalidCopyPolicy`) and keep every entry.
                    policy: AttachmentPolicy {
                        copy_on_death: copy_on_death && encoded.is_some(),
                        sync: false,
                    },
                    write: match write {
                        StoreWriteKind::Decoded => AttachmentWrite::Decoded,
                        StoreWriteKind::Default => AttachmentWrite::Default,
                        StoreWriteKind::Set => AttachmentWrite::Set,
                    },
                };
                let value = Arc::new(JavaAttachment {
                    jar: jar.to_owned(),
                    token,
                });
                let result = store.store(Holder::Player(player), namespace, name, value, form);
                (player, id, result)
            }
            StoreWrite::Discard { player, id } => {
                let (namespace, name) = split_id(&id);
                let result = store.discard(Holder::Player(player), namespace, name, false);
                (player, id, result)
            }
        };
        if let Err(error) = result {
            refused.push(format!("{id} on player {player:032x}: {error:?}"));
        }
    }
    refused
}

/// Opens one jar's batch if it has types the bridge runs, and takes the lifecycle step. Answers
/// the jar's unknown-call count before the batch, for [`close`]; `None` when no batch opened.
pub(crate) fn open(
    server: &mut dyn ServerHost,
    id: &str,
    jar: &NativeJar,
    host: &mut StdHost,
    lifecycle: Lifecycle,
) -> Option<usize> {
    let ids = host.attachment_store_ids();
    if ids.is_empty() {
        return None;
    }
    let players: Vec<u128> = server
        .players()
        .server_players()
        .into_iter()
        .map(|player| player.uuid())
        .chain(match lifecycle {
            // The leaving player is already off the list; their store may still answer.
            Lifecycle::Left(uuid) => Some(uuid),
            _ => None,
        })
        .collect();
    let view = snapshot(server.attachments(), id, &ids, &players);
    let before = host.unknown_host_call_count();
    host.open_attachment_batch(view);
    let budget = ExecutionBudget::within(JAR_BUDGET);
    match lifecycle {
        Lifecycle::Joined(player) => jar.attachment_player_joined(host, &budget, player),
        Lifecycle::Cloned(player, was_death) => {
            jar.attachment_player_cloned(host, &budget, player, was_death)
        }
        Lifecycle::Resynced(player) => host.count_attachment_initial_sync(player),
        Lifecycle::None | Lifecycle::Left(_) => {}
    }
    Some(before)
}

/// Closes a batch [`open`] opened: R1, then the journal onto the game's store, then the runtime's
/// NeoForge log lines. A player who left is forgotten either way.
pub(crate) fn close(
    server: &mut dyn ServerHost,
    id: &str,
    jar: &NativeJar,
    host: &mut StdHost,
    before: Option<usize>,
    lifecycle: Lifecycle,
) {
    if let Some(before) = before {
        let batch_unknown = host.unknown_host_call_count().saturating_sub(before);
        jar.attachment_batch_end(host, &ExecutionBudget::within(JAR_BUDGET), batch_unknown);
        let writes = host.close_attachment_batch();
        for refused in apply(server.attachments(), id, writes) {
            log::warn!("{LOG}: {id}: the game's attachment store refused {refused}");
        }
    }
    for line in host.take_attachment_log() {
        log::warn!("{LOG}: {id}: {line}");
    }
    if let Lifecycle::Left(player) = lifecycle {
        host.attachment_player_left(player);
    }
}

/// The bridge's counters for the report row.
pub(crate) fn stats_json(
    stats: &crate::runtime::neoforge_attachments::AttachmentStats,
) -> serde_json::Value {
    serde_json::json!({
        "types": stats.types,
        "refused_types": stats.refused_types,
        "live": stats.live,
        "defaults_created": stats.defaults_created,
        "decoded": stats.decoded,
        "decode_failures": stats.decode_failures,
        "encodes": stats.encodes,
        "re_encode_stores": stats.re_encode_stores,
        "encode_failures": stats.encode_failures,
        "encodes_withheld": stats.encodes_withheld,
        "tainted": stats.tainted,
        "sync_not_sent": stats.sync_not_sent,
        "refusals": stats.refusals,
        "batches": stats.batches,
    })
}

#[cfg(test)]
mod tests {
    //! The bridge against the game's own store (`mod_api::Attachments`, which answers for any
    //! holder): its `to_nbt` envelope, its death copy and its load are the game's code, not a
    //! fake. The Java side is the runtime's fixture mod, carryon-shaped, compiled with `javac`
    //! (`crates/horizon-native-java/tests/fixtures/attachments`), with expected bytes from the JVM
    //! oracle run against NeoForge 26.3.
    use super::*;
    use crate::runtime::neoforge_attachments::SYNC_NOT_SENT;
    use crate::runtime::neoforge_attachments::tests::{compound, fixture_jar_bytes, oracle};
    use crate::runtime::{MethodRef, NativeError, Value};
    use mod_api::{AttachmentError, Attachments};
    use std::time::Duration;

    const P: u128 = 0x00ff_00ff_00ff_00ff_00ff_00ff_00ff_00ff;
    const PLAYER: &str = "Lnet/minecraft/world/entity/player/Player;";
    const DATA: i32 = 0;
    const KEPT: i32 = 1;
    const OPAQUE: i32 = 3;

    struct Run {
        jar: NativeJar,
        host: StdHost,
        store: Attachments,
    }

    impl Run {
        fn new() -> Self {
            let jar = NativeJar::from_bytes(fixture_jar_bytes()).unwrap();
            let mut host = jar.seed_host(StdHost::new().with_environment_type("SERVER"));
            let budget = ExecutionBudget::within(Duration::from_secs(20));
            jar.invoke_persistent(
                &MethodRef {
                    owner: "demo/Demo".into(),
                    name: "init".into(),
                    descriptor: "(Lnet/neoforged/bus/api/IEventBus;)V".into(),
                },
                None,
                &[Value::Ref(Some("net/neoforged/bus/api/IEventBus".into()))],
                &mut host,
                &budget,
            )
            .unwrap();
            assert!(jar.register_attachment_types(&mut host, &budget).is_empty());
            Self {
                jar,
                host,
                store: Attachments::default(),
            }
        }

        fn call(&mut self, name: &str, which: i32) -> Result<Value, NativeError> {
            let descriptor = match name {
                "has" => format!("({PLAYER}I)Z"),
                _ => format!("({PLAYER}I)Ljava/lang/Object;"),
            };
            self.jar.invoke_persistent(
                &MethodRef {
                    owner: "demo/Demo".into(),
                    name: name.into(),
                    descriptor,
                },
                None,
                &[
                    Value::Ref(Some(format!("game/player/{P:032x}"))),
                    Value::Int(which),
                ],
                &mut self.host,
                &ExecutionBudget::within(Duration::from_secs(20)),
            )
        }

        fn select(&mut self, data: &Value, selected: i32) {
            self.jar
                .invoke_persistent(
                    &MethodRef {
                        owner: "demo/Demo".into(),
                        name: "select".into(),
                        descriptor: "(Ljava/lang/Object;I)V".into(),
                    },
                    None,
                    &[data.clone(), Value::Int(selected)],
                    &mut self.host,
                    &ExecutionBudget::within(Duration::from_secs(20)),
                )
                .unwrap();
        }

        /// One batch through the bridge's real snapshot and journal, over the game's store.
        fn batch<R>(&mut self, lifecycle: Lifecycle, body: impl FnOnce(&mut Self) -> R) -> R {
            let ids = self.host.attachment_store_ids();
            let view = snapshot(&mut self.store, "demo", &ids, &[P]);
            let before = self.host.unknown_host_call_count();
            self.host.open_attachment_batch(view);
            let budget = ExecutionBudget::within(Duration::from_secs(20));
            match lifecycle {
                Lifecycle::Joined(p) => {
                    self.jar
                        .attachment_player_joined(&mut self.host, &budget, p)
                }
                Lifecycle::Cloned(p, death) => {
                    self.jar
                        .attachment_player_cloned(&mut self.host, &budget, p, death)
                }
                _ => {}
            }
            let result = body(self);
            let unknown = self.host.unknown_host_call_count() - before;
            self.jar
                .attachment_batch_end(&mut self.host, &budget, unknown);
            let writes = self.host.close_attachment_batch();
            assert_eq!(apply(&mut self.store, "demo", writes), Vec::<String>::new());
            result
        }

        /// What the game saves under `horizon:attachments` for `demo:<name>`.
        fn saved(&self, name: &str) -> Option<Compound> {
            match self.store.to_nbt().get(&"demo".into()) {
                Some(Tag::Compound(names)) => match names.get(&name.into()) {
                    Some(Tag::Compound(envelope)) => Some(envelope.clone()),
                    _ => None,
                },
                _ => None,
            }
        }
    }

    fn envelope(value: Compound) -> Compound {
        Compound::from([("value".into(), Tag::Compound(value))])
    }

    /// The oracle's child for an encode case, as the game's compound.
    fn oracle_child(case: &str) -> Compound {
        let saved = compound(&oracle(case)["saved"]);
        match saved.get("carryon:carry_on_data") {
            Some(Nbt::Compound(child)) => to_game(child),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_carryon_shaped_value_is_saved_with_neoforges_bytes_inside_the_games_envelope() {
        let mut run = Run::new();
        run.batch(Lifecycle::None, |run| run.call("get", DATA).unwrap());
        // D1: `horizon:attachments.demo.data`, then `value`: the oracle's child, and no
        // `copy_on_death` or `sync` byte (no copyOnDeath; sync is never sent, D5).
        assert_eq!(
            run.saved("data"),
            Some(envelope(oracle_child("encode/empty")))
        );
        assert_eq!(
            run.host
                .unknown_host_calls()
                .into_iter()
                .find(|(key, _)| key == SYNC_NOT_SENT)
                .map(|(_, count)| count),
            Some(1)
        );
        assert!(run.store.take_sync_changes().is_empty());
        // An in-place change reaches the saved bytes at the end of its batch (R1).
        let value = run.batch(Lifecycle::None, |run| run.call("get", DATA).unwrap());
        run.batch(Lifecycle::None, |run| run.select(&value, 3));
        let expected = Compound::from([(
            "CarryOnData".into(),
            Tag::Compound(Compound::from([("selected".into(), Tag::Int(3))])),
        )]);
        assert_eq!(run.saved("data"), Some(envelope(expected)));
    }

    #[test]
    fn a_restart_reads_the_saved_bytes_back_at_join() {
        let mut run = Run::new();
        let value = run.batch(Lifecycle::None, |run| run.call("get", DATA).unwrap());
        run.batch(Lifecycle::None, |run| run.select(&value, 4));
        // The game's save and load: the map round-trips through NBT, the runtime starts empty.
        run.store = Attachments::from_nbt(run.store.to_nbt());
        run.host.reset_attachment_values();
        let reloaded = run.batch(Lifecycle::Joined(P), |run| run.call("get", DATA).unwrap());
        assert_ne!(reloaded, value);
        assert_eq!(run.host.attachment_stats().decoded, 1);
        assert_eq!(run.host.attachment_stats().defaults_created, 1);
    }

    #[test]
    fn the_games_death_copy_keeps_copy_on_death_types_and_drops_carryons() {
        let mut run = Run::new();
        run.batch(Lifecycle::None, |run| {
            let carried = run.call("get", DATA).unwrap();
            run.select(&carried, 2);
            let kept = run.call("get", KEPT).unwrap();
            run.select(&kept, 5);
        });
        assert_eq!(
            run.saved("kept")
                .and_then(|e| e.get(&"copy_on_death".into()).cloned()),
            Some(Tag::Byte(1))
        );
        // `guest_respawn`: `copy_on_death` then `replace_contents`, before `player_clone`.
        let copy = run.store.copy_on_death().unwrap();
        run.store.replace_contents(copy);
        let (kept, has_data) = run.batch(Lifecycle::Cloned(P, true), |run| {
            (
                run.call("get", KEPT).unwrap(),
                run.call("has", DATA).unwrap(),
            )
        });
        assert_eq!(has_data, Value::Int(0));
        assert_ne!(kept, Value::Ref(None));
        assert_eq!(run.host.attachment_stats().decoded, 1);
    }

    #[test]
    fn an_unmodelled_serializer_is_refused_and_the_store_never_holds_it() {
        let mut run = Run::new();
        let answer = run.batch(Lifecycle::None, |run| run.call("get", OPAQUE).unwrap());
        assert_eq!(answer, Value::Ref(None));
        assert_eq!(run.saved("opaque"), None);
        assert!(run.store.to_nbt().is_empty());
    }

    #[test]
    fn a_player_the_store_cannot_reach_is_refused_by_name() {
        struct Gone;
        impl mod_api::AttachmentHost for Gone {
            fn attachments(
                &mut self,
                _: Holder,
                _: mod_api::AttachmentAccess,
            ) -> Result<&mut Attachments, AttachmentError> {
                Err(AttachmentError::MissingHolder)
            }
        }
        let view = snapshot(&mut Gone, "demo", &["demo:data".into()], &[P]);
        assert_eq!(
            view.get(&P),
            Some(&Err("the game's store answered MissingHolder".to_owned()))
        );
    }

    #[test]
    fn every_tag_type_crosses_the_bridge_unchanged() {
        let tree = compound(&oracle("encode/every-type")["input"]);
        assert_eq!(to_runtime(&to_game(&tree)), Ok(tree));
        // A Java string holding a lone surrogate, which no Rust `String` can hold, is refused
        // rather than replaced.
        let Tag::String(mut lone) = Tag::String("x".into()) else {
            unreachable!()
        };
        lone.0 = vec![0xd800];
        let bad = Compound::from([("k".into(), Tag::String(lone))]);
        assert!(to_runtime(&bad).is_err());
    }
}
