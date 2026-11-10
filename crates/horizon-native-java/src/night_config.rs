//! NightConfig's read-with-default contract.
//!
//! TerraBlender and BiomesOPlenty build their configs through NightConfig's
//! `CommentedConfig.getOrElse(key, default)`: the file the runtime never
//! reads is empty, so the call answers the caller's own default. Without it
//! the unmodelled call answered `null`, and the very next `Integer.intValue`
//! refused the jar as a malformed marker.
//!
//! Only `getOrElse` is modelled. A plain `get` of a key nobody set has no
//! answer the runtime could know, so it keeps the neutral result and the
//! unknown-call report instead of inventing a value.

use crate::{MethodRef, Value};

/// The NightConfig interfaces that declare `getOrElse`.
const CONFIG_TYPES: &[&str] = &[
    "com/electronwill/nightconfig/core/UnmodifiableConfig",
    "com/electronwill/nightconfig/core/CommentedConfig",
    "com/electronwill/nightconfig/core/Config",
];

/// The read-with-default call, when `r` is one.
pub(crate) fn invoke(r: &MethodRef, args: &[Value]) -> Option<Value> {
    if !CONFIG_TYPES.contains(&r.owner.as_str()) || r.name != "getOrElse" {
        return None;
    }
    // `(String, T)` and the path-list form `(List<String>, T)` both return
    // the stored value or the default in their second argument.
    if !matches!(
        r.descriptor.as_str(),
        "(Ljava/lang/String;Ljava/lang/Object;)Ljava/lang/Object;"
            | "(Ljava/util/List;Ljava/lang/Object;)Ljava/lang/Object;"
    ) {
        return None;
    }
    args.get(1).cloned()
}
