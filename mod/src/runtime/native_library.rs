//! `System.load`/`System.loadLibrary` (and their `Runtime` twins): a jar that
//! loads native code is refused with a named reason.
//!
//! The bounded runtime runs bytecode only. Before this module `System.load`
//! fell through to the host's unknown-call path and answered `void`, so the
//! jar's next `native` method call met a library that was never loaded, and
//! `Runtime#load` failed as an unnamed missing method. Neither says what the
//! limit is.
//!
//! The refusal is deliberately **not** a Java exception. On a JVM a failed
//! load throws `UnsatisfiedLinkError`, which libraries catch to fall back to
//! a pure-Java variant (Velocity's natives, Netty's transports). Throwing it
//! here would steer the jar onto that fallback, a path the real game does not
//! take when the library loads: a guess. [`NativeError::NativeLibrary`] is not
//! catchable by the jar, so the entrypoint stops and says why.

use crate::runtime::{MethodRef, NativeError, Value, expect_string};

/// The refusal for a native-library load, or `None` for any other call.
///
/// `args` are the call's parameters without a receiver (`Runtime`'s receiver
/// travels separately), so the path or library name is the first one.
pub(crate) fn refusal(r: &MethodRef, args: &[Value]) -> Option<NativeError> {
    let loads = matches!(r.owner.as_str(), "java/lang/System" | "java/lang/Runtime")
        && matches!(r.name.as_str(), "load" | "loadLibrary")
        && r.descriptor == "(Ljava/lang/String;)V";
    if !loads {
        return None;
    }
    let library = match args.first() {
        Some(value @ Value::Ref(Some(_))) => expect_string(value).to_owned(),
        _ => "null".into(),
    };
    Some(NativeError::NativeLibrary {
        call: format!("{}#{}", r.owner, r.name),
        library,
    })
}
