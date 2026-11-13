//! `java.nio.file.Path` and NeoForge's `FMLPaths` constants as host handles
//! (#156 P2), written from the JDK 25 sources (`sun.nio.fs.UnixPath`) and
//! checked against a Java 25 run (`tests/fixture_host_values_p2.rs`).
//!
//! A path used to be the text `path:<p>`, so the jar string `"path:/x"` was
//! the path `/x` to every reader (`Files.exists("path:/x"...)` aside, it
//! printed as `/x`, was `instanceof Path` and `equals` the real path).
//!
//! * **Path**: key = the path text as `UnixPath` keeps it
//!   (`normalizeAndCheck`: repeated slashes one, no trailing slash but the
//!   root's), so two spellings of one path are one key, which is exactly what
//!   `UnixPath.equals` compares (the bytes of that text). Class
//!   `sun/nio/fs/UnixPath`. Not canonical: `Path.of("x")` twice is two
//!   objects. `hashCode` is `UnixPath.hashCode` over the path's bytes in
//!   `sun.jnu.encoding`; that encoding is the machine's locale, so a non-ASCII
//!   path's hash is counted (only ASCII is the same everywhere).
//! * **FMLPaths**: an enum constant of `net.neoforged.fml.loading.FMLPaths`,
//!   key = its name (`GAMEDIR`). Canonical. `toString` is `Enum.toString`
//!   (the name: `FMLPaths` does not override it), `hashCode` the identity
//!   hash (counted). `get()` answers the virtual root the loader hands out
//!   (`neoforge_loader.rs`).

use crate::runtime::Value;
use crate::runtime::host_ref::{ClassHierarchy, HostKind, HostRef, Methods, instance_of_class};

/// `self == other` as `Object.equals` answers it: `None` when only the JVM
/// knows (equal keys of a kind that is not canonical, two constructions).
pub(crate) fn identity_equals(handle: &HostRef, other: &Value) -> Option<bool> {
    match other {
        Value::Host(other) => handle.same_object(other),
        _ => Some(false),
    }
}

/// `UnixPath`'s supertypes (and itself), for `instanceof`/`checkcast`.
const PATH_TYPES: &[&str] = &[
    "sun/nio/fs/UnixPath",
    "java/nio/file/Path",
    "java/lang/Comparable",
    "java/lang/Iterable",
    "java/nio/file/Watchable",
    "java/lang/Object",
];

/// A path as `UnixPath` (and `java.io.File`'s `UnixFileSystem.normalize`)
/// keeps it: repeated slashes collapsed and a trailing slash dropped, the
/// root `/` kept.
pub(crate) fn unix_normal(path: &str) -> String {
    let mut normal = String::with_capacity(path.len());
    for c in path.chars() {
        if !(c == '/' && normal.ends_with('/')) {
            normal.push(c);
        }
    }
    if normal.len() > 1 && normal.ends_with('/') {
        normal.pop();
    }
    normal
}

/// `Path.of(text)`: a new path object.
pub(crate) fn path_value(text: &str) -> Value {
    Value::Host(HostRef::new(HostKind::Path, unix_normal(text)))
}

/// The text of a `Path` handle, `None` for any other value.
pub(crate) fn path_key(value: &Value) -> Option<&str> {
    match value {
        Value::Host(handle) if handle.kind() == HostKind::Path => Some(handle.key()),
        _ => None,
    }
}

/// The path text a call's `Path` (or, for the `java.io` overloads that take
/// one, `String`) argument names: a path handle's text, a string as it is,
/// `""` for anything else. A string is never read as a path handle.
pub(crate) fn path_text(value: &Value) -> &str {
    match value {
        Value::Ref(Some(text)) => text,
        value => path_key(value).unwrap_or_default(),
    }
}

/// `UnixPath.hashCode`: `31 * h + (byte & 0xff)` over the path's bytes.
fn path_hash(path: &str) -> Option<i32> {
    path.is_ascii().then(|| {
        path.bytes().fold(0i32, |hash, byte| {
            hash.wrapping_mul(31).wrapping_add(i32::from(byte))
        })
    })
}

/// `UnixPath.compareTo`: the first differing byte's difference
/// (`b & 0xff`), else the length difference. Bytes are `sun.jnu.encoding`'s,
/// so only two ASCII paths compare the same everywhere; `None` (counted)
/// otherwise, or for anything that is no path (Java's `ClassCastException`).
fn path_compare(path: &str, other: &Value) -> Option<i32> {
    let other = path_key(other)?;
    if !path.is_ascii() || !other.is_ascii() {
        return None;
    }
    Some(
        path.bytes()
            .zip(other.bytes())
            .find(|(a, b)| a != b)
            .map_or(path.len() as i32 - other.len() as i32, |(a, b)| {
                i32::from(a) - i32::from(b)
            }),
    )
}

pub(crate) const PATH: Methods = Methods {
    to_string: |handle, _| Ok(Some(handle.key().to_owned())),
    hash_code: |handle, _| Ok(path_hash(handle.key())),
    // `obj instanceof UnixPath p && compareTo(p) == 0`: the same bytes.
    equals: |handle, other, _| Ok(Some(path_key(other) == Some(handle.key()))),
    instance_of: |_, wanted, _| Some(PATH_TYPES.contains(&wanted)),
    crosses_jars: |_, _| true,
    compare_to: Some(|handle, other, _| Ok(path_compare(handle.key(), other))),
};

/// `FMLPaths`.
pub(crate) const FML_PATHS_CLASS: &str = "net/neoforged/fml/loading/FMLPaths";

/// `FMLPaths.<name>`, for the constants the loader answers.
pub(crate) fn fml_path_value(name: &str) -> Value {
    Value::Host(HostRef::new(HostKind::FmlPath, name))
}

/// The constant's name, `None` for any other value.
pub(crate) fn fml_path_name(value: &Value) -> Option<&str> {
    match value {
        Value::Host(handle) if handle.kind() == HostKind::FmlPath => Some(handle.key()),
        _ => None,
    }
}

pub(crate) const FML_PATHS: Methods = Methods {
    to_string: |handle, _| Ok(Some(handle.key().to_owned())),
    hash_code: |_, _| Ok(None),
    equals: |handle, other, _| Ok(identity_equals(handle, other)),
    instance_of: fml_paths_instance_of,
    crosses_jars: |_, _| true,
    // `Enum.compareTo` is the ordinal difference; the model keeps no
    // ordinals for `FMLPaths`: counted.
    compare_to: None,
};

/// An enum constant: its class, `Enum` and `Enum`'s interfaces.
fn fml_paths_instance_of(
    handle: &HostRef,
    wanted: &str,
    hierarchy: &dyn ClassHierarchy,
) -> Option<bool> {
    if matches!(
        wanted,
        "java/lang/Enum"
            | "java/lang/Comparable"
            | "java/io/Serializable"
            | "java/lang/constant/Constable"
    ) {
        return Some(true);
    }
    instance_of_class(handle, wanted, hierarchy)
}
