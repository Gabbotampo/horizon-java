//! NeoForge's mod-loading stages after the `@Mod` constructors: the lifecycle events FML and
//! NeoForge post to each mod's bus, in their order, with the event objects mods call into.
//!
//! # The order (`javap -c -p`, loader-12.0.12 and neoforge-26.3.0.101-universal)
//!
//! `ServerModLoader.load` (a client's `ClientModLoader` runs the same common steps) calls
//! `CommonModLoader.begin` and then `CommonModLoader.load`:
//!
//! | step | FML/NeoForge | event | here |
//! |---|---|---|---|
//! | Mod Construction | `ModLoader.constructMods`, per container `lambda$constructMods$0` | `FMLConstructModEvent`, after the constructors and `@EventBusSubscriber` injection | [`LoadingStage::Construct`] |
//! | Registry initialization | `CommonModLoader.lambda$begin$0`: `NewRegistryEvent`, `RegisterEvent` per registry | | not posted: counted |
//! | Config loading | `lambda$begin$1`: `ConfigTracker.loadConfigs` | `ModConfigEvent.Loading` | not posted: counted |
//! | | `NeoForge.EVENT_BUS.start()` | | |
//! | Common setup | `load` @8–18 | `FMLCommonSetupEvent` | [`LoadingStage::CommonSetup`] |
//! | Sided setup | `load` @21–48 | `FMLDedicatedServerSetupEvent` (`FMLClientSetupEvent` on a client) | [`LoadingStage::SidedSetup`] |
//! | Registration events | `RegistrationEvents.init` | `RegisterCapabilitiesEvent`, `RegisterTicketControllersEvent`, … | not posted: counted |
//! | Enqueue IMC | `load` @63–73 | `InterModEnqueueEvent` | [`LoadingStage::EnqueueImc`] |
//! | Process IMC | `load` @76–86 | `InterModProcessEvent` | [`LoadingStage::ProcessImc`] |
//! | Complete loading | `load` @89–118 | `FMLLoadCompleteEvent` | [`LoadingStage::LoadComplete`] |
//! | Network registry lock | `NetworkRegistry.setup` @16–23 | `RegisterPayloadHandlersEvent` | [`LoadingStage::NetworkSetup`] |
//!
//! Every `ParallelDispatchEvent` step is `ModLoader.dispatchParallelEvent`: one event per
//! container, posted with `ModContainer#acceptEvent` (the whole bus, `HIGHEST` to `LOWEST`), every
//! container in parallel, and then the step's `DeferredWorkQueue#runTasks` on the main thread.
//! `RegisterPayloadHandlersEvent` is `ModLoader.postEvent`: one shared event, phase by phase
//! (`EventPriority.values()`), each phase through every mod in `ModList` order.
//!
//! # The event objects
//!
//! An event is a marker (`neoforge-lifecycle|<class>|<container>`). The host answers what the
//! loader's classes answer:
//!
//! * `ModLifecycleEvent#getContainer`: the container the event was built for, the same
//!   `neoforge-mod-container/<id>/<version>` the `@Mod` constructor received;
//! * `ParallelDispatchEvent#enqueueWork(Runnable|Supplier)`: `DeferredWorkQueue#enqueueWork`
//!   appends the work to the step's queue and answers its `CompletableFuture`. `runTasks` runs the
//!   queue in enqueue order once every container's event was posted; a task that throws is
//!   recorded (`captureException` suppresses it) and the rest still run, and the step then fails.
//!   The future is a marker whose every method is counted;
//! * `RegisterPayloadHandlersEvent#registrar` is the host's payload registrar
//!   (`payload_registrations`).
//!
//! Anything else a handler calls on an event (`getIMCStream`, …) goes on to the host and is counted
//! as an unknown host call, as every unmodelled call is; a jar whose handler leans on one is not
//! `ran-clean`.
//!
//! # What a registration path counts
//!
//! A listener on a class [`delivered`] names is accepted like a game event's: these classes are
//! posted here, or, for a client class on a dedicated server (and the dedicated-server setup on a
//! client), never posted by NeoForge on this side either. Every other mod-bus event (the registry,
//! config and registration events above) is still counted under its name.
//!
//! # Departures, named
//!
//! * **L1** a stage that fails (a listener or a deferred task threw) is recorded against the jar
//!   and the later stages still run. NeoForge stops loading there (`ModLoader.waitForFuture`
//!   @153–166: `cancelLoading`, `ModLoadingException`), as it stops on a failed constructor; the
//!   runtime's failures are its own limits more often than the jar's, as for an entrypoint.
//! * **L2** containers are posted one after another, in this jar's order, and jars in the game's
//!   load order; NeoForge posts them in parallel, so no order between mods is promised there.
//! * **L3** `RegisterPayloadHandlersEvent`'s phases run per jar: every phase of one jar's
//!   containers before the next jar's. The only state it shares across mods is the payload
//!   registry, which each jar's host keeps separately.
//! * **L4** the registry, config and registration events between the stages are not posted; a
//!   listener on one stays counted.

use crate::events::ListenerCallback;
use crate::neoforge_bus::BusListener;
use crate::{ExecutionBudget, MethodRef, NativeJar, StdHost, Value, expect_ref};

const LIFECYCLE: &str = "net/neoforged/fml/event/lifecycle/";
const CONSTRUCT: &str = "net/neoforged/fml/event/lifecycle/FMLConstructModEvent";
const COMMON_SETUP: &str = "net/neoforged/fml/event/lifecycle/FMLCommonSetupEvent";
const SERVER_SETUP: &str = "net/neoforged/fml/event/lifecycle/FMLDedicatedServerSetupEvent";
const CLIENT_SETUP: &str = "net/neoforged/fml/event/lifecycle/FMLClientSetupEvent";
const ENQUEUE_IMC: &str = "net/neoforged/fml/event/lifecycle/InterModEnqueueEvent";
const PROCESS_IMC: &str = "net/neoforged/fml/event/lifecycle/InterModProcessEvent";
const LOAD_COMPLETE: &str = "net/neoforged/fml/event/lifecycle/FMLLoadCompleteEvent";
const PAYLOAD_HANDLERS: &str = "net/neoforged/neoforge/network/event/RegisterPayloadHandlersEvent";
const NEOFORGE_CLIENT: &str = "net/neoforged/neoforge/client/";
const EVENT_MARKER: &str = "neoforge-lifecycle|";
const FUTURE_MARKER: &str = "neoforge-deferred-work/";

/// One step of NeoForge's loading after the constructors, in the order it runs them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadingStage {
    Construct,
    CommonSetup,
    SidedSetup,
    EnqueueImc,
    ProcessImc,
    LoadComplete,
    NetworkSetup,
}

impl LoadingStage {
    /// Every stage, in NeoForge's order.
    pub const ALL: [LoadingStage; 7] = [
        LoadingStage::Construct,
        LoadingStage::CommonSetup,
        LoadingStage::SidedSetup,
        LoadingStage::EnqueueImc,
        LoadingStage::ProcessImc,
        LoadingStage::LoadComplete,
        LoadingStage::NetworkSetup,
    ];

    /// The step's name, as FML's progress bar and log name it.
    pub fn name(self) -> &'static str {
        match self {
            LoadingStage::Construct => "Mod Construction",
            LoadingStage::CommonSetup => "Common setup",
            LoadingStage::SidedSetup => "Sided setup",
            LoadingStage::EnqueueImc => "Enqueue IMC",
            LoadingStage::ProcessImc => "Process IMC",
            LoadingStage::LoadComplete => "Complete loading",
            LoadingStage::NetworkSetup => "Network registry lock",
        }
    }

    /// The event class this step posts on the side `server` names.
    pub fn event(self, server: bool) -> &'static str {
        match self {
            LoadingStage::Construct => CONSTRUCT,
            LoadingStage::CommonSetup => COMMON_SETUP,
            LoadingStage::SidedSetup if server => SERVER_SETUP,
            LoadingStage::SidedSetup => CLIENT_SETUP,
            LoadingStage::EnqueueImc => ENQUEUE_IMC,
            LoadingStage::ProcessImc => PROCESS_IMC,
            LoadingStage::LoadComplete => LOAD_COMPLETE,
            LoadingStage::NetworkSetup => PAYLOAD_HANDLERS,
        }
    }
}

/// Whether a NeoForge registration path accepts a listener on `class` without counting it: the game
/// fires it, a loading stage posts it on this side, or NeoForge never posts it on this side either.
pub(crate) fn delivered(class: &str, server: bool) -> bool {
    if crate::events::bus_event(class).is_some() {
        return true;
    }
    if LoadingStage::ALL
        .iter()
        .any(|stage| stage.event(server) == class)
    {
        return true;
    }
    if server {
        class == CLIENT_SETUP || class.starts_with(NEOFORGE_CLIENT)
    } else {
        class == SERVER_SETUP
    }
}

/// The event marker of `class`, built for `container` (none for `RegisterPayloadHandlersEvent`).
fn event_marker(class: &str, container: Option<&str>) -> Value {
    Value::Ref(Some(format!(
        "{EVENT_MARKER}{class}|{}",
        container.unwrap_or_default()
    )))
}

/// `(class, container)` of an event marker.
fn parse_event(marker: &str) -> Option<(&str, &str)> {
    marker.strip_prefix(EVENT_MARKER)?.split_once('|')
}

/// `instanceof` on an event marker: its class, every superclass in NeoForge's table, and
/// `IModBusEvent` (every lifecycle event is one).
pub(crate) fn instance_of(marker: &str, wanted: &str) -> bool {
    let Some((class, _)) = parse_event(marker) else {
        return false;
    };
    if wanted == "java/lang/Object" || wanted == "net/neoforged/fml/event/IModBusEvent" {
        return true;
    }
    let mut at = class.to_owned();
    for _ in 0..16 {
        if at == wanted {
            return true;
        }
        match crate::neoforge_bus::table_superclass(&at) {
            Some(parent) => at = parent,
            None => return false,
        }
    }
    false
}

/// One piece of work `enqueueWork` queued.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DeferredTask {
    /// The container that queued it (`TaskInfo.owner`).
    pub container: String,
    /// The `Runnable` or `Supplier`.
    pub work: Value,
    pub supplier: bool,
}

impl StdHost {
    /// The lifecycle event objects' own methods; `None` leaves a call to the rest of the chain,
    /// which counts it.
    pub(crate) fn neoforge_lifecycle_instance(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Option<Value> {
        let (class, container) = parse_event(expect_ref(receiver)?)?;
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("getContainer", "()Lnet/neoforged/fml/ModContainer;") if !container.is_empty() => {
                Some(Value::Ref(Some(container.to_owned())))
            }
            (
                "enqueueWork",
                "(Ljava/lang/Runnable;)Ljava/util/concurrent/CompletableFuture;"
                | "(Ljava/util/function/Supplier;)Ljava/util/concurrent/CompletableFuture;",
            ) if class.starts_with(LIFECYCLE) && !container.is_empty() => {
                let work = args.first()?.clone();
                let tasks = &mut self.neoforge_bus.deferred;
                tasks.push(DeferredTask {
                    container: container.to_owned(),
                    work,
                    supplier: r.descriptor.contains("Supplier"),
                });
                Some(Value::Ref(Some(format!("{FUTURE_MARKER}{}", tasks.len()))))
            }
            _ => None,
        }
    }
}

impl NativeJar {
    /// The container marker FML hands this jar's container `id` (`entry_constructor_args`).
    fn container_marker(&self, id: &str) -> String {
        let version = if self.neoforge_id.as_deref() == Some(id) {
            self.neoforge_version.as_deref().unwrap_or("unknown")
        } else {
            "unknown"
        };
        format!("neoforge-mod-container/{id}/{version}")
    }

    /// Posts `stage`'s event to this jar's containers. Answers one line per post a listener ended
    /// by throwing (that container's post stops there, as `EventBus#post` rethrows).
    pub fn run_loading_stage(
        &self,
        stage: LoadingStage,
        host: &mut StdHost,
        budget: &ExecutionBudget,
    ) -> Vec<String> {
        let server = host.environment_type == "SERVER";
        let class = stage.event(server);
        let containers = self.neoforge_container_ids();
        let mut failures = Vec::new();
        if stage == LoadingStage::NetworkSetup {
            // `ModLoader.postEvent`: one event, phase by phase, every container within a phase.
            let event = event_marker(class, None);
            for priority in 0..5u8 {
                for id in &containers {
                    let listeners = self.posted_listeners(host, class, id, Some(priority));
                    if let Err(failure) = self.post(stage, id, &listeners, &event, host, budget) {
                        failures.push(failure);
                        return failures;
                    }
                }
            }
            return failures;
        }
        for id in &containers {
            let event = event_marker(class, Some(&self.container_marker(id)));
            let listeners = self.posted_listeners(host, class, id, None);
            if let Err(failure) = self.post(stage, id, &listeners, &event, host, budget) {
                failures.push(failure);
            }
        }
        failures
    }

    /// The listeners container `id`'s bus holds for `class` (of one priority, or all), in the
    /// order its `post` runs them: priority, then registration. Taken before the post runs: a
    /// listener added while it runs is not part of it.
    fn posted_listeners(
        &self,
        host: &StdHost,
        class: &str,
        id: &str,
        priority: Option<u8>,
    ) -> Vec<BusListener> {
        let bus = match crate::neoforge_bus::mod_bus(Some(id)) {
            Value::Ref(Some(bus)) => bus,
            _ => return Vec::new(),
        };
        let mut listeners: Vec<BusListener> = host
            .neoforge_bus
            .listeners
            .iter()
            .filter(|listener| {
                listener.class == class
                    && listener.bus == bus
                    && priority.is_none_or(|priority| listener.priority == priority)
            })
            .cloned()
            .collect();
        listeners.sort_by_key(|listener| listener.priority);
        listeners
    }

    /// Runs `listeners` with `event`; the first that throws ends the post.
    fn post(
        &self,
        stage: LoadingStage,
        id: &str,
        listeners: &[BusListener],
        event: &Value,
        host: &mut StdHost,
        budget: &ExecutionBudget,
    ) -> Result<(), String> {
        for listener in listeners {
            if let Err(error) =
                self.invoke_listener_callback(&listener.callback, vec![event.clone()], host, budget)
            {
                let target = match &listener.callback {
                    ListenerCallback::Indy { target, .. } => target,
                    ListenerCallback::Object { method, .. }
                    | ListenerCallback::Static { method } => method,
                };
                return Err(format!(
                    "{}: {id}: {} listener {}#{}: {error}",
                    stage.name(),
                    listener.class.rsplit('/').next().unwrap_or(&listener.class),
                    target.owner,
                    target.name
                ));
            }
        }
        Ok(())
    }

    /// `DeferredWorkQueue#runTasks`: every task this jar's containers queued, in enqueue order.
    /// Answers one line per task that threw; the rest still run.
    pub fn run_deferred_work(&self, host: &mut StdHost, budget: &ExecutionBudget) -> Vec<String> {
        let tasks = std::mem::take(&mut host.neoforge_bus.deferred);
        let mut failures = Vec::new();
        for task in tasks {
            let Some(callback) = self.work_callback(&task) else {
                // A `Runnable` the runtime cannot run: counted, never skipped silently.
                let kind = if task.supplier {
                    "Supplier"
                } else {
                    "Runnable"
                };
                *host
                    .unknown_host_calls
                    .entry(format!(
                        "net/neoforged/fml/DeferredWorkQueue#runTasks()V (a {kind} the runtime cannot run)"
                    ))
                    .or_default() += 1;
                continue;
            };
            if let Err(error) = self.invoke_listener_callback(&callback, Vec::new(), host, budget) {
                let container = task
                    .container
                    .strip_prefix("neoforge-mod-container/")
                    .and_then(|rest| rest.split('/').next())
                    .unwrap_or(&task.container);
                failures.push(format!("deferred work of {container}: {error}"));
            }
        }
        failures
    }

    /// How a queued `Runnable#run` or `Supplier#get` is reached: a lambda or method reference, or
    /// a jar object's method.
    fn work_callback(&self, task: &DeferredTask) -> Option<ListenerCallback> {
        let text = expect_ref(&task.work)?;
        if text.starts_with("@indy:") {
            let marker = crate::parse_callback_marker(text).ok()?;
            return Some(ListenerCallback::Indy {
                target: marker.target,
                captures: marker.captures,
            });
        }
        let (class, _) = text.split_once('#')?;
        let (name, descriptor) = if task.supplier {
            ("get", "()Ljava/lang/Object;")
        } else {
            ("run", "()V")
        };
        let method = crate::events::resolve_method(self, class, name, descriptor)?;
        Some(ListenerCallback::Object {
            receiver: task.work.clone(),
            method,
        })
    }
}

#[cfg(test)]
#[path = "neoforge_lifecycle_tests.rs"]
pub(crate) mod tests;
