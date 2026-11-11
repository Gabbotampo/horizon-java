//! `checkcast` to a class the runtime holds (the jar's own or a linked
//! library's), checked against the class hierarchy it knows.
//!
//! The interpreter used to keep every reference unchanged, so
//! `(Impl) (Object) "str"` passed and the next call ran `Impl`'s body on a
//! string, where the JVM throws `ClassCastException` (JVMS §6.5 `checkcast`).
//! Now the cast has three outcomes:
//!
//! - **passes**: `null`, or a value whose class the runtime can follow to the
//!   target;
//! - **throws** a catchable `java/lang/ClassCastException`: a value that cannot
//!   be the target (a jar object of an unrelated jar class, a boxed number,
//!   a collection, an optional, a JDK enum ...);
//! - **counted**: a value whose class the runtime cannot follow (a game
//!   object, which a Mixin may make implement a jar interface; an opaque host
//!   marker; a string the untyped runtime cannot tell from a marker). The cast
//!   goes to the host as the call `Target#<checkcast>(Object)`, so it is
//!   counted like an unmodelled call (and refused by a strict host) instead of
//!   passing silently.
//!
//! Named departures:
//! - the exception message is `class A cannot be cast to class B` without the
//!   JVM's module/loader clause (`(A is in module java.base of loader
//!   'bootstrap'; B is in unnamed module of loader 'app')`);
//! - a lambda (an `@indy:` marker) cast to a jar *interface* passes uncounted:
//!   the marker does not record which functional interface the lambda
//!   implements, and javac only emits such a cast after an erased generic read.
//!   A lambda cast to a jar *class* still throws.

use super::*;

/// What the runtime knows about `value instanceof target`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Yes,
    No,
    Unknown,
}

const ACC_INTERFACE: u16 = 0x0200;

/// The class file's own access flags, read back from its bytes (the parsed
/// model keeps none). `None` for a class built in memory without bytes.
fn class_access(file: &ClassFile) -> Option<u16> {
    if file.raw.is_empty() {
        return None;
    }
    let mut reader = Reader::new(&file.raw);
    reader.skip(8).ok()?;
    read_pool(&mut reader).ok()?;
    reader.u2().ok()
}

impl<H: Host> Runtime<'_, H> {
    /// Run `checkcast target` on `value`: `Ok` when the cast passes (or was
    /// counted), the `ClassCastException` when it cannot.
    pub(super) fn checkcast(&mut self, value: &Value, target: &str) -> Result<(), NativeError> {
        if *value == Value::Ref(None) || target == "java/lang/Object" {
            return Ok(());
        }
        let Some(target_file) = self.class(target) else {
            // A JDK or game target: the runtime models those values by marker
            // and keeps its previous behaviour.
            return Ok(());
        };
        let target_is_interface =
            class_access(target_file).map(|access| access & ACC_INTERFACE != 0);
        let (verdict, runtime_class) = self.cast_verdict(value, target, target_is_interface);
        match verdict {
            Verdict::Yes => Ok(()),
            Verdict::No => Err(NativeError::UncaughtException {
                class: "java/lang/ClassCastException".into(),
                message: Some(format!(
                    "class {} cannot be cast to class {}",
                    runtime_class
                        .as_deref()
                        .unwrap_or("java.lang.Object")
                        .replace('/', "."),
                    target.replace('/', ".")
                )),
                frame: None,
            }),
            Verdict::Unknown => {
                let cast = MethodRef {
                    owner: target.to_owned(),
                    name: "<checkcast>".into(),
                    descriptor: format!("(Ljava/lang/Object;)L{target};"),
                };
                self.host
                    .invoke(&cast, std::slice::from_ref(value))
                    .map(|_| ())
            }
        }
    }

    fn cast_verdict(
        &self,
        value: &Value,
        target: &str,
        target_is_interface: Option<bool>,
    ) -> (Verdict, Option<String>) {
        let marker = match value {
            // A boxed number is a JDK object, never a jar type.
            Value::Int(_) | Value::Long(_) | Value::Float(_) => {
                return (Verdict::No, class_of_value(value));
            }
            Value::Void | Value::Ref(None) => return (Verdict::Yes, None),
            Value::Ref(Some(marker)) => marker.as_str(),
        };
        if marker.starts_with("@indy:") {
            // A lambda is an instance of its functional interface only.
            return match target_is_interface {
                Some(false) => (Verdict::No, Some("java/lang/Object".into())),
                _ => (Verdict::Yes, None),
            };
        }
        // A jar (or linked) object, or a jar enum constant marker.
        let object_class = marker
            .strip_prefix("enum/")
            .and_then(|rest| rest.rsplit_once('/').map(|(class, _)| class))
            .or_else(|| marker.split_once('#').map(|(class, _)| class))
            .filter(|class| self.class(class).is_some());
        if let Some(class) = object_class {
            return (
                self.hierarchy_verdict(class, target, target_is_interface),
                Some(class.to_owned()),
            );
        }
        // A JDK value the runtime builds itself (a box, a collection, an
        // optional, a stream, a path): JDK classes never extend or implement a
        // jar type.
        let jdk_kind = [
            "integer:",
            "long:",
            "short:",
            "byte:",
            "character:",
            "float:",
            "double:",
            "boolean:",
            "string-builder/",
            "optional/",
            "stream/",
            "path:",
            "list/",
            "set/",
            "map/",
        ]
        .iter()
        .any(|prefix| marker.starts_with(prefix))
            || matches!(
                marker,
                "list" | "set" | "map" | "optional" | "true" | "false"
            )
            // An object `new` made of a JDK class (`java/util/ArrayList#3`).
            || marker
                .split_once('#')
                .is_some_and(|(class, _)| class.starts_with("java/"));
        if jdk_kind {
            return (Verdict::No, class_of_value(value));
        }
        (Verdict::Unknown, None)
    }

    /// Walk `class`'s superclasses and interfaces for `target`.
    ///
    /// Leaving the classes the runtime holds is definitive for a JDK class
    /// (it never extends or implements a jar type) and for a class target (a
    /// game or library class cannot extend a jar class); for an interface
    /// target a Mixin may add the interface to the game superclass, so the
    /// answer is unknown.
    fn hierarchy_verdict(
        &self,
        class: &str,
        target: &str,
        target_is_interface: Option<bool>,
    ) -> Verdict {
        let mut current = class.to_owned();
        for _ in 0..MAX_SUPERCLASS_WALK {
            if current == target || self.mixin_adds_interface(&current, target) {
                return Verdict::Yes;
            }
            let Some(file) = self.class(&current) else {
                if current.starts_with("java/") || target_is_interface == Some(false) {
                    return Verdict::No;
                }
                return Verdict::Unknown;
            };
            if file
                .interface_names()
                .iter()
                .any(|interface| interface == target || self.interface_extends(interface, target))
            {
                return Verdict::Yes;
            }
            if file.super_name.is_empty() || file.super_name == current {
                return Verdict::No;
            }
            current = file.super_name.clone();
        }
        Verdict::Unknown
    }

    /// Whether one of the jar's `@Mixin`s gives `class` the interface `target`
    /// at weave time: an accessor/invoker interface that is itself the mixin,
    /// or a mixin class that implements it (a "duck" interface), naming
    /// `class` among its targets. The class file of `class` does not list
    /// that interface, so the plain walk cannot see it.
    fn mixin_adds_interface(&self, class: &str, target: &str) -> bool {
        self.classes.values().any(|mixin| {
            let provides = mixin.name == target
                || mixin.interface_names().iter().any(|interface| {
                    interface == target || self.interface_extends(interface, target)
                });
            provides
                && mixin
                    .annotations
                    .iter()
                    .filter(|annotation| annotation.descriptor == MIXIN_ANNOTATION_DESCRIPTOR)
                    .any(|annotation| {
                        mixin_target_owners(annotation)
                            .iter()
                            .any(|owner| owner == class)
                    })
        })
    }
}
