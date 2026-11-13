//! The watch service's objects as host handles (#156 P2): the
//! `StandardWatchEventKinds` constants, and the services, keys and events
//! `watch_service.rs` hands out. Their text forms (`watch-kind/…`,
//! `watch-service/<n>`, `watch-key/<n>`, `watch-event/<n>`) were Strings a
//! jar could spell; `"" + "watch-kind/ENTRY_CREATE"` printed `ENTRY_CREATE`.
//!
//! * a kind's key is its name; there is one object per constant
//!   (canonical), and `toString` is the name (`StdWatchEventKind`);
//! * a service, key or event's key is the number the model minted for it,
//!   one per object, so equal keys are the same object (canonical). None of
//!   their classes overrides `toString`/`hashCode`: identity, counted. Their
//!   state lives in the run's `WatchState`, so they do not cross to another
//!   jar's runtime.
//!
//! Classes are JDK 25's on Linux (`LinuxWatchService`), checked against a
//! Java 25 run (`tests/fixture_host_values_p2.rs`).

use crate::Value;
use crate::host_path::identity_equals;
use crate::host_ref::{ClassHierarchy, HostKind, HostRef, Methods};

/// `StandardWatchEventKinds.<name>`.
pub(crate) fn watch_kind(name: &str) -> Value {
    Value::Host(HostRef::new(HostKind::WatchKind, name))
}

/// The key of a watch handle of `kind`: a kind's name, an object's number.
pub(crate) fn watch_key(value: &Value, kind: HostKind) -> Option<&str> {
    match value {
        Value::Host(handle) if handle.kind() == kind => Some(handle.key()),
        _ => None,
    }
}

fn instance_of(handle: &HostRef, wanted: &str, _: &dyn ClassHierarchy) -> Option<bool> {
    let types: &[&str] = match handle.kind() {
        HostKind::WatchKind => &["java/nio/file/WatchEvent$Kind"],
        HostKind::WatchService => &[
            "sun/nio/fs/AbstractWatchService",
            "java/nio/file/WatchService",
            "java/io/Closeable",
            "java/lang/AutoCloseable",
        ],
        HostKind::WatchKey => &["sun/nio/fs/AbstractWatchKey", "java/nio/file/WatchKey"],
        _ => &["java/nio/file/WatchEvent"],
    };
    Some(wanted == "java/lang/Object" || wanted == handle.class() || types.contains(&wanted))
}

pub(crate) const WATCH_KIND: Methods = Methods {
    to_string: |handle, _| Ok(Some(handle.key().to_owned())),
    hash_code: |_, _| Ok(None),
    equals: |handle, other, _| Ok(identity_equals(handle, other)),
    instance_of,
    crosses_jars: |_, _| true,
};

/// A service, key or event: the run's own state.
pub(crate) const WATCH_OBJECT: Methods = Methods {
    to_string: |_, _| Ok(None),
    crosses_jars: |_, _| false,
    ..WATCH_KIND
};
