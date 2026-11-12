//! `java.util.PriorityQueue` as JDK 25 runs it.
//!
//! The host kept a priority queue as a FIFO list: `poll` answered the
//! oldest element, not the least (`PriorityQueue<Ver>` of 2,1,9,5 polled
//! `b,a,e,f`; the JDK polls `a,b,f,e`), and its iteration order, which is the
//! heap array's, was insertion order. This module keeps the host's list as
//! that array and runs the JDK's heap operations on it, calling the
//! comparator (or the elements' `compareTo`, `jar_compare.rs`) exactly as
//! `siftUp`/`siftDown`/`heapify` do:
//!
//! - `add`/`offer` append and sift up (`null` throws);
//! - `poll`/`remove()` take the root, move the last element there and sift
//!   it down; `peek`/`element`;
//! - `remove(Object)`/`contains` find an element with `o.equals(e)` in array
//!   order; `removeAt` sifts the last element down, then up;
//! - `removeIf`/`removeAll`/`retainAll` (`bulkRemove`) test each element in
//!   array order, compact, and `heapify`;
//! - the constructors: a capacity (below 1 throws), a comparator, or a
//!   collection (a `SortedSet` or another `PriorityQueue` lends its comparator
//!   and order; anything else is copied and heapified); `addAll` adds each.
//!
//! The comparator lives beside the host's row (`Host::collection_comparator`).

use super::*;

/// The queue's state for one call: its array and comparator (`None`:
/// natural order).
struct Heap {
    es: Vec<Value>,
    comparator: Option<Value>,
}

fn thrown(class: &str) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message: None,
        frame: None,
    }
}

impl<H: Host> Runtime<'_, H> {
    /// `comparator.compare(a, b)`, or `a.compareTo(b)` without one.
    fn heap_compare(
        &mut self,
        comparator: &Option<Value>,
        a: &Value,
        b: &Value,
    ) -> Result<i32, NativeError> {
        match comparator {
            Some(comparator) => self.compare_with(comparator, a, b),
            None => {
                if *a == Value::Ref(None) || *b == Value::Ref(None) {
                    return Err(thrown("java/lang/NullPointerException"));
                }
                self.natural_order(a, b)
            }
        }
    }

    /// `siftUp(k, x)`.
    fn sift_up(&mut self, heap: &mut Heap, mut k: usize, x: Value) -> Result<(), NativeError> {
        while k > 0 {
            let parent = (k - 1) >> 1;
            let e = heap.es[parent].clone();
            if self.heap_compare(&heap.comparator, &x, &e)? >= 0 {
                break;
            }
            heap.es[k] = e;
            k = parent;
        }
        heap.es[k] = x;
        Ok(())
    }

    /// `siftDown(k, x)` over the first `n` elements.
    fn sift_down(
        &mut self,
        heap: &mut Heap,
        mut k: usize,
        x: Value,
        n: usize,
    ) -> Result<(), NativeError> {
        let half = n >> 1;
        while k < half {
            let mut child = (k << 1) + 1;
            let mut c = heap.es[child].clone();
            let right = child + 1;
            if right < n && self.heap_compare(&heap.comparator, &c, &heap.es[right].clone())? > 0 {
                child = right;
                c = heap.es[child].clone();
            }
            if self.heap_compare(&heap.comparator, &x, &c)? <= 0 {
                break;
            }
            heap.es[k] = c;
            k = child;
        }
        heap.es[k] = x;
        Ok(())
    }

    /// `heapify()`.
    fn heapify(&mut self, heap: &mut Heap) -> Result<(), NativeError> {
        let n = heap.es.len();
        for i in (0..(n >> 1)).rev() {
            let x = heap.es[i].clone();
            self.sift_down(heap, i, x, n)?;
        }
        Ok(())
    }

    /// `removeAt(i)`.
    fn remove_at(&mut self, heap: &mut Heap, i: usize) -> Result<(), NativeError> {
        let s = heap.es.len() - 1;
        let moved = heap.es.pop().unwrap_or(Value::Ref(None));
        if s != i {
            self.sift_down(heap, i, moved.clone(), s)?;
            if heap.es[i] == moved {
                self.sift_up(heap, i, moved)?;
            }
        }
        Ok(())
    }

    /// `indexOf(o)`: `o.equals(es[i])` in array order.
    fn heap_index_of(&mut self, heap: &Heap, o: &Value) -> Result<Option<usize>, NativeError> {
        if *o == Value::Ref(None) {
            return Ok(None);
        }
        for (index, element) in heap.es.iter().enumerate() {
            if self.java_equals(o, element)? {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    /// Whether `value` is a host `PriorityQueue` (or a jar subclass's row).
    fn is_priority_queue(&self, value: &Value) -> bool {
        self.host_collection_class(value).as_deref() == Some("java/util/PriorityQueue")
    }

    /// The members of a `PriorityQueue`. `Ok(None)` leaves the call to the
    /// rest of the dispatch.
    pub(crate) fn invoke_priority_queue(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(receiver) = receiver.filter(|_| matches!(opcode, 0xb6 | 0xb7 | 0xb9)) else {
            return Ok(None);
        };
        if r.name == "<init>" {
            if r.owner != "java/util/PriorityQueue" {
                return Ok(None);
            }
            return self.construct_priority_queue(receiver, args);
        }
        if !self.is_priority_queue(receiver) {
            return Ok(None);
        }
        let Some(marker) = expect_ref(receiver).cloned() else {
            return Ok(None);
        };
        let mut heap = Heap {
            es: self.collection_elements(receiver)?,
            comparator: self.host.collection_comparator(&marker),
        };
        let before = heap.es.clone();
        let result = self.priority_queue_member(r, receiver, &mut heap, args);
        // The array as it is now, even after a comparator threw mid-sift
        // (the JDK's array is left the same way).
        if heap.es != before {
            self.host.replace_mutable_collection(receiver, heap.es);
        }
        result
    }

    fn priority_queue_member(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        heap: &mut Heap,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let boolean = |flag: bool| Some(Value::Int(i32::from(flag)));
        match (r.name.as_str(), r.descriptor.as_str(), args) {
            ("add" | "offer", "(Ljava/lang/Object;)Z", [element]) => {
                if *element == Value::Ref(None) {
                    return Err(thrown("java/lang/NullPointerException"));
                }
                let k = heap.es.len();
                heap.es.push(element.clone());
                self.sift_up(heap, k, element.clone())?;
                Ok(boolean(true))
            }
            ("poll" | "remove", "()Ljava/lang/Object;", []) => {
                if heap.es.is_empty() {
                    if r.name == "remove" {
                        return Err(thrown("java/util/NoSuchElementException"));
                    }
                    return Ok(Some(Value::Ref(None)));
                }
                let result = heap.es[0].clone();
                let x = heap.es.pop().unwrap_or(Value::Ref(None));
                let n = heap.es.len();
                if n > 0 {
                    self.sift_down(heap, 0, x, n)?;
                }
                Ok(Some(result))
            }
            ("peek" | "element", "()Ljava/lang/Object;", []) => match heap.es.first() {
                Some(first) => Ok(Some(first.clone())),
                None if r.name == "element" => Err(thrown("java/util/NoSuchElementException")),
                None => Ok(Some(Value::Ref(None))),
            },
            ("remove", "(Ljava/lang/Object;)Z", [element]) => {
                match self.heap_index_of(heap, element)? {
                    Some(index) => {
                        self.remove_at(heap, index)?;
                        Ok(boolean(true))
                    }
                    None => Ok(boolean(false)),
                }
            }
            ("contains", "(Ljava/lang/Object;)Z", [element]) => {
                Ok(boolean(self.heap_index_of(heap, element)?.is_some()))
            }
            ("clear", "()V", []) => {
                heap.es.clear();
                Ok(Some(Value::Void))
            }
            ("comparator", "()Ljava/util/Comparator;", []) => {
                Ok(Some(heap.comparator.clone().unwrap_or(Value::Ref(None))))
            }
            ("addAll", "(Ljava/util/Collection;)Z", [source]) => {
                if *source == Value::Ref(None) {
                    return Err(thrown("java/lang/NullPointerException"));
                }
                if source == receiver {
                    return Err(thrown("java/lang/IllegalArgumentException"));
                }
                if self.host_collection_kind(source).is_none() && !self.is_priority_queue(source) {
                    return Ok(None);
                }
                let mut changed = false;
                for element in self.collection_elements(source)? {
                    if element == Value::Ref(None) {
                        return Err(thrown("java/lang/NullPointerException"));
                    }
                    let k = heap.es.len();
                    heap.es.push(element.clone());
                    self.sift_up(heap, k, element)?;
                    changed = true;
                }
                Ok(boolean(changed))
            }
            (name @ ("removeAll" | "retainAll"), "(Ljava/util/Collection;)Z", [source]) => {
                if *source == Value::Ref(None) {
                    return Err(thrown("java/lang/NullPointerException"));
                }
                let retain = name == "retainAll";
                let mut doomed = Vec::with_capacity(heap.es.len());
                for element in heap.es.clone() {
                    let Some(contained) = self.argument_contains(source, &element)? else {
                        return self.host.unknown_operand_call(r).map(Some);
                    };
                    doomed.push(contained != retain);
                }
                self.bulk_remove(heap, &doomed).map(boolean)
            }
            ("removeIf", "(Ljava/util/function/Predicate;)Z", [predicate]) => {
                let mut doomed = Vec::with_capacity(heap.es.len());
                for element in heap.es.clone() {
                    let test = self.apply_callback(predicate, std::slice::from_ref(&element))?;
                    doomed.push(matches!(test, Value::Int(flag) if flag != 0));
                }
                self.bulk_remove(heap, &doomed).map(boolean)
            }
            _ => Ok(None),
        }
    }

    /// `bulkRemove`: drop the doomed elements, keep the rest in array order,
    /// and `heapify` when anything went.
    fn bulk_remove(&mut self, heap: &mut Heap, doomed: &[bool]) -> Result<bool, NativeError> {
        if !doomed.contains(&true) {
            return Ok(false);
        }
        let mut kept = Vec::with_capacity(heap.es.len());
        for (element, doomed) in heap.es.iter().zip(doomed) {
            if !doomed {
                kept.push(element.clone());
            }
        }
        heap.es = kept;
        self.heapify(heap)?;
        Ok(true)
    }

    /// The constructors.
    fn construct_priority_queue(
        &mut self,
        receiver: &Value,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        let Some(marker) = expect_ref(receiver).cloned() else {
            return Ok(None);
        };
        let empty = MethodRef {
            owner: "java/util/PriorityQueue".into(),
            name: "<init>".into(),
            descriptor: "()V".into(),
        };
        let (capacity, comparator, source) = match args {
            [] => (None, None, None),
            [Value::Int(capacity)] => (Some(*capacity), None, None),
            [Value::Int(capacity), comparator] => (Some(*capacity), Some(comparator.clone()), None),
            [other]
                if self.host_collection_kind(other).is_some() || self.is_priority_queue(other) =>
            {
                (None, None, Some(other.clone()))
            }
            [Value::Ref(None)] => return Err(thrown("java/lang/NullPointerException")),
            [comparator] => (None, Some(comparator.clone()), None),
            _ => return Ok(None),
        };
        if capacity.is_some_and(|capacity| capacity < 1) {
            return Err(thrown("java/lang/IllegalArgumentException"));
        }
        self.host.invoke_instance(&empty, receiver, &[])?;
        let comparator = comparator.filter(|comparator| *comparator != Value::Ref(None));
        let Some(source) = source else {
            if let Some(comparator) = &comparator {
                self.host
                    .set_collection_comparator(&marker, comparator.clone());
            }
            return Ok(Some(Value::Void));
        };
        let elements = self.collection_elements(&source)?;
        let source_marker = expect_ref(&source).cloned().unwrap_or_default();
        let sorted_set = self
            .host_collection_class(&source)
            .is_some_and(|class| class == "java/util/TreeSet");
        let lent = if sorted_set || self.is_priority_queue(&source) {
            self.host.collection_comparator(&source_marker)
        } else {
            None
        };
        // `initElementsFromCollection` checks for nulls only for one element
        // or with a comparator; `heapify` meets the rest.
        if (elements.len() == 1 || lent.is_some()) && elements.contains(&Value::Ref(None)) {
            return Err(thrown("java/lang/NullPointerException"));
        }
        let mut heap = Heap {
            es: elements,
            comparator: lent.clone(),
        };
        // A `SortedSet` is already in order; a `PriorityQueue` (exactly that
        // class) is already a heap; anything else is heapified.
        let already = sorted_set
            || (self.is_priority_queue(&source)
                && source_marker.starts_with("java/util/PriorityQueue#"));
        let result = if already {
            Ok(())
        } else {
            self.heapify(&mut heap)
        };
        if let Some(comparator) = lent {
            self.host.set_collection_comparator(&marker, comparator);
        }
        self.host.replace_mutable_collection(receiver, heap.es);
        result.map(|()| Some(Value::Void))
    }
}
