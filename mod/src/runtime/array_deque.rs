//! `ArrayDeque`'s physical layout, as JDK 25 keeps it.
//!
//! `ArrayDeque` has no `modCount`. Its `forEach` walks the live array from
//! `head` to the `tail` it read at the start, handing the action whatever is
//! in each slot (`null` once a removal emptied it), and throws
//! `ConcurrentModificationException` if `tail` moved; its iterator reads the
//! live array at its cursor and throws only when that slot is empty. Both
//! depend on where the elements sit in the array, which the host's list
//! (the elements in order) does not say.
//!
//! The array is a rotation: element `j` of the deque is in slot `(head + j)
//! % capacity` and every other slot is `null` (JDK 25 nulls every slot it
//! vacates). So a capacity and a `head` per deque ([`DequeLayout`]) give
//! every slot. This module follows them through each call, by JDK 25's
//! code: the constructors' capacities (`16 + 1`, `n + 1`, `c.size() + 1`),
//! `addLast`/`addFirst` and `grow` (which slides the first leg of a wrapped
//! array to the end), `pollFirst` moving `head`, `delete(i)` moving the
//! shorter side, `bulkRemove` keeping `head`, `clear` resetting it, and
//! `addAll` growing once first. A call that changed the deque in a way not
//! listed makes the layout unknown, and `forEach` or a step of an iterator
//! is then counted.

use super::*;

/// A deque's capacity and `head`: element `j` is in slot `(head + j) %
/// capacity`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DequeLayout {
    capacity: usize,
    head: usize,
}

impl DequeLayout {
    /// The slot after `slot` (`inc`).
    pub(crate) fn next_slot(self, slot: usize) -> usize {
        (slot + 1) % self.capacity
    }

    /// The slot `tail`, for `size` elements.
    pub(crate) fn tail(self, size: usize) -> usize {
        (self.head + size) % self.capacity
    }

    pub(crate) fn head(self) -> usize {
        self.head
    }

    pub(crate) fn capacity_of(self) -> usize {
        self.capacity
    }

    /// What slot `slot` holds: the element there, or `null`.
    pub(crate) fn slot(self, slot: usize, elements: &[Value]) -> Value {
        let index = (slot + self.capacity - self.head % self.capacity) % self.capacity;
        elements.get(index).cloned().unwrap_or(Value::Ref(None))
    }

    /// `grow(needed)` with `size` elements in (`tail == head` with an element
    /// at `head` is a full array).
    fn grow(self, needed: usize, size: usize) -> Self {
        let old = self.capacity;
        let jump = if old < 64 { old + 2 } else { old >> 1 };
        let capacity = if jump < needed {
            old + needed
        } else {
            old + jump
        };
        let tail = self.tail(size);
        let head = if tail < self.head || (tail == self.head && size > 0) {
            // Wrapped: the first leg slides to the end of the new array.
            self.head + (capacity - old)
        } else {
            self.head
        };
        DequeLayout { capacity, head }
    }

    /// `addLast` of one element onto `size`.
    fn add_last(self, size: usize) -> Self {
        let tail = (self.tail(size) + 1) % self.capacity;
        if tail == self.head {
            self.grow(1, size + 1)
        } else {
            self
        }
    }

    /// `addFirst` of one element onto `size`.
    fn add_first(self, size: usize) -> Self {
        let tail = self.tail(size);
        let head = (self.head + self.capacity - 1) % self.capacity;
        let layout = DequeLayout {
            capacity: self.capacity,
            head,
        };
        if head == tail {
            layout.grow(1, size + 1)
        } else {
            layout
        }
    }
}

/// The capacity a constructor gives, by its arguments (`None`: unknown).
fn constructed_capacity(
    args: &[Value],
    size_of: impl Fn(&Value) -> Option<usize>,
) -> Option<usize> {
    match args {
        [] => Some(16 + 1),
        [Value::Int(n)] => Some(if *n < 1 {
            1
        } else if *n == i32::MAX {
            i32::MAX as usize
        } else {
            *n as usize + 1
        }),
        [source] => size_of(source).map(|size| size + 1),
        _ => None,
    }
}

/// The layout after `name` ran, given the deque before and after it.
fn step(
    layout: DequeLayout,
    name: &str,
    needle: Option<&Value>,
    before: &[Value],
    after: &[Value],
) -> Option<DequeLayout> {
    let (n0, n1) = (before.len(), after.len());
    if n0 == n1 && name != "clear" {
        // A call that inserted or removed nothing moves nothing (`addAll`
        // of nothing grows only past `size + 1 > capacity`, which needs
        // something to add).
        return Some(layout);
    }
    Some(match name {
        "add" | "addLast" | "offer" | "offerLast" if n1 == n0 + 1 => layout.add_last(n0),
        "addFirst" | "offerFirst" | "push" if n1 == n0 + 1 => layout.add_first(n0),
        "addAll" if n1 > n0 => {
            let added = n1 - n0;
            let needed = n0 + added + 1;
            let layout = if needed > layout.capacity {
                layout.grow(needed - layout.capacity, n0)
            } else {
                layout
            };
            let mut layout = layout;
            for size in n0..n1 {
                layout = layout.add_last(size);
            }
            layout
        }
        "poll" | "pollFirst" | "pop" | "removeFirst" | "remove" | "element"
            if n1 + 1 == n0 && after[..] == before[1..] && needle.is_none() =>
        {
            DequeLayout {
                capacity: layout.capacity,
                head: layout.next_slot(layout.head),
            }
        }
        "pollLast" | "removeLast" if n1 + 1 == n0 => layout,
        "remove" | "removeFirstOccurrence" | "removeLastOccurrence" if n1 + 1 == n0 => {
            // `delete(i)` moves the shorter side: `front < back` moves the
            // elements before `i` forward (`head` moves on).
            let index = removed_index(name, needle, before, after)?;
            let (front, back) = (index, n0 - 1 - index);
            if front < back {
                DequeLayout {
                    capacity: layout.capacity,
                    head: layout.next_slot(layout.head),
                }
            } else {
                layout
            }
        }
        // `bulkRemoveModified` compacts towards `head`, which stays.
        "removeIf" | "removeAll" | "retainAll" if n1 < n0 => layout,
        "clear" => DequeLayout {
            capacity: layout.capacity,
            head: 0,
        },
        _ => return None,
    })
}

/// Which element `remove(Object)`/`removeFirstOccurrence`/
/// `removeLastOccurrence` took: the first (last) one with the needle's
/// marker, else the first place the lists differ.
fn removed_index(
    name: &str,
    needle: Option<&Value>,
    before: &[Value],
    after: &[Value],
) -> Option<usize> {
    let without = |index: usize| {
        before
            .iter()
            .enumerate()
            .filter(|(at, _)| *at != index)
            .map(|(_, value)| value)
            .eq(after.iter())
    };
    if let Some(needle) = needle {
        let marker = collection_item_marker(needle);
        let mut matches = before
            .iter()
            .enumerate()
            .filter(|(_, value)| collection_item_marker(value) == marker)
            .map(|(index, _)| index);
        let found = if name == "removeLastOccurrence" {
            matches.next_back()
        } else {
            matches.next()
        };
        if let Some(index) = found.filter(|index| without(*index)) {
            return Some(index);
        }
    }
    let index = before
        .iter()
        .zip(after)
        .position(|(a, b)| a != b)
        .unwrap_or(after.len());
    without(index).then_some(index)
}

impl<H: Host> Runtime<'_, H> {
    /// Whether `value` is a host `ArrayDeque` (or a jar subclass's row).
    pub(crate) fn is_array_deque(&self, value: &Value) -> bool {
        self.host_collection_class(value).as_deref() == Some("java/util/ArrayDeque")
    }

    /// The layout an `ArrayDeque` constructor gives, before it runs.
    pub(crate) fn deque_constructed(&mut self, args: &[Value]) -> Option<DequeLayout> {
        let capacity = constructed_capacity(args, |source| {
            self.host_collection_kind(source)
                .and_then(|_| self.host.mutable_collection_values(source))
                .map(|values| values.len())
        })?;
        Some(DequeLayout { capacity, head: 0 })
    }

    /// Follow a deque's layout through a call that returned.
    pub(crate) fn deque_stepped(
        &mut self,
        marker: &str,
        name: &str,
        needle: Option<&Value>,
        before: &[Value],
    ) {
        let Some(layout) = self.host.deque_layout(marker) else {
            return;
        };
        let after = self
            .collection_elements(&Value::Ref(Some(marker.to_owned())))
            .unwrap_or_default();
        let next = step(layout, name, needle, before, &after);
        self.host.store_deque_layout(marker, next);
    }

    /// `ArrayDeque.forEach`: the live array from `head` to the `tail` read
    /// at the start (`null` for an emptied slot), then a
    /// `ConcurrentModificationException` if `tail` moved. Counted when the
    /// layout is unknown or the array grew under it (the JDK keeps reading
    /// the old one).
    pub(crate) fn deque_for_each(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        action: &Value,
    ) -> Result<Option<Value>, NativeError> {
        if *action == Value::Ref(None) {
            return Err(NativeError::UncaughtException {
                class: "java/lang/NullPointerException".into(),
                message: None,
                frame: None,
            });
        }
        let Some(marker) = expect_ref(receiver).cloned() else {
            return Ok(None);
        };
        let Some(layout) = self.host.deque_layout(&marker) else {
            return self.host.unknown_operand_call(r).map(|_| None);
        };
        let size = self.collection_elements(receiver)?.len();
        let end = layout.tail(size);
        let mut slot = layout.head;
        let mut visited = 0;
        while visited < size {
            let live = self.collection_elements(receiver)?;
            let Some(now) = self
                .host
                .deque_layout(&marker)
                .filter(|now| now.capacity == layout.capacity)
            else {
                // Grown (or unknown): the JDK reads the array it captured.
                return self.host.unknown_operand_call(r).map(|_| None);
            };
            let element = now.slot(slot, &live);
            self.run_functional(
                (
                    "java/util/function/Consumer",
                    "accept",
                    "(Ljava/lang/Object;)V",
                ),
                action,
                &[element],
            )?;
            slot = layout.next_slot(slot);
            visited += 1;
        }
        let live = self.collection_elements(receiver)?.len();
        match self.host.deque_layout(&marker) {
            Some(now) if now.tail(live) != end => Err(NativeError::UncaughtException {
                class: "java/util/ConcurrentModificationException".into(),
                message: None,
                frame: None,
            }),
            Some(_) => Ok(Some(Value::Void)),
            None => self.host.unknown_operand_call(r).map(|_| Some(Value::Void)),
        }
    }
}
