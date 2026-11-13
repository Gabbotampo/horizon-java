//! FML's `InterModComms` (IMC): the message queues mods fill in `InterModEnqueueEvent` and read in
//! `InterModProcessEvent`, shared by every jar running in one game.
//!
//! # What FML does (`javap -c -p`, loader-12.0.12)
//!
//! `InterModComms` keeps one static `ConcurrentMap<String, ConcurrentLinkedQueue<IMCMessage>>`,
//! keyed by the receiving mod's id. Nothing in the loader drains or clears it, and nothing checks
//! the loading stage: a mod may send or read at any time.
//!
//! * `sendTo(modId, method, Supplier)` @0–53: `ModList.get().isLoaded(modId)` is false → answers
//!   `false` and queues nothing. Otherwise `computeIfAbsent(modId, new queue)` and appends an
//!   `IMCMessage(sender, modId, method, supplier)`, answering `true`. The sender is
//!   `ModLoadingContext.get().getActiveContainer().getModId()`: the container FML set active
//!   (`ModLoader.lambda$dispatchParallelTask$3` @33–55 around every construction and
//!   `ParallelDispatchEvent` post, `DeferredWorkQueue.lambda$makeRunnable$0` @4–53 around each
//!   deferred task), or, with none set, the `minecraft` container (`getActiveContainer` @0–33).
//! * `sendTo(senderModId, modId, method, Supplier)` @0–45: the same with the sender given.
//! * `isLoaded` is `indexedMods.containsKey` over a `Collectors.toMap` map: `null` is not loaded.
//! * `getMessages(modId, Predicate)` @0–34: no queue was ever made for `modId` → `Stream.empty()`;
//!   otherwise a sequential stream over `QueueFilteringSpliterator`, whose `tryAdvance` @0–60 walks
//!   the queue, tests each message's `method` with the predicate, skips a miss (it stays queued),
//!   and hands a match on and then removes it. `getMessages(modId)` is the same with a predicate
//!   that is always true. `ConcurrentHashMap.get(null)` throws `NullPointerException`.
//! * `IMCMessage` is a record: `senderModId()`, `modId()`, `method()`, `messageSupplier()`. The
//!   supplier is the sender's own object; the receiver calls `get()` on it whenever it likes.
//!
//! # Here
//!
//! The queues live in an [`ImcBus`] every host of one game shares: the game's content pass joins
//! each jar's host to one bus, so a message one jar sends reaches another. A host that joined none
//! (the probe's single jar) has a bus of its own. `isLoaded` is the host's own `ModList.isLoaded`
//! answer, or any mod of a jar on the bus.
//!
//! A message read by the jar that sent it hands back the jar's own supplier. A message from another
//! jar hands back a supplier marker: its `get()` runs the sender's supplier in the sender's
//! runtime, with the sender's host, at that moment (as the JVM runs it, lazily). That needs the
//! sender's host, which the game lends to the bus while the loading stages run
//! ([`ImcBus::lend`]). Named limits, each counted as an unknown host call and answered `null`:
//!
//! * the sending jar's host is not lent (after the loading stages, or the sender is itself waiting
//!   on this call further up);
//! * the supplier's value is a reference that only means something in the sender's runtime (a jar
//!   object, a collection): a `null`, a primitive box or a plain string crosses unchanged.
//!
//! # Departures, named
//!
//! * **I1** the runtime's streams are eager: `getMessages` tests every queued message and removes
//!   the matches when it is called, and the stream holds them. FML tests and removes each one as
//!   the stream reaches it. The two agree for a stream that is consumed to its end with a predicate
//!   that has no side effects; a message sent to the same mod while the stream is consumed is not
//!   part of it here (the JDK's weakly consistent iterator may see it).
//! * **I2** a `null` predicate is counted and answers an empty stream (FML throws on the first
//!   message the stream reaches).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::events::ListenerCallback;
use crate::{
    ExecutionBudget, Host, MethodRef, NativeError, NativeJar, Runtime, StdHost, Value, expect_ref,
};

const COMMS: &str = "net/neoforged/fml/InterModComms";
const MESSAGE: &str = "net/neoforged/fml/InterModComms$IMCMessage";
const MESSAGE_MARKER: &str = "neoforge-imc-message/";
const SUPPLIER_MARKER: &str = "neoforge-imc-supplier/";
const SEND: &str = "(Ljava/lang/String;Ljava/lang/String;Ljava/util/function/Supplier;)Z";
const SEND_AS: &str =
    "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/util/function/Supplier;)Z";
const READ: &str = "(Ljava/lang/String;)Ljava/util/stream/Stream;";
const READ_FILTERED: &str =
    "(Ljava/lang/String;Ljava/util/function/Predicate;)Ljava/util/stream/Stream;";
const SUPPLIER_GET: &str = "java/util/function/Supplier#get()Ljava/lang/Object;";

/// One queued or read `IMCMessage`, with the jar that sent it.
#[derive(Debug, Clone)]
struct Message {
    sender: Option<String>,
    receiver: String,
    method: Option<String>,
    supplier: Value,
    /// The key of the host that sent it ([`ImcBus::join`]).
    origin: String,
}

/// What one game's jars share: FML's `containerQueues`, every message ever sent (a read message
/// stays readable through its record), the loaded mods, and the hosts lent for cross-jar calls.
#[derive(Default)]
struct ImcState {
    /// Receiving mod id → the ids of its queued messages, in send order.
    queues: BTreeMap<String, VecDeque<usize>>,
    messages: Vec<Message>,
    mods: BTreeSet<String>,
    peers: BTreeMap<String, (Arc<NativeJar>, StdHost)>,
}

impl std::fmt::Debug for ImcState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImcState")
            .field("queues", &self.queues)
            .field("messages", &self.messages.len())
            .field("mods", &self.mods)
            .field("peers", &self.peers.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// A host's view of its bus: which bus, which jar it is there, and the container FML would have
/// set active in `ModLoadingContext` (`None`: the `minecraft` container).
#[derive(Debug, Clone, Default)]
pub struct ImcHandle {
    bus: Arc<Mutex<ImcState>>,
    jar: String,
    pub(crate) active: Option<String>,
}

fn lock(bus: &Mutex<ImcState>) -> MutexGuard<'_, ImcState> {
    bus.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The IMC queues of one game, shared by the hosts that [`join`](Self::join) it.
#[derive(Debug, Clone, Default)]
pub struct ImcBus {
    state: Arc<Mutex<ImcState>>,
}

impl ImcBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Joins `host` (the host of `jar`, known on this bus as `key`) to the bus: its sends and
    /// reads use the shared queues, and `jar`'s mods are loaded for every jar's `sendTo`.
    pub fn join(&self, key: &str, jar: &NativeJar, host: &mut StdHost) {
        self.add_mods(jar);
        host.imc = ImcHandle {
            bus: Arc::clone(&self.state),
            jar: key.to_owned(),
            active: None,
        };
    }

    /// Puts `jar`'s mods in the bus's `ModList`, before any jar runs.
    pub fn add_mods(&self, jar: &NativeJar) {
        let mut state = lock(&self.state);
        state.mods.extend(jar.neoforge_container_ids());
        state.mods.extend(jar.neoforge_id.clone());
    }

    /// Lends the host of the jar `key` to the bus while another jar runs, so that jar can call a
    /// supplier this one sent. [`Self::reclaim`] takes it back.
    pub fn lend(&self, key: &str, jar: Arc<NativeJar>, host: StdHost) {
        lock(&self.state).peers.insert(key.to_owned(), (jar, host));
    }

    pub fn reclaim(&self, key: &str) -> Option<StdHost> {
        lock(&self.state).peers.remove(key).map(|(_, host)| host)
    }
}

/// Sets the container FML's `ModLoadingContext` holds while a construction, a post or a deferred
/// task runs; answers the one it replaces.
pub(crate) fn set_active<H: Host + ?Sized>(host: &mut H, id: Option<&str>) -> Option<String> {
    let (imc, _) = host.neoforge_imc()?;
    std::mem::replace(&mut imc.active, id.map(str::to_owned))
}

/// `instanceof` for a message record and a cross-jar supplier.
pub(crate) fn instance_of(marker: &str, wanted: &str) -> bool {
    if marker.starts_with(MESSAGE_MARKER) {
        return matches!(wanted, MESSAGE | "java/lang/Record" | "java/lang/Object");
    }
    marker.starts_with(SUPPLIER_MARKER)
        && matches!(wanted, "java/util/function/Supplier" | "java/lang/Object")
}

fn count(unknown: &mut BTreeMap<String, usize>, call: &str, why: &str) {
    *unknown.entry(format!("{call} ({why})")).or_default() += 1;
}

fn text(value: &Value) -> Option<String> {
    expect_ref(value).cloned()
}

fn null_pointer() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    }
}

/// A value that means the same in another jar's runtime: `null`, a primitive box (it carries its
/// value) or a plain string.
fn crosses_jars(value: &Value) -> bool {
    match value {
        Value::Int(_) | Value::Long(_) | Value::Float(_) | Value::Boxed(_) | Value::Ref(None) => {
            true
        }
        Value::Ref(Some(text)) => crate::value_class::is_plain_string(text),
        // A host handle answers by its kind's row (`host_ref.rs`).
        Value::Host(host) => host.crosses_jars(&crosses_jars),
        Value::Void => false,
    }
}

/// How the sender's runtime reaches `Supplier#get` on `supplier`: a lambda or method reference, or
/// a jar object's own `get`.
fn supplier_callback(jar: &NativeJar, supplier: &Value) -> Option<ListenerCallback> {
    let text = expect_ref(supplier)?;
    if text.starts_with("@indy:") {
        let marker = crate::parse_callback_marker(text).ok()?;
        return Some(ListenerCallback::Indy {
            target: marker.target,
            captures: marker.captures,
        });
    }
    let (class, _) = text.split_once('#')?;
    let method = crate::events::resolve_method(jar, class, "get", "()Ljava/lang/Object;")?;
    Some(ListenerCallback::Object {
        receiver: supplier.clone(),
        method,
    })
}

impl<H: Host> Runtime<'_, H> {
    /// `InterModComms`'s static methods, an `IMCMessage`'s accessors and `get()` on another jar's
    /// supplier; `None` for any other call.
    pub(crate) fn invoke_neoforge_imc(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
        class: &str,
        method: &str,
    ) -> Result<Option<Value>, NativeError> {
        if r.owner == COMMS {
            return match (r.name.as_str(), r.descriptor.as_str()) {
                ("sendTo", SEND) => self.imc_send(None, &args[0], &args[1], &args[2]).map(Some),
                ("sendTo", SEND_AS) => self
                    .imc_send(Some(&args[0]), &args[1], &args[2], &args[3])
                    .map(Some),
                ("getMessages", READ) => self.imc_read(&args[0], None, class, method).map(Some),
                ("getMessages", READ_FILTERED) => self
                    .imc_read(&args[0], Some(&args[1]), class, method)
                    .map(Some),
                _ => Ok(None),
            };
        }
        let Some(marker) = receiver.and_then(expect_ref) else {
            return Ok(None);
        };
        if let Some(id) = marker.strip_prefix(MESSAGE_MARKER) {
            return Ok(self.imc_message_member(r, id));
        }
        if let Some(id) = marker.strip_prefix(SUPPLIER_MARKER)
            && r.name == "get"
            && r.descriptor == "()Ljava/lang/Object;"
        {
            return self.imc_foreign_get(id).map(Some);
        }
        Ok(None)
    }

    /// `ModList.isLoaded(id)`: the host's own answer, or a mod of any jar on the bus.
    fn imc_loaded(&mut self, id: &Value) -> Result<bool, NativeError> {
        let Value::Ref(Some(id)) = id else {
            return Ok(false);
        };
        let shared = self
            .host
            .neoforge_imc()
            .is_some_and(|(imc, _)| lock(&imc.bus).mods.contains(id));
        if shared {
            return Ok(true);
        }
        let call = |name: &str, descriptor: &str| MethodRef {
            owner: "net/neoforged/fml/ModList".into(),
            name: name.into(),
            descriptor: descriptor.into(),
        };
        let list = self
            .host
            .invoke(&call("get", "()Lnet/neoforged/fml/ModList;"), &[])?;
        let answer = self.host.invoke_instance(
            &call("isLoaded", "(Ljava/lang/String;)Z"),
            &list,
            &[Value::Ref(Some(id.clone()))],
        )?;
        Ok(matches!(answer, Value::Int(flag) if flag != 0))
    }

    fn imc_send(
        &mut self,
        sender: Option<&Value>,
        receiver: &Value,
        method: &Value,
        supplier: &Value,
    ) -> Result<Value, NativeError> {
        if !self.imc_loaded(receiver)? {
            return Ok(Value::Int(0));
        }
        let Some((imc, _)) = self.host.neoforge_imc() else {
            return Ok(Value::Int(0));
        };
        let sender = match sender {
            Some(sender) => text(sender),
            None => Some(imc.active.clone().unwrap_or_else(|| "minecraft".into())),
        };
        let receiver = text(receiver).unwrap_or_default();
        let message = Message {
            sender,
            receiver: receiver.clone(),
            method: text(method),
            supplier: supplier.clone(),
            origin: imc.jar.clone(),
        };
        let mut state = lock(&imc.bus);
        let id = state.messages.len();
        state.messages.push(message);
        state.queues.entry(receiver).or_default().push_back(id);
        Ok(Value::Int(1))
    }

    fn imc_read(
        &mut self,
        receiver: &Value,
        predicate: Option<&Value>,
        class: &str,
        method: &str,
    ) -> Result<Value, NativeError> {
        let Some(receiver) = text(receiver) else {
            return Err(null_pointer());
        };
        let Some((imc, unknown)) = self.host.neoforge_imc() else {
            return Ok(Value::Ref(None));
        };
        let bus = Arc::clone(&imc.bus);
        if predicate == Some(&Value::Ref(None)) {
            count(
                unknown,
                &format!("{COMMS}#getMessages{READ_FILTERED}"),
                "a null predicate",
            );
            return Ok(self.new_stream(class, method, Vec::new()));
        }
        // A copy of the queue: the predicate is jar code, and may send.
        let queued: Vec<(usize, Option<String>)> = {
            let state = lock(&bus);
            let Some(queue) = state.queues.get(&receiver) else {
                return Ok(self.new_stream(class, method, Vec::new()));
            };
            queue
                .iter()
                .map(|&id| (id, state.messages[id].method.clone()))
                .collect()
        };
        let mut taken = Vec::new();
        for (id, name) in queued {
            if let Some(predicate) = predicate {
                let name = Value::Ref(name);
                let result = match self.invoke_callback(predicate, std::slice::from_ref(&name))? {
                    Some(result) => result,
                    None => self.host.invoke_instance(
                        &MethodRef {
                            owner: "java/util/function/Predicate".into(),
                            name: "test".into(),
                            descriptor: "(Ljava/lang/Object;)Z".into(),
                        },
                        predicate,
                        std::slice::from_ref(&name),
                    )?,
                };
                if !matches!(result, Value::Int(flag) if flag != 0) {
                    continue;
                }
            }
            taken.push(id);
        }
        let mut state = lock(&bus);
        if let Some(queue) = state.queues.get_mut(&receiver) {
            queue.retain(|id| !taken.contains(id));
        }
        drop(state);
        let values = taken
            .into_iter()
            .map(|id| Value::Ref(Some(format!("{MESSAGE_MARKER}{id}"))))
            .collect();
        Ok(self.new_stream(class, method, values))
    }

    /// The record's accessors; anything else (`toString`, `equals`, `hashCode`) goes on and is
    /// counted.
    fn imc_message_member(&mut self, r: &MethodRef, id: &str) -> Option<Value> {
        let id: usize = id.parse().ok()?;
        let (imc, _) = self.host.neoforge_imc()?;
        let own = imc.jar.clone();
        let state = lock(&imc.bus);
        let message = state.messages.get(id)?;
        Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("senderModId", "()Ljava/lang/String;") => Value::Ref(message.sender.clone()),
            ("modId", "()Ljava/lang/String;") => Value::Ref(Some(message.receiver.clone())),
            ("method", "()Ljava/lang/String;") => Value::Ref(message.method.clone()),
            ("messageSupplier", "()Ljava/util/function/Supplier;") => {
                if message.origin == own || message.supplier == Value::Ref(None) {
                    message.supplier.clone()
                } else {
                    Value::Ref(Some(format!("{SUPPLIER_MARKER}{id}")))
                }
            }
            _ => return None,
        })
    }

    /// `get()` on a supplier another jar sent: run in that jar, with its host, now.
    fn imc_foreign_get(&mut self, id: &str) -> Result<Value, NativeError> {
        let Some((imc, _)) = self.host.neoforge_imc() else {
            return Ok(Value::Ref(None));
        };
        let bus = Arc::clone(&imc.bus);
        let peer = {
            let mut state = lock(&bus);
            let message = id
                .parse::<usize>()
                .ok()
                .and_then(|id| state.messages.get(id));
            match message.map(|message| (message.origin.clone(), message.supplier.clone())) {
                Some((origin, supplier)) => state
                    .peers
                    .remove(&origin)
                    .map(|(jar, host)| (origin, supplier, jar, host)),
                None => None,
            }
        };
        let Some((origin, supplier, jar, mut host)) = peer else {
            if let Some((_, unknown)) = self.host.neoforge_imc() {
                count(
                    unknown,
                    SUPPLIER_GET,
                    "an IMC message of a jar that is not reachable here",
                );
            }
            return Ok(Value::Ref(None));
        };
        let result = run_supplier(&jar, &supplier, &mut host, self.budget);
        lock(&bus).peers.insert(origin, (jar, host));
        let value = match result {
            Some(result) => result?,
            None => {
                if let Some((_, unknown)) = self.host.neoforge_imc() {
                    count(unknown, SUPPLIER_GET, "an IMC supplier its jar cannot run");
                }
                return Ok(Value::Ref(None));
            }
        };
        if crosses_jars(&value) {
            return Ok(value);
        }
        if let Some((_, unknown)) = self.host.neoforge_imc() {
            count(
                unknown,
                SUPPLIER_GET,
                "an IMC value that only exists in the sending jar",
            );
        }
        Ok(Value::Ref(None))
    }
}

/// Runs `supplier`'s `get()` in `jar` with `host`, the sending jar's own; `None` when it is no
/// supplier the runtime can reach.
fn run_supplier(
    jar: &NativeJar,
    supplier: &Value,
    host: &mut StdHost,
    budget: &ExecutionBudget,
) -> Option<Result<Value, NativeError>> {
    let callback = supplier_callback(jar, supplier)?;
    Some(
        jar.invoke_listener_callback(&callback, Vec::new(), host, budget)
            .map(|(_, value)| value),
    )
}

#[cfg(test)]
#[path = "neoforge_imc_tests.rs"]
pub(crate) mod tests;
