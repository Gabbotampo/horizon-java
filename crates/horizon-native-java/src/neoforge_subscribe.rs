//! NeoForge's `IEventBus#register(Object)` (bus-8.0.5's `EventBus#register`): an object's
//! instance `@SubscribeEvent` methods, or a class's static ones, each registered as a listener with
//! the annotation's `priority` and `receiveCanceled`.
//!
//! `EventBus#register`, in order (`javap -c -p net.neoforged.bus.EventBus`):
//!
//! 1. @0–11: a target the bus already holds is ignored (`listeners.containsKey(target)`).
//! 2. @12–87: a `java.lang.reflect.Method` target registers that one method (FML's
//!    `@EventBusSubscriber` path); not reached by a jar's own code here, and counted.
//! 3. @88–118: the class is the target itself (a `Class`) or the target's class, and
//!    `checkSupertypes` refuses one whose superclasses or interfaces declare a `@SubscribeEvent`
//!    method ("Attempting to register a listener object of type ...").
//! 4. @124–255: each declared `@SubscribeEvent` method whose staticness matches the call
//!    (`register(Class)` takes static ones, `register(object)` instance ones) is registered;
//!    one that does not match throws ("Expected @SubscribeEvent method ... to be static" / "to NOT
//!    be static"), after the methods before it were registered.
//! 5. @258–284: none registered: "... has no @SubscribeEvent methods, but register was called
//!    anyway."
//!
//! `registerListener` @0–112 then refuses a method without exactly one parameter, a parameter that
//! is not an `Event`, and one the bus's class checker refuses (the message runs `bus` and the class
//! together: "...not valid for this busclass X", as NeoForge's own concatenation does), before
//! `addToListeners`' abstract and abstract-chain checks ([`crate::neoforge_bus`]). A listener on a
//! class the game fires runs through the game's dispatch like an `addListener` one; any other is
//! counted, named after its event.
//!
//! # Departures, named
//!
//! * **R1** a class's methods are taken in class-file order. NeoForge takes
//!   `Class#getDeclaredMethods`' order, which the JVM leaves unspecified; it only shows between two
//!   methods of one class on the same event and priority, or in which mismatched method a refusal
//!   names.
//! * **R2** the runtime carries a class literal and a `@Mod` class's own instance (the `this` FML
//!   constructs) as the same bare class name. A bare name is read as the `Class`, except for a
//!   `@Mod` class: there it is read as the instance (`NeoForge.EVENT_BUS.register(this)` in the
//!   constructor), unless the class declares only static `@SubscribeEvent` methods. Each reading
//!   gives NeoForge's answer for the call it assumes; the other call on a `@Mod` class is answered
//!   as if it were that one. A `ServiceLoader` provider, also a bare name, is read as its class.
//! * **R3** `Class#getSimpleName` in the "to NOT be static" message is the name after the last `$`
//!   without leading digits, not read from the `InnerClasses` attribute; a top-level class whose
//!   own name holds a `$` would be named wrongly.
//! * **R4** a parameter type the runtime holds no class file for is printed as `class X` in the
//!   "not an Event subtype" message, never `interface X`, and a type outside the JDK, the game,
//!   NeoForge and the jar (another mod's class, not linked) is counted instead of judged.

use crate::events::ListenerCallback;
use crate::neoforge_bus::{BusKind, BusListener, bus_kind, bus_name, class_text, illegal_argument};
use crate::{
    AnnotationValue, ClassFile, Host, Method, MethodRef, NativeError, Reader, Runtime, Value,
    expect_ref, read_pool, utf8,
};

const SUBSCRIBE_EVENT: &str = "Lnet/neoforged/bus/api/SubscribeEvent;";
const ACC_INTERFACE: u16 = 0x0200;

/// Where a `register` call's target leads.
enum Target {
    /// `register(SomeClass.class)`: its static methods.
    Class(String),
    /// `register(object)`: the object's class's instance methods.
    Object(String),
}

impl<H: Host> Runtime<'_, H> {
    /// `IEventBus#register(Object)` on a NeoForge bus.
    pub(crate) fn bus_register(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<(), NativeError> {
        let bus = bus_name(receiver);
        let Some(target) = args.first().cloned() else {
            self.count_bus_call(r, "no target");
            return Ok(());
        };
        // 1. A target this bus already holds.
        if self.host.neoforge_bus().is_some_and(|(state, _)| {
            state
                .registered
                .iter()
                .any(|(on, held)| *on == bus && *held == target)
        }) {
            return Ok(());
        }
        let Some(kind) = self.register_target(&target) else {
            self.count_bus_call(r, "target is not a jar class or object");
            return Ok(());
        };
        let (class, is_class) = match &kind {
            Target::Class(class) => (class.clone(), true),
            Target::Object(class) => (class.clone(), false),
        };
        // 3. `checkSupertypes(class, class)`.
        self.check_supertypes(&class, &class, 0)?;
        // 4. The declared `@SubscribeEvent` methods, in class-file order (departure R1).
        let methods: Vec<Method> = self
            .class(&class)
            .map(|file| file.methods.clone())
            .unwrap_or_default();
        let mut registered = 0;
        for method in &methods {
            let Some(annotation) = method
                .annotations
                .iter()
                .find(|annotation| annotation.descriptor == SUBSCRIBE_EVENT)
            else {
                continue;
            };
            if method.is_static() == is_class {
                self.register_method(r, &bus, &target, &class, method, &annotation.values)?;
                registered += 1;
                continue;
            }
            let text = self.method_text(&class, method);
            return Err(illegal_argument(if is_class {
                format!(
                    "Expected @SubscribeEvent method {text} to be static\nbecause register() was called with a class type.\nEither make the method static, or call register() with an instance of {}.\n",
                    self.type_text(&class)
                )
            } else {
                format!(
                    "Expected @SubscribeEvent method {text} to NOT be static\nbecause register() was called with an instance type.\nEither make the method non-static, or call register({}.class).\n",
                    simple_name(&class)
                )
            }));
        }
        // 5. Nothing registered.
        if registered == 0 {
            return Err(illegal_argument(format!(
                "{} has no @SubscribeEvent methods, but register was called anyway.\nThe event bus only recognizes listener methods that have the @SubscribeEvent annotation.\n",
                self.type_text(&class)
            )));
        }
        Ok(())
    }

    /// The class a `register` target names: a jar object's class, or a jar class itself.
    fn register_target(&self, target: &Value) -> Option<Target> {
        let text = expect_ref(target)?;
        if let Some((class, _)) = text.split_once('#')
            && self.class(class).is_some()
        {
            return Some(Target::Object(class.to_owned()));
        }
        // Departure R2: a bare jar class name is that `Class`, or a `@Mod` class's instance.
        let file = self.class(text)?;
        let entry = file
            .annotations
            .iter()
            .any(|annotation| annotation.descriptor == "Lnet/neoforged/fml/common/Mod;");
        let subscribers = || {
            file.methods.iter().filter(|method| {
                method
                    .annotations
                    .iter()
                    .any(|annotation| annotation.descriptor == SUBSCRIBE_EVENT)
            })
        };
        let only_static = subscribers().next().is_some() && subscribers().all(Method::is_static);
        Some(if entry && !only_static {
            Target::Object(text.clone())
        } else {
            Target::Class(text.clone())
        })
    }

    /// `checkSupertypes(registered, at)`: no superclass or interface of `registered` may declare a
    /// `@SubscribeEvent` method. A class the runtime holds no file for (the JDK's, the game's,
    /// NeoForge's) declares none.
    fn check_supertypes(
        &self,
        registered: &str,
        at: &str,
        depth: usize,
    ) -> Result<(), NativeError> {
        if at == "java/lang/Object" || depth > 32 {
            return Ok(());
        }
        let Some(file) = self.class(at) else {
            return Ok(());
        };
        if at != registered
            && let Some(method) = file.methods.iter().find(|method| {
                method
                    .annotations
                    .iter()
                    .any(|annotation| annotation.descriptor == SUBSCRIBE_EVENT)
            })
        {
            return Err(illegal_argument(format!(
                "Attempting to register a listener object of type {},\nhowever its supertype {} has a @SubscribeEvent method: {}.\nThis is not allowed! Only the listener object can have @SubscribeEvent methods.\n",
                self.type_text(registered),
                self.type_text(at),
                self.method_text(at, method)
            )));
        }
        let (superclass, interfaces) = (file.super_name.clone(), file.interface_names());
        self.check_supertypes(registered, &superclass, depth + 1)?;
        for interface in interfaces {
            self.check_supertypes(registered, &interface, depth + 1)?;
        }
        Ok(())
    }

    /// `registerListener(target, method)` and `register(eventType, target, method)`.
    fn register_method(
        &mut self,
        r: &MethodRef,
        bus: &str,
        target: &Value,
        class: &str,
        method: &Method,
        values: &std::collections::BTreeMap<String, AnnotationValue>,
    ) -> Result<(), NativeError> {
        let parameters = crate::descriptor_arguments(&method.descriptor);
        if parameters.len() != 1 {
            return Err(illegal_argument(format!(
                "Method {} has @SubscribeEvent annotation. It has {} arguments, but event handler methods require a single argument only.",
                self.method_text(class, method),
                parameters.len()
            )));
        }
        let event = match parameters[0]
            .strip_prefix('L')
            .and_then(|rest| rest.strip_suffix(';'))
        {
            Some(event) => event.to_owned(),
            None => {
                return Err(illegal_argument(format!(
                    "Method {} has @SubscribeEvent annotation, but takes an argument that is not an Event subtype : {}",
                    self.method_text(class, method),
                    value_of_class(&parameters[0])
                )));
            }
        };
        let Some(facts) = self.event_facts(&event) else {
            if !self.surely_not_an_event(&event) {
                self.count_bus_call(r, &format!("{event} is not a known class"));
                return Ok(());
            }
            return Err(illegal_argument(format!(
                "Method {} has @SubscribeEvent annotation, but takes an argument that is not an Event subtype : {}",
                self.method_text(class, method),
                self.type_text(&event)
            )));
        };
        // The bus's class checker (`registerListener` @67–106).
        let refused = match bus_kind(Some(&Value::Ref(Some(bus.to_owned())))) {
            BusKind::Mod => !facts.mod_bus,
            BusKind::Game => facts.mod_bus,
            BusKind::Unknown => false,
        };
        if refused {
            return Err(illegal_argument(format!(
                "Method {} has @SubscribeEvent annotation, but takes an argument that is not valid for this bus{}",
                self.method_text(class, method),
                class_text(&event)
            )));
        }
        // `addToListeners` and `getListenerList`.
        if facts.is_abstract {
            return Err(illegal_argument(format!(
                "Cannot register listeners for abstract {}. Register a listener to one of its subclasses instead!",
                class_text(&event)
            )));
        }
        self.validate_abstract_chain(&facts.superclass)?;
        // The bus now holds the target, whether or not the game fires the event.
        if let Some((state, _)) = self.host.neoforge_bus()
            && !state
                .registered
                .iter()
                .any(|(on, held)| on == bus && held == target)
        {
            state.registered.push((bus.to_owned(), target.clone()));
        }
        if crate::events::bus_event(&event).is_none() {
            self.count_bus_call(r, &format!("no game event fires {event}"));
            return Ok(());
        }
        let priority = match values.get("priority") {
            Some(AnnotationValue::String(text)) => {
                crate::events::order::priority_of(text).unwrap_or(2)
            }
            _ => 2,
        };
        let asked = matches!(
            values.get("receiveCanceled"),
            Some(AnnotationValue::Bool(true))
        );
        let handler = MethodRef {
            owner: class.to_owned(),
            name: method.name.clone(),
            descriptor: method.descriptor.clone(),
        };
        let listener = BusListener {
            bus: bus.to_owned(),
            class: event,
            priority,
            // `SubscribeEventListener#invoke` checks `receiveCanceled`; on a class that is not
            // `ICancellableEvent`, `ListenerList.unwrapListeners` drops that check.
            receive_canceled: asked || !facts.cancellable,
            consumer: target.clone(),
            callback: if method.is_static() {
                ListenerCallback::Static { method: handler }
            } else {
                ListenerCallback::Object {
                    receiver: target.clone(),
                    method: handler,
                }
            },
        };
        if let Some((state, _)) = self.host.neoforge_bus() {
            state.listeners.push(listener);
        }
        Ok(())
    }

    /// Whether `Event.class.isAssignableFrom(class)` is surely false: the chain of jar classes
    /// ends in the JDK's, the game's or NeoForge's own classes, none of which is an event outside
    /// the table (departure R4 for the rest).
    fn surely_not_an_event(&self, class: &str) -> bool {
        let mut at = class.to_owned();
        for _ in 0..32 {
            match self.class(&at) {
                Some(file) => at = file.super_name.clone(),
                None => {
                    return [
                        "java/",
                        "javax/",
                        "jdk/",
                        "sun/",
                        "net/minecraft/",
                        "com/mojang/",
                        "net/neoforged/",
                    ]
                    .iter()
                    .any(|prefix| at.starts_with(prefix));
                }
            }
        }
        false
    }

    /// `Class#toString` for a class the runtime may hold: `interface X` for a jar interface,
    /// otherwise `class X` (departure R4).
    fn type_text(&self, class: &str) -> String {
        let interface = self
            .class(class)
            .and_then(crate::jdk_model::class_access_flags)
            .is_some_and(|access| access & ACC_INTERFACE != 0);
        if interface {
            format!("interface {}", class.replace('/', "."))
        } else {
            class_text(class)
        }
    }

    /// `Method#toString`: modifiers (`Modifier.methodModifiers()`, with `default` for an
    /// interface's default method), the return type, the declaring class and name, the parameter
    /// types joined by `,`, and `throws` with the `Exceptions` attribute's classes.
    fn method_text(&self, class: &str, method: &Method) -> String {
        let file = self.class(class);
        let interface = file
            .and_then(crate::jdk_model::class_access_flags)
            .is_some_and(|access| access & ACC_INTERFACE != 0);
        let modifiers = method.access & 0x0D3F;
        let default = interface && method.access & (0x0400 | 0x0001 | 0x0008) == 0x0001;
        let words = |mask: u16| -> Vec<&'static str> {
            [
                (0x0001, "public"),
                (0x0004, "protected"),
                (0x0002, "private"),
                (0x0400, "abstract"),
                (0x0008, "static"),
                (0x0010, "final"),
                (0x0020, "synchronized"),
                (0x0100, "native"),
                (0x0800, "strictfp"),
            ]
            .into_iter()
            .filter(|(bit, _)| mask & bit != 0)
            .map(|(_, word)| word)
            .collect()
        };
        let mut text = String::new();
        if !default {
            if modifiers != 0 {
                text.push_str(&words(modifiers).join(" "));
                text.push(' ');
            }
        } else {
            let access = modifiers & 0x0007;
            if access != 0 {
                text.push_str(&words(access).join(" "));
                text.push(' ');
            }
            text.push_str("default ");
            let rest = modifiers & !0x0007;
            if rest != 0 {
                text.push_str(&words(rest).join(" "));
                text.push(' ');
            }
        }
        let returns = crate::return_desc(&method.descriptor);
        let parameters: Vec<String> = crate::descriptor_arguments(&method.descriptor)
            .iter()
            .map(|parameter| type_name(parameter))
            .collect();
        text.push_str(&format!(
            "{} {}.{}({})",
            type_name(returns),
            class.replace('/', "."),
            method.name,
            parameters.join(",")
        ));
        let thrown = file
            .and_then(|file| method_exceptions(file, &method.name, &method.descriptor))
            .unwrap_or_default();
        if !thrown.is_empty() {
            let names: Vec<String> = thrown.iter().map(|name| name.replace('/', ".")).collect();
            text.push_str(" throws ");
            text.push_str(&names.join(","));
        }
        text
    }
}

/// `Class#getTypeName` of a field descriptor: `int`, `java.lang.String`, `java.lang.String[]`.
fn type_name(descriptor: &str) -> String {
    if let Some(component) = descriptor.strip_prefix('[') {
        return format!("{}[]", type_name(component));
    }
    match descriptor {
        "Z" => "boolean".into(),
        "B" => "byte".into(),
        "C" => "char".into(),
        "S" => "short".into(),
        "I" => "int".into(),
        "J" => "long".into(),
        "F" => "float".into(),
        "D" => "double".into(),
        "V" => "void".into(),
        other => other
            .strip_prefix('L')
            .and_then(|name| name.strip_suffix(';'))
            .unwrap_or(other)
            .replace('/', "."),
    }
}

/// `String.valueOf(Class)` for a primitive (`int`) or array (`class [Ljava.lang.String;`) type.
fn value_of_class(descriptor: &str) -> String {
    if descriptor.starts_with('[') {
        format!("class {}", descriptor.replace('/', "."))
    } else {
        type_name(descriptor)
    }
}

/// `Class#getSimpleName`, from the binary name (departure R3).
fn simple_name(class: &str) -> &str {
    let name = class.rsplit('/').next().unwrap_or(class);
    match name.rsplit_once('$') {
        Some((_, inner)) => inner.trim_start_matches(|c: char| c.is_ascii_digit()),
        None => name,
    }
}

/// The classes one method's `Exceptions` attribute names, read back from the class bytes.
fn method_exceptions(file: &ClassFile, name: &str, descriptor: &str) -> Option<Vec<String>> {
    let mut r = Reader::new(&file.raw);
    if r.u4().ok()? != 0xCAFE_BABE {
        return None;
    }
    r.skip(4).ok()?;
    let pool = read_pool(&mut r).ok()?;
    r.skip(6).ok()?;
    let interfaces = r.u2().ok()? as usize;
    r.skip(interfaces * 2).ok()?;
    let fields = r.u2().ok()?;
    for _ in 0..fields {
        r.skip(6).ok()?;
        skip_attributes(&mut r)?;
    }
    let methods = r.u2().ok()?;
    for _ in 0..methods {
        r.skip(2).ok()?;
        let this_name = utf8(&pool, r.u2().ok()?).ok()?;
        let this_descriptor = utf8(&pool, r.u2().ok()?).ok()?;
        if this_name != name || this_descriptor != descriptor {
            skip_attributes(&mut r)?;
            continue;
        }
        let count = r.u2().ok()?;
        let mut thrown = Vec::new();
        for _ in 0..count {
            let attribute = utf8(&pool, r.u2().ok()?).ok()?;
            let length = r.u4().ok()? as usize;
            if attribute != "Exceptions" {
                r.skip(length).ok()?;
                continue;
            }
            let classes = r.u2().ok()?;
            for _ in 0..classes {
                thrown.push(crate::class_name(&pool, r.u2().ok()?).ok()?);
            }
        }
        return Some(thrown);
    }
    None
}

fn skip_attributes(r: &mut Reader<'_>) -> Option<()> {
    let count = r.u2().ok()?;
    for _ in 0..count {
        r.skip(2).ok()?;
        let length = r.u4().ok()? as usize;
        r.skip(length).ok()?;
    }
    Some(())
}
