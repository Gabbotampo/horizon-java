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
//!   table matches it as the JVM does (`exception_is_subtype` already walks a
//!   jar class's superclasses).
//! * A throw that leaves its frame is a `NativeError::UncaughtException`,
//!   which carries the class and message but not the object. The object is
//!   kept here as the one **in flight**, and the handler that catches that
//!   class with that message receives the same object back
//!   (`Runtime::caught_throwable`): its identity, its fields and its cause
//!   survive the frames between `athrow` and `catch`. A throw the host or a
//!   JDK body raised has no object, and a handler still gets a new one.
//!
//! One execution runs on one thread, so the in-flight slot is thread-local;
//! each `athrow` replaces it.

use super::*;
use std::cell::RefCell;

thread_local! {
    /// The object the last uncaught `athrow` threw: `(class, message, value)`.
    static IN_FLIGHT: RefCell<Option<(String, Option<String>, Value)>> = const { RefCell::new(None) };
}

/// Remember `value` as the object an uncaught `athrow` of `class` with
/// `message` threw.
pub(crate) fn note_in_flight(class: &str, message: &Option<String>, value: &Value) {
    IN_FLIGHT.with(|slot| {
        *slot.borrow_mut() = Some((class.to_owned(), message.clone(), value.clone()));
    });
}

/// The in-flight object, when the throw a handler is catching is that one
/// (same class, same message).
pub(crate) fn take_in_flight(class: &str, message: &Option<String>) -> Option<Value> {
    IN_FLIGHT.with(|slot| {
        let mut slot = slot.borrow_mut();
        let matches = slot
            .as_ref()
            .is_some_and(|(thrown, text, _)| thrown == class && text == message);
        if matches {
            slot.take().map(|(_, _, value)| value)
        } else {
            None
        }
    })
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

    /// Whether the jar class `class` extends a JDK throwable.
    fn jar_throwable(&self, class: &str) -> bool {
        let mut current = class.to_owned();
        for _ in 0..MAX_SUPERCLASS_WALK {
            let Some(file) = self.class(&current) else {
                return current != class && is_throwable_owner(&current);
            };
            if file.super_name.is_empty() {
                return false;
            }
            current = file.super_name.clone();
        }
        false
    }
}
