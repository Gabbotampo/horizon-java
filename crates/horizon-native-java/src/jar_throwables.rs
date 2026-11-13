//! Throwables a jar defines, and the identity of every thrown object.
//!
//! A jar class that extends a JDK exception (`class MyCCE extends
//! ClassCastException`) is a `class#n` object whose name does not end in
//! `Exception`/`Error`, so the name-based check `athrow` used called it a
//! plain `java.lang.Throwable`: no `catch (ClassCastException)` matched it,
//! and the handler that finally caught it received a fresh object of the
//! wrong class with no message. Java throws the object itself.
//!
//! * [`Runtime::throwable_class`] is the thrown object's own class whenever
//!   its superclass chain reaches a JDK throwable, so the frame's exception
//!   table matches it as the JVM does, and [`throwable_is_instance`] walks the
//!   JDK part of that chain for `instanceof`/`checkcast`.
//! * A throw that leaves its frame is a `NativeError::UncaughtException`,
//!   which carries the class, the message and the **origin** (its `frame`):
//!   the throwing instruction (`Class#method +pc`) for an `athrow`, `throw of
//!   <class>` for a throw the host makes of a jar's object
//!   (`Optional.orElseThrow(supplier)`), followed by this throw's own serial
//!   number after [`TAG_SEPARATOR`]. The origin travels unchanged while the
//!   error unwinds. [`Runtime::throw_object`] files the thrown object under it,
//!   and only the handler that catches an error with that exact origin gets
//!   the same object back ([`Runtime::caught_object`]): identity, fields and
//!   cause survive.
//!
//! The serial makes every throw's key unique, so no other error can ever
//! carry it: not a host exception raised at the same pc (in an overload, or
//! `throw null` at the `athrow` itself), not a later throw from the same
//! instruction, not an earlier execution's uncaught throw. The error's
//! `Display` hides the serial, so diagnostics read as before. The table is
//! thread-local (one execution runs on one thread), bounded, and emptied
//! when an execution starts ([`start_execution`]).

use super::*;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

/// Separates an origin from its throw's serial number in an error's `frame`.
pub(crate) const TAG_SEPARATOR: char = '\u{1f}';

/// The part of an error's `frame` a diagnostic prints (without the serial).
pub(crate) fn printed_frame(frame: &str) -> &str {
    frame.split(TAG_SEPARATOR).next().unwrap_or(frame)
}

/// Forget every filed throw: an execution is starting on this thread, and
/// nothing an earlier one threw can be caught in it.
pub(crate) fn start_execution() {
    FILED.with(|filed| filed.borrow_mut().clear());
}

/// Thrown objects kept at once; the oldest is dropped beyond this.
const MAX_FILED: usize = 256;

/// One filed throw: its origin, class, message and object.
type Filed = (String, String, Option<String>, Value);

thread_local! {
    static FILED: RefCell<VecDeque<Filed>> = const { RefCell::new(VecDeque::new()) };
    static NEXT_TAG: Cell<u64> = const { Cell::new(0) };
}

/// `origin` with a serial number no other throw on this thread carries.
fn unique_origin(origin: &str) -> String {
    let tag = NEXT_TAG.with(|next| {
        let tag = next.get();
        next.set(tag.wrapping_add(1));
        tag
    });
    format!("{origin}{TAG_SEPARATOR}{tag}")
}

/// File `value` as the object thrown from `origin`; a newer throw from the
/// same origin replaces an older one.
fn file(origin: &str, class: &str, message: &Option<String>, value: &Value) {
    FILED.with(|filed| {
        let mut filed = filed.borrow_mut();
        filed.retain(|(key, ..)| key != origin);
        if filed.len() >= MAX_FILED {
            filed.pop_front();
        }
        filed.push_back((
            origin.to_owned(),
            class.to_owned(),
            message.clone(),
            value.clone(),
        ));
    });
}

/// Whether the JDK throwable `class` is `wanted` or extends it, through the
/// JDK's throwable hierarchy (`IllegalArgumentException` → `RuntimeException`
/// → `Exception` → `Throwable` → `Object`); `Throwable` is `Serializable`.
pub(crate) fn throwable_is_instance(class: &str, wanted: &str) -> bool {
    let mut current = class.to_owned();
    for _ in 0..MAX_SUPERCLASS_WALK {
        if current == wanted {
            return true;
        }
        if current == "java/lang/Throwable" {
            return matches!(wanted, "java/io/Serializable" | "java/lang/Object");
        }
        match jdk_exception_superclass(&current) {
            Some(parent) => current = parent.to_owned(),
            None => return false,
        }
    }
    false
}

impl<H: Host> Runtime<'_, H> {
    /// The Java class of a thrown value: a jar class whose superclass chain
    /// reaches a JDK throwable is itself; anything else keeps the name-based
    /// answer (`java/lang/Throwable` when the marker is not a throwable).
    pub(crate) fn throwable_class(&self, value: &Value) -> String {
        if let Some(class) =
            expect_ref(value).and_then(|marker| jvm_dispatch::constructed_object_class(marker))
            && !is_throwable_owner(class)
            && self.jar_throwable(class)
        {
            return class.to_owned();
        }
        thrown_class(value)
    }

    /// The message a thrown value's `Throwable` constructor recorded, for a
    /// JDK throwable marker or a jar throwable object.
    pub(crate) fn thrown_text(&self, value: &Value) -> Option<String> {
        let marker = expect_ref(value)?;
        let jar = jvm_dispatch::constructed_object_class(marker)
            .is_some_and(|class| self.jar_throwable(class));
        if jar {
            return self.host.throwable_message(marker);
        }
        thrown_message(value, self.host)
    }

    /// `throw value`, as an error carrying `origin` (the `athrow`
    /// instruction) or a fresh tag; the object is filed under it.
    pub(crate) fn throw_object(&mut self, value: &Value, origin: Option<String>) -> NativeError {
        let class = self.throwable_class(value);
        let message = self.thrown_text(value);
        let origin = unique_origin(
            &origin.unwrap_or_else(|| format!("throw of {}", class.replace('/', "."))),
        );
        file(&origin, &class, &message, value);
        NativeError::UncaughtException {
            class,
            message,
            frame: Some(origin),
        }
    }

    /// The object a handler receives for an error of `class` with `message`
    /// whose origin is `frame`: the filed object of that throw, else `None`.
    pub(crate) fn caught_object(
        &self,
        class: &str,
        message: &Option<String>,
        frame: Option<&str>,
    ) -> Option<Value> {
        let frame = frame?;
        FILED.with(|filed| {
            let mut filed = filed.borrow_mut();
            let index = filed.iter().position(|(origin, thrown, text, _)| {
                origin == frame && thrown == class && text == message
            })?;
            filed.remove(index).map(|(.., value)| value)
        })
    }

    /// `(value instanceof target, value's class)` when `target` is a JDK
    /// throwable class and `value` a throwable object (a jar exception or a
    /// JDK one); `None` otherwise.
    pub(crate) fn throwable_cast(&self, value: &Value, target: &str) -> Option<(bool, String)> {
        if self.class(target).is_some()
            || (target != "java/lang/Throwable" && jdk_exception_superclass(target).is_none())
        {
            return None;
        }
        let class = jvm_dispatch::constructed_object_class(expect_ref(value)?)?;
        let root = if self.class(class).is_some() {
            self.throwable_root(class)?
        } else if is_throwable_owner(class) {
            class.to_owned()
        } else {
            return None;
        };
        // A JDK class the hierarchy table does not know has no verdict here.
        if root != "java/lang/Throwable" && jdk_exception_superclass(&root).is_none() {
            return None;
        }
        Some((throwable_is_instance(&root, target), class.to_owned()))
    }

    /// Whether the jar class `class` extends a JDK throwable.
    pub(crate) fn jar_throwable(&self, class: &str) -> bool {
        self.throwable_root(class).is_some()
    }

    /// The JDK throwable class a jar class's superclass chain reaches.
    pub(crate) fn throwable_root(&self, class: &str) -> Option<String> {
        let mut current = class.to_owned();
        for _ in 0..MAX_SUPERCLASS_WALK {
            let Some(file) = self.class(&current) else {
                return (current != class && is_throwable_owner(&current)).then_some(current);
            };
            if file.super_name.is_empty() {
                return None;
            }
            current = file.super_name.clone();
        }
        None
    }

    /// `Throwable.toString`, `getLocalizedMessage` and `getMessage` on a jar
    /// throwable whose classes do not override the member itself (an
    /// override already ran as the receiver's own body). JDK 25:
    /// `toString` is `getClass().getName()`, then `": " +
    /// getLocalizedMessage()` when that is not null; `getLocalizedMessage` is
    /// `getMessage()`; each of those calls dispatches to the jar's overrides.
    pub(crate) fn invoke_jar_throwable(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
    ) -> Result<Option<Value>, NativeError> {
        if !matches!(opcode, 0xb6 | 0xb9) {
            return Ok(None);
        }
        let Some(receiver) = receiver else {
            return Ok(None);
        };
        let Some(class) = expect_ref(receiver)
            .and_then(|marker| jvm_dispatch::constructed_object_class(marker))
            .filter(|class| self.jar_throwable(class))
            .map(str::to_owned)
        else {
            return Ok(None);
        };
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("toString", "()Ljava/lang/String;") => {
                let name = class.replace('/', ".");
                let message = self.throwable_member(receiver, "getLocalizedMessage")?;
                Ok(Some(Value::Ref(Some(match message {
                    Value::Ref(Some(text)) => format!("{name}: {text}"),
                    _ => name,
                }))))
            }
            ("getLocalizedMessage", "()Ljava/lang/String;") => {
                self.throwable_member(receiver, "getMessage").map(Some)
            }
            ("getMessage", "()Ljava/lang/String;") => {
                let marker = expect_ref(receiver).cloned().unwrap_or_default();
                Ok(Some(Value::Ref(self.host.throwable_message(&marker))))
            }
            _ => Ok(None),
        }
    }

    /// `this.<name>()` on a jar throwable: its own override, else the JDK
    /// body above.
    fn throwable_member(&mut self, receiver: &Value, name: &str) -> Result<Value, NativeError> {
        let r = MethodRef {
            owner: "java/lang/Throwable".into(),
            name: name.into(),
            descriptor: "()Ljava/lang/String;".into(),
        };
        if let Some(owner) = self.jvm_selected_jar_body(&r, 0xb6, Some(receiver))? {
            return self.invoke(&owner, name, &r.descriptor, Some(receiver.clone()), &[]);
        }
        Ok(self
            .invoke_jar_throwable(&r, 0xb6, Some(receiver))?
            .unwrap_or(Value::Ref(None)))
    }
}
