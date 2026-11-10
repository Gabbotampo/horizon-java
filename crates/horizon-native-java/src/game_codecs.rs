//! The game's codec factories that return an object a bootstrap only stores.
//!
//! `StringRepresentable.fromEnum(Supplier)` builds the `EnumCodec` a class
//! keeps in a static field (fzzy_config's `ContextInput.CODEC`). It never
//! returns null on the JVM, and a Kotlin bootstrap calls
//! `Intrinsics.checkNotNullExpressionValue` on the result, so a neutral null
//! turned a valid class initializer into a NullPointerException. The host
//! answers the object, not its encode/decode behaviour: those calls stay
//! unmodelled and are still reported.

use crate::{Host, MethodRef, Runtime, Value};

impl<H: Host> Runtime<'_, H> {
    /// The game codec factories this module answers; `None` for any other.
    pub(super) fn invoke_game_codec_factory(&mut self, r: &MethodRef) -> Option<Value> {
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            (
                "net/minecraft/util/StringRepresentable",
                "fromEnum",
                "(Ljava/util/function/Supplier;)Lnet/minecraft/util/StringRepresentable$EnumCodec;",
            ) => Some(Value::Ref(Some("string-representable-codec".into()))),
            _ => None,
        }
    }
}
