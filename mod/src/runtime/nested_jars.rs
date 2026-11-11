//! The jars a jar bundles (`META-INF/jars/*.jar`) are on its class path.
//!
//! Fabric's "Jar-in-Jar" (and NeoForge's JarJar) puts a bundled library's
//! classes on the mod's class path: `respackopts` never declares
//! `dev.jfronny.commons.ref.WeakSet` itself, it bundles `libjf-base` and calls
//! it. The runtime read the outer jar's classes only, so the class was missing
//! and the first call on one of its objects failed the listener
//! (`docs/in-game-survey.md`, "Top causes" §5).
//!
//! A bundled jar is linked into the host's class path exactly like a sibling
//! jar the caller linked with `--link` (or the game's dependency linking):
//! same duplicate rule (an earlier jar keeps the class name), same caps, and
//! its `META-INF/services` rows are seeded too. One level only, as a loader
//! does: a jar bundled inside a bundled jar is not expanded again.

use crate::runtime::{NativeJar, StdHost};

/// Where Fabric and NeoForge keep the bundled jars.
const NESTED_DIR: &str = "META-INF/jars/";
/// How many bundled jars one outer jar may contribute. A real bundle is a
/// handful of libraries; anything past this is not a shape a loader ships.
const MAX_NESTED_JARS: usize = 32;

/// One bundled jar's outcome, for a caller that wants to report it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NestedJar {
    /// The entry inside the outer jar (`META-INF/jars/lib.jar`).
    pub entry: String,
    /// Classes the link added, or why the jar was skipped.
    pub result: Result<usize, String>,
}

/// The bundled jars of `jar`, parsed and in entry-name order (so the class
/// path a pack yields does not depend on zip order), up to
/// [`MAX_NESTED_JARS`].
pub fn parsed(jar: &NativeJar) -> Vec<(String, NativeJar)> {
    let mut entries = jar
        .resource_bytes
        .iter()
        .filter(|(name, _)| name.starts_with(NESTED_DIR) && name.ends_with(".jar"))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    entries
        .into_iter()
        .take(MAX_NESTED_JARS)
        .filter_map(|(entry, bytes)| {
            NativeJar::from_bytes(bytes)
                .ok()
                .map(|nested| (entry.clone(), nested))
        })
        .collect()
}

/// Link every bundled jar of `jar` into `host`'s class path.
///
/// A bundled jar that is not a zip, or that would push the linked table past
/// its caps, is skipped with its reason instead of failing the mod: one
/// damaged bundle must not cost the outer jar its own classes.
pub fn link_nested_jars(host: &mut StdHost, jar: &NativeJar) -> Vec<NestedJar> {
    let mut out = Vec::new();
    let mut entries = jar
        .resource_bytes
        .iter()
        .filter(|(name, _)| name.starts_with(NESTED_DIR) && name.ends_with(".jar"))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(right.0));
    for (entry, bytes) in entries.into_iter().take(MAX_NESTED_JARS) {
        let nested = match NativeJar::from_bytes(bytes) {
            Ok(nested) => nested,
            Err(error) => {
                out.push(NestedJar {
                    entry: entry.clone(),
                    result: Err(error.to_string()),
                });
                continue;
            }
        };
        let outer = jar
            .fabric_id
            .as_deref()
            .or(jar.neoforge_id.as_deref())
            .unwrap_or("jar");
        let result = host
            .link_jar(format!("{outer}!{entry}"), &nested)
            .map_err(|error| error.to_string());
        out.push(NestedJar {
            entry: entry.clone(),
            result,
        });
    }
    out
}
