//! `AbstractSet` and `AbstractQueue` as JDK 25 runs them (their
//! `AbstractCollection` members are `collection.rs`'s).

use super::*;

/// The members this file implements: `(owner, name, descriptor)`.
pub(super) const BODIES: &[(&str, &str, &str)] = &[
    (ABSTRACT_SET, "equals", "(Ljava/lang/Object;)Z"),
    (ABSTRACT_SET, "hashCode", "()I"),
    (ABSTRACT_SET, "removeAll", "(Ljava/util/Collection;)Z"),
    (ABSTRACT_QUEUE, "add", "(Ljava/lang/Object;)Z"),
    (ABSTRACT_QUEUE, "remove", "()Ljava/lang/Object;"),
    (ABSTRACT_QUEUE, "element", "()Ljava/lang/Object;"),
    (ABSTRACT_QUEUE, "clear", "()V"),
    (ABSTRACT_QUEUE, "addAll", "(Ljava/util/Collection;)Z"),
];

impl<H: Host> Runtime<'_, H> {
    /// An `AbstractSet` or `AbstractQueue` member.
    pub(super) fn set_queue_body(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Ref(None));
        let own = Site::Virtual;
        match (owner, name, descriptor) {
            (ABSTRACT_SET, "equals", _) => {
                let o = arg(0);
                if o == *this {
                    return Ok(boolean(true));
                }
                if !self.java_instance_of(&o, "java/util/Set")? {
                    return Ok(boolean(false));
                }
                if self.size_of(Site::Interface, COLLECTION, &o)?
                    != self.size_of(own, owner, this)?
                {
                    return Ok(boolean(false));
                }
                // `catch (ClassCastException | NullPointerException unused)`.
                match self.call_virtual(
                    own,
                    ABSTRACT_SET,
                    "containsAll",
                    "(Ljava/util/Collection;)Z",
                    this,
                    &[o],
                ) {
                    Ok(answer) => Ok(answer),
                    Err(error)
                        if self.catches(&error, "java/lang/ClassCastException")
                            || self.catches(&error, "java/lang/NullPointerException") =>
                    {
                        Ok(boolean(false))
                    }
                    Err(error) => Err(error),
                }
            }
            (ABSTRACT_SET, "hashCode", _) => {
                let it = self.iterator_of(own, owner, this)?;
                let mut h = 0i32;
                while self.has_next(&it)? {
                    let e = self.next(&it)?;
                    if e != Value::Ref(None) {
                        h = h.wrapping_add(self.virtual_hash(&e)?);
                    }
                }
                Ok(Value::Int(h))
            }
            (ABSTRACT_SET, "removeAll", _) => {
                let c = arg(0);
                require_non_null(&c)?;
                if self.size_of(own, owner, this)?
                    > self.size_of(Site::Interface, COLLECTION, &c)?
                {
                    let mut modified = false;
                    let each = self.iterator_of(Site::Interface, COLLECTION, &c)?;
                    while self.has_next(&each)? {
                        let e = self.next(&each)?;
                        let removed = self.call_virtual(
                            own,
                            ABSTRACT_SET,
                            "remove",
                            "(Ljava/lang/Object;)Z",
                            this,
                            &[e],
                        )?;
                        modified |= truth(&removed);
                    }
                    Ok(boolean(modified))
                } else {
                    let it = self.iterator_of(own, owner, this)?;
                    self.remove_matching(&it, &c, true)
                }
            }
            (ABSTRACT_QUEUE, "add", _) => {
                let offered = self.call_virtual(
                    own,
                    ABSTRACT_QUEUE,
                    "offer",
                    "(Ljava/lang/Object;)Z",
                    this,
                    &[arg(0)],
                )?;
                if truth(&offered) {
                    Ok(boolean(true))
                } else {
                    Err(throw(
                        "java/lang/IllegalStateException",
                        Some("Queue full".into()),
                    ))
                }
            }
            (ABSTRACT_QUEUE, "remove" | "element", _) => {
                let probe = if name == "remove" { "poll" } else { "peek" };
                let x = self.call_virtual(
                    own,
                    ABSTRACT_QUEUE,
                    probe,
                    "()Ljava/lang/Object;",
                    this,
                    &[],
                )?;
                if x == Value::Ref(None) {
                    return Err(throw("java/util/NoSuchElementException", None));
                }
                Ok(x)
            }
            (ABSTRACT_QUEUE, "clear", _) => {
                while self.call_virtual(
                    own,
                    ABSTRACT_QUEUE,
                    "poll",
                    "()Ljava/lang/Object;",
                    this,
                    &[],
                )? != Value::Ref(None)
                {}
                Ok(Value::Void)
            }
            (ABSTRACT_QUEUE, "addAll", _) => {
                let c = arg(0);
                if c == Value::Ref(None) {
                    return Err(throw("java/lang/NullPointerException", None));
                }
                if c == *this {
                    return Err(throw("java/lang/IllegalArgumentException", None));
                }
                self.add_each(owner, this, &c)
            }
            _ => Err(NativeError::Metadata(format!(
                "no JDK body for {owner}#{name}{descriptor}"
            ))),
        }
    }
}
