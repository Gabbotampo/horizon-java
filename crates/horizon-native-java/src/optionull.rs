//! `net.minecraft.Optionull`, the game's null-handling helper, run exactly as
//! the 26.3 class does.
//!
//! Every member is a pure static method: it holds no game state and reads
//! nothing but its arguments, so its behaviour is fully known from the
//! bytecode (the rule for pure vanilla helpers, 2026-11-13). Mojang's class is
//! never interpreted or shipped; this is the behaviour, written from a reading
//! of `server-game.jar`'s `Optionull` (`javap -c -l`), with the expected values
//! of `tests/fixture_optionull.rs` taken from that class on JDK 25.
//!
//! What the class does:
//!
//! * `orElse(t, defaultValue)` is `Objects.requireNonNullElse(t,
//!   defaultValue)`: `t` when present, else `defaultValue`, and an NPE with
//!   the message `defaultObj` when both are null.
//! * `map(t, map)`, `mapOrDefault(t, map, defaultValue)` and
//!   `mapOrElse(t, map, elseSupplier)`: a null `t` answers `null`,
//!   `defaultValue` or `elseSupplier.get()` without touching `map`; a present
//!   `t` answers `map.apply(t)` (which may itself be null) without touching the
//!   fallback.
//! * `first(collection)`, `firstOrDefault(collection, defaultValue)` and
//!   `firstOrElse(collection, elseSupplier)`: `collection.iterator()`, then
//!   `next()` if `hasNext()`, else `null`, `defaultValue` or
//!   `elseSupplier.get()`.
//! * `isNullOrEmpty(array)` (nine overloads, one per array type): `array ==
//!   null || array.length == 0`.
//!
//! A null `map`, `elseSupplier` or `collection` the method actually invokes
//! throws the JVM's helpful NPE, with the `because "<name>" is null` clause
//! the class's local-variable table names (`null_checks.rs` leaves that
//! clause out because it can't know the name in general; here it is known).
//!
//! The functional arguments run the way `invokeinterface` would: a lambda or
//! method reference through `Runtime::invoke_callback`, an object of a jar
//! class through its own body, and a host value through the host's model
//! (which counts it if it has none). A collection argument that is a jar
//! object never reaches this module: `jar_operands.rs` counts it first, as it
//! does for any model handed a jar collection.

use super::*;

const OWNER: &str = "net/minecraft/Optionull";

const APPLY: (&str, &str, &str) = (
    "java/util/function/Function",
    "apply",
    "(Ljava/lang/Object;)Ljava/lang/Object;",
);
const GET: (&str, &str, &str) = ("java/util/function/Supplier", "get", "()Ljava/lang/Object;");
const ITERATOR: (&str, &str, &str) = ("java/util/Collection", "iterator", "()Ljava/util/Iterator;");
const HAS_NEXT: (&str, &str, &str) = ("java/util/Iterator", "hasNext", "()Z");
const NEXT: (&str, &str, &str) = ("java/util/Iterator", "next", "()Ljava/lang/Object;");

fn method(target: (&str, &str, &str)) -> MethodRef {
    MethodRef {
        owner: target.0.to_owned(),
        name: target.1.to_owned(),
        descriptor: target.2.to_owned(),
    }
}

/// The NPE an `invokeinterface` on the null parameter `name` throws.
fn null_parameter(target: (&str, &str, &str), name: &str) -> NativeError {
    let NativeError::UncaughtException { class, message, .. } =
        null_checks::invoke_on_null(&method(target))
    else {
        unreachable!("invoke_on_null builds an uncaught exception");
    };
    NativeError::UncaughtException {
        class,
        message: message.map(|text| format!("{text} because \"{name}\" is null")),
        frame: None,
    }
}

fn is_null(value: &Value) -> bool {
    matches!(value, Value::Ref(None))
}

impl<H: Host> Runtime<'_, H> {
    /// Answer a static `Optionull` call, or `Ok(None)` for any other call.
    pub(crate) fn invoke_optionull(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if r.owner != OWNER {
            return Ok(None);
        }
        let arg = |index: usize| StdHost::argument(r, args, index).cloned();
        let value = match (r.name.as_str(), r.descriptor.as_str()) {
            ("orElse", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;") => {
                let (t, default_value) = (arg(0)?, arg(1)?);
                if !is_null(&t) {
                    t
                } else if !is_null(&default_value) {
                    default_value
                } else {
                    // `Objects.requireNonNull(defaultObj, "defaultObj")`.
                    return Err(NativeError::UncaughtException {
                        class: "java/lang/NullPointerException".into(),
                        message: Some("defaultObj".into()),
                        frame: None,
                    });
                }
            }
            ("map", "(Ljava/lang/Object;Ljava/util/function/Function;)Ljava/lang/Object;") => {
                let t = arg(0)?;
                if is_null(&t) {
                    Value::Ref(None)
                } else {
                    self.optionull_apply(&arg(1)?, t)?
                }
            }
            (
                "mapOrDefault",
                "(Ljava/lang/Object;Ljava/util/function/Function;Ljava/lang/Object;)Ljava/lang/Object;",
            ) => {
                let t = arg(0)?;
                if is_null(&t) {
                    arg(2)?
                } else {
                    self.optionull_apply(&arg(1)?, t)?
                }
            }
            (
                "mapOrElse",
                "(Ljava/lang/Object;Ljava/util/function/Function;Ljava/util/function/Supplier;)Ljava/lang/Object;",
            ) => {
                let t = arg(0)?;
                if is_null(&t) {
                    self.optionull_get(&arg(2)?)?
                } else {
                    self.optionull_apply(&arg(1)?, t)?
                }
            }
            ("first", "(Ljava/util/Collection;)Ljava/lang/Object;") => {
                match self.optionull_first(&arg(0)?)? {
                    Some(first) => first,
                    None => Value::Ref(None),
                }
            }
            ("firstOrDefault", "(Ljava/util/Collection;Ljava/lang/Object;)Ljava/lang/Object;") => {
                match self.optionull_first(&arg(0)?)? {
                    Some(first) => first,
                    None => arg(1)?,
                }
            }
            (
                "firstOrElse",
                "(Ljava/util/Collection;Ljava/util/function/Supplier;)Ljava/lang/Object;",
            ) => match self.optionull_first(&arg(0)?)? {
                Some(first) => first,
                None => self.optionull_get(&arg(1)?)?,
            },
            (
                "isNullOrEmpty",
                "([Ljava/lang/Object;)Z"
                | "([Z)Z"
                | "([B)Z"
                | "([C)Z"
                | "([S)Z"
                | "([I)Z"
                | "([J)Z"
                | "([F)Z"
                | "([D)Z",
            ) => {
                let array = arg(0)?;
                let empty = is_null(&array) || self.array(&array)?.is_empty();
                Value::Int(i32::from(empty))
            }
            _ => return Ok(None),
        };
        Ok(Some(value))
    }

    /// `map.apply(t)`, the parameter named `map` in every `map*` method.
    fn optionull_apply(&mut self, map: &Value, t: Value) -> Result<Value, NativeError> {
        if is_null(map) {
            return Err(null_parameter(APPLY, "map"));
        }
        self.optionull_interface_call(APPLY, map, &[t])
    }

    /// `elseSupplier.get()`, the parameter named `elseSupplier` in
    /// `mapOrElse` and `firstOrElse`.
    fn optionull_get(&mut self, supplier: &Value) -> Result<Value, NativeError> {
        if is_null(supplier) {
            return Err(null_parameter(GET, "elseSupplier"));
        }
        self.optionull_interface_call(GET, supplier, &[])
    }

    /// `collection.iterator()`, then `next()` when `hasNext()`: `Some(first)`,
    /// or `None` for an empty collection.
    fn optionull_first(&mut self, collection: &Value) -> Result<Option<Value>, NativeError> {
        if is_null(collection) {
            return Err(null_parameter(ITERATOR, "collection"));
        }
        let iterator = self
            .host
            .invoke_instance(&method(ITERATOR), collection, &[])?;
        let has_next = self
            .host
            .invoke_instance(&method(HAS_NEXT), &iterator, &[])?;
        if matches!(has_next, Value::Int(0)) {
            return Ok(None);
        }
        self.host
            .invoke_instance(&method(NEXT), &iterator, &[])
            .map(Some)
    }

    /// One functional-interface call, dispatched as `invokeinterface` would:
    /// a lambda or method reference runs its target, a jar object runs its
    /// own body, and anything else is the host's to answer (or count).
    fn optionull_interface_call(
        &mut self,
        target: (&str, &str, &str),
        receiver: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        if let Some(value) = self.invoke_callback(receiver, args)? {
            // The metafactory boxes a primitive target result for the
            // interface's `Object` return (`String::length` as a `Function`).
            return box_callback_result(self, receiver, value, target.2);
        }
        let call = method(target);
        if let Some((owner, true)) = self.dispatch_owner(&call, 0xb9, Some(receiver)) {
            return self.invoke(
                &owner,
                &call.name,
                &call.descriptor,
                Some(receiver.clone()),
                args,
            );
        }
        self.host.invoke_instance(&call, receiver, args)
    }
}
