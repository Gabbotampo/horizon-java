//! `Collection.forEach`/`removeIf` and `Map.forEach` run the jar's own
//! callback.
//!
//! The host's collection model answers the non-callback surface a collection
//! has (`size`, `get`, `add`, `iterator`...), but a method that takes a
//! functional interface needs a jar body, which only the interpreter can run.
//! The host used to answer `forEach` itself by calling `Consumer.accept` back
//! through its own model, which never reaches a jar lambda: the consumer
//! silently did nothing. `Chunky#<init>` showed it (its `Server#getWorlds`
//! collects the levels with `Iterable.forEach`, the list stayed empty, and the
//! next `List.get(0)` threw — `docs/in-game-survey.md`, "Top causes" §5).
//!
//! These three members are answered here, in the interpreter, before the host
//! sees the call:
//!
//! * `forEach(Consumer)` on a collection: `accept` once per element, in
//!   iteration order.
//! * `forEach(BiConsumer)` on a map: `accept` once per entry, in the map's
//!   iteration order.
//! * `removeIf(Predicate)` on a collection: `test` once per element, and the
//!   elements it does not select stay, in their original order; the answer is
//!   whether anything was removed (the JDK's `Collection.removeIf` contract).
//!
//! The callback is whichever of the two shapes the value is: an indy lambda or
//! method reference (`Runtime::invoke_callback`), or an object of a jar class
//! that implements the interface — for the latter the interface call is
//! dispatched the way the `invokeinterface` opcode would dispatch it. A
//! callback that is neither (a host marker) keeps the host's old answer.

use crate::runtime::{
    Host, MethodRef, NativeError, Runtime, Value, is_collection_owner, is_map_owner,
    parse_collection_value, runtime_collection_values,
};

/// `java/util/function/Consumer.accept`.
const ACCEPT_OBJECT: (&str, &str, &str) = (
    "java/util/function/Consumer",
    "accept",
    "(Ljava/lang/Object;)V",
);
/// `java/util/function/BiConsumer.accept`.
const ACCEPT_ENTRY: (&str, &str, &str) = (
    "java/util/function/BiConsumer",
    "accept",
    "(Ljava/lang/Object;Ljava/lang/Object;)V",
);
/// `java/util/function/Predicate.test`.
const TEST: (&str, &str, &str) = (
    "java/util/function/Predicate",
    "test",
    "(Ljava/lang/Object;)Z",
);

impl<H: Host> Runtime<'_, H> {
    /// `Collection.forEach`, `Map.forEach` and `Collection.removeIf` on a
    /// collection the host holds. `Ok(None)` leaves the call to the rest of the
    /// dispatch chain.
    pub(crate) fn invoke_collection_callback(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if !is_collection_owner(&r.owner) {
            return Ok(None);
        }
        let map = is_map_owner(&r.owner);
        let receiver = match receiver {
            Some(receiver @ Value::Ref(Some(_))) => receiver,
            _ => return Ok(None),
        };
        let values = match runtime_collection_values(self.host, receiver) {
            Ok(values) => values,
            // A marker that is not a collection keeps the normal dispatch (a
            // jar object typed as one has its own overrides, or the host's
            // refusal).
            Err(_) => return Ok(None),
        };
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("forEach", "(Ljava/util/function/Consumer;)V") if !map => {
                let callback = args.first().cloned().unwrap_or(Value::Ref(None));
                for value in owned_values(&values)? {
                    self.run_functional(ACCEPT_OBJECT, &callback, &[value])?;
                }
                Ok(Some(Value::Void))
            }
            ("forEach", "(Ljava/util/function/BiConsumer;)V") if map => {
                let callback = args.first().cloned().unwrap_or(Value::Ref(None));
                for pair in owned_values(&values)?.chunks(2) {
                    if let [key, value] = pair {
                        self.run_functional(
                            ACCEPT_ENTRY,
                            &callback,
                            &[key.clone(), value.clone()],
                        )?;
                    }
                }
                Ok(Some(Value::Void))
            }
            ("removeIf", "(Ljava/util/function/Predicate;)Z") if !map => {
                let callback = args.first().cloned().unwrap_or(Value::Ref(None));
                let mut kept = Vec::new();
                let mut removed = false;
                for value in owned_values(&values)? {
                    let keep = !self.predicate_holds(&callback, &value)?;
                    removed |= !keep;
                    if keep {
                        kept.push(value);
                    }
                }
                if removed {
                    self.host.replace_mutable_collection(receiver, kept);
                }
                Ok(Some(Value::Int(i32::from(removed))))
            }
            _ => Ok(None),
        }
    }

    /// Run one functional-interface call on `callback`, answering whether it
    /// ran. An indy lambda or a method reference is the callback body; an
    /// object of a jar class that implements the interface is an
    /// `invokeinterface` to that class's body. Anything else (a host marker
    /// standing in for a callback) did not run.
    fn run_functional(
        &mut self,
        target: (&str, &str, &str),
        callback: &Value,
        args: &[Value],
    ) -> Result<bool, NativeError> {
        if self.invoke_callback(callback, args)?.is_some() {
            return Ok(true);
        }
        let call = MethodRef {
            owner: target.0.to_owned(),
            name: target.1.to_owned(),
            descriptor: target.2.to_owned(),
        };
        if let Some((owner, jar_owned)) = self.dispatch_owner(&call, 0xb9, Some(callback))
            && jar_owned
        {
            self.invoke(
                &owner,
                &call.name,
                &call.descriptor,
                Some(callback.clone()),
                args,
            )?;
            return Ok(true);
        }
        Ok(false)
    }

    /// `Predicate.test`, as a boolean. A predicate the runtime cannot run did
    /// not select the element, which is what the host's neutral answer was.
    fn predicate_holds(&mut self, callback: &Value, value: &Value) -> Result<bool, NativeError> {
        if let Some(answer) = self.invoke_callback(callback, std::slice::from_ref(value))? {
            return Ok(matches!(answer, Value::Int(flag) if flag != 0));
        }
        let call = MethodRef {
            owner: TEST.0.to_owned(),
            name: TEST.1.to_owned(),
            descriptor: TEST.2.to_owned(),
        };
        if let Some((owner, jar_owned)) = self.dispatch_owner(&call, 0xb9, Some(callback))
            && jar_owned
        {
            let answer = self.invoke(
                &owner,
                &call.name,
                &call.descriptor,
                Some(callback.clone()),
                std::slice::from_ref(value),
            )?;
            return Ok(matches!(answer, Value::Int(flag) if flag != 0));
        }
        Ok(false)
    }
}

/// The host's collection values, each parsed back into a `Value`.
fn owned_values(values: &[String]) -> Result<Vec<Value>, NativeError> {
    values
        .iter()
        .map(|value| parse_collection_value(value))
        .collect()
}
