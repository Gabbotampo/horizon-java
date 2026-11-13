//! `Collections.synchronizedMap`, `synchronizedList` and `synchronizedSet`, as
//! JDK 25 builds them: a view that runs every call on the collection it wraps.
//!
//! JDK 25's `SynchronizedMap`/`SynchronizedList`/`SynchronizedSet` keep the
//! wrapped collection (`Objects.requireNonNull(m)`, so a null throws an NPE with
//! no message) and forward each member to it inside `synchronized (mutex)`.
//! The runtime runs one thread, so the lock changes nothing observable: a
//! write through the view is a write to the wrapped collection and the other
//! way round, and `equals`/`hashCode`/`toString` are the wrapped collection's.
//! Forge Config API Port and night-config keep their registries in such views;
//! unanswered, the view was an empty stand-in and every write through it was
//! lost.
//!
//! A view is a collection marker of the wrapped kind (so `instanceof Map`
//! holds) recorded in `StdHost::synchronized_views` against the collection it
//! wraps. `StdHost::invoke_instance` forwards every call on it, and the
//! value readers (`mutable_values`) and writers (`replace_mutable_collection`)
//! the interpreter's own collection code uses resolve it the same way.
//!
//! Named departures: the view is a different object from the wrapped
//! collection (`==` is false, as in the JDK), but its class is the wrapped
//! kind's rather than `Collections$SynchronizedMap`, and `keySet()`,
//! `values()`, `entrySet()`, `subList` and `iterator()` answer the wrapped
//! collection's own views rather than synchronized wrappers of them (the same
//! elements and writes; only their identity and class differ).
//! `synchronizedCollection` and the sorted/navigable overloads stay counted.

use super::*;

impl StdHost {
    /// `Collections.synchronized{Map,List,Set}`; `None` for any other call.
    pub(crate) fn synchronized_view(
        &mut self,
        r: &MethodRef,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let kind = match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            ("java/util/Collections", "synchronizedMap", "(Ljava/util/Map;)Ljava/util/Map;") => {
                "map"
            }
            ("java/util/Collections", "synchronizedList", "(Ljava/util/List;)Ljava/util/List;") => {
                "list"
            }
            ("java/util/Collections", "synchronizedSet", "(Ljava/util/Set;)Ljava/util/Set;") => {
                "set"
            }
            _ => return None,
        };
        let Some(Value::Ref(Some(wrapped))) = args.first() else {
            return Some(Err(NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: None,
                frame: None,
            }));
        };
        let wrapped = self.synchronized_target(wrapped).to_owned();
        let view = self.empty_collection(kind);
        if let Some(marker) = expect_ref(&view) {
            self.synchronized_views.insert(marker.to_owned(), wrapped);
        }
        Some(Ok(view))
    }

    /// The collection a synchronized view wraps, or `marker` itself.
    pub(crate) fn synchronized_target<'a>(&'a self, marker: &'a str) -> &'a str {
        self.synchronized_views
            .get(marker)
            .map_or(marker, String::as_str)
    }

    /// The wrapped collection when `receiver` is a synchronized view.
    pub(crate) fn synchronized_receiver(&self, receiver: &Value) -> Option<Value> {
        let marker = expect_ref(receiver)?;
        self.synchronized_views
            .get(marker)
            .map(|wrapped| Value::Ref(Some(wrapped.clone())))
    }
}
