//! FML's `@EventBusSubscriber` injection (`loader-12.0.12`, `AutomaticEventSubscriber.inject`),
//! through the same validated `register` path `IEventBus#register` takes (`neoforge_subscribe.rs`).
//!
//! # When it runs
//!
//! `FMLModContainer#constructMod` (`javap -c -p`) builds every `@Mod` class of the container
//! (@0–310) and then, still inside the construction task, calls `AutomaticEventSubscriber.inject`
//! (@388–395). A constructor that throws ends the task before the injection (@313–364), so a
//! container whose `@Mod` constructor failed never injects. `ModLoader.lambda$constructMods$0`
//! then posts `FMLConstructModEvent` to the container (`neoforge_lifecycle.rs`).
//!
//! # What `inject` does, in order (`lambda$inject$4`)
//!
//! For every class carrying `@net.neoforged.fml.common.EventBusSubscriber` (26.3's only such
//! annotation: the older `Mod$EventBusSubscriber` is not in loader-12.0.12, so FML never reads it):
//!
//! 1. @0–58: the annotation's `modid`, else the id of the `@Mod` annotation on the same class, else
//!    the container's own id, must equal the container's id;
//! 2. @63–72: the annotation's `value` (default `{CLIENT, DEDICATED_SERVER}`, `getSides`) must hold
//!    the running `Dist`;
//! 3. @96–107: `Class.forName(name, true, loader)`: the class is initialised;
//! 4. @114–344: each declared method with `@SubscribeEvent`, in `getDeclaredMethods` order:
//!    * not static: `IllegalArgumentException("Method " + m + " annotated with @SubscribeEvent is
//!      not static")` (@156–182);
//!    * not exactly one parameter, or one that is not an `Event`: `IllegalArgumentException("Method
//!      " + m + " annotated with @SubscribeEvent must have only one parameter that is an Event
//!      subtype")` (@185–224);
//!    * an `IModBusEvent` goes to the container's bus (@227–300), anything else to the game bus
//!      (@308–336), each through `IEventBus#register(Object)` with the `Method` itself. bus-8.0.5's
//!      `EventBus#register` @12–87 takes a `Method` straight to `registerListener(method, method)`:
//!      the parameter, class-checker, abstract and abstract-chain checks of
//!      [`Runtime::register_method`].
//! 5. @95–381: any exception is rethrown as `new RuntimeException(e)`, which fails the container
//!    (`FMLModContainer#constructMod` @416–466).
//!
//! A listener on an event the game or the loader's lifecycle fires is registered like any other; a
//! listener on any other event is counted under that event, named after `inject`, as every other
//! registration path counts one.
//!
//! # Departures, named
//!
//! * **I1** subscriber classes are taken in class-name order. FML takes them in
//!   `ModFileScanData#getAnnotations` order, a `HashSet`'s; it only shows in which class a refusal
//!   names when two classes of one container are both refused, and in the registration order of
//!   two subscriber methods in different classes on the same event and priority.
//! * **I2** methods are taken in class-file order (as #96's **R1**).
//! * **I3** the containers are the jar's metadata id and its `@Mod` classes' ids. A second mod a
//!   `neoforge.mods.toml` lists with no `@Mod` class gets no container here, so its subscribers
//!   are not injected.

use crate::neoforge_bus::{GAME_BUS, illegal_argument, mod_bus};
use crate::{AnnotationValue, ExecutionBudget, MethodRef, NativeError, NativeJar, StdHost, Value};

const EVENT_BUS_SUBSCRIBER: &str = "Lnet/neoforged/fml/common/EventBusSubscriber;";
const MOD: &str = "Lnet/neoforged/fml/common/Mod;";
const SUBSCRIBE_EVENT: &str = "Lnet/neoforged/bus/api/SubscribeEvent;";

/// The call NeoForge makes, used to name what `inject` counts.
fn inject_ref() -> MethodRef {
    MethodRef {
        owner: "net/neoforged/fml/javafmlmod/AutomaticEventSubscriber".into(),
        name: "inject".into(),
        descriptor: "(Lnet/neoforged/fml/ModContainer;Lnet/neoforged/neoforgespi/language/ModFileScanData;Ljava/lang/Module;)V".into(),
    }
}

/// A NeoForge annotation's `String` member.
fn string_member<'a>(annotation: &'a crate::Annotation, name: &str) -> Option<&'a str> {
    match annotation.values.get(name)? {
        AnnotationValue::String(text) => Some(text),
        _ => None,
    }
}

/// `getSides(value)`: the `Dist` names the annotation's `value` holds, `None` for its default (both).
fn sides(annotation: &crate::Annotation) -> Option<Vec<String>> {
    fn names(value: &AnnotationValue, out: &mut Vec<String>) {
        match value {
            AnnotationValue::String(text) => {
                out.push(text.rsplit('.').next().unwrap_or(text).to_owned())
            }
            AnnotationValue::Array(values) => values.iter().for_each(|value| names(value, out)),
            _ => {}
        }
    }
    let value = annotation.values.get("value")?;
    let mut out = Vec::new();
    names(value, &mut out);
    Some(out)
}

impl NativeJar {
    /// The mod containers FML builds for this jar, in order: the metadata's mod, then each `@Mod`
    /// class's id (departure **I3**).
    pub fn neoforge_container_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.neoforge_id.iter().cloned().collect();
        for class in &self.neoforge_entrypoints {
            if let Some(id) = self.neoforge_mod_id(class)
                && !ids.iter().any(|known| known == id)
            {
                ids.push(id.to_owned());
            }
        }
        ids
    }

    /// The id of a class's NeoForge `@Mod` annotation.
    pub(crate) fn neoforge_mod_id(&self, class: &str) -> Option<&str> {
        let annotation = self
            .classes
            .get(class)?
            .annotations
            .iter()
            .find(|annotation| annotation.descriptor == MOD)?;
        string_member(annotation, "value").filter(|id| !id.is_empty() && !id.contains('/'))
    }

    /// `AutomaticEventSubscriber.inject` for every container of this jar whose `@Mod` classes all
    /// constructed (`failed_classes` names the entry classes whose constructor failed). Answers
    /// one line per container whose injection threw, as FML fails that container.
    pub fn inject_event_subscribers(
        &self,
        host: &mut StdHost,
        budget: &ExecutionBudget,
        failed_classes: &[&str],
    ) -> Vec<String> {
        let mut failures = Vec::new();
        for id in self.neoforge_container_ids() {
            let constructor_failed = failed_classes
                .iter()
                .any(|class| self.neoforge_mod_id(&class.replace('.', "/")) == Some(id.as_str()));
            if constructor_failed {
                continue;
            }
            let ran = self
                .with_resident_runtime(host, budget, |runtime| runtime.inject_container(self, &id));
            if let Ok(Err(error)) | Err(error) = ran {
                failures.push(format!("{id}: @EventBusSubscriber injection: {error}"));
            }
        }
        failures
    }
}

impl crate::Runtime<'_, StdHost> {
    /// `lambda$inject$4` for every subscriber class of `container`.
    pub(crate) fn inject_container(
        &mut self,
        jar: &NativeJar,
        container: &str,
    ) -> Result<(), NativeError> {
        let dist = if self.host.environment_type == "SERVER" {
            "DEDICATED_SERVER"
        } else {
            "CLIENT"
        };
        let subscribers: Vec<(String, crate::Annotation)> = jar
            .classes
            .iter()
            .filter_map(|(name, file)| {
                let annotation = file
                    .annotations
                    .iter()
                    .find(|annotation| annotation.descriptor == EVENT_BUS_SUBSCRIBER)?;
                Some((name.clone(), annotation.clone()))
            })
            .collect();
        for (class, annotation) in subscribers {
            // 1. The mod id.
            let modid = string_member(&annotation, "modid")
                .or_else(|| jar.neoforge_mod_id(&class))
                .unwrap_or(container);
            if modid != container {
                continue;
            }
            // 2. The side.
            if sides(&annotation).is_some_and(|sides| !sides.iter().any(|side| side == dist)) {
                continue;
            }
            self.inject_class(&class, &container_bus(container))
                .map_err(wrap)?;
        }
        Ok(())
    }

    /// Steps 3 and 4 for one class.
    fn inject_class(&mut self, class: &str, container_bus: &str) -> Result<(), NativeError> {
        self.ensure_initialized(class)?;
        let methods: Vec<crate::Method> = self
            .class(class)
            .map(|file| file.methods.clone())
            .unwrap_or_default();
        let r = inject_ref();
        for method in &methods {
            let Some(annotation) = method
                .annotations
                .iter()
                .find(|annotation| annotation.descriptor == SUBSCRIBE_EVENT)
            else {
                continue;
            };
            if !method.is_static() {
                return Err(illegal_argument(format!(
                    "Method {} annotated with @SubscribeEvent is not static",
                    self.method_text(class, method)
                )));
            }
            let not_an_event = || {
                illegal_argument(format!(
                    "Method {} annotated with @SubscribeEvent must have only one parameter that is an Event subtype",
                    self.method_text(class, method)
                ))
            };
            let parameters = crate::descriptor_arguments(&method.descriptor);
            let [parameter] = parameters.as_slice() else {
                return Err(not_an_event());
            };
            let Some(event) = parameter
                .strip_prefix('L')
                .and_then(|rest| rest.strip_suffix(';'))
            else {
                return Err(not_an_event());
            };
            let Some(facts) = self.event_facts(event) else {
                if self.surely_not_an_event(event) {
                    return Err(not_an_event());
                }
                // A type the runtime cannot judge (another mod's class): counted, as #96's R4.
                self.count_bus_call(&r, &format!("{event} is not a known class"));
                continue;
            };
            let bus = if facts.mod_bus {
                container_bus.to_owned()
            } else {
                GAME_BUS.to_owned()
            };
            let target = Value::Ref(Some(format!(
                "java/lang/reflect/Method#{class}.{}{}",
                method.name, method.descriptor
            )));
            self.register_method(&r, &bus, &target, class, method, &annotation.values)?;
        }
        Ok(())
    }
}

/// `new RuntimeException(e)` around a Java exception the injection threw (@372–381); the message
/// is the cause's `toString`. An error of the runtime itself (a limit, not a Java exception) is
/// kept as it is.
fn wrap(error: NativeError) -> NativeError {
    match error {
        NativeError::UncaughtException {
            class,
            message,
            frame,
        } => {
            let cause = class.replace('/', ".");
            NativeError::UncaughtException {
                class: "java/lang/RuntimeException".into(),
                message: Some(match message {
                    Some(message) => format!("{cause}: {message}"),
                    None => cause,
                }),
                frame,
            }
        }
        other => other,
    }
}

/// The mod bus marker of `container`, as `FMLModContainer#getEventBus` answers it.
fn container_bus(container: &str) -> String {
    match mod_bus(Some(container)) {
        Value::Ref(Some(marker)) => marker,
        _ => unreachable!("mod_bus is a marker"),
    }
}
