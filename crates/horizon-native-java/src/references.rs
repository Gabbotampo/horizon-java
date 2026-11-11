//! `java.lang.ref` references: `WeakReference`, `SoftReference`,
//! `PhantomReference` and `ReferenceQueue`.
//!
//! A weak set or cache (jfronny commons' `WeakSet`, Guava's maps) keeps its
//! entries behind a `WeakReference` registered with a `ReferenceQueue`, and
//! asks the queue for references whose referent was collected. This runtime
//! never collects an object — the heap is a table of markers that lives for
//! the invocation, and there is no GC to run — so:
//!
//! * `get()` answers the referent it was built with, for the object's whole
//!   life (a JVM may clear it at any GC, so this is the "no collection yet"
//!   answer, not a made-up one);
//! * `clear()` clears it, as the JDK's does;
//! * `isEnqueued()`/`enqueue()` answer false: nothing is ever enqueued, because
//!   nothing is collected;
//! * `ReferenceQueue.poll()` answers `null`: the queue is always empty. The
//!   blocking `remove()` is NOT modelled — in the JDK it waits for a reference
//!   that this runtime can never produce, which would hang the callback for
//!   ever; it stays a counted unmodelled call instead.
//!
//! The visible consequence is named in the PR: a `WeakSet` here holds its
//! entries for the whole run (no expiry), which is what a `HashSet` would do.

use crate::{MethodRef, NativeError, StdHost, Value, expect_ref};

/// Owners whose constructor registers a referent.
const REFERENCE_OWNERS: &[&str] = &[
    "java/lang/ref/Reference",
    "java/lang/ref/WeakReference",
    "java/lang/ref/SoftReference",
    "java/lang/ref/PhantomReference",
];

/// The queue marker prefix: `new ReferenceQueue()` is
/// `java/lang/ref/ReferenceQueue#<n>`.
const QUEUE_CLASS: &str = "java/lang/ref/ReferenceQueue";

impl StdHost {
    /// `java.lang.ref.Reference`/`ReferenceQueue` members, or `None` for a
    /// class this module does not answer.
    pub(crate) fn reference_member(
        &mut self,
        r: &MethodRef,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        if r.owner == QUEUE_CLASS {
            let marker = receiver.and_then(expect_ref)?.clone();
            return Some(match (r.name.as_str(), r.descriptor.as_str()) {
                ("<init>", "()V") => Ok(Value::Void),
                // Nothing is ever collected, so nothing is ever enqueued.
                ("poll", "()Ljava/lang/ref/Reference;") => Ok(Value::Ref(None)),
                ("isEmpty", "()Z") => Ok(Value::Int(1)),
                ("clear", "()V") => Ok(Value::Void),
                _ => Err(NativeError::Metadata(format!(
                    "unsupported ReferenceQueue member {}#{} ({marker})",
                    r.name, r.descriptor
                ))),
            });
        }
        if !REFERENCE_OWNERS.contains(&r.owner.as_str()) {
            return None;
        }
        let marker = receiver.and_then(expect_ref)?.clone();
        Some(match (r.name.as_str(), r.descriptor.as_str()) {
            // `Reference(T referent)` and `Reference(T referent, ReferenceQueue)`:
            // the queue only ever receives a reference this runtime never
            // enqueues, so it is accepted and not kept.
            ("<init>", "(Ljava/lang/Object;)V")
            | ("<init>", "(Ljava/lang/Object;Ljava/lang/ref/ReferenceQueue;)V") => {
                let referent = args.first().cloned().unwrap_or(Value::Ref(None));
                self.references.insert(marker, referent);
                Ok(Value::Void)
            }
            ("get", "()Ljava/lang/Object;") => Ok(self
                .references
                .get(&marker)
                .cloned()
                .unwrap_or(Value::Ref(None))),
            ("clear", "()V") => {
                self.references.insert(marker, Value::Ref(None));
                Ok(Value::Void)
            }
            ("isEnqueued", "()Z") | ("enqueue", "()Z") => Ok(Value::Int(0)),
            ("refersTo", "(Ljava/lang/Object;)Z") => {
                let referent = self
                    .references
                    .get(&marker)
                    .cloned()
                    .unwrap_or(Value::Ref(None));
                let wanted = args.first().cloned().unwrap_or(Value::Ref(None));
                Ok(Value::Int(i32::from(referent == wanted)))
            }
            _ => Err(NativeError::Metadata(format!(
                "unsupported Reference member {}#{} ({marker})",
                r.name, r.descriptor
            ))),
        })
    }
}
