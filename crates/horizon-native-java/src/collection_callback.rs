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
//! These members are answered here, in the interpreter, before the host
//! sees the call:
//!
//! * `forEach(Consumer)` on a collection: `accept` once per element, in
//!   iteration order.
//! * `forEach(BiConsumer)` on a map: `accept` once per entry, in the map's
//!   iteration order.
//! * `removeIf(Predicate)` on a collection: `test` once per element, and the
//!   elements it does not select stay, in their original order; the answer is
//!   whether anything was removed (the JDK's `Collection.removeIf` contract).
//! * `forEachRemaining(Consumer)` on a host collection's iterator: `next()`
//!   and `accept` once per remaining element. The host's own answer drained
//!   the iterator without running the action. When the action throws, the
//!   iterator is left where JDK 25's class leaves it: `ArrayList.Itr` writes
//!   its cursor only after the loop (so it resumes at the first remaining
//!   element), `LinkedList.ListItr` advances after each `accept` returns (so
//!   it resumes at the element that threw), `ArrayDeque`'s iterator is
//!   exhausted before the loop, and the `Iterator.forEachRemaining` default
//!   (`List.of`, the hash and tree sets, a map's views) has already passed
//!   the element. An iterator whose JDK class the host does not record
//!   (a list a collector built, an unmodifiable view) is counted then.
//!
//! The callback is whichever of the two shapes the value is: an indy lambda or
//! method reference (`Runtime::invoke_callback`), or an object of a jar class
//! that implements the interface — for the latter the interface call is
//! dispatched the way the `invokeinterface` opcode would dispatch it. A
//! callback that is neither (a host marker) keeps the host's old answer.

use crate::{
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
        if matches!(
            r.owner.as_str(),
            "java/util/Iterator" | "java/util/ListIterator"
        ) && r.name == "forEachRemaining"
            && r.descriptor == "(Ljava/util/function/Consumer;)V"
        {
            return self.iterator_for_each_remaining(r, receiver, args);
        }
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

    /// `forEachRemaining(action)` on a host iterator, as its JDK 25 class
    /// runs it.
    fn iterator_for_each_remaining(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(it) = receiver else {
            return Ok(None);
        };
        let Some(marker) = crate::expect_ref(it).cloned() else {
            return Ok(None);
        };
        let Some((start, len, source)) = self.host.host_iterator(&marker) else {
            return Ok(None);
        };
        let action = args.first().cloned().unwrap_or(Value::Ref(None));
        if action == Value::Ref(None) {
            return Err(NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: None,
                frame: None,
            });
        }
        // A fail-fast iterator checks `modCount` as its JDK class does
        // (`iteration_guard.rs`).
        if let Some(answer) = self.guarded_for_each_remaining(r, it, &action)? {
            return Ok(Some(answer));
        }
        let mut at = start;
        while at < len {
            let element = self.host.host_iterator_next(&marker)?;
            at += 1;
            let ran = match self.run_functional(ACCEPT_OBJECT, &action, &[element]) {
                Ok(ran) => ran,
                Err(error) => {
                    let resume = match iteration_kind(source.as_deref()) {
                        IterationKind::CursorAtEnd => Some(start),
                        IterationKind::AdvanceAfterAccept => Some(at - 1),
                        IterationKind::Exhausted => Some(len),
                        IterationKind::Default => None,
                        IterationKind::Unrecorded => {
                            self.host.unknown_operand_call(r)?;
                            None
                        }
                    };
                    if let Some(resume) = resume {
                        self.host.set_host_iterator_cursor(&marker, resume);
                    }
                    return Err(error);
                }
            };
            if !ran {
                // An action the runtime cannot run (a host marker).
                self.host.unknown_operand_call(r)?;
            }
        }
        Ok(Some(Value::Void))
    }

    /// Run one functional-interface call on `callback`, answering whether it
    /// ran. An indy lambda or a method reference is the callback body; an
    /// object of a jar class that implements the interface is an
    /// `invokeinterface` to that class's body. Anything else (a host marker
    /// standing in for a callback) did not run.
    pub(crate) fn run_functional(
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
    pub(crate) fn predicate_holds(
        &mut self,
        callback: &Value,
        value: &Value,
    ) -> Result<bool, NativeError> {
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

/// Where a JDK iterator's `forEachRemaining` leaves its cursor when the
/// action throws.
enum IterationKind {
    /// `ArrayList.Itr`/`Vector.Itr`: cursor and `lastRet` are written once,
    /// after the loop.
    CursorAtEnd,
    /// `LinkedList.ListItr`: advances after each `accept` returns.
    AdvanceAfterAccept,
    /// `ArrayDeque`'s iterator: `remaining = 0` before the loop.
    Exhausted,
    /// `Iterator.forEachRemaining`: `action.accept(next())`.
    Default,
    /// The host does not record which JDK class the iterator is.
    Unrecorded,
}

/// The `forEachRemaining` of the iterator over a host collection whose class
/// (or encoded kind) is `source`.
fn iteration_kind(source: Option<&str>) -> IterationKind {
    match source {
        Some("java/util/ArrayList" | "java/util/Vector" | "java/util/Stack") => {
            IterationKind::CursorAtEnd
        }
        Some("java/util/LinkedList") => IterationKind::AdvanceAfterAccept,
        Some("java/util/ArrayDeque") => IterationKind::Exhausted,
        // `Set.of`/`Collectors.toSet()` values and a map's key, value and
        // entry views iterate with the default, as do these classes.
        Some(
            "set"
            | "java/util/HashSet"
            | "java/util/LinkedHashSet"
            | "java/util/TreeSet"
            | "java/util/PriorityQueue",
        ) => IterationKind::Default,
        _ => IterationKind::Unrecorded,
    }
}
