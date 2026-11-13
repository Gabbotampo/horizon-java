//! Host objects as typed handles: `Value::Host(HostRef { kind, key })` (#156).
//!
//! The runtime used to name every host object with text (`optional/…`,
//! `identifier/…`, `list/…`), the same `Value::Ref` a Java `String` is, so a
//! jar string spelled like a marker was that object to every reader that
//! checked the prefix. A [`HostRef`] is a separate `Value` kind: nothing a
//! jar spells can be one.
//!
//! The handle carries its family ([`HostKind`]) and the family's identity
//! text (`key`): what followed the old marker's prefix. Tables keyed by that
//! text keep working, so each family moves on its own (#156's plan, P1–P17).
//!
//! # Identity
//!
//! Each kind says whether two handles with equal keys are one object
//! ([`Identity`], the kind table below):
//!
//! * **canonical**: the JDK hands out one object per key (an enum constant, a
//!   `Class`, `Optional.empty()`, `OptionalInt.empty()`): `==` is key
//!   equality;
//! * **by value**: equal keys are equal values (`equals`/`hashCode` agree)
//!   but may be different objects (`Optional.of(5)` twice, `OptionalInt.of(5)`
//!   twice). An identity operation on two of them (`==`, `!=`) is counted,
//!   unless the same handle provably flows to both sides, which the identity
//!   analysis decides (#126). Equal keys never answer identity.

use std::sync::Arc;

/// A host object: its family and the family's identity text.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HostRef {
    pub kind: HostKind,
    pub key: Arc<str>,
}

/// The families moved onto handles so far; the rest are still `Ref` text
/// (#156's inventory).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum HostKind {
    /// `java.util.Optional`: key `""` when empty, else the held value's
    /// `value_marker`.
    Optional,
    /// `java.util.OptionalInt`: key `""` when empty, else the `int`.
    OptionalInt,
    /// `java.util.OptionalLong`: key `""` when empty, else the `long`.
    OptionalLong,
    /// `java.util.OptionalDouble`: key `""` when empty, else the `double`'s
    /// bits (`f64::to_bits`), so `NaN` and `-0.0` stay exact.
    OptionalDouble,
}

/// When two handles of one kind with equal keys are one object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Identity {
    /// Always: one object per key.
    Canonical,
    /// Never from the key: equal values, maybe different objects.
    ByValue,
    /// The empty key is the kind's singleton (`Optional.empty()`); any other
    /// key is by value.
    CanonicalWhenEmpty,
}

/// One row of the kind table.
struct KindInfo {
    kind: HostKind,
    /// The tag in a value marker (`h<tag>:<key>`); unique per kind.
    tag: &'static str,
    /// The class `getClass()` answers.
    class: &'static str,
    identity: Identity,
}

/// Every kind: its marker tag, its class, its identity rule.
const KINDS: &[KindInfo] = &[
    KindInfo {
        kind: HostKind::Optional,
        tag: "opt",
        class: "java/util/Optional",
        identity: Identity::CanonicalWhenEmpty,
    },
    KindInfo {
        kind: HostKind::OptionalInt,
        tag: "opti",
        class: "java/util/OptionalInt",
        identity: Identity::CanonicalWhenEmpty,
    },
    KindInfo {
        kind: HostKind::OptionalLong,
        tag: "optl",
        class: "java/util/OptionalLong",
        identity: Identity::CanonicalWhenEmpty,
    },
    KindInfo {
        kind: HostKind::OptionalDouble,
        tag: "optd",
        class: "java/util/OptionalDouble",
        identity: Identity::CanonicalWhenEmpty,
    },
];

impl HostKind {
    fn info(self) -> &'static KindInfo {
        KINDS
            .iter()
            .find(|info| info.kind == self)
            .expect("every HostKind has a row in KINDS")
    }

    /// The class an object of this kind is (`getClass()`).
    pub fn class(self) -> &'static str {
        self.info().class
    }

    /// The kind's identity rule.
    pub fn identity(self) -> Identity {
        self.info().identity
    }

    fn from_tag(tag: &str) -> Option<Self> {
        KINDS
            .iter()
            .find(|info| info.tag == tag)
            .map(|info| info.kind)
    }
}

impl HostRef {
    pub fn new(kind: HostKind, key: impl Into<Arc<str>>) -> Self {
        Self {
            kind,
            key: key.into(),
        }
    }

    /// Whether two handles equal to this one are the same object.
    pub fn canonical(&self) -> bool {
        match self.kind.identity() {
            Identity::Canonical => true,
            Identity::ByValue => false,
            Identity::CanonicalWhenEmpty => self.key.is_empty(),
        }
    }

    /// The value-marker form (`h<tag>:<key>`, the key's separators escaped
    /// as a reference's are), for collections and captures.
    pub(crate) fn marker(&self) -> String {
        format!(
            "h{}:{}",
            self.kind.info().tag,
            self.key.replace('\\', "\\\\").replace(',', "\\,")
        )
    }

    /// Read [`Self::marker`] back, without its leading `h`.
    pub(crate) fn parse_marker(body: &str) -> Option<Self> {
        let (tag, key) = body.split_once(':')?;
        Some(Self::new(HostKind::from_tag(tag)?, key))
    }
}

impl<H: crate::Host> crate::Runtime<'_, H> {
    /// `if_acmp` on two handles: different handles are different objects; a
    /// canonical kind's equal handles are one object; equal by-value handles
    /// are counted (Java's answer is an identity the handle does not keep)
    /// and keep the old `true`. `None` when either side is no handle.
    pub(crate) fn host_same_object(
        &mut self,
        lhs: &crate::Value,
        rhs: &crate::Value,
    ) -> Result<Option<bool>, crate::NativeError> {
        let (crate::Value::Host(left), crate::Value::Host(right)) = (lhs, rhs) else {
            return Ok(None);
        };
        if left != right {
            return Ok(Some(false));
        }
        if left.canonical() {
            return Ok(Some(true));
        }
        self.host.unknown_operand_call(&crate::MethodRef {
            owner: "java/lang/Object".into(),
            name: "if_acmp".into(),
            descriptor: "(Ljava/lang/Object;Ljava/lang/Object;)Z".into(),
        })?;
        Ok(Some(true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_one_row_with_a_unique_tag() {
        for (index, info) in KINDS.iter().enumerate() {
            assert!(
                KINDS[index + 1..]
                    .iter()
                    .all(|other| other.tag != info.tag && other.kind != info.kind),
                "{:?}",
                info.kind
            );
            let handle = HostRef::new(info.kind, "a,b\\c");
            assert_eq!(
                HostRef::parse_marker(&handle.marker()[1..]).map(|h| h.kind),
                Some(info.kind)
            );
        }
    }
}
