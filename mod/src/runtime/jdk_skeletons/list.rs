//! `AbstractList` (with its iterators and sublists), `AbstractSequentialList`
//! and the `List` defaults, as JDK 25 runs them.
//!
//! `AbstractList` keeps one piece of state the subclass shares: `protected
//! transient int modCount`. The jar's own `modCount++` writes it under the
//! field key the JDK declares it with (`Runtime::field_owner` resolves the
//! inherited field to `java/util/AbstractList`), so the iterators below read
//! exactly what the subclass wrote, and `ConcurrentModificationException`
//! fires exactly where the JDK's `checkForComodification` would.
//!
//! The JDK's private classes the bodies construct are objects of their JDK
//! names (`java/util/AbstractList$Itr#n`, `$ListItr`, `$SubList`,
//! `$RandomAccessSubList`, `$SubList$1`) whose fields are kept like any
//! object's, under the JDK's own field names. A call on one dispatches to
//! the members below and then, for what the class inherits, to the
//! skeleton bodies and interface defaults like any other receiver.

use super::*;

/// `java.util.AbstractList`.
pub(crate) const ABSTRACT_LIST: &str = "java/util/AbstractList";
/// `java.util.AbstractSequentialList`.
pub(crate) const SEQUENTIAL_LIST: &str = "java/util/AbstractSequentialList";
/// `java.util.List`.
const LIST: &str = "java/util/List";
/// `java.util.ListIterator`.
const LIST_ITERATOR: &str = "java/util/ListIterator";
/// `AbstractList.Itr`, the iterator `AbstractList.iterator()` builds.
pub(super) const ITR: &str = "java/util/AbstractList$Itr";
/// `AbstractList.ListItr`, `listIterator(int)`'s.
pub(super) const LIST_ITR: &str = "java/util/AbstractList$ListItr";
/// `AbstractList.SubList`.
pub(super) const SUB_LIST: &str = "java/util/AbstractList$SubList";
/// `AbstractList.RandomAccessSubList`, a `SubList` of a `RandomAccess` list.
pub(super) const RANDOM_ACCESS_SUB_LIST: &str = "java/util/AbstractList$RandomAccessSubList";
/// The anonymous `ListIterator` `SubList.listIterator(int)` builds.
pub(super) const SUB_LIST_ITERATOR: &str = "java/util/AbstractList$SubList$1";

/// The members this file implements on the skeletons and `List`.
pub(super) const BODIES: &[(&str, &str, &str)] = &[
    (ABSTRACT_LIST, "add", "(Ljava/lang/Object;)Z"),
    (
        ABSTRACT_LIST,
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
    ),
    (ABSTRACT_LIST, "add", "(ILjava/lang/Object;)V"),
    (ABSTRACT_LIST, "remove", "(I)Ljava/lang/Object;"),
    (ABSTRACT_LIST, "indexOf", "(Ljava/lang/Object;)I"),
    (ABSTRACT_LIST, "lastIndexOf", "(Ljava/lang/Object;)I"),
    (ABSTRACT_LIST, "clear", "()V"),
    (ABSTRACT_LIST, "addAll", "(ILjava/util/Collection;)Z"),
    (ABSTRACT_LIST, "iterator", "()Ljava/util/Iterator;"),
    (ABSTRACT_LIST, "listIterator", "()Ljava/util/ListIterator;"),
    (ABSTRACT_LIST, "listIterator", "(I)Ljava/util/ListIterator;"),
    (ABSTRACT_LIST, "subList", "(II)Ljava/util/List;"),
    (ABSTRACT_LIST, "equals", "(Ljava/lang/Object;)Z"),
    (ABSTRACT_LIST, "hashCode", "()I"),
    (ABSTRACT_LIST, "removeRange", "(II)V"),
    (SEQUENTIAL_LIST, "get", "(I)Ljava/lang/Object;"),
    (
        SEQUENTIAL_LIST,
        "set",
        "(ILjava/lang/Object;)Ljava/lang/Object;",
    ),
    (SEQUENTIAL_LIST, "add", "(ILjava/lang/Object;)V"),
    (SEQUENTIAL_LIST, "remove", "(I)Ljava/lang/Object;"),
    (SEQUENTIAL_LIST, "addAll", "(ILjava/util/Collection;)Z"),
    (SEQUENTIAL_LIST, "iterator", "()Ljava/util/Iterator;"),
    (LIST, "sort", "(Ljava/util/Comparator;)V"),
    (LIST, "replaceAll", "(Ljava/util/function/UnaryOperator;)V"),
    (LIST, "getFirst", "()Ljava/lang/Object;"),
    (LIST, "getLast", "()Ljava/lang/Object;"),
    (LIST, "removeFirst", "()Ljava/lang/Object;"),
    (LIST, "removeLast", "()Ljava/lang/Object;"),
    (LIST, "addFirst", "(Ljava/lang/Object;)V"),
    (LIST, "addLast", "(Ljava/lang/Object;)V"),
];

/// The members the private classes declare themselves: `(class, name,
/// descriptor)`. A member not listed is inherited (`ListItr.next` is
/// `Itr`'s; a sublist's `contains` is `AbstractCollection`'s).
pub(super) const OWN_MEMBERS: &[(&str, &str, &str)] = &[
    (ITR, "hasNext", "()Z"),
    (ITR, "next", "()Ljava/lang/Object;"),
    (ITR, "remove", "()V"),
    (LIST_ITR, "hasPrevious", "()Z"),
    (LIST_ITR, "previous", "()Ljava/lang/Object;"),
    (LIST_ITR, "nextIndex", "()I"),
    (LIST_ITR, "previousIndex", "()I"),
    (LIST_ITR, "set", "(Ljava/lang/Object;)V"),
    (LIST_ITR, "add", "(Ljava/lang/Object;)V"),
    (SUB_LIST, "set", "(ILjava/lang/Object;)Ljava/lang/Object;"),
    (SUB_LIST, "get", "(I)Ljava/lang/Object;"),
    (SUB_LIST, "size", "()I"),
    (SUB_LIST, "add", "(ILjava/lang/Object;)V"),
    (SUB_LIST, "remove", "(I)Ljava/lang/Object;"),
    (SUB_LIST, "removeRange", "(II)V"),
    (SUB_LIST, "addAll", "(Ljava/util/Collection;)Z"),
    (SUB_LIST, "addAll", "(ILjava/util/Collection;)Z"),
    (SUB_LIST, "iterator", "()Ljava/util/Iterator;"),
    (SUB_LIST, "listIterator", "(I)Ljava/util/ListIterator;"),
    (SUB_LIST, "subList", "(II)Ljava/util/List;"),
    (RANDOM_ACCESS_SUB_LIST, "subList", "(II)Ljava/util/List;"),
    (SUB_LIST_ITERATOR, "hasNext", "()Z"),
    (SUB_LIST_ITERATOR, "next", "()Ljava/lang/Object;"),
    (SUB_LIST_ITERATOR, "hasPrevious", "()Z"),
    (SUB_LIST_ITERATOR, "previous", "()Ljava/lang/Object;"),
    (SUB_LIST_ITERATOR, "nextIndex", "()I"),
    (SUB_LIST_ITERATOR, "previousIndex", "()I"),
    (SUB_LIST_ITERATOR, "remove", "()V"),
    (SUB_LIST_ITERATOR, "set", "(Ljava/lang/Object;)V"),
    (SUB_LIST_ITERATOR, "add", "(Ljava/lang/Object;)V"),
];

/// `AbstractList.modCount`.
const MOD_COUNT: (&str, &str, &str) = (ABSTRACT_LIST, "modCount", "I");
/// `Itr`'s fields.
const CURSOR: (&str, &str, &str) = (ITR, "cursor", "I");
const LAST_RET: (&str, &str, &str) = (ITR, "lastRet", "I");
const EXPECTED: (&str, &str, &str) = (ITR, "expectedModCount", "I");
const ITR_LIST: (&str, &str, &str) = (ITR, "this$0", "Ljava/util/AbstractList;");
/// `SubList`'s fields.
const ROOT: (&str, &str, &str) = (SUB_LIST, "root", "Ljava/util/AbstractList;");
const PARENT: (&str, &str, &str) = (SUB_LIST, "parent", "Ljava/util/AbstractList$SubList;");
const OFFSET: (&str, &str, &str) = (SUB_LIST, "offset", "I");
const SIZE: (&str, &str, &str) = (SUB_LIST, "size", "I");
/// `SubList$1`'s fields.
const SUB_OUTER: (&str, &str, &str) = (
    SUB_LIST_ITERATOR,
    "this$0",
    "Ljava/util/AbstractList$SubList;",
);
const SUB_INNER: (&str, &str, &str) = (SUB_LIST_ITERATOR, "i", "Ljava/util/ListIterator;");

/// `catch (IndexOutOfBoundsException e)`.
const IOOBE: &str = "java/lang/IndexOutOfBoundsException";
/// `java.util.NoSuchElementException`.
const NSEE: &str = "java/util/NoSuchElementException";
/// `java.util.ConcurrentModificationException`.
const CME: &str = "java/util/ConcurrentModificationException";

/// `Objects.checkIndex(index, length)`.
fn check_index(index: i32, length: i32) -> Result<(), NativeError> {
    if index < 0 || index >= length {
        return Err(throw(
            IOOBE,
            Some(format!("Index {index} out of bounds for length {length}")),
        ));
    }
    Ok(())
}

impl<H: Host> Runtime<'_, H> {
    /// A field of an object this module or a jar keeps (`modCount`, an
    /// iterator's cursor): the JVM default until written.
    pub(super) fn jdk_field(
        &self,
        object: &Value,
        (owner, name, descriptor): (&str, &str, &str),
    ) -> Value {
        expect_ref(object)
            .and_then(|marker| {
                self.fields
                    .get(&format!("{marker}|{owner}#{name}{descriptor}"))
            })
            .cloned()
            .unwrap_or_else(|| Value::default_for(descriptor))
    }

    fn jdk_int_field(&self, object: &Value, field: (&str, &str, &str)) -> i32 {
        int_of(&self.jdk_field(object, field))
    }

    pub(super) fn put_jdk_field(
        &mut self,
        object: &Value,
        (owner, name, descriptor): (&str, &str, &str),
        value: Value,
    ) {
        if let Some(marker) = expect_ref(object) {
            self.fields
                .insert(format!("{marker}|{owner}#{name}{descriptor}"), value);
        }
    }

    /// `list.get(index)` as `invokevirtual AbstractList.get`.
    fn list_get(&mut self, list: &Value, index: i32) -> Result<Value, NativeError> {
        self.call_virtual(
            Site::Virtual,
            ABSTRACT_LIST,
            "get",
            "(I)Ljava/lang/Object;",
            list,
            &[Value::Int(index)],
        )
    }

    /// `list.listIterator(index)` as `invokevirtual AbstractList.listIterator`.
    fn list_iterator_at(&mut self, list: &Value, index: i32) -> Result<Value, NativeError> {
        self.call_virtual(
            Site::Virtual,
            ABSTRACT_LIST,
            "listIterator",
            "(I)Ljava/util/ListIterator;",
            list,
            &[Value::Int(index)],
        )
    }

    /// `AbstractList.rangeCheckForAdd`: `size()` is asked only for a
    /// non-negative index, and again for the message.
    fn range_check_for_add(&mut self, list: &Value, index: i32) -> Result<(), NativeError> {
        if index < 0 || index > self.size_of(Site::Virtual, ABSTRACT_LIST, list)? {
            let size = self.size_of(Site::Virtual, ABSTRACT_LIST, list)?;
            return Err(throw(IOOBE, Some(format!("Index: {index}, Size: {size}"))));
        }
        Ok(())
    }

    /// `AbstractList.subListRangeCheck(from, to, size)`.
    fn sub_list_range_check(from: i32, to: i32, size: i32) -> Result<(), NativeError> {
        if from < 0 {
            return Err(throw(IOOBE, Some(format!("fromIndex = {from}"))));
        }
        if to > size {
            return Err(throw(IOOBE, Some(format!("toIndex = {to}"))));
        }
        if from > to {
            return Err(throw(
                "java/lang/IllegalArgumentException",
                Some(format!("fromIndex({from}) > toIndex({to})")),
            ));
        }
        Ok(())
    }

    /// `new Itr()` / `new ListItr(index)` over `list`.
    fn new_list_itr(&mut self, list: &Value, class: &str, cursor: i32) -> Value {
        let itr = self.new_object(class);
        self.put_jdk_field(&itr, ITR_LIST, list.clone());
        self.put_jdk_field(&itr, CURSOR, Value::Int(cursor));
        self.put_jdk_field(&itr, LAST_RET, Value::Int(-1));
        let expected = self.jdk_field(list, MOD_COUNT);
        self.put_jdk_field(&itr, EXPECTED, expected);
        itr
    }

    /// `Itr.checkForComodification`.
    fn itr_check(&self, itr: &Value) -> Result<(), NativeError> {
        let list = self.jdk_field(itr, ITR_LIST);
        if self.jdk_int_field(&list, MOD_COUNT) != self.jdk_int_field(itr, EXPECTED) {
            return Err(throw(CME, None));
        }
        Ok(())
    }

    /// `Itr.next` (`forward`) and `ListItr.previous`: read `get(i)`, and turn
    /// an `IndexOutOfBoundsException` into the JDK's
    /// `NoSuchElementException(cause)` after a comodification check.
    fn itr_step(&mut self, itr: &Value, forward: bool) -> Result<Value, NativeError> {
        self.itr_check(itr)?;
        let list = self.jdk_field(itr, ITR_LIST);
        let cursor = self.jdk_int_field(itr, CURSOR);
        let i = if forward { cursor } else { cursor - 1 };
        match self.list_get(&list, i) {
            Ok(value) => {
                self.put_jdk_field(itr, LAST_RET, Value::Int(i));
                self.put_jdk_field(itr, CURSOR, Value::Int(if forward { i + 1 } else { i }));
                Ok(value)
            }
            // `throw new NoSuchElementException(e)`: `e` is the cause.
            Err(error) if self.catches(&error, IOOBE) => {
                self.itr_check(itr)?;
                Err(self.throw_caused(NSEE, error))
            }
            Err(error) => Err(error),
        }
    }

    /// A `ListItr` write (`remove`, `set`, `add`): run `call`, then resync
    /// `expectedModCount`; an `IndexOutOfBoundsException` from the list
    /// becomes a `ConcurrentModificationException`.
    fn itr_write(
        &mut self,
        itr: &Value,
        call: impl FnOnce(&mut Self, &Value) -> Result<(), NativeError>,
    ) -> Result<(), NativeError> {
        let list = self.jdk_field(itr, ITR_LIST);
        match call(self, &list) {
            Ok(()) => {
                let count = self.jdk_field(&list, MOD_COUNT);
                self.put_jdk_field(itr, EXPECTED, count);
                Ok(())
            }
            Err(error) if self.catches(&error, IOOBE) => Err(throw(CME, None)),
            Err(error) => Err(error),
        }
    }

    /// `new SubList(root, from, to)` (or `RandomAccessSubList`), or the
    /// nested form whose `parent` is a sublist.
    fn new_sub_list(
        &mut self,
        class: &str,
        outer: &Value,
        nested: bool,
        from: i32,
        to: i32,
    ) -> Value {
        let sub = self.new_object(class);
        let (root, offset) = if nested {
            (
                self.jdk_field(outer, ROOT),
                self.jdk_int_field(outer, OFFSET) + from,
            )
        } else {
            (outer.clone(), from)
        };
        self.put_jdk_field(&sub, ROOT, root.clone());
        self.put_jdk_field(
            &sub,
            PARENT,
            if nested {
                outer.clone()
            } else {
                Value::Ref(None)
            },
        );
        self.put_jdk_field(&sub, OFFSET, Value::Int(offset));
        self.put_jdk_field(&sub, SIZE, Value::Int(to - from));
        let count = self.jdk_field(&root, MOD_COUNT);
        self.put_jdk_field(&sub, MOD_COUNT, count);
        sub
    }

    /// `SubList.checkForComodification`.
    fn sub_check(&self, sub: &Value) -> Result<(), NativeError> {
        let root = self.jdk_field(sub, ROOT);
        if self.jdk_int_field(&root, MOD_COUNT) != self.jdk_int_field(sub, MOD_COUNT) {
            return Err(throw(CME, None));
        }
        Ok(())
    }

    /// `SubList.rangeCheckForAdd`, against the sublist's own `size` field.
    fn sub_range_check_for_add(&self, sub: &Value, index: i32) -> Result<(), NativeError> {
        let size = self.jdk_int_field(sub, SIZE);
        if index < 0 || index > size {
            return Err(throw(IOOBE, Some(format!("Index: {index}, Size: {size}"))));
        }
        Ok(())
    }

    /// `SubList.updateSizeAndModCount(delta)`, up the parent chain.
    fn sub_update(&mut self, sub: &Value, delta: i32) {
        let root = self.jdk_field(sub, ROOT);
        let count = self.jdk_field(&root, MOD_COUNT);
        let mut current = sub.clone();
        while current != Value::Ref(None) {
            let size = self.jdk_int_field(&current, SIZE);
            self.put_jdk_field(&current, SIZE, Value::Int(size.wrapping_add(delta)));
            self.put_jdk_field(&current, MOD_COUNT, count.clone());
            current = self.jdk_field(&current, PARENT);
        }
    }

    /// A member a private class declares itself.
    pub(super) fn internal_body(
        &mut self,
        class: &str,
        name: &str,
        descriptor: &str,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Ref(None));
        let index = |index: usize| int_of(&arg(index));
        match (class, name, descriptor) {
            (ITR, "hasNext", _) => {
                let list = self.jdk_field(this, ITR_LIST);
                let size = self.size_of(Site::Virtual, ABSTRACT_LIST, &list)?;
                Ok(boolean(self.jdk_int_field(this, CURSOR) != size))
            }
            (ITR, "next", _) => self.itr_step(this, true),
            (ITR, "remove", _) => {
                let last = self.jdk_int_field(this, LAST_RET);
                if last < 0 {
                    return Err(throw("java/lang/IllegalStateException", None));
                }
                self.itr_check(this)?;
                self.itr_write(this, |runtime, list| {
                    runtime.call_virtual(
                        Site::Virtual,
                        ABSTRACT_LIST,
                        "remove",
                        "(I)Ljava/lang/Object;",
                        list,
                        &[Value::Int(last)],
                    )?;
                    let cursor = runtime.jdk_int_field(this, CURSOR);
                    if last < cursor {
                        runtime.put_jdk_field(this, CURSOR, Value::Int(cursor - 1));
                    }
                    runtime.put_jdk_field(this, LAST_RET, Value::Int(-1));
                    Ok(())
                })?;
                Ok(Value::Void)
            }
            (LIST_ITR, "hasPrevious", _) => Ok(boolean(self.jdk_int_field(this, CURSOR) != 0)),
            (LIST_ITR, "previous", _) => self.itr_step(this, false),
            (LIST_ITR, "nextIndex", _) => Ok(Value::Int(self.jdk_int_field(this, CURSOR))),
            (LIST_ITR, "previousIndex", _) => Ok(Value::Int(self.jdk_int_field(this, CURSOR) - 1)),
            (LIST_ITR, "set", _) => {
                let last = self.jdk_int_field(this, LAST_RET);
                if last < 0 {
                    return Err(throw("java/lang/IllegalStateException", None));
                }
                self.itr_check(this)?;
                let element = arg(0);
                self.itr_write(this, |runtime, list| {
                    runtime
                        .call_virtual(
                            Site::Virtual,
                            ABSTRACT_LIST,
                            "set",
                            "(ILjava/lang/Object;)Ljava/lang/Object;",
                            list,
                            &[Value::Int(last), element],
                        )
                        .map(drop)
                })?;
                Ok(Value::Void)
            }
            (LIST_ITR, "add", _) => {
                self.itr_check(this)?;
                let element = arg(0);
                self.itr_write(this, |runtime, list| {
                    let i = runtime.jdk_int_field(this, CURSOR);
                    runtime.call_virtual(
                        Site::Virtual,
                        ABSTRACT_LIST,
                        "add",
                        "(ILjava/lang/Object;)V",
                        list,
                        &[Value::Int(i), element],
                    )?;
                    runtime.put_jdk_field(this, LAST_RET, Value::Int(-1));
                    runtime.put_jdk_field(this, CURSOR, Value::Int(i + 1));
                    Ok(())
                })?;
                Ok(Value::Void)
            }
            (SUB_LIST, "set" | "get" | "remove", _) => {
                let i = index(0);
                check_index(i, self.jdk_int_field(this, SIZE))?;
                self.sub_check(this)?;
                let root = self.jdk_field(this, ROOT);
                let at = Value::Int(self.jdk_int_field(this, OFFSET) + i);
                let result = match name {
                    "set" => self.call_virtual(
                        Site::Virtual,
                        ABSTRACT_LIST,
                        "set",
                        descriptor,
                        &root,
                        &[at, arg(1)],
                    )?,
                    "get" => self.call_virtual(
                        Site::Virtual,
                        ABSTRACT_LIST,
                        "get",
                        descriptor,
                        &root,
                        &[at],
                    )?,
                    _ => {
                        let removed = self.call_virtual(
                            Site::Virtual,
                            ABSTRACT_LIST,
                            "remove",
                            descriptor,
                            &root,
                            &[at],
                        )?;
                        self.sub_update(this, -1);
                        removed
                    }
                };
                Ok(result)
            }
            (SUB_LIST, "size", _) => {
                self.sub_check(this)?;
                Ok(Value::Int(self.jdk_int_field(this, SIZE)))
            }
            (SUB_LIST, "add", _) => {
                let i = index(0);
                self.sub_range_check_for_add(this, i)?;
                self.sub_check(this)?;
                let root = self.jdk_field(this, ROOT);
                let at = Value::Int(self.jdk_int_field(this, OFFSET) + i);
                self.call_virtual(
                    Site::Virtual,
                    ABSTRACT_LIST,
                    "add",
                    descriptor,
                    &root,
                    &[at, arg(1)],
                )?;
                self.sub_update(this, 1);
                Ok(Value::Void)
            }
            (SUB_LIST, "removeRange", _) => {
                let (from, to) = (index(0), index(1));
                self.sub_check(this)?;
                let root = self.jdk_field(this, ROOT);
                let offset = self.jdk_int_field(this, OFFSET);
                self.call_virtual(
                    Site::Virtual,
                    ABSTRACT_LIST,
                    "removeRange",
                    descriptor,
                    &root,
                    &[Value::Int(offset + from), Value::Int(offset + to)],
                )?;
                self.sub_update(this, from - to);
                Ok(Value::Void)
            }
            (SUB_LIST, "addAll", "(Ljava/util/Collection;)Z") => {
                let size = Value::Int(self.jdk_int_field(this, SIZE));
                self.call_virtual(
                    Site::Virtual,
                    SUB_LIST,
                    "addAll",
                    "(ILjava/util/Collection;)Z",
                    this,
                    &[size, arg(0)],
                )
            }
            (SUB_LIST, "addAll", _) => {
                let (i, c) = (index(0), arg(1));
                self.sub_range_check_for_add(this, i)?;
                let count = self.size_of(Site::Interface, COLLECTION, &c)?;
                if count == 0 {
                    return Ok(boolean(false));
                }
                self.sub_check(this)?;
                let root = self.jdk_field(this, ROOT);
                let at = Value::Int(self.jdk_int_field(this, OFFSET) + i);
                self.call_virtual(
                    Site::Virtual,
                    ABSTRACT_LIST,
                    "addAll",
                    descriptor,
                    &root,
                    &[at, c],
                )?;
                self.sub_update(this, count);
                Ok(boolean(true))
            }
            (SUB_LIST, "iterator", _) => self.call_virtual(
                Site::Virtual,
                SUB_LIST,
                "listIterator",
                "()Ljava/util/ListIterator;",
                this,
                &[],
            ),
            (SUB_LIST, "listIterator", _) => {
                let i = index(0);
                self.sub_check(this)?;
                self.sub_range_check_for_add(this, i)?;
                let iterator = self.new_object(SUB_LIST_ITERATOR);
                self.put_jdk_field(&iterator, SUB_OUTER, this.clone());
                let root = self.jdk_field(this, ROOT);
                let inner = self.list_iterator_at(&root, self.jdk_int_field(this, OFFSET) + i)?;
                self.put_jdk_field(&iterator, SUB_INNER, inner);
                Ok(iterator)
            }
            (SUB_LIST | RANDOM_ACCESS_SUB_LIST, "subList", _) => {
                let (from, to) = (index(0), index(1));
                Self::sub_list_range_check(from, to, self.jdk_int_field(this, SIZE))?;
                Ok(self.new_sub_list(class, this, true, from, to))
            }
            (SUB_LIST_ITERATOR, _, _) => self.sub_iterator_body(name, this, args),
            _ => Err(NativeError::Metadata(format!(
                "no JDK body for {class}#{name}{descriptor}"
            ))),
        }
    }

    /// `SubList$1`: the sublist's list iterator over the root's.
    fn sub_iterator_body(
        &mut self,
        name: &str,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let outer = self.jdk_field(this, SUB_OUTER);
        let inner = self.jdk_field(this, SUB_INNER);
        let offset = self.jdk_int_field(&outer, OFFSET);
        let inner_call = |runtime: &mut Self, name: &str, descriptor: &str, args: &[Value]| {
            runtime.call_virtual(
                Site::Interface,
                LIST_ITERATOR,
                name,
                descriptor,
                &inner,
                args,
            )
        };
        let own = |runtime: &mut Self, name: &str, descriptor: &str| {
            runtime.call_virtual(
                Site::Virtual,
                SUB_LIST_ITERATOR,
                name,
                descriptor,
                this,
                &[],
            )
        };
        match name {
            "hasNext" => {
                let next = int_of(&own(self, "nextIndex", "()I")?);
                Ok(boolean(next < self.jdk_int_field(&outer, SIZE)))
            }
            "hasPrevious" => Ok(boolean(int_of(&own(self, "previousIndex", "()I")?) >= 0)),
            "next" | "previous" => {
                let (probe, step) = if name == "next" {
                    ("hasNext", "next")
                } else {
                    ("hasPrevious", "previous")
                };
                if !truth(&own(self, probe, "()Z")?) {
                    return Err(throw(NSEE, None));
                }
                inner_call(self, step, "()Ljava/lang/Object;", &[])
            }
            "nextIndex" | "previousIndex" => {
                let index = int_of(&inner_call(self, name, "()I", &[])?);
                Ok(Value::Int(index - offset))
            }
            "remove" => {
                inner_call(self, "remove", "()V", &[])?;
                self.sub_update(&outer, -1);
                Ok(Value::Void)
            }
            "set" => inner_call(self, "set", "(Ljava/lang/Object;)V", args),
            _ => {
                inner_call(self, "add", "(Ljava/lang/Object;)V", args)?;
                self.sub_update(&outer, 1);
                Ok(Value::Void)
            }
        }
    }

    /// An `AbstractList`/`AbstractSequentialList` member or a `List` default.
    pub(super) fn list_body(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Ref(None));
        let index = |index: usize| int_of(&arg(index));
        let own = Site::Virtual;
        match (owner, name, descriptor) {
            (ABSTRACT_LIST, "add", "(Ljava/lang/Object;)Z") => {
                let size = self.size_of(own, ABSTRACT_LIST, this)?;
                self.call_virtual(
                    own,
                    ABSTRACT_LIST,
                    "add",
                    "(ILjava/lang/Object;)V",
                    this,
                    &[Value::Int(size), arg(0)],
                )?;
                Ok(boolean(true))
            }
            (ABSTRACT_LIST, "set" | "add" | "remove", _) => {
                Err(throw("java/lang/UnsupportedOperationException", None))
            }
            (ABSTRACT_LIST, "indexOf" | "lastIndexOf", _) => {
                let o = arg(0);
                let forward = name == "indexOf";
                let it = if forward {
                    self.call_virtual(
                        own,
                        ABSTRACT_LIST,
                        "listIterator",
                        "()Ljava/util/ListIterator;",
                        this,
                        &[],
                    )?
                } else {
                    let size = self.size_of(own, ABSTRACT_LIST, this)?;
                    self.list_iterator_at(this, size)?
                };
                let (probe, step, report) = if forward {
                    ("hasNext", "next", "previousIndex")
                } else {
                    ("hasPrevious", "previous", "nextIndex")
                };
                let iterator = |runtime: &mut Self, name: &str, descriptor: &str| {
                    runtime.call_virtual(Site::Interface, LIST_ITERATOR, name, descriptor, &it, &[])
                };
                while truth(&iterator(self, probe, "()Z")?) {
                    let e = iterator(self, step, "()Ljava/lang/Object;")?;
                    let found = if o == Value::Ref(None) {
                        e == Value::Ref(None)
                    } else {
                        self.java_equals(&o, &e)?
                    };
                    if found {
                        return iterator(self, report, "()I");
                    }
                }
                Ok(Value::Int(-1))
            }
            (ABSTRACT_LIST, "clear", _) => {
                let size = self.size_of(own, ABSTRACT_LIST, this)?;
                self.call_virtual(
                    own,
                    ABSTRACT_LIST,
                    "removeRange",
                    "(II)V",
                    this,
                    &[Value::Int(0), Value::Int(size)],
                )?;
                Ok(Value::Void)
            }
            (ABSTRACT_LIST, "addAll", _) => {
                let (mut at, c) = (index(0), arg(1));
                self.range_check_for_add(this, at)?;
                let it = self.iterator_of(Site::Interface, COLLECTION, &c)?;
                let mut modified = false;
                while self.has_next(&it)? {
                    let e = self.next(&it)?;
                    self.call_virtual(
                        own,
                        ABSTRACT_LIST,
                        "add",
                        "(ILjava/lang/Object;)V",
                        this,
                        &[Value::Int(at), e],
                    )?;
                    at += 1;
                    modified = true;
                }
                Ok(boolean(modified))
            }
            (ABSTRACT_LIST, "iterator", _) => Ok(self.new_list_itr(this, ITR, 0)),
            (ABSTRACT_LIST, "listIterator", "()Ljava/util/ListIterator;") => {
                self.list_iterator_at(this, 0)
            }
            (ABSTRACT_LIST, "listIterator", _) => {
                let at = index(0);
                self.range_check_for_add(this, at)?;
                Ok(self.new_list_itr(this, LIST_ITR, at))
            }
            (ABSTRACT_LIST, "subList", _) => {
                let (from, to) = (index(0), index(1));
                let size = self.size_of(own, ABSTRACT_LIST, this)?;
                Self::sub_list_range_check(from, to, size)?;
                let class = if self.java_instance_of(this, "java/util/RandomAccess")? {
                    RANDOM_ACCESS_SUB_LIST
                } else {
                    SUB_LIST
                };
                Ok(self.new_sub_list(class, this, false, from, to))
            }
            (ABSTRACT_LIST, "equals", _) => {
                let o = arg(0);
                if o == *this {
                    return Ok(boolean(true));
                }
                if !self.java_instance_of(&o, LIST)? {
                    return Ok(boolean(false));
                }
                let e1 = self.call_virtual(
                    own,
                    ABSTRACT_LIST,
                    "listIterator",
                    "()Ljava/util/ListIterator;",
                    this,
                    &[],
                )?;
                let e2 = self.call_virtual(
                    Site::Interface,
                    LIST,
                    "listIterator",
                    "()Ljava/util/ListIterator;",
                    &o,
                    &[],
                )?;
                let step = |runtime: &mut Self, it: &Value, name: &str, descriptor: &str| {
                    runtime.call_virtual(Site::Interface, LIST_ITERATOR, name, descriptor, it, &[])
                };
                while truth(&step(self, &e1, "hasNext", "()Z")?)
                    && truth(&step(self, &e2, "hasNext", "()Z")?)
                {
                    let o1 = step(self, &e1, "next", "()Ljava/lang/Object;")?;
                    let o2 = step(self, &e2, "next", "()Ljava/lang/Object;")?;
                    let equal = if o1 == Value::Ref(None) {
                        o2 == Value::Ref(None)
                    } else {
                        self.java_equals(&o1, &o2)?
                    };
                    if !equal {
                        return Ok(boolean(false));
                    }
                }
                let more = truth(&step(self, &e1, "hasNext", "()Z")?)
                    || truth(&step(self, &e2, "hasNext", "()Z")?);
                Ok(boolean(!more))
            }
            (ABSTRACT_LIST, "hashCode", _) => {
                let it = self.iterator_of(own, ABSTRACT_LIST, this)?;
                let mut hash = 1i32;
                while self.has_next(&it)? {
                    let e = self.next(&it)?;
                    let h = if e == Value::Ref(None) {
                        0
                    } else {
                        self.java_hash(&e)?
                    };
                    hash = hash.wrapping_mul(31).wrapping_add(h);
                }
                Ok(Value::Int(hash))
            }
            (ABSTRACT_LIST, "removeRange", _) => {
                let (from, to) = (index(0), index(1));
                let it = self.list_iterator_at(this, from)?;
                for _ in 0..to.saturating_sub(from).max(0) {
                    self.call_virtual(
                        Site::Interface,
                        LIST_ITERATOR,
                        "next",
                        "()Ljava/lang/Object;",
                        &it,
                        &[],
                    )?;
                    self.call_virtual(Site::Interface, LIST_ITERATOR, "remove", "()V", &it, &[])?;
                }
                Ok(Value::Void)
            }
            (SEQUENTIAL_LIST, _, _) => self.sequential_body(name, descriptor, this, args),
            (LIST, _, _) => self.list_default(name, this, args),
            _ => Err(NativeError::Metadata(format!(
                "no JDK body for {owner}#{name}{descriptor}"
            ))),
        }
    }

    /// `AbstractSequentialList`: each member over `listIterator(index)`, with
    /// a `NoSuchElementException` turned into `IndexOutOfBoundsException:
    /// Index: i`.
    fn sequential_body(
        &mut self,
        name: &str,
        descriptor: &str,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Ref(None));
        if name == "iterator" {
            return self.call_virtual(
                Site::Virtual,
                ABSTRACT_LIST,
                "listIterator",
                "()Ljava/util/ListIterator;",
                this,
                &[],
            );
        }
        let at = int_of(&arg(0));
        let step =
            |runtime: &mut Self, it: &Value, name: &str, descriptor: &str, args: &[Value]| {
                runtime.call_virtual(Site::Interface, LIST_ITERATOR, name, descriptor, it, args)
            };
        let result = (|| {
            let it = self.call_virtual(
                Site::Virtual,
                SEQUENTIAL_LIST,
                "listIterator",
                "(I)Ljava/util/ListIterator;",
                this,
                &[Value::Int(at)],
            )?;
            match (name, descriptor) {
                ("get", _) => step(self, &it, "next", "()Ljava/lang/Object;", &[]),
                ("set", _) => {
                    let old = step(self, &it, "next", "()Ljava/lang/Object;", &[])?;
                    step(self, &it, "set", "(Ljava/lang/Object;)V", &[arg(1)])?;
                    Ok(old)
                }
                ("add", _) => step(self, &it, "add", "(Ljava/lang/Object;)V", &[arg(1)]),
                ("remove", _) => {
                    let removed = step(self, &it, "next", "()Ljava/lang/Object;", &[])?;
                    step(self, &it, "remove", "()V", &[])?;
                    Ok(removed)
                }
                _ => {
                    let source = self.iterator_of(Site::Interface, COLLECTION, &arg(1))?;
                    let mut modified = false;
                    while self.has_next(&source)? {
                        let e = self.next(&source)?;
                        step(self, &it, "add", "(Ljava/lang/Object;)V", &[e])?;
                        modified = true;
                    }
                    Ok(boolean(modified))
                }
            }
        })();
        match result {
            Err(error) if self.catches(&error, NSEE) => {
                Err(throw(IOOBE, Some(format!("Index: {at}"))))
            }
            other => other,
        }
    }

    /// A `List` default; its calls on `this` are `invokeinterface List`.
    fn list_default(
        &mut self,
        name: &str,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let arg = |index: usize| args.get(index).cloned().unwrap_or(Value::Ref(None));
        let call = |runtime: &mut Self, name: &str, descriptor: &str, args: &[Value]| {
            runtime.call_virtual(Site::Interface, LIST, name, descriptor, this, args)
        };
        let step =
            |runtime: &mut Self, it: &Value, name: &str, descriptor: &str, args: &[Value]| {
                runtime.call_virtual(Site::Interface, LIST_ITERATOR, name, descriptor, it, args)
            };
        match name {
            "sort" => {
                let a = call(self, "toArray", "()[Ljava/lang/Object;", &[])?;
                self.call_static(
                    "java/util/Arrays",
                    "sort",
                    "([Ljava/lang/Object;Ljava/util/Comparator;)V",
                    &[a.clone(), arg(0)],
                )?;
                let i = call(self, "listIterator", "()Ljava/util/ListIterator;", &[])?;
                let sorted = self.array(&a)?.clone();
                for e in sorted {
                    step(self, &i, "next", "()Ljava/lang/Object;", &[])?;
                    step(self, &i, "set", "(Ljava/lang/Object;)V", &[e])?;
                }
                Ok(Value::Void)
            }
            "replaceAll" => {
                let operator = arg(0);
                require_non_null(&operator)?;
                let li = call(self, "listIterator", "()Ljava/util/ListIterator;", &[])?;
                while truth(&step(self, &li, "hasNext", "()Z", &[])?) {
                    let e = step(self, &li, "next", "()Ljava/lang/Object;", &[])?;
                    let replaced = self.call_virtual(
                        Site::Interface,
                        "java/util/function/UnaryOperator",
                        "apply",
                        "(Ljava/lang/Object;)Ljava/lang/Object;",
                        &operator,
                        &[e],
                    )?;
                    step(self, &li, "set", "(Ljava/lang/Object;)V", &[replaced])?;
                }
                Ok(Value::Void)
            }
            "addFirst" => {
                call(
                    self,
                    "add",
                    "(ILjava/lang/Object;)V",
                    &[Value::Int(0), arg(0)],
                )?;
                Ok(Value::Void)
            }
            "addLast" => {
                call(self, "add", "(Ljava/lang/Object;)Z", &[arg(0)])?;
                Ok(Value::Void)
            }
            _ => {
                if truth(&call(self, "isEmpty", "()Z", &[])?) {
                    return Err(throw(NSEE, None));
                }
                let at = if name.ends_with("First") {
                    0
                } else {
                    int_of(&call(self, "size", "()I", &[])?) - 1
                };
                if name.starts_with("get") {
                    call(self, "get", "(I)Ljava/lang/Object;", &[Value::Int(at)])
                } else {
                    call(self, "remove", "(I)Ljava/lang/Object;", &[Value::Int(at)])
                }
            }
        }
    }

    /// `Collection.stream()` of a `RandomAccess` list: `List.spliterator`'s
    /// `RandomAccessSpliterator` reads `size()` once and then `get(i)` through
    /// `List.get`, and checks an `AbstractList`'s `modCount` at the end.
    pub(super) fn random_access_values(&mut self, this: &Value) -> Result<Vec<Value>, NativeError> {
        let abstract_list = self.java_instance_of(this, ABSTRACT_LIST)?;
        let expected = abstract_list.then(|| self.jdk_int_field(this, MOD_COUNT));
        let size = self.size_of(Site::Interface, LIST, this)?;
        let mut values = Vec::new();
        for i in 0..size {
            values.push(self.call_virtual(
                Site::Interface,
                LIST,
                "get",
                "(I)Ljava/lang/Object;",
                this,
                &[Value::Int(i)],
            )?);
        }
        if let Some(expected) = expected
            && self.jdk_int_field(this, MOD_COUNT) != expected
        {
            return Err(throw(CME, None));
        }
        Ok(values)
    }
}
