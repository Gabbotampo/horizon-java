//! The old player of a clone (`PlayerEvent.Clone#getOriginal`, Fabric `COPY_FROM`'s `oldPlayer`)
//! as an attachment holder, while the clone batch is open (arismoko/horizon#3040).
//!
//! NeoForge's `ServerPlayer.restoreFrom(old, keepEverything)` posts `Clone(this, old, wasDeath)`
//! with the old `ServerPlayer` still whole: its attachment map holds every value it had, copied
//! types and transient ones alike, as the same objects the jar last saw. The game hands over a
//! read view of that player's store (`Holder::OriginalPlayer`), and this runtime keeps the Java
//! objects it held for the player just before the clone step dropped or replaced them. A holder
//! call on the original's marker reads those; a saved value never decoded decodes now into the
//! original's own object, and `getData` on an absent type runs the default, as on the old player.
//!
//! Writes (`setData`, `removeData`, a default) change only the original's map here: the old player
//! is discarded after the event, so no store write follows, and the replacement player's values
//! never change.

use super::*;

/// The marker prefix of a clone's old player (`game/original-player/<uuid>`).
pub const ORIGINAL_PREFIX: &str = "game/original-player/";

/// The old player of the clone being fired.
#[derive(Clone, Debug)]
pub(super) struct Original {
    pub(super) player: u128,
    /// The old player's store entries for this jar's types, or why they could not be read.
    entries: Result<BTreeMap<String, StoreEntry>, String>,
    /// The old player's Java values, by id: the objects it held at the clone, then whatever a
    /// listener decoded, defaulted or set on it.
    pub(super) live: BTreeMap<String, Value>,
}

impl Original {
    /// Whether the old player's NeoForge map exists (`removeData` does nothing without one).
    fn allocated(&self) -> bool {
        !self.live.is_empty()
            || matches!(&self.entries, Ok(entries) if entries.values().any(|e| *e != StoreEntry::Absent))
    }
}

/// The UUID of an original-player marker.
pub(super) fn original_uuid(receiver: Option<&Value>) -> Option<u128> {
    let hex = receiver
        .and_then(expect_ref)?
        .strip_prefix(ORIGINAL_PREFIX)?;
    (hex.len() == 32).then(|| u128::from_str_radix(hex, 16).ok())?
}

impl StdHost {
    /// Opens the clone's old player inside the open batch, before the clone step: `entries` is the
    /// game's view of the old player's store. A value this jar stored keeps the object the runtime
    /// held for it under that token; the clone step that follows drops or replaces those objects
    /// on the new player only.
    pub fn open_original_attachments(
        &mut self,
        player: u128,
        entries: Result<BTreeMap<String, StoreEntry>, String>,
    ) {
        let state = &mut self.neoforge_attachments;
        let live = match &entries {
            Ok(entries) => entries
                .iter()
                .filter_map(|(id, entry)| {
                    let StoreEntry::Ours(token) = entry else {
                        return None;
                    };
                    let live = state.live.get(&(player, id.clone()))?;
                    (live.token == *token).then(|| (id.clone(), live.value.clone()))
                })
                .collect(),
            Err(_) => BTreeMap::new(),
        };
        state.original = Some(Original {
            player,
            entries,
            live,
        });
    }
}

impl<H: Host> Runtime<'_, H> {
    /// The open clone's old player, when it is `player`.
    fn original(&mut self, player: u128) -> Option<&mut Original> {
        self.attachments()?
            .original
            .as_mut()
            .filter(|original| original.player == player)
    }

    /// The old player's value for `id` now, decoding a saved one into its own object.
    fn original_current(&mut self, player: u128, id: &str, spec: &TypeSpec) -> Current {
        let Some(original) = self.original(player) else {
            return Current::Refused("the original player expired with its clone event".into());
        };
        if let Some(value) = original.live.get(id) {
            return Current::Value(value.clone());
        }
        let entry = match &original.entries {
            Err(why) => return Current::Refused(why.clone()),
            Ok(entries) => entries.get(id).cloned().unwrap_or(StoreEntry::Absent),
        };
        match entry {
            StoreEntry::Absent => Current::Absent,
            StoreEntry::Ours(_) => {
                Current::Refused("the store holds a value this runtime no longer has".into())
            }
            StoreEntry::Foreign => {
                Current::Refused("the game's store holds another mod's value under this key".into())
            }
            StoreEntry::Unreadable(why) => {
                Current::Refused(format!("saved value not readable: {why}"))
            }
            // NeoForge's load skips a saved type with no serializer: the old player never had it.
            StoreEntry::Raw(_) if !matches!(spec.serializer, Serializer::MapCodec { .. }) => {
                Current::Absent
            }
            StoreEntry::Raw(saved) => {
                let Serializer::MapCodec { codec, .. } = spec.serializer.clone() else {
                    unreachable!("matched above");
                };
                match self.decode_map_codec(&codec, &saved) {
                    Coded::Ok(value) => {
                        if let Some(original) = self.original(player) {
                            original.live.insert(id.to_owned(), value.clone());
                        }
                        Current::Value(value)
                    }
                    // The old player's load already skipped a value it could not decode.
                    Coded::Failed(_) => Current::Absent,
                    Coded::Unmodelled(why) => {
                        Current::Refused(format!("saved value not decoded: {why}"))
                    }
                }
            }
        }
    }

    /// A holder call on the original's marker, after the type was resolved and bridged.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn original_holder_call(
        &mut self,
        r: &MethodRef,
        op: HolderOp,
        player: u128,
        id: &str,
        subject: &str,
        spec: &TypeSpec,
        holder: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let current = self.original_current(player, id, spec);
        if let Current::Refused(why) = &current {
            return Ok(self.attachment_refusal(r, subject, why));
        }
        let value = |current: Current| match current {
            Current::Value(value) => value,
            _ => Value::Ref(None),
        };
        Ok(match op {
            HolderOp::Sync => {
                self.count_sync(spec);
                Value::Void
            }
            HolderOp::Get => match current {
                Current::Value(value) if value != Value::Ref(None) => value,
                _ => {
                    let before = self.attachment_unknown_total();
                    let default_args = if spec.holder_arg {
                        vec![holder.clone()]
                    } else {
                        Vec::new()
                    };
                    let made = self.invoke_callback(&spec.default, &default_args)?;
                    let Some(made) = made.filter(|_| self.attachment_unknown_total() == before)
                    else {
                        self.attachment_refusal(r, subject, "the default supplier cannot run");
                        return Err(NativeError::Metadata(format!(
                            "attachment {id}: the default supplier cannot run; nothing was inserted"
                        )));
                    };
                    if let Some(original) = self.original(player) {
                        original.live.insert(id.to_owned(), made.clone());
                    }
                    self.count_sync(spec);
                    made
                }
            },
            HolderOp::Set => {
                let data = args.get(1).cloned().unwrap_or(Value::Ref(None));
                if data == Value::Ref(None) {
                    return Err(npe(None));
                }
                if let Some(original) = self.original(player) {
                    original.live.insert(id.to_owned(), data);
                }
                self.count_sync(spec);
                value(current)
            }
            HolderOp::Has => Value::Int(i32::from(matches!(current, Current::Value(_)))),
            HolderOp::ExistingOrNull => value(current),
            HolderOp::Existing => match value(current) {
                Value::Ref(Some(marker)) => Value::Ref(Some(format!("optional/{marker}"))),
                _ => Value::Ref(Some(EMPTY_OPTIONAL.into())),
            },
            HolderOp::Remove => {
                if !self
                    .original(player)
                    .is_some_and(|original| original.allocated())
                {
                    Value::Ref(None)
                } else {
                    if let Some(original) = self.original(player) {
                        original.live.remove(id);
                        if let Ok(entries) = &mut original.entries {
                            entries.insert(id.to_owned(), StoreEntry::Absent);
                        }
                    }
                    self.count_sync(spec);
                    value(current)
                }
            }
        })
    }
}
