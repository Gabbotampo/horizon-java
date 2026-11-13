//! The `Iterable`, `Collection` and `Iterator` default methods as JDK 25
//! runs them. A default's calls on `this` are `invokeinterface`, as in the
//! JDK's interface bodies.

use super::*;

/// The defaults this file implements: `(interface, name, descriptor)`.
pub(super) const BODIES: &[(&str, &str, &str)] = &[
    (
        "java/lang/Iterable",
        "forEach",
        "(Ljava/util/function/Consumer;)V",
    ),
    (COLLECTION, "removeIf", "(Ljava/util/function/Predicate;)Z"),
    (
        COLLECTION,
        "toArray",
        "(Ljava/util/function/IntFunction;)[Ljava/lang/Object;",
    ),
    (COLLECTION, "stream", "()Ljava/util/stream/Stream;"),
    (COLLECTION, "parallelStream", "()Ljava/util/stream/Stream;"),
    (ITERATOR, "remove", "()V"),
    (
        ITERATOR,
        "forEachRemaining",
        "(Ljava/util/function/Consumer;)V",
    ),
];

/// `Consumer.accept(Object)`.
const ACCEPT: (&str, &str, &str) = (
    "java/util/function/Consumer",
    "accept",
    "(Ljava/lang/Object;)V",
);

impl<H: Host> Runtime<'_, H> {
    /// Whether the default `owner.name` runs here for `this`: a stream of a
    /// `RandomAccess` list reads it by index (`List.spliterator`), which this
    /// file does not run; nor does it run a class's own `spliterator()`.
    pub(super) fn default_runs_here(&mut self, owner: &str, name: &str, this: &Value) -> bool {
        // A list's stream of a `RandomAccess` list reads it by index
        // (`List.spliterator`), which this file does not run; nor does it run
        // a class's own `spliterator()` override.
        if owner == COLLECTION && name.ends_with("tream") {
            let spliterator = MethodRef {
                owner: COLLECTION.into(),
                name: "spliterator".into(),
                descriptor: "()Ljava/util/Spliterator;".into(),
            };
            let overridden = self
                .jvm_selected_jar_body(&spliterator, 0xb9, Some(this))
                .ok()
                .flatten()
                .is_some();
            let random_access = expect_ref(this)
                .and_then(|marker| jvm_dispatch::constructed_object_class(marker))
                .is_some_and(|class| {
                    self.class_is_instance(class, "java/util/List")
                        && self.class_is_instance(class, "java/util/RandomAccess")
                });
            return !overridden && !random_access;
        }
        true
    }

    /// A JDK interface's default method.
    pub(super) fn interface_default_body(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Ref(None));
        match (owner, name) {
            ("java/lang/Iterable", "forEach") => {
                let action = arg(0);
                require_non_null(&action)?;
                let it = self.iterator_of(Site::Interface, "java/lang/Iterable", this)?;
                self.each_remaining(&it, &action)?;
                Ok(Value::Void)
            }
            (COLLECTION, "removeIf") => {
                let filter = arg(0);
                require_non_null(&filter)?;
                let each = self.iterator_of(Site::Interface, COLLECTION, this)?;
                let mut removed = false;
                while self.has_next(&each)? {
                    let e = self.next(&each)?;
                    let selected = self.call_functional(
                        "java/util/function/Predicate",
                        "test",
                        "(Ljava/lang/Object;)Z",
                        &filter,
                        &[e],
                    )?;
                    if truth(&selected) {
                        self.remove_current(&each)?;
                        removed = true;
                    }
                }
                Ok(boolean(removed))
            }
            (COLLECTION, "toArray") => {
                let array = self.call_functional(
                    "java/util/function/IntFunction",
                    "apply",
                    "(I)Ljava/lang/Object;",
                    &arg(0),
                    &[Value::Int(0)],
                )?;
                self.call_virtual(
                    Site::Interface,
                    COLLECTION,
                    "toArray",
                    "([Ljava/lang/Object;)[Ljava/lang/Object;",
                    this,
                    &[array],
                )
            }
            (COLLECTION, "stream" | "parallelStream") => {
                // `StreamSupport.stream(spliterator(), false)`: the
                // collection's `IteratorSpliterator` asks for `iterator()`
                // and `size()` when the terminal operation starts, then
                // drains the iterator. The runtime's streams are eager, so
                // this traversal happens here, when the stream is made.
                let it = self.iterator_of(Site::Interface, COLLECTION, this)?;
                self.size_of(Site::Interface, COLLECTION, this)?;
                let mut values = Vec::new();
                while self.has_next(&it)? {
                    values.push(self.next(&it)?);
                }
                Ok(self.new_stream("java/util/Collection", name, values))
            }
            (ITERATOR, "remove") => Err(throw(
                "java/lang/UnsupportedOperationException",
                Some("remove".into()),
            )),
            (ITERATOR, "forEachRemaining") => {
                let action = arg(0);
                require_non_null(&action)?;
                self.each_remaining(this, &action)?;
                Ok(Value::Void)
            }
            _ => Err(NativeError::Metadata(format!(
                "no JDK default for {owner}#{name}{descriptor}"
            ))),
        }
    }

    /// `while (it.hasNext()) action.accept(it.next());`
    fn each_remaining(&mut self, it: &Value, action: &Value) -> Result<(), NativeError> {
        while self.has_next(it)? {
            let e = self.next(it)?;
            self.call_functional(ACCEPT.0, ACCEPT.1, ACCEPT.2, action, &[e])?;
        }
        Ok(())
    }
}
