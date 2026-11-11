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
pub const HOST_PROVIDED_MODS: &[&str] = &[
    "minecraft",
    "java",
    "fabricloader",
    "fabric-api",
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
/// A bundled jar (`META-INF/jars/*.jar`) is a mod of its own in a real loader:
/// `libjf` ships `libjf-base` and its dependents declare a dependency on that
/// id, so the ids a bundle provides answer for its dependents. The bundled
/// jars' own requirements are not checked here — a library bundle asks for the
/// loader's own surfaces, which [`HOST_PROVIDED_MODS`] already covers.
fn mod_ids(jar: &NativeJar) -> Vec<String> {
    let outer = [jar.fabric_id.as_deref(), jar.neoforge_id.as_deref()];
    let nested = crate::runtime::nested_jars::parsed(jar)
        .into_iter()
        .flat_map(|(_, nested)| [nested.fabric_id, nested.neoforge_id])
        .collect::<Vec<_>>();
    outer
        .into_iter()
        .flatten()
        .chain(nested.iter().filter_map(Option::as_deref))
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Build the run order, link closure and missing-dependency rows for `jars`.
pub fn plan(jars: &[(&str, &NativeJar)]) -> LoadPlan {
    let mut provided: BTreeMap<String, usize> = BTreeMap::new();
    for (index, (_, jar)) in jars.iter().enumerate() {
        for id in mod_ids(jar) {
            provided.entry(id).or_insert(index);
        }
    }
    let is_host = |id: &str| {
        HOST_PROVIDED_MODS
            .iter()
            .any(|host| host.eq_ignore_ascii_case(id))
    };

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
                None if is_host(required) => {}
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
