//! A listener façade's world border and held items.
//!
//! The event façade (`events.rs`) stands markers in for the server's levels
//! and players. Two reads real jars make next were measured in the fourth
//! in-game survey (`docs/in-game-survey.md`): Chunky wraps every level's
//! `getWorldBorder()` at `SERVER_STARTED`, and Stack Refill's `UseItemCallback`
//! starts with `player.getItemInHand(hand)` and `player.isCreative()`. This
//! module answers them from what the game hands the firing:
//!
//! - **World border.** `ServerLevel#getWorldBorder()` is a
//!   `game/world-border/<dimension>` marker when the game gave that level's
//!   border bounds ([`crate::events::FacadeLevel::world_border`]). Its
//!   `getMinX/MinZ/MaxX/MaxZ()` are those bounds exactly: the game's bounds
//!   are vanilla's no-argument getters (`extent.getMinX(0.0F)`: the centre
//!   minus half the size, clamped to the absolute world limit, using the
//!   previous size while the border moves). `isWithinBounds` compares with
//!   them as vanilla does. `getCenterX/Z()` and `getSize()` refuse: the game
//!   gives only the bounds, and they do not determine the centre exactly
//!   (vanilla rounds `centre -/+ size / 2.0`, so a centre of 0.1 with size 10
//!   gives bounds whose midpoint is 0.09999999999999964; a bound on the
//!   absolute limit has been clamped), nor the size while the border moves
//!   (vanilla's `getSize()` is the current lerped size, the bounds use the
//!   previous one).
//! - **Held items.** When the hook names what the firing's player holds
//!   ([`HeldItems`] on [`crate::events::EventContext`]), `getItemInHand`,
//!   `getMainHandItem` and `getOffhandItem` answer a
//!   `game/stack/<uuid>/<main|off>` marker for that firing only, and
//!   `isCreative()` answers the game mode the hook read. A stack answers
//!   `getItem`, `getCount`, `isEmpty` and `toString` as vanilla's `ItemStack`
//!   does (an empty hand is `ItemStack.EMPTY`: air, count 0); its item answers
//!   `toString` with its registry name. Everything else refuses and is counted.

use crate::{MethodRef, NativeError, Value, expect_ref, stable_hash};

/// One stack a hand holds, as the game reported it: a non-empty stack.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FacadeStack {
    /// The item's registry name (`minecraft:bread`).
    pub item: String,
    /// The stack size, at least 1.
    pub count: i32,
}

/// What the firing's player holds and their game mode, read by the hook for
/// this one firing. A hand of `None` is `ItemStack.EMPTY`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeldItems {
    pub main_hand: Option<FacadeStack>,
    pub off_hand: Option<FacadeStack>,
    /// `Player.isCreative()`; `None` when the game could not say, so the read
    /// refuses.
    pub creative: Option<bool>,
}

pub(crate) const WORLD_BORDER: &str = "game/world-border/";
pub(crate) const STACK: &str = "game/stack/";
pub(crate) const ITEM: &str = "game/item/";

const STACK_DESCRIPTOR: &str = "()Lnet/minecraft/world/item/ItemStack;";
const HAND_ENUM: &str = "enum/net/minecraft/world/InteractionHand/";

fn marker(text: String) -> Value {
    Value::Ref(Some(text))
}

fn refuse(r: &MethodRef) -> Result<Value, NativeError> {
    Err(NativeError::UnknownHostCall(r.clone()))
}

/// `ServerLevel#getWorldBorder()` for a level whose bounds the game gave.
pub(crate) fn border_marker(dimension: &str) -> Value {
    marker(format!("{WORLD_BORDER}{dimension}"))
}

/// `Object`'s identity members on one of this module's markers: two reads of
/// the same border or hand are the same object, as in vanilla.
fn identity(text: &str, r: &MethodRef, args: &[Value]) -> Option<Value> {
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("equals", "(Ljava/lang/Object;)Z") => Some(Value::Int(i32::from(
            args.first().and_then(expect_ref).is_some_and(|o| o == text),
        ))),
        ("hashCode", "()I") => Some(Value::Int(stable_hash(text))),
        _ => None,
    }
}

/// Answer a call on a world-border marker, from the level's bounds
/// (`[min_x, min_z, max_x, max_z]`, `None` when the level is no longer known).
pub(crate) fn border_answer(
    text: &str,
    bounds: Option<[f64; 4]>,
    r: &MethodRef,
    args: &[Value],
) -> Result<Value, NativeError> {
    if let Some(value) = identity(text, r, args) {
        return Ok(value);
    }
    let Some([min_x, min_z, max_x, max_z]) = bounds else {
        return refuse(r);
    };
    let within = |x: f64, z: f64, margin: f64| {
        Value::Int(i32::from(
            x >= min_x - margin && x < max_x + margin && z >= min_z - margin && z < max_z + margin,
        ))
    };
    let double = |index: usize| match args.get(index) {
        Some(Value::Float(value)) => Some(*value),
        _ => None,
    };
    let answer = match (r.name.as_str(), r.descriptor.as_str()) {
        ("getMinX", "()D") => Some(Value::Float(min_x)),
        ("getMinZ", "()D") => Some(Value::Float(min_z)),
        ("getMaxX", "()D") => Some(Value::Float(max_x)),
        ("getMaxZ", "()D") => Some(Value::Float(max_z)),
        ("isWithinBounds", "(DD)Z") => match (double(0), double(1)) {
            (Some(x), Some(z)) => Some(within(x, z, 0.0)),
            _ => None,
        },
        ("isWithinBounds", "(DDD)Z") => match (double(0), double(1), double(2)) {
            (Some(x), Some(z), Some(margin)) => Some(within(x, z, margin)),
            _ => None,
        },
        // `isWithinBounds(pos.getX(), pos.getZ())`, the ints widened.
        ("isWithinBounds", "(Lnet/minecraft/core/BlockPos;)Z") => args
            .first()
            .and_then(expect_ref)
            .and_then(|text| text.strip_prefix("block-pos/"))
            .and_then(|coords| {
                let mut parts = coords.split(',').map(|part| part.parse::<i32>().ok());
                Some((parts.next()??, parts.next()??, parts.next()??))
            })
            .map(|(x, _, z)| within(f64::from(x), f64::from(z), 0.0)),
        _ => None,
    };
    answer.map_or_else(|| refuse(r), Ok)
}

/// A held-item or game-mode member of the player `uuid`, or `None` when `r`
/// is none of them. The inner `None` refuses: the firing is not about this
/// player, or the hook did not read what they hold.
pub(crate) fn player_member(
    player: Option<u128>,
    held: Option<&HeldItems>,
    uuid: u128,
    r: &MethodRef,
    args: &[Value],
) -> Option<Option<Value>> {
    let held = held.filter(|_| player == Some(uuid));
    let hand = |off: bool| held.map(|_| stack_marker(uuid, off));
    Some(match (r.name.as_str(), r.descriptor.as_str()) {
        (
            "getItemInHand",
            "(Lnet/minecraft/world/InteractionHand;)Lnet/minecraft/world/item/ItemStack;",
        ) => match args
            .first()
            .and_then(expect_ref)
            .and_then(|text| text.strip_prefix(HAND_ENUM))
        {
            Some("MAIN_HAND") => hand(false),
            Some("OFF_HAND") => hand(true),
            // `null` or an unknown hand: vanilla throws; refuse.
            _ => None,
        },
        ("getMainHandItem", STACK_DESCRIPTOR) => hand(false),
        ("getOffhandItem", STACK_DESCRIPTOR) => hand(true),
        ("isCreative", "()Z") => held
            .and_then(|held| held.creative)
            .map(|creative| Value::Int(i32::from(creative))),
        _ => return None,
    })
}

fn stack_marker(uuid: u128, off: bool) -> Value {
    marker(format!(
        "{STACK}{uuid:032x}/{}",
        if off { "off" } else { "main" }
    ))
}

/// Answer a call on a held-stack marker (`rest` is what follows
/// [`STACK`]), from the firing's player and held items.
pub(crate) fn stack_answer(
    text: &str,
    rest: &str,
    player: Option<u128>,
    held: Option<&HeldItems>,
    r: &MethodRef,
    args: &[Value],
) -> Result<Value, NativeError> {
    if let Some(value) = identity(text, r, args) {
        return Ok(value);
    }
    let stack = rest.split_once('/').and_then(|(uuid, hand)| {
        let uuid = u128::from_str_radix(uuid, 16).ok()?;
        let held = held.filter(|_| player == Some(uuid))?;
        match hand {
            "main" => Some(held.main_hand.as_ref()),
            "off" => Some(held.off_hand.as_ref()),
            _ => None,
        }
    });
    // Outside the firing that handed it out the stack is unknown.
    let Some(stack) = stack else {
        return refuse(r);
    };
    // `ItemStack.isEmpty()`: `EMPTY`, air or a count of 0 or less; such a
    // stack's `getItem()` is air and its `getCount()` 0.
    let stack = stack.filter(|s| s.count > 0 && s.item != "minecraft:air");
    let item = stack.map_or("minecraft:air", |s| s.item.as_str());
    let count = stack.map_or(0, |s| s.count);
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("isEmpty", "()Z") => Ok(Value::Int(i32::from(stack.is_none()))),
        ("getCount", "()I") => Ok(Value::Int(count)),
        ("getItem", "()Lnet/minecraft/world/item/Item;") => Ok(marker(format!("{ITEM}{item}"))),
        // `getCount() + " " + getItem()`.
        ("toString", "()Ljava/lang/String;") => Ok(marker(format!("{count} {item}"))),
        _ => refuse(r),
    }
}

/// Answer a call on an item marker (`id` follows [`ITEM`]).
pub(crate) fn item_answer(
    text: &str,
    id: &str,
    r: &MethodRef,
    args: &[Value],
) -> Result<Value, NativeError> {
    if let Some(value) = identity(text, r, args) {
        return Ok(value);
    }
    match (r.name.as_str(), r.descriptor.as_str()) {
        // `BuiltInRegistries.ITEM.wrapAsHolder(this).getRegisteredName()`.
        ("toString", "()Ljava/lang/String;") => Ok(marker(id.to_owned())),
        _ => refuse(r),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StdHost;
    use crate::events::{EventContext, EventFacade, EventFiring, FacadeLevel, GameEvent};
    use std::sync::Arc;

    fn call(owner: &str, name: &str, descriptor: &str) -> MethodRef {
        MethodRef {
            owner: owner.into(),
            name: name.into(),
            descriptor: descriptor.into(),
        }
    }

    const BORDER: &str = "net/minecraft/world/level/border/WorldBorder";

    fn border(bounds: [f64; 4], name: &str, descriptor: &str, args: &[Value]) -> Option<Value> {
        border_answer(
            "game/world-border/minecraft:overworld",
            Some(bounds),
            &call(BORDER, name, descriptor),
            args,
        )
        .ok()
    }

    /// The bounds and `isWithinBounds` answers match a real `WorldBorder` (26.3
    /// server jar, on a JVM) with centre (12, -8) and size 40, whose getters
    /// gave min (-8, -28), max (32, 12); the max edge is outside, the min edge
    /// inside.
    #[test]
    fn a_border_answers_its_bounds_and_containment_as_vanilla() {
        let bounds = [-8.0, -28.0, 32.0, 12.0];
        let d = Value::Float;
        assert_eq!(border(bounds, "getMinX", "()D", &[]), Some(d(-8.0)));
        assert_eq!(border(bounds, "getMinZ", "()D", &[]), Some(d(-28.0)));
        assert_eq!(border(bounds, "getMaxX", "()D", &[]), Some(d(32.0)));
        assert_eq!(border(bounds, "getMaxZ", "()D", &[]), Some(d(12.0)));
        let within = |args: &[Value]| border(bounds, "isWithinBounds", "(DD)Z", args);
        assert_eq!(within(&[d(32.0), d(0.0)]), Some(Value::Int(0)));
        assert_eq!(within(&[d(-8.0), d(-28.0)]), Some(Value::Int(1)));
        assert_eq!(within(&[d(31.9), d(11.9)]), Some(Value::Int(1)));
        assert_eq!(
            border(
                bounds,
                "isWithinBounds",
                "(DDD)Z",
                &[d(33.0), d(0.0), d(1.5)]
            ),
            Some(Value::Int(1))
        );
        let pos = Value::Ref(Some("block-pos/31,64,11".into()));
        assert_eq!(
            border(
                bounds,
                "isWithinBounds",
                "(Lnet/minecraft/core/BlockPos;)Z",
                &[pos]
            ),
            Some(Value::Int(1))
        );
    }

    /// Centre and size are not read from the bounds: a real `WorldBorder` with
    /// centre 0.1 and size 10 has bounds -4.9 and 5.1, whose midpoint is not
    /// 0.1, and the default border's bounds sit on the clamp limit.
    #[test]
    fn centre_and_size_refuse_because_the_bounds_do_not_determine_them() {
        for bounds in [
            [-4.9, -5.0, 5.1, 5.0],
            [-29_999_984.0, -29_999_984.0, 29_999_984.0, 29_999_984.0],
        ] {
            assert_eq!(border(bounds, "getCenterX", "()D", &[]), None);
            assert_eq!(border(bounds, "getCenterZ", "()D", &[]), None);
            assert_eq!(border(bounds, "getSize", "()D", &[]), None);
        }
        // A level whose border the game did not give refuses every read.
        assert!(
            border_answer(
                "game/world-border/minecraft:the_end",
                None,
                &call(BORDER, "getMinX", "()D"),
                &[]
            )
            .is_err()
        );
    }

    fn text(value: Value) -> String {
        match value {
            Value::Ref(Some(text)) => text,
            other => panic!("not a reference: {other:?}"),
        }
    }

    /// A use-item firing hands the clicked player's stacks out: `getItemInHand`
    /// on each hand, the stack's count, item and `toString` (vanilla's
    /// `getCount() + " " + getItem()`, the item its registry name), and an empty
    /// hand as `ItemStack.EMPTY` (air, count 0). Another player's hands, and the
    /// same markers outside the firing, refuse.
    #[test]
    fn the_clicked_players_hands_answer_for_that_firing_only() {
        let alex = 0xa1e7_u128;
        let level = FacadeLevel {
            id: "minecraft:overworld".into(),
            world_border: Some([-8.0, -28.0, 32.0, 12.0]),
            ..FacadeLevel::default()
        };
        let context = EventContext {
            player: Some(alex),
            held: Some(HeldItems {
                main_hand: Some(FacadeStack {
                    item: "minecraft:bread".into(),
                    count: 2,
                }),
                off_hand: None,
                creative: Some(true),
            }),
            ..EventContext::default()
        };
        let facade = EventFacade {
            levels: vec![level],
            ..EventFacade::default()
        };
        let mut host = StdHost::new();
        host.game_event = Some(Box::new(EventFiring::new(
            GameEvent::UseItem,
            Arc::new(facade),
            context,
        )));
        let player = |uuid: u128| Value::Ref(Some(format!("game/player/{uuid:032x}")));
        let hand = |name: &str| Value::Ref(Some(format!("{HAND_ENUM}{name}")));
        let player_class = "net/minecraft/world/entity/player/Player";
        let in_hand = call(
            player_class,
            "getItemInHand",
            "(Lnet/minecraft/world/InteractionHand;)Lnet/minecraft/world/item/ItemStack;",
        );
        let mut ask = |receiver: &Value, r: &MethodRef, args: &[Value]| {
            host.game_event_call(r, receiver, args)
                .expect("a façade marker")
        };
        let main = ask(&player(alex), &in_hand, &[hand("MAIN_HAND")]).unwrap();
        let off = ask(&player(alex), &in_hand, &[hand("OFF_HAND")]).unwrap();
        let stack = "net/minecraft/world/item/ItemStack";
        let count = call(stack, "getCount", "()I");
        let empty = call(stack, "isEmpty", "()Z");
        let item = call(stack, "getItem", "()Lnet/minecraft/world/item/Item;");
        let to_string = call("java/lang/Object", "toString", "()Ljava/lang/String;");
        assert_eq!(ask(&main, &count, &[]).unwrap(), Value::Int(2));
        assert_eq!(ask(&main, &empty, &[]).unwrap(), Value::Int(0));
        assert_eq!(
            text(ask(&main, &to_string, &[]).unwrap()),
            "2 minecraft:bread"
        );
        let bread = ask(&main, &item, &[]).unwrap();
        assert_eq!(
            text(ask(&bread, &to_string, &[]).unwrap()),
            "minecraft:bread"
        );
        assert_eq!(ask(&off, &count, &[]).unwrap(), Value::Int(0));
        assert_eq!(ask(&off, &empty, &[]).unwrap(), Value::Int(1));
        let air = ask(&off, &item, &[]).unwrap();
        assert_eq!(text(ask(&air, &to_string, &[]).unwrap()), "minecraft:air");
        // The same hand read twice is the same object.
        let again = call(
            player_class,
            "getMainHandItem",
            "()Lnet/minecraft/world/item/ItemStack;",
        );
        assert_eq!(ask(&player(alex), &again, &[]).unwrap(), main);
        assert_eq!(
            ask(&player(alex), &call(player_class, "isCreative", "()Z"), &[]).unwrap(),
            Value::Int(1)
        );
        // Another player's hands were not read for this firing.
        assert!(ask(&player(0xb0b), &in_hand, &[hand("MAIN_HAND")]).is_err());
        // The level's border is the one the game gave.
        let level = Value::Ref(Some("game/level/minecraft:overworld".into()));
        let get_border = call(
            "net/minecraft/server/level/ServerLevel",
            "getWorldBorder",
            "()Lnet/minecraft/world/level/border/WorldBorder;",
        );
        let world_border = ask(&level, &get_border, &[]).unwrap();
        assert_eq!(
            ask(&world_border, &call(BORDER, "getMaxX", "()D"), &[]).unwrap(),
            Value::Float(32.0)
        );
        // Outside the firing the stack is unknown.
        host.game_event = None;
        assert!(host.game_event_call(&count, &main, &[]).unwrap().is_err());
    }
}
