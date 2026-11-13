//! Loader event objects as host handles (#156 P8): the event buses, Fabric's `Event` objects and
//! their invokers, NeoForge's IMC messages and suppliers, the future `enqueueWork` answers, and
//! the stand-in events a host caller dispatches.
//!
//! They used to be text (`neoforge-bus/game`, `fabric-event/<owner>.<FIELD>`,
//! `neoforge-imc-message/3`, `event/cancelled/x`) a jar `String` could spell: the String
//! `"neoforge-bus/game"` was an `IEventBus` to `instanceof`, and `isCanceled()` answered `true` on
//! any value spelled `cancelled/...`. Each is now a [`HostRef`] whose key is the old marker
//! without its prefix, so every table keyed by that text keeps working.
//!
//! Every key holds its parts through the one escaping encoder (`HostRef::of_values`), so no jar
//! text (a mod id, a field name) can spell another key.
//!
//! What each kind is on a JVM (`javap -p` of bus-8.0.5, loader-12.0.12, fabric-api-base 2.0.6):
//!
//! | kind | key | class | identity |
//! |---|---|---|---|
//! | [`HostKind::NeoForgeBus`] | `game`, `mod` + its id, `mod` alone (mod not known) | `net.neoforged.bus.EventBus` | one bus per key, except `mod` |
//! | [`HostKind::ForgeBus`] | the event class whose `BUS` it is | not known; surely Forge's `bus.EventBus` | one per event class |
//! | [`HostKind::FabricEvent`] | `field` + owner + name, `array` + serial (`createArrayBacked`), `registry` + its text | `…impl.base.event.ArrayBackedEvent` | one per key |
//! | [`HostKind::FabricInvoker`] | its event's key | not known (a lambda or proxy of the callback type) | a fresh value: `register` builds a new one |
//! | [`HostKind::ImcMessage`] | the message's index on the IMC bus | `InterModComms$IMCMessage` | one per message |
//! | [`HostKind::ImcSupplier`] | the message's index | not known (the sender's lambda); surely a `Supplier` | one per message |
//! | [`HostKind::DeferredWork`] | the task's index in the stage's queue | `CompletableFuture` | one per task |
//! | [`HostKind::PostedEvent`] | cancelled flag + name | not known; surely a bus `Event`, cancellable when cancelled | a fresh value |
//!
//! None of these classes overrides `toString` or `hashCode` (`IMCMessage` is a record whose
//! supplier prints and hashes by identity), so both are unknown here and counted, as the calls on
//! the old markers were. `equals` is `Object.equals`, identity: known when
//! [`HostRef::same_object`] knows, counted when not. `IMCMessage.equals` compares the record's
//! components, so only the same message is known equal.
//!
//! A known class is a [`ClassRule::FromKey`](crate::runtime::host_ref) rule, not a fixed class: a fixed
//! class claims that only handles are its objects (`HostKind::of_class`), and a jar can make its
//! own `CompletableFuture` or `IMCMessage`. A class the runtime does not know is a
//! `ClassRule::Unknown` with the types the object surely is: `instanceof` those (and their
//! supertypes) is exact, `getClass()` and anything else that needs the class is counted.
//!
//! Departures, named:
//! * **E1** the buses, events, IMC objects and futures do not cross to another jar's runtime
//!   (`crosses_jars`), as their markers did not: on a JVM `NeoForge.EVENT_BUS` is one object for
//!   every mod.

use crate::runtime::host_ref::{ClassHierarchy, HostKind, HostRef, Methods, Nested, instance_of_class};
use crate::runtime::{NativeError, Value};

const OBJECT: &str = "java/lang/Object";
const NEOFORGE_BUS: &str = "net/neoforged/bus/EventBus";
const NEOFORGE_BUS_API: &str = "net/neoforged/bus/api/IEventBus";
const FABRIC_EVENT: &str = "net/fabricmc/fabric/impl/base/event/ArrayBackedEvent";
const IMC_MESSAGE: &str = "net/neoforged/fml/InterModComms$IMCMessage";
const FUTURE: &str = "java/util/concurrent/CompletableFuture";

/// The key of `value` when it is a handle of `kind`.
pub(crate) fn key_of(value: &Value, kind: HostKind) -> Option<&str> {
    match value {
        Value::Host(handle) if handle.kind() == kind => Some(handle.key()),
        _ => None,
    }
}

fn handle(kind: HostKind, key: impl Into<std::sync::Arc<str>>) -> Value {
    Value::Host(HostRef::new(kind, key))
}

fn text(text: &str) -> Value {
    Value::Ref(Some(text.to_owned()))
}

/// A handle whose key holds `values`, through the one escaping encoder: a key never joins jar text
/// (a mod id, a class or field name, a registry) with separators of its own.
fn handle_of(kind: HostKind, values: &[Value]) -> Value {
    Value::Host(HostRef::of_values(kind, values))
}

/// The values a handle of `kind` holds in its key.
fn values_of(value: &Value, kind: HostKind) -> Option<Vec<Value>> {
    match value {
        Value::Host(handle) if handle.kind() == kind => handle.values(),
        _ => None,
    }
}

/// `NeoForge.EVENT_BUS`.
pub(crate) fn game_bus() -> Value {
    handle_of(HostKind::NeoForgeBus, &[text("game")])
}

/// The mod bus of mod `id` (`FMLModContainer#getEventBus`); without an id when the mod is
/// unknown.
pub(crate) fn mod_bus(id: Option<&str>) -> Value {
    match id {
        Some(id) => handle_of(HostKind::NeoForgeBus, &[text("mod"), text(id)]),
        None => handle_of(HostKind::NeoForgeBus, &[text("mod")]),
    }
}

/// `Some(true)` for the game bus, `Some(false)` for a mod bus, `None` for no bus handle.
pub(crate) fn is_game_bus(value: &Value) -> Option<bool> {
    let values = values_of(value, HostKind::NeoForgeBus)?;
    Some(values.first() == Some(&text("game")))
}

/// A Forge EventBus 7 per-event `BUS` field: the bus of `event`.
pub(crate) fn forge_bus(event: &str) -> Value {
    handle_of(HostKind::ForgeBus, &[text(event)])
}

/// The event class a Forge bus handle is the `BUS` of.
pub(crate) fn forge_bus_event(value: &Value) -> Option<String> {
    match values_of(value, HostKind::ForgeBus)?.as_slice() {
        [Value::Ref(Some(event))] => Some(event.clone()),
        _ => None,
    }
}

/// The Fabric `Event` an API field holds (`owner.name`).
pub(crate) fn fabric_field_event(owner: &str, name: &str) -> Value {
    handle_of(
        HostKind::FabricEvent,
        &[text("field"), text(owner), text(name)],
    )
}

/// The `serial`th event `EventFactory.createArrayBacked` made.
pub(crate) fn fabric_array_event(serial: usize) -> Value {
    handle_of(
        HostKind::FabricEvent,
        &[text("array"), Value::Long(serial as i64)],
    )
}

/// `RegistryEntryAddedCallback.event(registry)`, named by its registry's text.
pub(crate) fn fabric_registry_event(registry: &str) -> Value {
    handle_of(HostKind::FabricEvent, &[text("registry"), text(registry)])
}

/// The `(owner, field)` an API field's event names.
pub(crate) fn fabric_field_of(value: &Value) -> Option<(String, String)> {
    match values_of(value, HostKind::FabricEvent)?.as_slice() {
        [kind, Value::Ref(Some(owner)), Value::Ref(Some(name))] if *kind == text("field") => {
            Some((owner.clone(), name.clone()))
        }
        _ => None,
    }
}

/// How diagnostics name a Fabric event: `owner.FIELD`, the `createArrayBacked` serial, or
/// `…RegistryEntryAddedCallback.event/<registry>`, as the markers spelled them.
pub(crate) fn fabric_event_name(value: &Value) -> Option<String> {
    Some(match values_of(value, HostKind::FabricEvent)?.as_slice() {
        [_, Value::Ref(Some(owner)), Value::Ref(Some(name))] => format!("{owner}.{name}"),
        [_, Value::Long(serial)] => serial.to_string(),
        [_, Value::Ref(Some(registry))] => format!(
            "net/fabricmc/fabric/api/event/registry/RegistryEntryAddedCallback.event/{registry}"
        ),
        _ => return None,
    })
}

/// `event.invoker()`: the invoker of the Fabric event `receiver` (the empty key when the receiver
/// is no event the runtime made).
pub(crate) fn fabric_invoker(receiver: &Value) -> Value {
    handle(
        HostKind::FabricInvoker,
        key_of(receiver, HostKind::FabricEvent).unwrap_or(""),
    )
}

/// The `IMCMessage` record at `index` on the IMC bus.
pub(crate) fn imc_message(index: usize) -> Value {
    handle(HostKind::ImcMessage, index.to_string())
}

/// The supplier of the message at `index`, sent from another jar's runtime.
pub(crate) fn imc_supplier(index: usize) -> Value {
    handle(HostKind::ImcSupplier, index.to_string())
}

/// The `CompletableFuture` `enqueueWork` answers for the `serial`th queued task.
pub(crate) fn deferred_work(serial: usize) -> Value {
    handle(HostKind::DeferredWork, serial.to_string())
}

/// An event a host caller dispatches ([`crate::runtime::StdHost::dispatch_event`]) or hands a jar to post:
/// a stand-in named `name`. A cancelled one is a cancellable NeoForge event; one that is not is
/// only surely an `Event`. No jar value is one.
pub fn posted_event(name: &str, cancelled: bool) -> Value {
    handle_of(
        HostKind::PostedEvent,
        &[Value::Int(i32::from(cancelled)), text(name)],
    )
}

/// Whether `value` is a posted stand-in event made cancelled. A jar's own event object is never
/// cancelled here: the runtime does not track `setCanceled` on it.
pub(crate) fn is_cancelled_event(value: &Value) -> bool {
    values_of(value, HostKind::PostedEvent)
        .is_some_and(|values| values.first() == Some(&Value::Int(1)))
}

/// `Object.equals`: identity, where it is known.
fn identity_equals(
    handle: &HostRef,
    other: &Value,
    _: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    Ok(match other {
        Value::Host(other) => handle.same_object(other),
        _ => Some(false),
    })
}

/// `IMCMessage.equals`, a record's: its components compared. The same message is equal; another
/// message may hold equal components (the same supplier object sent twice), which the key does
/// not say, so that answer is unknown (counted).
fn record_equals(
    handle: &HostRef,
    other: &Value,
    _: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    Ok(match other {
        Value::Host(other) if other.kind() == HostKind::ImcMessage => {
            handle.same_object(other).filter(|same| *same)
        }
        _ => Some(false),
    })
}

/// `instanceof` for a kind whose whole type is known: `true` for the listed types, `false` for
/// any other (the classes are the loader's own, no jar extends them).
fn exactly(wanted: &str, types: &[&str]) -> Option<bool> {
    Some(wanted == OBJECT || types.contains(&wanted))
}

/// `Object.toString`/`Object.hashCode`: an identity hash, unknown here (counted).
fn unknown_text(_: &HostRef, _: &mut dyn Nested) -> Result<Option<String>, NativeError> {
    Ok(None)
}

fn unknown_hash(_: &HostRef, _: &mut dyn Nested) -> Result<Option<i32>, NativeError> {
    Ok(None)
}

/// Departure E1: a loader object never crosses to another jar's runtime.
fn stays_in_its_jar(_: &HostRef, _: &dyn Fn(&Value) -> bool) -> bool {
    false
}

/// The methods every P8 kind shares; only `instance_of` differs.
const fn methods(instance_of: fn(&HostRef, &str, &dyn ClassHierarchy) -> Option<bool>) -> Methods {
    Methods {
        to_string: unknown_text,
        hash_code: unknown_hash,
        equals: identity_equals,
        instance_of,
        crosses_jars: stays_in_its_jar,
    }
}

pub(crate) const NEOFORGE_BUS_METHODS: Methods = methods(|_, wanted, _| {
    exactly(
        wanted,
        &[
            NEOFORGE_BUS,
            NEOFORGE_BUS_API,
            "net/neoforged/bus/api/IEventExceptionHandler",
        ],
    )
});
pub(crate) const FABRIC_EVENT_METHODS: Methods = methods(|_, wanted, _| {
    exactly(
        wanted,
        &[FABRIC_EVENT, "net/fabricmc/fabric/api/event/Event"],
    )
});
/// A kind whose class is not known: `instanceof` answers from its declared types
/// (`ClassRule::Unknown`).
pub(crate) const DECLARED_TYPE_METHODS: Methods = methods(instance_of_class);
pub(crate) const IMC_MESSAGE_METHODS: Methods = Methods {
    equals: record_equals,
    ..methods(|_, wanted, _| exactly(wanted, &[IMC_MESSAGE, "java/lang/Record"]))
};
pub(crate) const DEFERRED_WORK_METHODS: Methods = methods(|_, wanted, _| {
    exactly(
        wanted,
        &[
            FUTURE,
            "java/util/concurrent/Future",
            "java/util/concurrent/CompletionStage",
        ],
    )
});

/// The classes `getClass()` answers ([`crate::runtime::host_ref::ClassRule::FromKey`]).
pub(crate) fn neoforge_bus_class(_: &str) -> &str {
    NEOFORGE_BUS
}
pub(crate) fn fabric_event_class(_: &str) -> &str {
    FABRIC_EVENT
}
pub(crate) fn imc_message_class(_: &str) -> &str {
    IMC_MESSAGE
}
pub(crate) fn future_class(_: &str) -> &str {
    FUTURE
}

/// A mod bus whose mod is not known may be any mod's: two of them are not provably one.
pub(crate) fn bus_canonical(key: &str) -> bool {
    key != crate::runtime::value_marker(&text("mod"))
}

/// The types each kind of unknown class surely is (`ClassRule::Unknown`), for its key.
pub(crate) fn forge_bus_types(_: &str) -> &'static [&'static str] {
    &["net/minecraftforge/eventbus/api/bus/EventBus"]
}
/// A Fabric invoker is its event's callback type, which the runtime does not know.
pub(crate) fn no_types(_: &str) -> &'static [&'static str] {
    &[]
}
pub(crate) fn supplier_types(_: &str) -> &'static [&'static str] {
    &["java/util/function/Supplier"]
}
/// A cancelled stand-in is a cancellable event; any other only surely an `Event`.
pub(crate) fn posted_event_types(key: &str) -> &'static [&'static str] {
    const EVENT: &str = "net/neoforged/bus/api/Event";
    let cancelled = HostRef::new(HostKind::PostedEvent, key)
        .values()
        .is_some_and(|values| values.first() == Some(&Value::Int(1)));
    if cancelled {
        &[EVENT, "net/neoforged/bus/api/ICancellableEvent"]
    } else {
        &[EVENT]
    }
}

#[cfg(test)]
#[path = "host_events_tests.rs"]
mod tests;
