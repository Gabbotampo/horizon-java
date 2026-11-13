//! `AbstractCollection` as JDK 25 runs it.
//!
//! Every body below is the JDK's documented algorithm with its calls in the
//! JDK's order. Calls on `this` are the JDK's `invokevirtual` of the
//! skeleton's own member, so a subclass override runs; calls on another
//! object are the interface calls the JDK makes (`Collection.contains`,
//! `Iterator.next`).

use super::*;

/// The members this file implements: `(owner, name, descriptor)`.
pub(super) const BODIES: &[(&str, &str, &str)] = &[
    (ABSTRACT_COLLECTION, "isEmpty", "()Z"),
    (ABSTRACT_COLLECTION, "contains", "(Ljava/lang/Object;)Z"),
    (ABSTRACT_COLLECTION, "toArray", "()[Ljava/lang/Object;"),
    (
        ABSTRACT_COLLECTION,
        "toArray",
        "([Ljava/lang/Object;)[Ljava/lang/Object;",
    ),
    (ABSTRACT_COLLECTION, "add", "(Ljava/lang/Object;)Z"),
    (ABSTRACT_COLLECTION, "remove", "(Ljava/lang/Object;)Z"),
    (
        ABSTRACT_COLLECTION,
        "containsAll",
        "(Ljava/util/Collection;)Z",
    ),
    (ABSTRACT_COLLECTION, "addAll", "(Ljava/util/Collection;)Z"),
    (
        ABSTRACT_COLLECTION,
        "removeAll",
        "(Ljava/util/Collection;)Z",
    ),
    (
        ABSTRACT_COLLECTION,
        "retainAll",
        "(Ljava/util/Collection;)Z",
    ),
    (ABSTRACT_COLLECTION, "clear", "()V"),
    (ABSTRACT_COLLECTION, "toString", "()Ljava/lang/String;"),
];

/// `ArraysSupport.SOFT_MAX_ARRAY_LENGTH`.
const SOFT_MAX_ARRAY_LENGTH: usize = i32::MAX as usize - 8;

impl<H: Host> Runtime<'_, H> {
    /// An `AbstractCollection` member.
    pub(super) fn collection_body(
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
            (ABSTRACT_COLLECTION, "isEmpty", _) => {
                Ok(boolean(self.size_of(own, owner, this)? == 0))
            }
            (ABSTRACT_COLLECTION, "contains", _) => {
                let it = self.iterator_of(own, owner, this)?;
                Ok(boolean(self.find_in(&it, &arg(0))?))
            }
            (ABSTRACT_COLLECTION, "toArray", "()[Ljava/lang/Object;") => self.array_of(this),
            (ABSTRACT_COLLECTION, "toArray", _) => self.array_into(this, &arg(0)),
            (ABSTRACT_COLLECTION, "add", _) => {
                Err(throw("java/lang/UnsupportedOperationException", None))
            }
            (ABSTRACT_COLLECTION, "remove", _) => {
                let it = self.iterator_of(own, owner, this)?;
                let found = self.find_in(&it, &arg(0))?;
                if found {
                    self.remove_current(&it)?;
                }
                Ok(boolean(found))
            }
            (ABSTRACT_COLLECTION, "containsAll", _) => {
                let c = arg(0);
                let it = self.iterator_of(Site::Interface, COLLECTION, &c)?;
                while self.has_next(&it)? {
                    let e = self.next(&it)?;
                    let contained = self.call_virtual(
                        own,
                        ABSTRACT_COLLECTION,
                        "contains",
                        "(Ljava/lang/Object;)Z",
                        this,
                        &[e],
                    )?;
                    if !truth(&contained) {
                        return Ok(boolean(false));
                    }
                }
                Ok(boolean(true))
            }
            (ABSTRACT_COLLECTION, "addAll", _) => self.add_each(owner, this, &arg(0)),
            (ABSTRACT_COLLECTION, "removeAll" | "retainAll", _) => {
                let c = arg(0);
                require_non_null(&c)?;
                let removing = name == "removeAll";
                let it = self.iterator_of(own, owner, this)?;
                self.remove_matching(&it, &c, removing)
            }
            (ABSTRACT_COLLECTION, "clear", _) => {
                let it = self.iterator_of(own, owner, this)?;
                while self.has_next(&it)? {
                    self.next(&it)?;
                    self.remove_current(&it)?;
                }
                Ok(Value::Void)
            }
            (ABSTRACT_COLLECTION, "toString", _) => {
                let it = self.iterator_of(own, owner, this)?;
                if !self.has_next(&it)? {
                    return Ok(Value::Ref(Some("[]".into())));
                }
                let mut text = String::from("[");
                loop {
                    let e = self.next(&it)?;
                    if e == *this {
                        text.push_str("(this Collection)");
                    } else {
                        text.push_str(&self.virtual_text(&e)?);
                    }
                    if !self.has_next(&it)? {
                        text.push(']');
                        return Ok(Value::Ref(Some(text)));
                    }
                    text.push_str(", ");
                }
            }
            _ => Err(NativeError::Metadata(format!(
                "no JDK body for {owner}#{name}{descriptor}"
            ))),
        }
    }

    /// `AbstractCollection.contains`'s search: advance `it` until an element
    /// equal to `o` (`o.equals(e)`, or `e == null` for a null `o`).
    fn find_in(&mut self, it: &Value, o: &Value) -> Result<bool, NativeError> {
        while self.has_next(it)? {
            let e = self.next(it)?;
            let equal = if *o == Value::Ref(None) {
                e == Value::Ref(None)
            } else {
                self.virtual_equals(o, &e)?
            };
            if equal {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// `for (E e : c) if (add(e)) modified = true;`
    pub(super) fn add_each(
        &mut self,
        owner: &str,
        this: &Value,
        c: &Value,
    ) -> Result<Value, NativeError> {
        let it = self.iterator_of(Site::Interface, COLLECTION, c)?;
        let mut modified = false;
        while self.has_next(&it)? {
            let e = self.next(&it)?;
            let added = self.call_virtual(
                Site::Virtual,
                owner,
                "add",
                "(Ljava/lang/Object;)Z",
                this,
                &[e],
            )?;
            modified |= truth(&added);
        }
        Ok(boolean(modified))
    }

    /// `removeAll`/`retainAll` over `it`: remove each element `c.contains`
    /// (or does not, for `retainAll`).
    pub(super) fn remove_matching(
        &mut self,
        it: &Value,
        c: &Value,
        removing: bool,
    ) -> Result<Value, NativeError> {
        let mut modified = false;
        while self.has_next(it)? {
            let e = self.next(it)?;
            let contained = self.call_virtual(
                Site::Interface,
                COLLECTION,
                "contains",
                "(Ljava/lang/Object;)Z",
                c,
                &[e],
            )?;
            if truth(&contained) == removing {
                self.remove_current(it)?;
                modified = true;
            }
        }
        Ok(boolean(modified))
    }

    /// `AbstractCollection.toArray()`.
    fn array_of(&mut self, this: &Value) -> Result<Value, NativeError> {
        let size = self.size_of(Site::Virtual, ABSTRACT_COLLECTION, this)?;
        let length = array_size(size)?;
        let r = self.new_reference_array("java/lang/Object", vec![Value::Ref(None); length]);
        let it = self.iterator_of(Site::Virtual, ABSTRACT_COLLECTION, this)?;
        for i in 0..length {
            if !self.has_next(&it)? {
                return Ok(self.copy_of(&r, i));
            }
            let e = self.next(&it)?;
            self.store_element(&r, i, e)?;
        }
        if self.has_next(&it)? {
            self.finish_to_array(&r, &it)
        } else {
            Ok(r)
        }
    }

    /// `AbstractCollection.toArray(T[] a)`.
    fn array_into(&mut self, this: &Value, a: &Value) -> Result<Value, NativeError> {
        let size = self.size_of(Site::Virtual, ABSTRACT_COLLECTION, this)?;
        // `a.length` on a null array, after `size()`.
        if *a == Value::Ref(None) {
            return Err(throw(
                "java/lang/NullPointerException",
                Some("Cannot read the array length".into()),
            ));
        }
        let given = self.array(a)?.len();
        let r = if given >= size.max(0) as usize {
            a.clone()
        } else {
            let length = array_size(size)?;
            let component = self.array_component(a);
            self.new_reference_array(&component, vec![Value::Ref(None); length])
        };
        let length = self.array(&r)?.len();
        let it = self.iterator_of(Site::Virtual, ABSTRACT_COLLECTION, this)?;
        for i in 0..length {
            if !self.has_next(&it)? {
                if r == *a {
                    self.store_element(&r, i, Value::Ref(None))?;
                } else if given < i {
                    return Ok(self.copy_of(&r, i));
                } else {
                    let copied = self.array(&r)?[..i].to_vec();
                    for (index, value) in copied.into_iter().enumerate() {
                        self.store_element(a, index, value)?;
                    }
                    if given > i {
                        self.store_element(a, i, Value::Ref(None))?;
                    }
                }
                return Ok(a.clone());
            }
            let e = self.next(&it)?;
            self.store_element(&r, i, e)?;
        }
        if self.has_next(&it)? {
            self.finish_to_array(&r, &it)
        } else {
            Ok(r)
        }
    }

    /// `AbstractCollection.finishToArray`: keep reading `it` into copies of
    /// `r` grown by `ArraysSupport.newLength(len, 1, len / 2 + 1)`, then trim.
    fn finish_to_array(&mut self, r: &Value, it: &Value) -> Result<Value, NativeError> {
        let mut r = r.clone();
        let mut len = self.array(&r)?.len();
        let mut i = len;
        while self.has_next(it)? {
            if i == len {
                len += 1usize.max((len >> 1) + 1);
                if len > SOFT_MAX_ARRAY_LENGTH {
                    return Err(throw(
                        "java/lang/OutOfMemoryError",
                        Some("Required array length too large".into()),
                    ));
                }
                r = self.copy_of(&r, len);
            }
            let e = self.next(it)?;
            self.store_element(&r, i, e)?;
            i += 1;
        }
        Ok(if i == len { r } else { self.copy_of(&r, i) })
    }

    /// `Arrays.copyOf(r, length)`: a new array of `r`'s component, padded
    /// with `null` or truncated.
    fn copy_of(&mut self, r: &Value, length: usize) -> Value {
        let mut values = self.array(r).cloned().unwrap_or_default();
        values.resize(length, Value::Ref(None));
        let component = self.array_component(r);
        self.new_reference_array(&component, values)
    }
}

/// An array length from a `size()` answer: the JVM's
/// `NegativeArraySizeException` (message: the length) for a negative one.
fn array_size(size: i32) -> Result<usize, NativeError> {
    usize::try_from(size).map_err(|_| {
        throw(
            "java/lang/NegativeArraySizeException",
            Some(size.to_string()),
        )
    })
}
