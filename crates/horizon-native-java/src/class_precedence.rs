//! A jar's own copy of a loader class the run's loader does not provide runs
//! instead of the host's model of it.
//!
//! The host answers the NeoForge and legacy Forge namespaces (`net/neoforged/`,
//! `net/minecraftforge/`) by name (`host_models_class`), so a linked jar's
//! copy of one of those classes was never interpreted. That is right on a
//! NeoForge run, where FML and NeoForge define those classes before any mod
//! jar's copy. On a Fabric run nothing provides them except a jar that ships
//! them: Forge Config API Port bundles NeoForge's `net/neoforged/fml/config`
//! and `net/neoforged/neoforge/common/ModConfigSpec` classes so Fabric mods can
//! use the NeoForge config API. A JVM loads the port's bytecode there, and
//! calls such as `ConfigTracker.INSTANCE.registerConfig` run the port's code.
//! The host's NeoForge model answered them instead (census row 8,
//! 2026-11-13: cropsloverain's `ConfigTracker.registerConfig` and
//! `IConfigSpec.isEmpty`, counted).
//!
//! The rule (decided 2026-11-13): when a jar in the run (the running jar, a
//! linked sibling or one of their nested jars) ships a class in a namespace
//! the run's loader does not itself provide, that jar's bytecode runs. The
//! run's loader follows `entrypoint_role.rs`: a jar with Fabric metadata runs
//! on Fabric, any other jar on NeoForge (whose model here also answers legacy
//! Forge's `net/minecraftforge/`).
//!
//! | run | provided (host-modelled) | a jar's copy runs |
//! |---|---|---|
//! | Fabric | `net/fabricmc/` | `net/neoforged/`, `net/minecraftforge/` |
//! | NeoForge | `net/neoforged/`, `net/minecraftforge/` | (none) |
//!
//! A class in those namespaces that no jar in the run ships keeps the host's
//! answer, as before. `net/fabricmc/` on a NeoForge run is not covered:
//! platform roots are dropped when a jar is linked (`LinkedClassTable`), so a
//! NeoForge jar bundling Fabric API classes still sees the host's Fabric
//! model.
//!
//! Two places apply the rule. `StdHost::models_class` stops claiming such a
//! class, so class resolution (`Runtime::class`), `new`, field access and
//! virtual dispatch reach the jar's class. And [`Runtime::invoke_jar_loader_copy`]
//! runs a static or `invokespecial` call (a constructor, a `super` call) on it
//! before the owner-keyed loader models in the dispatch chain
//! (`mod_config_spec.rs` matches `ModConfigSpec$Builder` by owner name) can
//! answer it.

use super::*;

/// The namespaces the NeoForge loader model answers.
const NEOFORGE_NAMESPACES: &[&str] = &["net/neoforged/", "net/minecraftforge/"];

/// The loader a jar's execution runs on, for class precedence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RunLoader {
    /// Not seeded from a jar: every host-modelled namespace stays the host's,
    /// as before the rule existed.
    #[default]
    Unknown,
    /// Fabric Loader: a jar with Fabric metadata.
    Fabric,
    /// FML (NeoForge, or legacy Forge through the same model).
    NeoForge,
}

impl RunLoader {
    /// Whether `class` is in a namespace this loader does not provide, so a
    /// jar's copy of it is the only definition the run has.
    fn lacks(self, class: &str) -> bool {
        match self {
            RunLoader::Fabric => NEOFORGE_NAMESPACES
                .iter()
                .any(|namespace| class.starts_with(namespace)),
            RunLoader::NeoForge | RunLoader::Unknown => false,
        }
    }
}

/// Whether `class` is in one of the loader namespaces this rule can hand to a
/// jar's copy.
fn in_loader_namespace(class: &str) -> bool {
    NEOFORGE_NAMESPACES
        .iter()
        .any(|namespace| class.starts_with(namespace))
}

impl NativeJar {
    /// The loader this jar runs on (`entrypoint_role.rs`'s rule).
    pub fn run_loader(&self) -> RunLoader {
        if self.has_fabric_metadata() {
            RunLoader::Fabric
        } else {
            RunLoader::NeoForge
        }
    }

    /// This jar's own classes in the loader namespaces, which a run seeded
    /// from it may have to run itself.
    pub(crate) fn loader_namespace_classes(&self) -> BTreeSet<String> {
        self.classes
            .keys()
            .filter(|name| in_loader_namespace(name))
            .cloned()
            .collect()
    }
}

impl StdHost {
    /// Run as a jar on `loader` would, for class precedence. `seed_host` sets
    /// it from the jar.
    pub fn with_run_loader(mut self, loader: RunLoader) -> Self {
        self.run_loader = loader;
        self
    }

    /// Whether a jar in this run ships `class` and the run's loader does not
    /// provide it, so the jar's bytecode runs instead of the host's model.
    pub(crate) fn runs_jar_copy(&self, class: &str) -> bool {
        self.run_loader.lacks(class)
            && (self.own_loader_classes.contains(class) || self.linked_classes.contains_key(class))
    }
}

impl<H: Host> Runtime<'_, H> {
    /// Run a static or `invokespecial` call whose owner is a jar's copy of a
    /// loader class the run's loader lacks, before the owner-keyed loader
    /// models see it. `Ok(None)` for every other call.
    pub(crate) fn invoke_jar_loader_copy(
        &mut self,
        r: &MethodRef,
        opcode: u8,
        receiver: Option<&Value>,
        args: &[Value],
    ) -> Result<Option<Value>, NativeError> {
        if !matches!(opcode, 0xb7 | 0xb8)
            || !in_loader_namespace(&r.owner)
            || self.host.models_class(&r.owner)
        {
            return Ok(None);
        }
        let Some((owner, true)) = self.dispatch_owner(r, opcode, receiver) else {
            return Ok(None);
        };
        self.invoke(&owner, &r.name, &r.descriptor, receiver.cloned(), args)
            .map(Some)
    }
}
