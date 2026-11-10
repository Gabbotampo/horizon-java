//! The NeoForge loader surface a mod reaches during bootstrap: the physical
//! side, the loader's directory handles, the mod list and its containers.
//!
//! Every answer is a fact the loader exposes, not a guess: the side is the
//! host's own role, the directories are the same virtual roots Fabric's loader
//! answers, a mod container exists exactly for an id the jar's metadata named
//! (plus the always-loaded ids), and a game version is whatever the caller
//! seeded. A question outside that set is left to the rest of the dispatch
//! chain, which still reports and refuses it.

use crate::runtime::{MethodRef, StdHost, Value, is_always_loaded_mod};

/// `FMLEnvironment.dist` and `FMLEnvironment.getDist` answer one of these.
const CLIENT: &str = "dist/CLIENT";
const DEDICATED_SERVER: &str = "dist/DEDICATED_SERVER";

/// The virtual roots `FMLPaths` resolves to. They match the Fabric loader's
/// answers, because both runtimes put a mod's files under the same game root.
fn fml_path_root(name: &str) -> Option<&'static str> {
    match name {
        "GAMEDIR" => Some("path:/horizon/game"),
        "CONFIGDIR" => Some("path:/horizon/config"),
        "MODSDIR" => Some("path:/horizon/mods"),
        _ => None,
    }
}

fn dist_value(server: bool) -> Value {
    Value::Ref(Some(if server { DEDICATED_SERVER } else { CLIENT }.into()))
}

/// The concrete descriptor of a loader method `Class.getMethod` can select
/// on a class the jar does not hold, so `Method.invoke` can dispatch it
/// instead of refusing a method the host really models.
///
/// Only the entries the module answers are listed; anything else keeps the
/// old refusal.
pub(crate) fn reflected_static_descriptor(
    owner: &str,
    name: &str,
    arguments: usize,
) -> Option<&'static str> {
    match (owner, name, arguments) {
        ("net/neoforged/fml/loading/FMLEnvironment", "getDist", 0) => {
            Some("()Lnet/neoforged/api/distmarker/Dist;")
        }
        ("net/neoforged/fml/loading/FMLEnvironment", "isProduction", 0) => Some("()Z"),
        ("net/neoforged/fml/ModList", "get", 0) => Some("()Lnet/neoforged/fml/ModList;"),
        _ => None,
    }
}

impl StdHost {
    /// The side-dependent loader constants and statics.
    pub(super) fn neoforge_loader_static(&mut self, r: &MethodRef) -> Option<Value> {
        let server = self.environment_type == "SERVER";
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            (
                "net/neoforged/fml/loading/FMLEnvironment",
                "dist",
                "Lnet/neoforged/api/distmarker/Dist;",
            ) => Some(dist_value(server)),
            (
                "net/neoforged/api/distmarker/Dist",
                "CLIENT",
                "Lnet/neoforged/api/distmarker/Dist;",
            ) => Some(Value::Ref(Some(CLIENT.into()))),
            (
                "net/neoforged/api/distmarker/Dist",
                "DEDICATED_SERVER",
                "Lnet/neoforged/api/distmarker/Dist;",
            ) => Some(Value::Ref(Some(DEDICATED_SERVER.into()))),
            (
                "net/neoforged/fml/loading/FMLPaths",
                name,
                "Lnet/neoforged/fml/loading/FMLPaths;",
            ) => {
                fml_path_root(name).map(|path| Value::Ref(Some(format!("fml-path/{name}/{path}"))))
            }
            _ => None,
        }
    }

    /// The loader's static methods: a call with no receiver in the jar.
    pub(super) fn neoforge_loader_call(&mut self, r: &MethodRef, _args: &[Value]) -> Option<Value> {
        let server = self.environment_type == "SERVER";
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            (
                "net/neoforged/fml/loading/FMLEnvironment",
                "getDist",
                "()Lnet/neoforged/api/distmarker/Dist;",
            ) => Some(dist_value(server)),
            ("net/neoforged/fml/loading/FMLEnvironment", "isProduction", "()Z") => {
                Some(Value::Int(1))
            }
            ("net/neoforged/fml/ModList", "get", "()Lnet/neoforged/fml/ModList;") => {
                Some(Value::Ref(Some("neoforge-mod-list".into())))
            }
            // The real game reports the running client's own version; the
            // host answers the version it was seeded with, and nothing when
            // the caller did not supply one.
            (
                "net/minecraft/DetectedVersion",
                "tryDetectVersion",
                "()Lnet/minecraft/WorldVersion;",
            ) => Some(
                self.game_version
                    .as_ref()
                    .map_or(Value::Ref(None), |version| {
                        Value::Ref(Some(format!("world-version/{version}")))
                    }),
            ),
            _ => None,
        }
    }

    /// The loader object methods: `FMLPaths.get`, the side test, a mod
    /// container's event bus and info, and the version string behind it.
    pub(super) fn neoforge_loader_instance(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        _args: &[Value],
    ) -> Option<Value> {
        let marker = match receiver {
            Value::Ref(Some(marker)) => marker.as_str(),
            _ => return None,
        };
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            ("net/neoforged/fml/loading/FMLPaths", "get", "()Ljava/nio/file/Path;") => {
                let (_, path) = marker.strip_prefix("fml-path/")?.split_once('/')?;
                Some(Value::Ref(Some(path.to_owned())))
            }
            ("net/neoforged/api/distmarker/Dist", "isClient", "()Z") => {
                Some(Value::Int(i32::from(marker == CLIENT)))
            }
            ("net/neoforged/api/distmarker/Dist", "isDedicatedServer", "()Z") => {
                Some(Value::Int(i32::from(marker == DEDICATED_SERVER)))
            }
            ("net/neoforged/api/distmarker/Dist", "toString", "()Ljava/lang/String;") => {
                Some(Value::Ref(Some(enum_name(marker).to_owned())))
            }
            (
                "net/neoforged/fml/ModContainer" | "net/minecraftforge/fml/ModContainer",
                "getEventBus" | "getModEventBus",
                "()Lnet/neoforged/bus/api/IEventBus;",
            ) => Some(Value::Ref(Some("net/neoforged/bus/api/IEventBus".into()))),
            (
                "net/neoforged/fml/ModContainer" | "net/minecraftforge/fml/ModContainer",
                "getModInfo",
                "()Lnet/neoforged/neoforgespi/language/IModInfo;",
            ) => {
                let (id, version) = mod_container_parts(marker)?;
                Some(Value::Ref(Some(format!("mod-info/{id}/{version}"))))
            }
            (
                "net/neoforged/neoforgespi/language/IModInfo"
                | "net/neoforged/neoforgespi/language/IModInfo$ModVersion",
                "getModId",
                "()Ljava/lang/String;",
            ) => Some(Value::Ref(Some(mod_info_parts(marker)?.0.to_owned()))),
            (
                "net/neoforged/neoforgespi/language/IModInfo"
                | "net/neoforged/neoforgespi/language/IModInfo$ModVersion",
                "getVersion",
                "()Lorg/apache/maven/artifact/versioning/ArtifactVersion;",
            ) => {
                let (_, version) = mod_info_parts(marker)?;
                Some(Value::Ref(Some(format!("artifact-version/{version}"))))
            }
            (
                "org/apache/maven/artifact/versioning/ArtifactVersion",
                "toString",
                "()Ljava/lang/String;",
            ) => Some(Value::Ref(Some(
                marker
                    .strip_prefix("artifact-version/")
                    .unwrap_or(marker)
                    .to_owned(),
            ))),
            ("net/minecraft/WorldVersion", "name" | "getName", "()Ljava/lang/String;") => {
                Some(Value::Ref(Some(world_version(marker)?.to_owned())))
            }
            _ => None,
        }
    }

    /// Whether `id` is a mod container this host can answer, and its version.
    ///
    /// The jar's own id and its declared dependencies are seeded in `mods`.
    /// The game and the loader are always present; their version is the
    /// caller-seeded game version (or `unknown` when the host was not told).
    pub(super) fn neoforge_mod_version(&self, id: &str) -> Option<&str> {
        if let Some(version) = self.mods.get(id) {
            return Some(version);
        }
        if id == "minecraft" {
            // The game's own version is only known when the caller seeded it;
            // an empty answer keeps the container absent instead of inventing
            // a release a mod would then compare against.
            return self.game_version.as_deref();
        }
        is_always_loaded_mod(id).then_some("unknown")
    }
}

fn enum_name(marker: &str) -> &str {
    marker.rsplit('/').next().unwrap_or(marker)
}

fn mod_container_parts(marker: &str) -> Option<(&str, &str)> {
    let rest = marker.strip_prefix("neoforge-mod-container/")?;
    rest.split_once('/')
}

fn mod_info_parts(marker: &str) -> Option<(&str, &str)> {
    let rest = marker.strip_prefix("mod-info/")?;
    rest.split_once('/')
}

fn world_version(marker: &str) -> Option<&str> {
    marker.strip_prefix("world-version/")
}
