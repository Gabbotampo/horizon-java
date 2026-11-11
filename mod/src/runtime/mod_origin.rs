//! Fabric Loader's `ModOrigin`: where a mod container came from.
//!
//! A mod that asks the loader about another mod reads
//! `FabricLoader.getModContainer(id).get().getOrigin()` and prints or compares
//! it. The runtime answers `ModContainer.getOrigin()` with a marker naming the
//! container's mod id, and this module answers `ModOrigin`'s members from that
//! id with Fabric Loader 0.19.5's own behaviour
//! (`net/fabricmc/loader/impl/metadata/ModOriginImpl`):
//!
//! * kind `PATH`: `getPaths()` is the container's root path and `toString()`
//!   is every path's text joined with `File.pathSeparator` (`:`, the host
//!   platform is Linux); `getParentModId()`/`getParentSubLocation()` throw
//!   `UnsupportedOperationException("kindPATH doesn't have a parent mod")` and
//!   `"… a parent sub-location"`.
//! * kind `NESTED`: `toString()` is `parentModId:parentSubLocation`,
//!   `getPaths()` throws `"kindNESTED doesn't have paths"`.
//! * kind `UNKNOWN`: `toString()` is `"unknown"`; every accessor throws.
//!
//! Named departure: the path is the runtime's virtual mod root
//! (`/horizon/mods/<id>`, the same value `ModContainer.getRootPaths()` answers),
//! not the jar's real filesystem path — the runtime never exposes host paths.
//! Every container this runtime hands out is a `PATH` origin (a loaded jar), so
//! `NESTED` and `UNKNOWN` markers are only reachable if a jar fabricates one.

use super::*;

/// The marker prefix one `ModOrigin` value carries.
pub(super) const ORIGIN_PREFIX: &str = "mod-origin/";
const ORIGIN_OWNER: &str = "net/fabricmc/loader/api/metadata/ModOrigin";
const ORIGIN_KIND: &str = "net/fabricmc/loader/api/metadata/ModOrigin$Kind";

/// The `ModOrigin` marker for the container `id` names: a `PATH` origin whose
/// one root path is the virtual mod directory.
pub(super) fn origin_value(id: &str) -> Value {
    Value::Ref(Some(format!("{ORIGIN_PREFIX}path/{id}")))
}

/// The root path text the virtual mod directory gives a mod id.
fn root_path(id: &str) -> String {
    format!("/horizon/mods/{id}")
}

/// Answer one `ModOrigin` or `ModOrigin$Kind` call, or `None` when the receiver
/// is not an origin.
pub(super) fn invoke(
    r: &MethodRef,
    receiver: Option<&Value>,
    args: &[Value],
) -> Result<Option<Value>, NativeError> {
    let Some(marker) = receiver.and_then(expect_ref) else {
        return Ok(None);
    };
    // The kind constant (`enum/<Kind>/PATH`): its name, ordinal and text.
    if r.owner == ORIGIN_KIND {
        let Some(named) = marker
            .strip_prefix("enum/")
            .and_then(|rest| rest.strip_prefix(ORIGIN_KIND))
            .and_then(|rest| rest.strip_prefix('/'))
        else {
            return Ok(None);
        };
        return Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
            ("name", "()Ljava/lang/String;") | ("toString", "()Ljava/lang/String;") => {
                Value::Ref(Some(named.to_owned()))
            }
            ("ordinal", "()I") => Value::Int(match named {
                "PATH" => 0,
                "NESTED" => 1,
                _ => 2,
            }),
            _ => return Ok(None),
        }));
    }
    let Some(rest) = marker.strip_prefix(ORIGIN_PREFIX) else {
        return Ok(None);
    };
    let (kind, argument) = rest.split_once('/').unwrap_or((rest, ""));
    let named = kind.to_ascii_uppercase();
    if r.owner != ORIGIN_OWNER {
        return Ok(None);
    }
    Ok(Some(match (r.name.as_str(), r.descriptor.as_str()) {
        ("getKind", "()Lnet/fabricmc/loader/api/metadata/ModOrigin$Kind;") => {
            Value::Ref(Some(format!("enum/{ORIGIN_KIND}/{named}")))
        }
        ("getPaths", "()Ljava/util/List;") => {
            if kind != "path" {
                return Err(unsupported(&format!("kind {named} doesn't have paths")));
            }
            collection_value("list", &[path_value(&root_path(argument))])
        }
        ("getParentModId", "()Ljava/lang/String;") => {
            if kind != "nested" {
                return Err(unsupported(&format!(
                    "kind {named} doesn't have a parent mod"
                )));
            }
            Value::Ref(Some(
                argument
                    .split_once('|')
                    .map_or(argument, |(parent, _)| parent)
                    .to_owned(),
            ))
        }
        ("getParentSubLocation", "()Ljava/lang/String;") => {
            if kind != "nested" {
                return Err(unsupported(&format!(
                    "kind {named} doesn't have a parent sub-location"
                )));
            }
            Value::Ref(Some(
                argument
                    .split_once('|')
                    .map_or_else(String::new, |(_, sub)| sub.to_owned()),
            ))
        }
        // `ModOriginImpl.toString`: a PATH origin joins its paths with the
        // platform path separator, a NESTED one is `parent:sublocation`, and an
        // unknown one is the word `unknown`.
        ("toString", "()Ljava/lang/String;") => Value::Ref(Some(match kind {
            "path" => root_path(argument),
            "nested" => argument.replace('|', ":"),
            _ => "unknown".into(),
        })),
        ("hashCode", "()I") => Value::Int(stable_hash(marker)),
        ("equals", "(Ljava/lang/Object;)Z") => {
            Value::Int(i32::from(args.first().map(expect_string) == Some(marker)))
        }
        _ => return Ok(None),
    }))
}

/// Fabric's refusal for an accessor the origin's kind does not support: an
/// `UnsupportedOperationException` a jar's own `catch` sees.
fn unsupported(message: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/UnsupportedOperationException".into(),
        message: Some(message.to_owned()),
        frame: None,
    }
}
