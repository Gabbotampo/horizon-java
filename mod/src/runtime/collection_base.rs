//! The JDK's abstract collection base bodies a jar class inherits.
//!
//! `invokeinterface Set.isEmpty` on an object whose class extends
//! `java.util.AbstractSet` resolves to `AbstractCollection.isEmpty`, whose
//! body is `size() == 0`. That class is a JDK class with no classfile here, so
//! the interface call used to fall through to the host's collection model,
//! which requires the receiver to be one of its own collection markers and
//! failed with "expected a native collection marker" — a hard error that cost
//! the whole listener (respackopts' `WeakSet.isEmpty`,
//! `docs/in-game-survey.md`, "Top causes" §5).
//!
//! Only the one member is answered, and only for a receiver that is an object
//! of a class the runtime holds (a jar or linked class) whose own `size()`
//! exists: `isEmpty` is then exactly `size() == 0`, as the JDK defines it.
//! Every other inherited base body, and every marker collection, keeps the
//! existing dispatch.
//!
//! Both call shapes reach here: `invokeinterface Set.isEmpty` on a receiver
//! typed as an interface (respackopts), and `invokevirtual Bag.isEmpty` on a
//! receiver typed as the jar subclass, which `javac` resolves to the inherited
//! `AbstractCollection.isEmpty` (same body).

use crate::runtime::{Host, MethodRef, NativeError, Runtime, Value, expect_ref};

impl<H: Host> Runtime<'_, H> {
    /// `Collection.isEmpty` on a jar object that inherits `AbstractCollection`'s
    /// body. `Ok(None)` keeps the normal dispatch.
    pub(crate) fn invoke_collection_default(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        _args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if !(r.name == "isEmpty" && r.descriptor == "()Z") {
            return Ok(None);
        }
        let Some(receiver) = receiver else {
            return Ok(None);
        };
        let Some(marker) = expect_ref(receiver) else {
            return Ok(None);
        };
        let Some((class, _)) = marker.split_once('#') else {
            return Ok(None);
        };
        // Only a jar (or linked) class has a body to ask. Its own `<init>` made
        // the host's collection table hold an empty row for the object (the
        // JDK-collection-subclass rule), so that row is not evidence the class
        // is a collection: ask the class first.
        if self.class(class).is_none() {
            return Ok(None);
        }
        // Only `AbstractCollection`/`AbstractMap` derive `isEmpty` from
        // `size()`; a concrete JDK collection (`HashMap`, `ArrayList`) reads
        // its own count, so a subclass's `size()` override does not change it.
        if self.stateless_jar_object(receiver).is_none() {
            return Ok(None);
        }
        let Some((owner, _)) = self.member_owner(class, "size", "()I") else {
            return Ok(None);
        };
        let size = self.invoke(&owner, "size", "()I", Some(receiver.clone()), &[])?;
        Ok(Some(Value::Int(i32::from(matches!(size, Value::Int(0))))))
    }
}
