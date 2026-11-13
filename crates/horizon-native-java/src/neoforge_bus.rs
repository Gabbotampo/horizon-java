//! NeoForge's event bus (`net.neoforged.bus`, bus-8.0.5, the one NeoForge 26.3.0.101 ships):
//! `IEventBus#addListener` in all eight overloads, on the two kinds of bus a mod reaches.
//!
//! # The buses
//!
//! A mod sees two buses, built differently (`javap -c -p` of the 26.3 jars):
//!
//! * **its mod bus**, `FMLModContainer#getEventBus` (`loader-12.0.12`, `FMLModContainer.<init>`
//!   @31–62: `BusBuilder.builder().setExceptionHandler(..).markerType(IModBusEvent.class)
//!   .allowPerPhasePost().build()`), also the `IEventBus` a `@Mod` constructor receives. Its class
//!   checker (`BusBuilder#markerType` @17–24, `lambda$markerType$0`) accepts only `IModBusEvent`
//!   subclasses: "This bus only accepts subclasses of interface net.neoforged.fml.event.IModBusEvent,
//!   which class X is not."
//! * **the game bus**, `NeoForge.EVENT_BUS` (`NeoForge.<clinit>`: `startShutdown().classChecker(..)`),
//!   whose checker (`NeoForge.lambda$static$0`) refuses them: "IModBusEvent events are not allowed on
//!   the common NeoForge bus! Use a mod bus instead."
//!
//! They are the bus handles [`game_bus`] and [`mod_bus`] (`host_events.rs`); any other `IEventBus` value
//! (one the runtime cannot trace to either) skips the class checker, which is departure **B1**.
//!
//! # `addListener`, as `EventBus` runs it
//!
//! Every public overload lands in `addListener(EventPriority, Predicate, Class, Consumer)`; the four
//! without a `Class` first resolve it with `getEventClass` (@0–51): typetools'
//! `TypeResolver.resolveRawArgument(Consumer.class, consumer.getClass())`. For a lambda or method
//! reference that is typetools' `populateLambdaArgs` over the implementation method `getMemberRef`
//! finds in the lambda class's constant pool: with no parameters (an unbound `Event::method`
//! reference) the event is the method's declaring class, otherwise its **last** parameter (captured
//! values come first). For an object of a jar class it is the `Consumer<X>` type argument in that
//! class's (or a jar superclass's) generic `Signature`. Then, in order:
//!
//! 1. the bus's class checker (above); a refusal is rethrown as `IllegalArgumentException("Listener
//!    for event class X takes an argument that is not valid for this bus", cause)` (@0–33);
//! 2. `addToListeners` @0–26: an abstract event class is `IllegalArgumentException("Cannot register
//!    listeners for abstract class X. Register a listener to one of its subclasses instead!")`;
//! 3. `getListenerList` → `validateAbstractChain` @0–48 when the superclass is abstract: every
//!    abstract ancestor's superclass must be abstract too, else "Abstract event class A has a
//!    non-abstract superclass class B. The superclass must be made abstract.";
//! 4. `ListenerList#register(priority, listener)`: appended to that priority's list.
//!
//! `post` runs `ListenerList#getListeners`: `HIGHEST` to `LOWEST`, registration order within one
//! priority, and within one priority the event class's own list before its superclass's
//! (`getListeners(EventPriority)` @0–57). Every class the game fires has only abstract ancestors
//! (checked against the class table by a test), and abstract classes cannot hold listeners, so a
//! listener fires exactly for the class it named. A listener added without `receiveCanceled`
//! carries `passNotGenericFilter`'s `!isCanceled()` predicate, checked when it is reached; on a
//! class that is not `ICancellableEvent` the list unwraps it (`unwrapListeners`), so it always runs.
//!
//! # What fires and what is counted
//!
//! A listener whose class the game fires ([`crate::events::bus_event`]) becomes a
//! [`GameListener`](crate::events::GameListener) through [`StdHost::game_listeners`], next to the
//! listeners every other path registers, and runs through the same dispatch. Every other
//! registration is accepted, as NeoForge accepts it, and **counted** as an unknown host call named
//! after its event (`... (no game event fires X)`): the jar is not `ran-clean` while a listener it
//! registered can never run here. Counted the same way: an event type the runtime cannot resolve
//! where typetools would, a priority that is not an `EventPriority` constant, and a callback the
//! runtime cannot run.
//!
//! # Departures, named
//!
//! * **B1** a bus value the runtime cannot trace to a mod bus or the game bus skips the class
//!   checker (it cannot tell which one NeoForge would run).
//! * **B2** an exception NeoForge builds with a cause (the class checker's) is thrown with its
//!   message only; the cause's message is not attached.
//! * **B3** `getEventClass`'s `IllegalStateException("Failed to resolve consumer event type: " +
//!   consumer)` names the lambda class's runtime name (`Demo$$Lambda/0x...@hash`), which has no
//!   stable value; where typetools would fail the registration is counted instead of thrown.
//! * **B4** `unregister(consumer)` removes the listeners whose consumer has the same marker: two
//!   evaluations of one capturing lambda with equal captures are one consumer here and two objects
//!   on a JVM.
//! * **B5** a client event registered on a dedicated server is accepted without being counted: the
//!   runtime fires it on a client, and NeoForge never fires it on a server either.
//! * **B6** an unbound method reference (`SomeEvent::method`) resolves only when its owner is a jar
//!   class (the declaring class is found by walking the jar's superclasses); one whose owner is a
//!   NeoForge class is counted, since the runtime does not know which class declares the method.

use crate::events::ListenerCallback;
use crate::neoforge_event_classes::{ABSTRACT, CANCELLABLE, EVENT_CLASSES, MOD_BUS};
use crate::{Host, MethodRef, NativeError, Runtime, Value, expect_ref, parse_callback_marker};

pub(crate) use crate::host_events::{game_bus, mod_bus};

/// The bus interface every call names.
pub(crate) const BUS: &str = "net/neoforged/bus/api/IEventBus";
const EVENT: &str = "net/neoforged/bus/api/Event";
const MOD_BUS_EVENT: &str = "net/neoforged/fml/event/IModBusEvent";
const CANCELLABLE_EVENT: &str = "net/neoforged/bus/api/ICancellableEvent";
const CONSUMER: &str = "java/util/function/Consumer";

/// The bus a bus call's receiver is (a bus handle, or a value the runtime cannot trace), `null`
/// for none.
pub(crate) fn bus_value(receiver: Option<&Value>) -> Value {
    receiver.cloned().unwrap_or(Value::Ref(None))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BusKind {
    Mod,
    Game,
    /// A value the runtime cannot trace to either (departure B1).
    Unknown,
}

pub(crate) fn bus_kind(receiver: Option<&Value>) -> BusKind {
    match receiver.and_then(crate::host_events::is_game_bus) {
        Some(true) => BusKind::Game,
        Some(false) => BusKind::Mod,
        None => BusKind::Unknown,
    }
}

/// One listener a jar added, in registration order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BusListener {
    /// The event class it listens to (`net/neoforged/neoforge/event/tick/ServerTickEvent$Post`).
    pub class: String,
    /// An index into [`crate::events::order::PRIORITIES`].
    pub priority: u8,
    pub receive_canceled: bool,
    /// The `Consumer` object (`addListener`) or the registered object or class (`register`),
    /// which `unregister` names.
    pub consumer: Value,
    /// The bus it is on, since `unregister` acts on one bus.
    pub bus: Value,
    pub callback: ListenerCallback,
}

/// What one jar's buses hold.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BusState {
    pub(crate) listeners: Vec<BusListener>,
    /// The `(bus, target)` pairs `register` added a listener for: `EventBus#register` @0–11
    /// returns at once for a target its `listeners` map already holds.
    pub(crate) registered: Vec<(Value, Value)>,
    /// The work `ParallelDispatchEvent#enqueueWork` queued for the running loading stage's
    /// `DeferredWorkQueue` (`neoforge_lifecycle.rs`).
    pub(crate) deferred: Vec<crate::neoforge_lifecycle::DeferredTask>,
    /// The host is a dedicated server (`StdHost::with_environment_type`).
    pub(crate) server: bool,
}

/// What the runtime knows about one event class: the generated table's facts for NeoForge's own,
/// the class file's for a jar's.
pub(crate) struct EventFacts {
    pub(crate) is_abstract: bool,
    pub(crate) cancellable: bool,
    pub(crate) mod_bus: bool,
    pub(crate) superclass: String,
}

fn table_facts(class: &str) -> Option<EventFacts> {
    let at = EVENT_CLASSES
        .binary_search_by(|(name, _, _)| (*name).cmp(class))
        .ok()?;
    let (_, superclass, flags) = EVENT_CLASSES[at];
    Some(EventFacts {
        is_abstract: flags & ABSTRACT != 0,
        cancellable: flags & CANCELLABLE != 0,
        mod_bus: flags & MOD_BUS != 0,
        superclass: superclass.to_owned(),
    })
}

/// The superclass NeoForge's table names for one of its event classes.
pub(crate) fn table_superclass(class: &str) -> Option<String> {
    table_facts(class).map(|facts| facts.superclass)
}

/// `Class#toString`: `class a.b.C$D` (every event class here is a class, not an interface).
pub(crate) fn class_text(internal: &str) -> String {
    format!("class {}", internal.replace('/', "."))
}

pub(crate) fn illegal_argument(message: String) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(message),
        frame: None,
    }
}

/// How one `addListener` overload passes its arguments: where the priority, `receiveCanceled`,
/// event class and consumer sit (bus-8.0.5's `IEventBus`).
struct Overload {
    priority: Option<usize>,
    receive_canceled: Option<usize>,
    class: Option<usize>,
    consumer: usize,
}

fn overload(descriptor: &str) -> Option<Overload> {
    if !descriptor.ends_with(")V") {
        return None;
    }
    let mut shape = Overload {
        priority: None,
        receive_canceled: None,
        class: None,
        consumer: usize::MAX,
    };
    let arguments = crate::descriptor_arguments(descriptor);
    for (at, argument) in arguments.iter().enumerate() {
        match argument.as_str() {
            "Lnet/neoforged/bus/api/EventPriority;" if at == 0 => shape.priority = Some(at),
            "Z" if shape.receive_canceled.is_none() && shape.class.is_none() => {
                shape.receive_canceled = Some(at)
            }
            "Ljava/lang/Class;" => shape.class = Some(at),
            "Ljava/util/function/Consumer;" if at + 1 == arguments.len() => shape.consumer = at,
            _ => return None,
        }
    }
    (shape.consumer != usize::MAX).then_some(shape)
}

impl<H: Host> Runtime<'_, H> {
    /// The bus surface: `addListener` (every bus-8 overload) and `unregister`. `None` leaves the
    /// call to the rest of the chain (the older `IEventBus`-returning overloads, `register`,
    /// `post`).
    pub(crate) fn invoke_neoforge_bus(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.owner != BUS || self.host.neoforge_bus().is_none() {
            return Ok(None);
        }
        match r.name.as_str() {
            "addListener" => {
                let Some(shape) = overload(&r.descriptor) else {
                    return Ok(None);
                };
                match self.add_listener(&shape, bus_kind(receiver), args)? {
                    Ok(mut listener) => {
                        listener.bus = bus_value(receiver);
                        if let Some((state, _)) = self.host.neoforge_bus() {
                            state.listeners.push(listener);
                        }
                    }
                    Err(why) => self.count_bus_call(r, &why),
                }
                Ok(Some(Value::Void))
            }
            // `register(Object)`: `neoforge_subscribe.rs`.
            "register" if r.descriptor == "(Ljava/lang/Object;)V" => {
                self.bus_register(r, receiver, args)?;
                Ok(Some(Value::Void))
            }
            // `EventBus#unregister` @0–16: the listeners this bus holds under that object go; the
            // host's own record of older registrations is cleared by the rest of the chain.
            "unregister" if r.descriptor == "(Ljava/lang/Object;)V" => {
                let bus = bus_value(receiver);
                if let (Some(target), Some((state, _))) = (args.first(), self.host.neoforge_bus()) {
                    state
                        .listeners
                        .retain(|listener| &listener.consumer != target || listener.bus != bus);
                    state
                        .registered
                        .retain(|(on, registered)| registered != target || *on != bus);
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn count_bus_call(&mut self, r: &MethodRef, why: &str) {
        if let Some((_, unknown)) = self.host.neoforge_bus() {
            *unknown
                .entry(format!("{}#{}{} ({why})", r.owner, r.name, r.descriptor))
                .or_default() += 1;
        }
    }

    /// One `addListener`: the listener, why it is counted instead, or the exception NeoForge
    /// throws.
    fn add_listener(
        &mut self,
        shape: &Overload,
        bus: BusKind,
        args: &[Value],
    ) -> Result<Result<BusListener, String>, NativeError> {
        let counted = |why: &str| Ok(Err(why.to_owned()));
        let priority = match shape.priority {
            None => 2,
            Some(at) => match args
                .get(at)
                .and_then(expect_ref)
                .and_then(|text| crate::events::order::priority_of(text))
            {
                Some(priority) => priority,
                None => return counted("priority is not an EventPriority constant"),
            },
        };
        let asked = shape
            .receive_canceled
            .is_some_and(|at| matches!(args.get(at), Some(Value::Int(v)) if *v != 0));
        let Some(consumer) = args.get(shape.consumer).cloned() else {
            return counted("no consumer");
        };
        let class = match shape.class {
            Some(at) => match args.get(at).and_then(expect_ref) {
                Some(class) => class.clone(),
                None => return counted("event class is not a class literal"),
            },
            None => match self.consumer_event_class(&consumer) {
                Some(class) => class,
                None => return counted("event type not resolved"),
            },
        };
        let Some(facts) = self.event_facts(&class) else {
            return counted(&format!("{class} is not a known event class"));
        };
        // 1. The bus's class checker (`addListener` @0–33).
        let refused = match bus {
            BusKind::Mod => !facts.mod_bus,
            BusKind::Game => facts.mod_bus,
            BusKind::Unknown => false,
        };
        if refused {
            return Err(illegal_argument(format!(
                "Listener for event {} takes an argument that is not valid for this bus",
                class_text(&class)
            )));
        }
        // 2. `addToListeners` @0–26.
        if facts.is_abstract {
            return Err(illegal_argument(format!(
                "Cannot register listeners for abstract {}. Register a listener to one of its subclasses instead!",
                class_text(&class)
            )));
        }
        // 3. `getListenerList` @18–35 → `validateAbstractChain`.
        self.validate_abstract_chain(&facts.superclass)?;
        if !crate::neoforge_lifecycle::delivered(&class, self.bus_on_server()) {
            return counted(&format!("no game event fires {class}"));
        }
        let Some(callback) = self.consumer_callback(&consumer, &class) else {
            return counted("listener callback cannot run");
        };
        Ok(Ok(BusListener {
            bus: Value::Ref(None),
            class,
            priority,
            // `ListenerList.unwrapListeners`: on a class that is not `ICancellableEvent` the
            // `!isCanceled()` filter is dropped, and the listener always runs.
            receive_canceled: asked || !facts.cancellable,
            consumer,
            callback,
        }))
    }

    /// Whether this host is a dedicated server: what a loading stage posts depends on the side.
    pub(crate) fn bus_on_server(&mut self) -> bool {
        self.host
            .neoforge_bus()
            .is_some_and(|(state, _)| state.server)
    }

    /// `validateAbstractChain(superclass)`, run when the superclass is abstract.
    pub(crate) fn validate_abstract_chain(&self, superclass: &str) -> Result<(), NativeError> {
        let mut at = superclass.to_owned();
        for _ in 0..64 {
            if at == EVENT {
                return Ok(());
            }
            let Some(facts) = self.event_facts(&at) else {
                return Ok(());
            };
            if !facts.is_abstract {
                // Only an abstract superclass starts the walk (`getListenerList` @18–28).
                return Ok(());
            }
            let parent = self.event_facts(&facts.superclass);
            if parent.as_ref().is_some_and(|parent| !parent.is_abstract) {
                return Err(illegal_argument(format!(
                    "Abstract event {} has a non-abstract superclass {}. The superclass must be made abstract.",
                    class_text(&at),
                    class_text(&facts.superclass)
                )));
            }
            at = facts.superclass;
        }
        Ok(())
    }

    /// The facts of an event class: NeoForge's from the table, a jar's (or linked jar's) from its
    /// class file, as long as its superclass chain reaches `Event`.
    pub(crate) fn event_facts(&self, class: &str) -> Option<EventFacts> {
        if let Some(facts) = table_facts(class) {
            return Some(facts);
        }
        let file = self.class(class)?;
        let parent = self.event_facts(&file.super_name)?;
        let access = crate::jdk_model::class_access_flags(file)?;
        Some(EventFacts {
            is_abstract: access & 0x0400 != 0,
            cancellable: parent.cancellable || self.implements(class, CANCELLABLE_EVENT, 0),
            mod_bus: parent.mod_bus || self.implements(class, MOD_BUS_EVENT, 0),
            superclass: file.super_name.clone(),
        })
    }

    /// Whether jar class or interface `class` names `wanted` among its interfaces, transitively
    /// through the jar's own interfaces.
    fn implements(&self, class: &str, wanted: &str, depth: usize) -> bool {
        if depth > 16 {
            return false;
        }
        let Some(file) = self.class(class) else {
            return false;
        };
        file.interface_names()
            .iter()
            .any(|name| name == wanted || self.implements(name, wanted, depth + 1))
    }

    /// typetools' `resolveRawArgument(Consumer.class, consumer.getClass())`, where the runtime can
    /// follow it; `None` where it cannot (counted, departure B3).
    fn consumer_event_class(&self, consumer: &Value) -> Option<String> {
        let text = expect_ref(consumer)?;
        if text.starts_with("@indy:") {
            let marker = parse_callback_marker(text).ok()?;
            let parameters = crate::descriptor_arguments(&marker.target.descriptor);
            // `populateLambdaArgs`: an implementation method with one parameter fewer than
            // `accept` (none) is an unbound instance reference, and `T` is the method's declaring
            // class, which only a jar class's own methods show (departure B6); otherwise `T` is
            // the parameter `accept`'s argument lands on, the last.
            let raw = match parameters.last() {
                None => return self.declaring_class(&marker.target),
                Some(last) => last,
            };
            return raw
                .strip_prefix('L')
                .and_then(|rest| rest.strip_suffix(';'))
                .map(str::to_owned);
        }
        // An object of a jar class: the `Consumer<X>` its own or a jar superclass's signature
        // implements.
        let (class, _) = text.split_once('#')?;
        let mut at = class.to_owned();
        for _ in 0..32 {
            let file = self.class(&at)?;
            if let Some(found) = file
                .class_signature()
                .as_deref()
                .and_then(consumer_argument)
            {
                return Some(found);
            }
            at = file.super_name.clone();
        }
        None
    }

    /// The jar class that declares `method`, walking up from its owner.
    fn declaring_class(&self, method: &MethodRef) -> Option<String> {
        let mut at = method.owner.clone();
        for _ in 0..32 {
            let file = self.class(&at)?;
            if file.method(&method.name, &method.descriptor).is_some() {
                return Some(at);
            }
            at = file.super_name.clone();
        }
        None
    }

    /// How the game runs the consumer when the event fires: a lambda or method reference, or the
    /// `accept` of a jar object (its typed `accept(Event)` when it declares one, so the bridge's
    /// cast of a game event is not needed).
    fn consumer_callback(&self, consumer: &Value, class: &str) -> Option<ListenerCallback> {
        let text = expect_ref(consumer)?;
        if text.starts_with("@indy:") {
            let marker = parse_callback_marker(text).ok()?;
            return Some(ListenerCallback::Indy {
                target: marker.target,
                captures: marker.captures,
            });
        }
        let (owner, _) = text.split_once('#')?;
        let typed = format!("(L{class};)V");
        for descriptor in [typed.as_str(), "(Ljava/lang/Object;)V"] {
            let mut at = owner.to_owned();
            for _ in 0..32 {
                let Some(file) = self.class(&at) else {
                    break;
                };
                if file.method("accept", descriptor).is_some() {
                    return Some(ListenerCallback::Object {
                        receiver: consumer.clone(),
                        method: MethodRef {
                            owner: at,
                            name: "accept".into(),
                            descriptor: descriptor.into(),
                        },
                    });
                }
                at = file.super_name.clone();
            }
        }
        None
    }
}

/// The class a generic class signature's `Consumer<X>` supertype names, when `X` is a class type
/// (`Lpkg/Event;` or `Lpkg/Event<..>;`). A type variable or wildcard argument answers `None`.
fn consumer_argument(signature: &str) -> Option<String> {
    let mut rest = signature;
    // Formal type parameters: `<T:Ljava/lang/Object;>`.
    if rest.starts_with('<') {
        rest = &rest[matching_angle(rest)? + 1..];
    }
    while !rest.is_empty() {
        let end = class_type_end(rest)?;
        let (one, after) = rest.split_at(end);
        if let Some(arguments) = one
            .strip_prefix('L')
            .and_then(|s| s.strip_prefix(CONSUMER))
            .and_then(|s| s.strip_prefix('<'))
        {
            let argument = arguments.strip_prefix('L')?;
            let stop = argument.find([';', '<'])?;
            return Some(argument[..stop].to_owned());
        }
        rest = after;
    }
    None
}

/// The index of the `>` that closes the `<` at `text[0]`.
fn matching_angle(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (at, ch) in text.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// The length of the class type signature at the start of `text` (`Lpkg/A<..>.B<..>;`).
fn class_type_end(text: &str) -> Option<usize> {
    if !text.starts_with('L') {
        return None;
    }
    let mut depth = 0usize;
    for (at, ch) in text.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.checked_sub(1)?,
            ';' if depth == 0 => return Some(at + 1),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
#[path = "neoforge_bus_tests.rs"]
pub(crate) mod fixture_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fired_neoforge_class_has_only_abstract_ancestors() {
        // The dispatch fires a listener only for the class it named. NeoForge also runs the
        // listeners of each superclass, so that is only the same while no fired class has a
        // concrete ancestor (abstract classes cannot hold listeners: `addToListeners` @0–26).
        let mut checked = 0;
        for (class, superclass, flags) in EVENT_CLASSES {
            if crate::events::bus_event(class).is_none() {
                continue;
            }
            checked += 1;
            assert_eq!(flags & ABSTRACT, 0, "{class} is fired but abstract");
            let mut at = superclass.to_string();
            while at != EVENT {
                let facts = table_facts(&at).unwrap_or_else(|| panic!("{at} not in the table"));
                assert!(facts.is_abstract, "{class} has a concrete ancestor {at}");
                at = facts.superclass;
            }
        }
        assert!(
            checked >= 30,
            "only {checked} fired classes found in the table"
        );
    }

    #[test]
    fn fired_classes_agree_with_neoforge_on_cancellation() {
        // A listener cancels through the façade only where `GameEvent::cancellable` allows it;
        // NeoForge allows it exactly on `ICancellableEvent` classes.
        for (class, _, flags) in EVENT_CLASSES {
            if let Some((event, _)) = crate::events::bus_event(class) {
                assert_eq!(
                    event.cancellable(),
                    flags & CANCELLABLE != 0,
                    "{class} ({event:?})"
                );
            }
        }
    }

    #[test]
    fn consumer_type_argument_is_read_from_class_signatures() {
        assert_eq!(
            consumer_argument(
                "Ljava/lang/Object;Ljava/util/function/Consumer<Lnet/neoforged/neoforge/event/tick/ServerTickEvent$Post;>;"
            )
            .as_deref(),
            Some("net/neoforged/neoforge/event/tick/ServerTickEvent$Post")
        );
        assert_eq!(
            consumer_argument(
                "<T:Lnet/neoforged/bus/api/Event;>Ljava/lang/Object;Ljava/util/function/Consumer<TT;>;"
            ),
            None,
            "a type variable is left to typetools' bound resolution, which is counted"
        );
        assert_eq!(
            consumer_argument(
                "Ljava/lang/Object;Ljava/util/function/Supplier<Ljava/lang/String;>;"
            ),
            None
        );
    }
}
