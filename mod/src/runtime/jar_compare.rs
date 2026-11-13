//! Natural order (`Comparable.compareTo`) of jar objects, for every modelled
//! comparator, sort and priority queue.
//!
//! The modelled comparators compared jar objects by their marker strings
//! (construction order) or refused them as "not modelled". A record that
//! implements `Comparable` sorts by its own `compareTo` on a JVM, which may
//! count, log or throw: [`Runtime::natural_order`] runs it through the
//! interpreter (javac's `compareTo(Object)` bridge, so the checkcast to the
//! record's type happens as it does on a JVM). An enum constant compares by
//! ordinal (`Enum.compareTo`). A jar object with no `compareTo` body, or a
//! platform value compared with a jar object, is counted as
//! `Comparable#compareTo` and answers `0`.

use super::*;

/// `Comparable.compareTo(Object)`, the erased bridge every implementer has.
const COMPARE_TO: &str = "(Ljava/lang/Object;)I";

impl<H: Host> Runtime<'_, H> {
    /// `((Comparable) a).compareTo(b)`.
    pub(crate) fn natural_order(&mut self, a: &Value, b: &Value) -> Result<i32, NativeError> {
        // A host handle's natural order is its kind's row (`host_ref.rs`);
        // a kind with none, or a pair it cannot compare, is counted.
        if let Value::Host(handle) = a {
            return match self.host_ref_compare(handle, b)? {
                Some(order) => Ok(order),
                None => self.count_compare_to(),
            };
        }
        let Some(class) = self.interpreted_object_class(a) else {
            if self.interpreted_object_class(b).is_some() {
                // A platform value's `compareTo` of a jar object: a JVM throws
                // `ClassCastException` or runs code the host does not hold.
                return self.count_compare_to();
            }
            return jdk_stream::natural_compare(a, b);
        };
        if let Some((owner, true)) = self.member_owner(&class, "compareTo", COMPARE_TO)
            && self
                .class(&owner)
                .and_then(|file| file.method("compareTo", COMPARE_TO))
                .is_some_and(|method| method.code.is_some())
        {
            return match self.invoke(
                &owner,
                "compareTo",
                COMPARE_TO,
                Some(a.clone()),
                std::slice::from_ref(b),
            )? {
                Value::Int(order) => Ok(order),
                other => Err(NativeError::Metadata(format!(
                    "{owner}.compareTo answered {}",
                    describe_value(&other)
                ))),
            };
        }
        // `Enum.compareTo` is `ordinal - other.ordinal` (same enum class).
        let ordinal = |value: &Value| {
            expect_ref(value)
                .and_then(|marker| self.enum_constants.get(marker))
                .map(|(_, ordinal)| *ordinal)
        };
        if let (Some(left), Some(right)) = (ordinal(a), ordinal(b)) {
            return Ok(left - right);
        }
        self.count_compare_to()
    }

    /// Count `Comparable.compareTo` as unknown and answer "equal".
    fn count_compare_to(&mut self) -> Result<i32, NativeError> {
        self.host.unknown_operand_call(&MethodRef {
            owner: "java/lang/Comparable".into(),
            name: "compareTo".into(),
            descriptor: COMPARE_TO.into(),
        })?;
        Ok(0)
    }
}
