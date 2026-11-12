//! Which jars in one mods folder depend on which, and the order to run them.
//!
//! A real Fabric or NeoForge loader puts every installed mod on one class path
//! and orders the entrypoint run by the metadata's dependency graph. The native
//! path gives each jar its own host, so a jar that calls a class from a declared
//! dependency only reaches it when the caller links that sibling jar
//! ([`crate::runtime::StdHost::link_jar`]). This module answers the graph questions the
//! game-side loader needs without reading bytes: the dependency-first run
//! order, each jar's transitive siblings to link, and the required mod ids that
//! no sibling or host-provided surface answers.

use std::collections::{BTreeMap, BTreeSet};

use crate::runtime::NativeJar;

/// Mod ids the host itself answers, so a jar that requires one is not missing
/// a sibling jar.
///
/// The loaders and Java are always present on a real loader. `fabric-api` is
/// modelled by the host's Fabric API surface (`platform_class_present`), which
/// is why the corpus runs Fabric mods without shipping the API jar.
///
/// A mod may depend on one Fabric API module instead of the umbrella id. The
/// modules listed here are the ones whose entry points that surface answers,
/// to the same degree it answers them under `fabric-api`:
/// - `fabric-api-base`: `Event` and `EventFactory.createArrayBacked`;
/// - `fabric-lifecycle-events-v1`: the server lifecycle and tick events, which
///   the game fires (`events.rs`);
/// - `fabric-command-api-v2`: `CommandRegistrationCallback`, fired with the
///   command façade;
/// - `fabric-resource-loader-v1`: `ResourceLoader.get`; a reload listener is
///   accepted and never runs (the runtime has no resource reload).
///
/// Any other module id still has to be a jar: listing it would claim a
/// surface the host does not model.
pub const HOST_PROVIDED_MODS: &[&str] = &[
    "minecraft",
    "java",
    "fabricloader",
    "fabric-api",
    "fabric-api-base",
    "fabric-lifecycle-events-v1",
    "fabric-command-api-v2",
    "fabric-resource-loader-v1",
    "neoforge",
    "forge",
];

/// The graph answers for one folder of jars, indexed like the input slice.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LoadPlan {
    /// Every jar's index, dependencies before their dependents. A dependency
    /// cycle keeps the input order for the jars it closes over.
    pub order: Vec<usize>,
    /// For each jar, the required mod ids that no sibling and no host-provided
    /// surface answers. A jar with a row here cannot load.
    pub missing: BTreeMap<usize, Vec<String>>,
    /// For each jar, the sibling indices to link, dependencies first. This is
    /// the transitive closure, because a dependency's own dependencies are on
    /// the class path of a real loader too.
    pub links: Vec<Vec<usize>>,
}

impl LoadPlan {
    /// The siblings jar `index` links, in class-path order (deepest dependency
    /// first).
    pub fn links_for(&self, index: usize) -> &[usize] {
        self.links.get(index).map_or(&[], Vec::as_slice)
    }
}

/// One jar's mod ids, lowercased because loader mod ids are compared without
/// case.
///
/// The jar's own Fabric and NeoForge ids, then the mods its bundled jars make
/// resolvable, by each loader's own rule: [`fabric_provided`] (a mod answers
/// for its `provides` aliases too, and the jars its `jars` list names are mods,
/// recursively) and [`neoforge_provided`] (the jars its JarJar metadata names
/// are mod files, recursively). `libjf` ships `libjf-base` the Fabric way, and
/// `libjf-base` provides `libjf_base`, the spelling `respackopts`' NeoForge
/// `[[dependencies]]` use; `respackopts` bundles its `muscript` the NeoForge
/// way.
///
/// One departure: an id the host answers ([`HOST_PROVIDED_MODS`]) is not
/// counted when only a bundled jar or a `provides` alias names it. `libjf-base` bundles `fabric-lifecycle-events-v1`; a real
/// loader lets that copy satisfy `spark`'s dependency, but here the host's
/// model answers it, and counting the copy would only link `libjf` into
/// `spark`'s class path and order it first. Whether the id resolves is the
/// same either way.
fn mod_ids(jar: &NativeJar) -> Vec<String> {
    let mut bundled = Vec::new();
    let entry = |name: &str| jar.resource_bytes.get(name).cloned();
    let mut budget = MAX_NESTED_MODS;
    fabric_provided(&entry, 0, &mut budget, &mut bundled);
    let mut budget = MAX_NESTED_MODS;
    neoforge_provided(&entry, 0, &mut budget, &mut bundled);
    let mut ids: Vec<String> = [jar.fabric_id.as_deref(), jar.neoforge_id.as_deref()]
        .into_iter()
        .flatten()
        .map(str::to_owned)
        .chain(bundled.into_iter().filter(|id| !is_host_provided(id)))
        .collect();
    ids = ids.iter().map(|id| id.to_ascii_lowercase()).collect();
    ids.sort();
    ids.dedup();
    ids
}

/// How deep Jar-in-Jar is followed. Both loaders recurse without a depth
/// bound; a real bundle is two levels deep (`libjf` → `libjf-base` → a Fabric
/// API module).
const MAX_NESTING: usize = 8;
/// How many nested jars one top-level jar may contribute ids from, per loader.
const MAX_NESTED_MODS: usize = 256;
/// The largest entry read from a nested jar (the top level reads the outer
/// jar's own bounded resource bytes).
const MAX_NESTED_ENTRY_BYTES: u64 = 16 << 20;

/// Answers one entry of a jar by its exact name.
type Entries<'a> = dyn Fn(&str) -> Option<Vec<u8>> + 'a;

/// The ids one jar's `fabric.mod.json` makes resolvable, appended to `ids`,
/// as Fabric Loader 0.19.5 does it:
///
/// - `ModResolver.selectMod` puts the candidate in the selected-mods map
///   under `getId()` and under every id of `getProvides()` (the metadata's
///   `provides`), so a dependency on any of them is satisfied by it.
/// - `ModDiscoverer$ModScanTask` reads `getJars()` (the metadata's `jars`)
///   and, for each `file`, looks the entry up by that exact name
///   (`ZipFile.getEntry`); `isValidNestedJarEntry` keeps it when it exists,
///   is not a directory and its name ends with `.jar`. Each kept jar is
///   scanned the same way (`computeNestedMods`), so its own `jars` are mods
///   too. A jar in `META-INF/jars/` that the list does not name is not a mod,
///   and an entry without a `fabric.mod.json` contributes nothing.
///
/// At `depth` 0 the jar is the top-level one, whose id is already counted.
fn fabric_provided(entry: &Entries<'_>, depth: usize, budget: &mut usize, ids: &mut Vec<String>) {
    let Some(metadata) = entry("fabric.mod.json")
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    else {
        return;
    };
    if depth > 0
        && let Some(id) = metadata.get("id").and_then(serde_json::Value::as_str)
    {
        ids.push(id.to_owned());
    }
    if let Some(provides) = metadata
        .get("provides")
        .and_then(serde_json::Value::as_array)
    {
        ids.extend(
            provides
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned),
        );
    }
    let files = metadata
        .get("jars")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|jar| jar.get("file").and_then(serde_json::Value::as_str))
        .filter(|file| file.ends_with(".jar"));
    for file in files {
        nested(entry, file, depth, budget, &mut |nested, budget| {
            fabric_provided(nested, depth + 1, budget, ids)
        });
    }
}

/// The ids one jar's NeoForge metadata makes resolvable, appended to `ids`,
/// as FancyModLoader 12.0.12 with JarJarSelector 0.5.3 does it:
///
/// - `JarSelector.detect` reads `META-INF/jarjar/metadata.json`
///   (`Constants.CONTAINED_JARS_METADATA_PATH`) from each mod file and
///   `recursivelyDetectContainedJars` follows every `jars[].path` into the
///   jar it names, reading that jar's own metadata the same way.
/// - `JarInJarDependencyLocator.scanMods` adds each selected jar to the
///   discovery pipeline as a mod file, so every `[[mods]]` `modId` of its
///   `META-INF/neoforge.mods.toml` is a mod.
///
/// Not modelled: JarJar's version selection (`JarSelector.select` keeps one
/// version per `group:artifact` and fails the launch on disjoint ranges);
/// every named jar answers for its ids here. A named path that is missing
/// fails a NeoForge launch (`Mod file {} declares Jar-in-Jar {} but does not
/// contain it`) and is skipped here.
fn neoforge_provided(entry: &Entries<'_>, depth: usize, budget: &mut usize, ids: &mut Vec<String>) {
    if depth > 0 {
        let toml = entry("META-INF/neoforge.mods.toml").or_else(|| entry("META-INF/mods.toml"));
        if let Some(text) = toml.and_then(|bytes| String::from_utf8(bytes).ok()) {
            ids.extend(neoforge_mod_ids(&text));
        }
    }
    let Some(metadata) = entry("META-INF/jarjar/metadata.json")
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    else {
        return;
    };
    let paths = metadata
        .get("jars")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|jar| jar.get("path").and_then(serde_json::Value::as_str));
    for path in paths {
        nested(entry, path, depth, budget, &mut |nested, budget| {
            neoforge_provided(nested, depth + 1, budget, ids)
        });
    }
}

/// Open the nested jar `name` of the jar `entry` reads and hand its entries
/// to `scan`, within [`MAX_NESTING`] and the caller's jar budget.
fn nested(
    entry: &Entries<'_>,
    name: &str,
    depth: usize,
    budget: &mut usize,
    scan: &mut dyn FnMut(&Entries<'_>, &mut usize),
) {
    if depth >= MAX_NESTING || *budget == 0 {
        return;
    }
    let Some(bytes) = entry(name) else {
        return;
    };
    *budget -= 1;
    let Ok(zip) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else {
        return;
    };
    let zip = std::cell::RefCell::new(zip);
    scan(&|name| read_entry(&mut zip.borrow_mut(), name), budget);
}

/// One file entry of a nested jar, when it exists, is a file and fits
/// [`MAX_NESTED_ENTRY_BYTES`].
fn read_entry(zip: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str) -> Option<Vec<u8>> {
    let mut file = zip.by_name(name).ok()?;
    if file.is_dir() || file.size() > MAX_NESTED_ENTRY_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut bytes).ok()?;
    Some(bytes)
}

/// Every `modId` of a `[[mods]]` record in a NeoForge (or Forge) mods TOML. A
/// block header ends the record before it, so a `[[dependencies.x]]` block's
/// `modId` (a requirement, not a mod) is never read as one.
fn neoforge_mod_ids(text: &str) -> Vec<String> {
    let mut in_mods = false;
    let mut ids = Vec::new();
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_mods = line == "[[mods]]";
            continue;
        }
        if !in_mods {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim() == "modId"
        {
            ids.push(value.trim().trim_matches('"').trim_matches('\'').to_owned());
        }
    }
    ids
}

/// Whether the host answers `id` itself ([`HOST_PROVIDED_MODS`]).
fn is_host_provided(id: &str) -> bool {
    HOST_PROVIDED_MODS
        .iter()
        .any(|host| host.eq_ignore_ascii_case(id))
}

/// Build the run order, link closure and missing-dependency rows for `jars`.
pub fn plan(jars: &[(&str, &NativeJar)]) -> LoadPlan {
    let mut provided: BTreeMap<String, usize> = BTreeMap::new();
    for (index, (_, jar)) in jars.iter().enumerate() {
        for id in mod_ids(jar) {
            provided.entry(id).or_insert(index);
        }
    }

    let mut missing = BTreeMap::new();
    let mut dependencies: Vec<Vec<usize>> = vec![Vec::new(); jars.len()];
    for (index, (_, jar)) in jars.iter().enumerate() {
        let mut missing_here = Vec::new();
        for required in &jar.required_mods {
            let key = required.to_ascii_lowercase();
            match provided.get(&key) {
                Some(&provider) if provider != index => {
                    if !dependencies[index].contains(&provider) {
                        dependencies[index].push(provider);
                    }
                }
                Some(_) => {}
                None if is_host_provided(required) => {}
                None => missing_here.push(required.clone()),
            }
        }
        if !missing_here.is_empty() {
            missing_here.sort();
            missing_here.dedup();
            missing.insert(index, missing_here);
        }
    }

    let mut order = Vec::with_capacity(jars.len());
    let mut visited = vec![false; jars.len()];
    let mut visiting = vec![false; jars.len()];
    for index in 0..jars.len() {
        visit(
            index,
            &dependencies,
            &mut visited,
            &mut visiting,
            &mut order,
        );
    }

    let mut links = vec![Vec::new(); jars.len()];
    for index in 0..jars.len() {
        let mut closure = Vec::new();
        let mut seen = BTreeSet::new();
        for &dependency in &dependencies[index] {
            collect_links(
                index,
                dependency,
                &dependencies,
                &mut seen,
                &mut closure,
                &mut vec![false; jars.len()],
            );
        }
        closure.retain(|&linked| linked != index);
        links[index] = closure;
    }

    LoadPlan {
        order,
        missing,
        links,
    }
}

/// Depth-first post-order: a jar joins the run order after every dependency it
/// can reach. A back edge is a cycle and is skipped, so the walk terminates.
fn visit(
    index: usize,
    dependencies: &[Vec<usize>],
    visited: &mut [bool],
    visiting: &mut [bool],
    order: &mut Vec<usize>,
) {
    if visited[index] || visiting[index] {
        return;
    }
    visiting[index] = true;
    for &dependency in &dependencies[index] {
        visit(dependency, dependencies, visited, visiting, order);
    }
    visiting[index] = false;
    visited[index] = true;
    order.push(index);
}

/// The dependencies of one jar, deepest first, without duplicates or a cycle.
fn collect_links(
    root: usize,
    index: usize,
    dependencies: &[Vec<usize>],
    seen: &mut BTreeSet<usize>,
    closure: &mut Vec<usize>,
    visiting: &mut [bool],
) {
    if index == root || visiting[index] || !seen.insert(index) {
        return;
    }
    visiting[index] = true;
    for &dependency in &dependencies[index] {
        collect_links(root, dependency, dependencies, seen, closure, visiting);
    }
    visiting[index] = false;
    closure.push(index);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jar(id: &str, requires: &[&str]) -> NativeJar {
        NativeJar {
            fabric_id: Some(id.to_owned()),
            required_mods: requires.iter().map(|id| (*id).to_owned()).collect(),
            ..NativeJar::default()
        }
    }

    #[test]
    fn a_dependency_runs_before_its_dependent_and_is_linked() {
        let a = jar("a", &["b"]);
        let b = jar("b", &[]);
        let jars = [("a.jar", &a), ("b.jar", &b)];
        let plan = plan(&jars);
        assert_eq!(plan.order, [1, 0]);
        assert_eq!(plan.links_for(0), [1]);
        assert!(plan.links_for(1).is_empty());
        assert!(plan.missing.is_empty());
    }

    #[test]
    fn a_transitive_dependency_is_linked_deepest_first() {
        let a = jar("a", &["b"]);
        let b = jar("b", &["c"]);
        let c = jar("c", &[]);
        let jars = [("a.jar", &a), ("b.jar", &b), ("c.jar", &c)];
        let plan = plan(&jars);
        assert_eq!(plan.order, [2, 1, 0]);
        assert_eq!(plan.links_for(0), [2, 1]);
        assert_eq!(plan.links_for(1), [2]);
    }

    #[test]
    fn a_missing_required_mod_is_named_and_the_hosts_own_are_not() {
        let a = jar("a", &["fabric-api", "minecraft", "collective", "cupboard"]);
        let b = jar("b", &[]);
        let jars = [("a.jar", &a), ("b.jar", &b)];
        let plan = plan(&jars);
        assert_eq!(
            plan.missing.get(&0).unwrap(),
            &["collective".to_owned(), "cupboard".to_owned()]
        );
        assert!(plan.links_for(0).is_empty());
    }

    /// spark's and libjf's declared requirements (their `fabric.mod.json`):
    /// the modelled Fabric API modules are the host's, an unmodelled one is not.
    #[test]
    fn modelled_fabric_api_modules_are_the_hosts_and_others_are_missing() {
        let spark = jar(
            "spark",
            &[
                "fabricloader",
                "minecraft",
                "fabric-api-base",
                "fabric-command-api-v2",
                "fabric-lifecycle-events-v1",
            ],
        );
        let libjf = jar(
            "libjf",
            &[
                "fabricloader",
                "minecraft",
                "fabric-resource-loader-v1",
                "fabric-lifecycle-events-v1",
                "fabric-command-api-v2",
            ],
        );
        let rendering = jar("hud", &["fabric-rendering-v1"]);
        let jars = [
            ("spark.jar", &spark),
            ("libjf.jar", &libjf),
            ("hud.jar", &rendering),
        ];
        let plan = plan(&jars);
        assert_eq!(plan.missing.get(&0), None);
        assert_eq!(plan.missing.get(&1), None);
        assert_eq!(
            plan.missing.get(&2).unwrap(),
            &["fabric-rendering-v1".to_owned()]
        );
    }

    #[test]
    fn a_cycle_terminates_and_keeps_every_jar() {
        let a = jar("a", &["b"]);
        let b = jar("b", &["a"]);
        let jars = [("a.jar", &a), ("b.jar", &b)];
        let plan = plan(&jars);
        assert_eq!(plan.order.len(), 2);
        assert_eq!(plan.order, [1, 0]);
        assert_eq!(plan.links_for(0), [1]);
    }

    /// A zip holding `entries`, as a jar on disk would.
    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn fabric_jar(metadata: &str, nested: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut entries: Vec<(&str, &[u8])> = vec![("fabric.mod.json", metadata.as_bytes())];
        entries.extend(nested.iter().map(|(name, bytes)| (*name, bytes.as_slice())));
        zip_of(&entries)
    }

    /// `libjf`'s layout, with the rules Fabric Loader 0.19.5's bytecode reads
    /// (`ModResolver.selectMod`, `ModDiscoverer$ModScanTask`): a listed
    /// nested jar answers for its id and its `provides`, its own listed jars
    /// do too, and a jar the `jars` list does not name, or names without the
    /// `.jar` suffix, is not a mod.
    #[test]
    fn nested_jars_provide_their_ids_and_aliases_as_fabric_loader_selects_them() {
        let module = fabric_jar(
            r#"{"schemaVersion":1,"id":"api-module","version":"1","provides":["api_module_alias"]}"#,
            &[],
        );
        let base = fabric_jar(
            r#"{"schemaVersion":1,"id":"lib-base","version":"1","provides":["lib_base"],
                "jars":[{"file":"META-INF/jars/api-module.jar"}]}"#,
            &[("META-INF/jars/api-module.jar", module)],
        );
        let elsewhere = fabric_jar(
            r#"{"schemaVersion":1,"id":"lib-elsewhere","version":"1"}"#,
            &[],
        );
        let unlisted = fabric_jar(
            r#"{"schemaVersion":1,"id":"lib-unlisted","version":"1"}"#,
            &[],
        );
        let zipped = fabric_jar(
            r#"{"schemaVersion":1,"id":"lib-zipped","version":"1"}"#,
            &[],
        );
        let no_metadata = zip_of(&[("README", b"not a mod")]);
        let outer = NativeJar::from_bytes(&fabric_jar(
            r#"{"schemaVersion":1,"id":"lib","version":"1","jars":[
                {"file":"META-INF/jars/lib-base.jar"},
                {"file":"libs/lib-elsewhere.jar"},
                {"file":"META-INF/jars/lib-zipped.zip"},
                {"file":"META-INF/jars/plain.jar"},
                {"file":"META-INF/jars/absent.jar"}]}"#,
            &[
                ("META-INF/jars/lib-base.jar", base),
                ("libs/lib-elsewhere.jar", elsewhere),
                ("META-INF/jars/lib-unlisted.jar", unlisted),
                ("META-INF/jars/lib-zipped.zip", zipped),
                ("META-INF/jars/plain.jar", no_metadata),
            ],
        ))
        .unwrap();
        let dependent = jar(
            "dependent",
            &[
                "lib",
                "lib-base",
                "lib_base",
                "api-module",
                "api_module_alias",
                "lib-elsewhere",
                "lib-unlisted",
                "lib-zipped",
            ],
        );
        let jars = [("dependent.jar", &dependent), ("lib.jar", &outer)];
        let plan = plan(&jars);
        assert_eq!(
            plan.missing.get(&0).unwrap(),
            &["lib-unlisted".to_owned(), "lib-zipped".to_owned()]
        );
        assert_eq!(plan.links_for(0), [1]);
        assert_eq!(plan.order, [1, 0]);
    }

    /// A NeoForge layout, with the rules FancyModLoader 12.0.12 and
    /// JarJarSelector 0.5.3 read (`JarSelector.recursivelyDetectContainedJars`,
    /// `JarInJarDependencyLocator.scanMods`): every jar
    /// `META-INF/jarjar/metadata.json` names is a mod file whose `[[mods]]`
    /// are mods, its own JarJar jars too; a jar in `META-INF/jars/` the
    /// metadata does not name is not, and a dependency's `modId` is not a mod.
    #[test]
    fn jarjar_jars_provide_their_mod_ids_as_neoforge_selects_them() {
        let toml = |mods: &[&str], extra: &str| {
            let mut text = String::from("modLoader = \"javafml\"\n");
            for id in mods {
                text += &format!("[[mods]]\nmodId = \"{id}\"\nversion = \"1\"\n");
            }
            text + extra
        };
        let jarjar = |paths: &[&str]| {
            let jars = paths
                .iter()
                .map(|path| {
                    format!(r#"{{"identifier":{{"group":"g","artifact":"a"}},"path":"{path}"}}"#)
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(r#"{{"jars":[{jars}]}}"#)
        };
        let deep = zip_of(&[(
            "META-INF/neoforge.mods.toml",
            toml(&["deep_nf"], "").as_bytes(),
        )]);
        let inner_toml = toml(
            &["inner_a", "inner_b"],
            "[[dependencies.inner_a]]\nmodId = \"dep_x\"\ntype = \"required\"\n",
        );
        let inner_jarjar = jarjar(&["META-INF/jars/deep.jar"]);
        let inner = zip_of(&[
            ("META-INF/neoforge.mods.toml", inner_toml.as_bytes()),
            ("META-INF/jarjar/metadata.json", inner_jarjar.as_bytes()),
            ("META-INF/jars/deep.jar", &deep),
        ]);
        let unlisted = zip_of(&[(
            "META-INF/neoforge.mods.toml",
            toml(&["unlisted_nf"], "").as_bytes(),
        )]);
        let outer_toml = toml(&["outer"], "");
        let outer_jarjar = jarjar(&["META-INF/jars/inner.jar", "META-INF/jars/absent.jar"]);
        let outer = NativeJar::from_bytes(&zip_of(&[
            ("META-INF/neoforge.mods.toml", outer_toml.as_bytes()),
            ("META-INF/jarjar/metadata.json", outer_jarjar.as_bytes()),
            ("META-INF/jars/inner.jar", &inner),
            ("META-INF/jars/unlisted.jar", &unlisted),
        ]))
        .unwrap();
        let dependent = jar(
            "dependent",
            &[
                "outer",
                "inner_a",
                "inner_b",
                "deep_nf",
                "unlisted_nf",
                "dep_x",
            ],
        );
        let jars = [("dependent.jar", &dependent), ("outer.jar", &outer)];
        let plan = plan(&jars);
        assert_eq!(
            plan.missing.get(&0).unwrap(),
            &["dep_x".to_owned(), "unlisted_nf".to_owned()]
        );
        assert_eq!(plan.links_for(0), [1]);
    }

    /// The named departure in [`mod_ids`]: a bundled copy of a module the
    /// host models does not make its bundle a dependency; the host answers.
    #[test]
    fn a_bundled_host_module_does_not_link_its_bundle() {
        let events = fabric_jar(
            r#"{"schemaVersion":1,"id":"fabric-lifecycle-events-v1","version":"1"}"#,
            &[],
        );
        let library = NativeJar::from_bytes(&fabric_jar(
            r#"{"schemaVersion":1,"id":"library","version":"1",
                "jars":[{"file":"META-INF/jars/events.jar"}]}"#,
            &[("META-INF/jars/events.jar", events)],
        ))
        .unwrap();
        let dependent = jar("dependent", &["fabric-lifecycle-events-v1"]);
        let jars = [("dependent.jar", &dependent), ("library.jar", &library)];
        let plan = plan(&jars);
        assert!(plan.missing.is_empty(), "missing: {:?}", plan.missing);
        assert!(plan.links_for(0).is_empty());
    }

    /// A top-level mod's own `provides` answers for it too (`selectMod` puts
    /// every alias in the map, whatever the candidate's depth).
    #[test]
    fn a_jars_own_provides_alias_satisfies_a_dependency() {
        let provider = NativeJar::from_bytes(&fabric_jar(
            r#"{"schemaVersion":1,"id":"config-v2","version":"2","provides":["config-v1"]}"#,
            &[],
        ))
        .unwrap();
        let dependent = jar("dependent", &["config-v1"]);
        let jars = [("dependent.jar", &dependent), ("config.jar", &provider)];
        let plan = plan(&jars);
        assert!(plan.missing.is_empty(), "missing: {:?}", plan.missing);
        assert_eq!(plan.links_for(0), [1]);
    }

    #[test]
    fn an_id_match_is_case_insensitive() {
        let a = jar("a", &["Collective"]);
        let b = jar("collective", &[]);
        let jars = [("a.jar", &a), ("b.jar", &b)];
        let plan = plan(&jars);
        assert_eq!(plan.links_for(0), [1]);
        assert!(plan.missing.is_empty());
    }
}
