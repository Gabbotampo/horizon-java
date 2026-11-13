//! `FabricLoader.getAllMods` and each container's own `ModMetadata`, as Fabric
//! Loader 0.19.5 reports them.
//!
//! **The list.** `FabricLoaderImpl.getAllMods` is
//! `Collections.unmodifiableList(mods)`, and `setup` adds a container for each
//! mod `ModResolver.resolve` selected, in the resolver's order.
//! `findCompatibleSet` ends with `sort(Comparator.comparing(ModCandidateImpl::getId))`
//! (bytecode offset 841), and the dev-only shuffle never runs in production, so
//! the list is in mod-id order. The run holds:
//!
//! * every jar in it that has Fabric metadata, with its own `fabric.mod.json`:
//!   the running jar, its linked siblings and the nested (JiJ) jars of both;
//! * the loader's built-in mods: `java`, `minecraft`, `fabricloader`, and
//!   `mixinextras` (a nested jar of Fabric Loader since 0.15);
//! * the Fabric API ids the host provides (`dependencies::HOST_PROVIDED_MODS`).
//!
//! A dependency a jar declares but nothing ships is not a container, as on a
//! real loader (the host used to list those as `unknown`).
//!
//! **A jar's metadata** comes from its own document, read the way
//! `V1ModMetadataParser` reads it (checked against the 0.19.5 parser on JDK 25:
//! `tests/fixture_fabric_mods.rs`). `getType` is `fabric`; an absent or empty
//! `name` is the id; an absent `description` is `""`; `environment` `*`, absent,
//! `client` or `server` are `UNIVERSAL`/`CLIENT`/`SERVER`; `authors` and
//! `contributors` are people (a string, or `{name, contact}`); `license` is a
//! string or a list; `custom` values keep their JSON types.
//!
//! **`fabricloader` and `mixinextras`** are the containers Fabric Loader
//! 0.19.5 builds for itself and for the MixinExtras 0.5.5 jar it bundles: V1
//! metadata from those jars' own `fabric.mod.json` ([`LOADER_METADATA`],
//! [`MIXINEXTRAS_METADATA`]). Named departure: Horizon has no real Fabric
//! Loader; it reports the version whose behaviour the runtime reproduces
//! (0.19.5, the loader this runtime is ported from and checked against). A jar
//! in the run that ships its own MixinExtras keeps its own metadata, where
//! Fabric would pick the newest version.
//!
//! **The Fabric API modules the host provides** (`fabric-api`,
//! `fabric-api-base`, `fabric-command-api-v2`, `fabric-lifecycle-events-v1`,
//! `fabric-resource-loader-v1`) answer the metadata those modules carry in
//! `fabric-api-0.165.0+26.3`, the release the host's Fabric API model
//! reproduces (`events.rs`): the same named departure, Horizon reports the
//! release whose behaviour it reproduces. A jar in the run that ships its own
//! copy of one of these keeps its own metadata.
//!
//! **`java` and `minecraft`** are the loader's `BuiltinModMetadata` (see
//! `builtin_metadata`).
//!
//! **Counted, not answered:** iterating a custom object or array
//! (`serde_json` here keeps object keys sorted, not in file order),
//! `ContactInformation.asMap` (a `HashMap` in Fabric), `getCustomValues`,
//! `getDependencies`, a custom number's `getAsNumber` (Fabric's is a
//! `LazilyParsedNumber`), an icon path, and a `getAs*` of the wrong type.

use super::*;

const MOD_METADATA: &str = "net/fabricmc/loader/api/metadata/ModMetadata";
const PERSON: &str = "net/fabricmc/loader/api/metadata/Person";
const CONTACT: &str = "net/fabricmc/loader/api/metadata/ContactInformation";
const CUSTOM_VALUE: &str = "net/fabricmc/loader/api/metadata/CustomValue";
const CV_OBJECT: &str = "net/fabricmc/loader/api/metadata/CustomValue$CvObject";
const CV_ARRAY: &str = "net/fabricmc/loader/api/metadata/CustomValue$CvArray";
const MOD_ENVIRONMENT: &str = "net/fabricmc/loader/api/metadata/ModEnvironment";
const CV_TYPE: &str = "net/fabricmc/loader/api/metadata/CustomValue$CvType";

/// `ModEnvironment`'s constants, in declaration order.
const ENVIRONMENTS: &[&str] = &["CLIENT", "SERVER", "UNIVERSAL"];
/// `CustomValue.CvType`'s constants, in declaration order.
const CV_TYPES: &[&str] = &["OBJECT", "ARRAY", "STRING", "NUMBER", "BOOLEAN", "NULL"];

/// Fabric Loader's built-in mods beside the host-provided ids.
const BUILTIN_MODS: &[&str] = &["java", "minecraft", "fabricloader", "mixinextras"];

/// Fabric Loader 0.19.5's own `fabric.mod.json` (Apache-2.0), less its
/// `jars` list: the metadata its `fabricloader` container reports.
const LOADER_METADATA: &str = r#"{"schemaVersion":1,"id":"fabricloader","name":"Fabric Loader","version":"0.19.5","environment":"*","description":"The base mod loader.","contact":{"homepage":"https://fabricmc.net","irc":"ircs://irc.esper.net:6697/fabric","issues":"https://github.com/FabricMC/fabric-loader/issues","sources":"https://github.com/FabricMC/fabric-loader"},"license":"Apache-2.0","icon":"assets/fabricloader/icon.png","authors":["FabricMC"]}"#;

/// MixinExtras 0.5.5's `fabric.mod.json` (MIT), the jar Fabric Loader 0.19.5
/// bundles, less its `mixins` list.
const MIXINEXTRAS_METADATA: &str = r#"{"schemaVersion":1,"id":"mixinextras","provides":["com_github_llamalad7_mixinextras"],"version":"0.5.5","name":"MixinExtras","description":"Companion library to Mixin with lots of features to improve the compatibility and concision of your mixins!","authors":["LlamaLad7"],"contact":{"homepage":"https://github.com/LlamaLad7/MixinExtras","sources":"https://github.com/LlamaLad7/MixinExtras"},"license":"MIT","environment":"*","depends":{"fabricloader":">=0.14.25"},"custom":{"modmenu":{"badges":["library"]}}}"#;

/// The Fabric API modules the host provides, as `fabric-api-0.165.0+26.3`
/// (Apache-2.0) ships them: the umbrella jar's and its nested module jars'
/// own `fabric.mod.json`, less their `depends`, `entrypoints`, `mixins`,
/// `accessWidener` and `jars`. That is the release the host's Fabric API
/// event model reproduces (`events.rs`).
const FABRIC_API_METADATA: &str = r#"{"schemaVersion":1,"id":"fabric-api","name":"Fabric API","version":"0.165.0+26.3","environment":"*","description":"Core API module providing key hooks and intercompatibility features.","license":"Apache-2.0","icon":"assets/fabric/icon.png","contact":{"homepage":"https://fabricmc.net","irc":"irc://irc.esper.net:6667/fabric","issues":"https://github.com/FabricMC/fabric/issues","sources":"https://github.com/FabricMC/fabric"},"authors":["FabricMC"]}"#;
const FABRIC_API_BASE_METADATA: &str = r#"{"schemaVersion":1,"id":"fabric-api-base","name":"Fabric API Base","version":"2.0.6+fcdff87f5d","environment":"*","description":"Contains the essentials for Fabric API modules.","license":"Apache-2.0","icon":"assets/fabric-api-base/icon.png","contact":{"homepage":"https://fabricmc.net","irc":"irc://irc.esper.net:6667/fabric","issues":"https://github.com/FabricMC/fabric/issues","sources":"https://github.com/FabricMC/fabric"},"authors":["FabricMC"],"custom":{"fabric-api:module-lifecycle":"stable"}}"#;
const FABRIC_COMMAND_API_V2_METADATA: &str = r#"{"schemaVersion":1,"id":"fabric-command-api-v2","name":"Fabric Command API (v2)","version":"3.1.2+fcdff87f5d","environment":"*","description":"Adds command-related hooks.","license":"Apache-2.0","icon":"assets/fabric-command-api-v2/icon.png","contact":{"homepage":"https://fabricmc.net","irc":"irc://irc.esper.net:6667/fabric","issues":"https://github.com/FabricMC/fabric/issues","sources":"https://github.com/FabricMC/fabric"},"authors":["FabricMC"],"custom":{"fabric-api:module-lifecycle":"stable"}}"#;
const FABRIC_LIFECYCLE_EVENTS_V1_METADATA: &str = r#"{"schemaVersion":1,"id":"fabric-lifecycle-events-v1","name":"Fabric Lifecycle Events (v1)","version":"4.1.9+ffef5f675d","environment":"*","description":"Events for the game's lifecycle.","license":"Apache-2.0","icon":"assets/fabric-lifecycle-events-v1/icon.png","contact":{"homepage":"https://fabricmc.net","irc":"irc://irc.esper.net:6667/fabric","issues":"https://github.com/FabricMC/fabric/issues","sources":"https://github.com/FabricMC/fabric"},"authors":["FabricMC"],"custom":{"fabric-api:module-lifecycle":"stable"}}"#;
const FABRIC_RESOURCE_LOADER_V1_METADATA: &str = r#"{"schemaVersion":1,"id":"fabric-resource-loader-v1","name":"Fabric Resource Loader (v1)","version":"3.1.0+5af7744b5d","environment":"*","description":"Asset and data resource loading.","license":"Apache-2.0","icon":"assets/fabric-resource-loader-v1/icon.png","contact":{"homepage":"https://fabricmc.net","irc":"irc://irc.esper.net:6667/fabric","issues":"https://github.com/FabricMC/fabric/issues","sources":"https://github.com/FabricMC/fabric"},"authors":["FabricMC"],"custom":{"fabric-api:module-lifecycle":"stable"}}"#;

/// `java.specification.version` on the Java 25 that Minecraft 26.3 requires
/// (`ModDiscoverer.createJavaMod` strips a leading `1.`; there is none).
const JAVA_SPECIFICATION_VERSION: &str = "25";

const PERSON_PREFIX: &str = "fabric-person/";
const CONTACT_PREFIX: &str = "fabric-contact/";
const CV_PREFIX: &str = "fabric-cv/";

fn enum_marker(owner: &str, name: &str) -> Value {
    Value::Ref(Some(format!("{owner}/{name}")))
}

fn text(value: impl Into<String>) -> Value {
    Value::Ref(Some(value.into()))
}

/// RFC 6901 escaping of one JSON-pointer segment.
fn pointer_segment(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// `(id, pointer)` of a `<prefix><id><pointer>` marker; the pointer keeps
/// its leading `/` (a mod id never contains one).
fn split_marker<'a>(marker: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
    let rest = marker.strip_prefix(prefix)?;
    Some(rest.find('/').map_or((rest, ""), |at| rest.split_at(at)))
}

/// The pointer to `key` under `pointer`.
fn child(pointer: &str, key: &str) -> String {
    format!("{pointer}/{}", pointer_segment(key))
}

fn cv_type(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Object(_) => "OBJECT",
        serde_json::Value::Array(_) => "ARRAY",
        serde_json::Value::String(_) => "STRING",
        serde_json::Value::Number(_) => "NUMBER",
        serde_json::Value::Bool(_) => "BOOLEAN",
        serde_json::Value::Null => "NULL",
    }
}

impl StdHost {
    /// Record a Fabric jar's own `fabric.mod.json` as its container's
    /// metadata. The first jar that names an id keeps it.
    pub(crate) fn record_fabric_metadata(&mut self, jar: &NativeJar) {
        let Some(document) = jar
            .resource_bytes
            .get("fabric.mod.json")
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
        else {
            return;
        };
        if let Some(id) = document.get("id").and_then(serde_json::Value::as_str) {
            self.fabric_metadata
                .entry(id.to_owned())
                .or_insert(document);
        }
    }

    /// A jar container's metadata document: one recorded for the run, or the
    /// host's own `fabric.mod.json` resource when it names `id`.
    fn fabric_document(&self, id: &str) -> Option<std::borrow::Cow<'_, serde_json::Value>> {
        if let Some(document) = self.fabric_metadata.get(id) {
            return Some(std::borrow::Cow::Borrowed(document));
        }
        let own = self
            .resource_bytes
            .get("fabric.mod.json")
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
            .filter(|document| document.get("id").and_then(serde_json::Value::as_str) == Some(id));
        if let Some(document) = own {
            return Some(std::borrow::Cow::Owned(document));
        }
        let loader = match id {
            "fabricloader" => LOADER_METADATA,
            "mixinextras" => MIXINEXTRAS_METADATA,
            "fabric-api" => FABRIC_API_METADATA,
            "fabric-api-base" => FABRIC_API_BASE_METADATA,
            "fabric-command-api-v2" => FABRIC_COMMAND_API_V2_METADATA,
            "fabric-lifecycle-events-v1" => FABRIC_LIFECYCLE_EVENTS_V1_METADATA,
            "fabric-resource-loader-v1" => FABRIC_RESOURCE_LOADER_V1_METADATA,
            _ => return None,
        };
        serde_json::from_str(loader)
            .ok()
            .map(std::borrow::Cow::Owned)
    }

    /// Whether `id` is a container the loader builds without a jar.
    fn builtin_fabric_mod(id: &str) -> bool {
        BUILTIN_MODS.contains(&id)
            || (dependencies::HOST_PROVIDED_MODS.contains(&id)
                && !matches!(id, "neoforge" | "forge"))
    }

    /// Every container `getAllMods` lists, in mod-id order.
    fn fabric_containers(&self) -> Vec<Value> {
        let mut ids = self
            .fabric_metadata
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        ids.extend(
            BUILTIN_MODS
                .iter()
                .chain(dependencies::HOST_PROVIDED_MODS)
                .filter(|id| Self::builtin_fabric_mod(id))
                .map(|id| (*id).to_owned()),
        );
        ids.into_iter()
            .map(|id| {
                let version = self
                    .fabric_document(&id)
                    .and_then(|document| {
                        document
                            .get("version")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| "unknown".to_owned());
                text(format!("mod-container/{id}/{version}"))
            })
            .collect()
    }

    /// Answer the loader's container list and metadata surface, or `None`
    /// for a call this module does not own.
    pub(crate) fn fabric_mods_instance(
        &mut self,
        r: &MethodRef,
        receiver: &Value,
        args: &[Value],
    ) -> Option<Result<Value, NativeError>> {
        let marker = expect_ref(receiver).map(String::as_str);
        match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
            ("net/fabricmc/loader/api/FabricLoader", "getAllMods", "()Ljava/util/Collection;") => {
                let containers = self.fabric_containers();
                Some(Ok(collection_value("list", &containers)))
            }
            (MOD_METADATA, ..) => {
                let id = marker?
                    .strip_prefix("metadata/")?
                    .split('/')
                    .next()?
                    .to_owned();
                if let Some(document) = self.fabric_document(&id).map(std::borrow::Cow::into_owned)
                {
                    Some(self.jar_metadata(r, &id, &document, args))
                } else if Self::builtin_fabric_mod(&id) {
                    Some(self.builtin_metadata(r, &id))
                } else {
                    None
                }
            }
            (PERSON, ..) => {
                let (id, pointer) = split_marker(marker?, PERSON_PREFIX)?;
                let person = self.fabric_document(id)?.pointer(pointer)?.clone();
                let (id, pointer) = (id.to_owned(), pointer.to_owned());
                Some(match r.name.as_str() {
                    "getName" => Ok(text(match &person {
                        serde_json::Value::Object(fields) => fields
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        other => other.as_str().unwrap_or_default().to_owned(),
                    })),
                    "getContact" => Ok(text(format!(
                        "{CONTACT_PREFIX}{id}{}",
                        child(&pointer, "contact")
                    ))),
                    _ => self.unknown_host_call(r),
                })
            }
            (CONTACT, ..) => {
                let (id, pointer) = split_marker(marker?, CONTACT_PREFIX)?;
                let contact = self
                    .fabric_document(id)
                    .and_then(|document| document.pointer(pointer).cloned());
                Some(match (r.name.as_str(), r.descriptor.as_str()) {
                    ("get", "(Ljava/lang/String;)Ljava/util/Optional;") => {
                        let key = expect_string(Self::argument(r, args, 0).ok()?);
                        Ok(contact
                            .as_ref()
                            .and_then(|contact| contact.get(key))
                            .and_then(serde_json::Value::as_str)
                            .map_or_else(empty_optional, |value| {
                                crate::optional_wrap(text(value.to_owned()))
                            }))
                    }
                    _ => self.unknown_host_call(r),
                })
            }
            (CUSTOM_VALUE | CV_OBJECT | CV_ARRAY, ..) => {
                let (id, pointer) = split_marker(marker?, CV_PREFIX)?;
                let value = self.fabric_document(id)?.pointer(pointer)?.clone();
                let (id, pointer) = (id.to_owned(), pointer.to_owned());
                Some(self.custom_value_call(r, &id, &pointer, &value, args))
            }
            (MOD_ENVIRONMENT | CV_TYPE, "ordinal" | "name" | "toString", _) => {
                let constants = if r.owner == MOD_ENVIRONMENT {
                    ENVIRONMENTS
                } else {
                    CV_TYPES
                };
                let name = marker?.strip_prefix(r.owner.as_str())?.strip_prefix('/')?;
                let ordinal = constants.iter().position(|constant| *constant == name)?;
                Some(Ok(match r.name.as_str() {
                    "ordinal" => Value::Int(ordinal as i32),
                    _ => text(name),
                }))
            }
            (MOD_ENVIRONMENT, "matches", "(Lnet/fabricmc/api/EnvType;)Z") => {
                let name = marker?.strip_prefix(MOD_ENVIRONMENT)?.strip_prefix('/')?;
                let side = Self::argument(r, args, 0).ok().map(enum_name).unwrap_or("");
                Some(Ok(Value::Int(i32::from(
                    name == "UNIVERSAL" || name == side,
                ))))
            }
            _ => None,
        }
    }

    /// `ModEnvironment` and `CvType` constants, the markers the answers above
    /// hand out, so `==` against them holds.
    pub(crate) fn fabric_mods_static(r: &MethodRef) -> Option<Value> {
        let constants = match r.owner.as_str() {
            MOD_ENVIRONMENT => ENVIRONMENTS,
            CV_TYPE => CV_TYPES,
            _ => return None,
        };
        constants
            .contains(&r.name.as_str())
            .then(|| enum_marker(&r.owner, &r.name))
    }

    fn jar_metadata(
        &mut self,
        r: &MethodRef,
        id: &str,
        document: &serde_json::Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let field = |name: &str| document.get(name);
        let strings = |name: &str| -> Vec<Value> {
            match field(name) {
                Some(serde_json::Value::String(one)) => vec![text(one.clone())],
                Some(serde_json::Value::Array(many)) => many
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(text)
                    .collect(),
                _ => Vec::new(),
            }
        };
        match (r.name.as_str(), r.descriptor.as_str()) {
            ("getId", "()Ljava/lang/String;") => Ok(text(id)),
            ("getType", "()Ljava/lang/String;") => Ok(text("fabric")),
            ("getVersion", "()Lnet/fabricmc/loader/api/Version;") => Ok(field("version")
                .and_then(serde_json::Value::as_str)
                .map_or(Value::Ref(None), text)),
            ("getName", "()Ljava/lang/String;") => Ok(text(
                field("name")
                    .and_then(serde_json::Value::as_str)
                    .filter(|name| !name.is_empty())
                    .unwrap_or(id),
            )),
            ("getDescription", "()Ljava/lang/String;") => Ok(text(
                field("description")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
            )),
            ("getEnvironment", "()Lnet/fabricmc/loader/api/metadata/ModEnvironment;") => {
                let name = match field("environment").and_then(serde_json::Value::as_str) {
                    Some("client") => "CLIENT",
                    Some("server") => "SERVER",
                    _ => "UNIVERSAL",
                };
                Ok(enum_marker(MOD_ENVIRONMENT, name))
            }
            ("getAuthors" | "getContributors", "()Ljava/util/Collection;") => {
                let key = if r.name == "getAuthors" {
                    "authors"
                } else {
                    "contributors"
                };
                let count = field(key)
                    .and_then(serde_json::Value::as_array)
                    .map_or(0, Vec::len);
                let people = (0..count)
                    .map(|index| text(format!("{PERSON_PREFIX}{id}/{key}/{index}")))
                    .collect::<Vec<_>>();
                Ok(collection_value("list", &people))
            }
            ("getContact", "()Lnet/fabricmc/loader/api/metadata/ContactInformation;") => {
                Ok(text(format!("{CONTACT_PREFIX}{id}/contact")))
            }
            ("getLicense", "()Ljava/util/Collection;") => {
                Ok(collection_value("list", &strings("license")))
            }
            ("getProvides", "()Ljava/util/Collection;") => {
                Ok(collection_value("list", &strings("provides")))
            }
            ("containsCustomValue", "(Ljava/lang/String;)Z") => {
                let key = expect_string(Self::argument(r, args, 0)?);
                let present = field("custom").is_some_and(|custom| custom.get(key).is_some());
                Ok(Value::Int(i32::from(present)))
            }
            (
                "getCustomValue",
                "(Ljava/lang/String;)Lnet/fabricmc/loader/api/metadata/CustomValue;",
            ) => {
                let key = expect_string(Self::argument(r, args, 0)?);
                let present = field("custom").is_some_and(|custom| custom.get(key).is_some());
                Ok(if present {
                    text(format!("{CV_PREFIX}{id}{}", child("/custom", key)))
                } else {
                    Value::Ref(None)
                })
            }
            ("getIconPath", "(I)Ljava/util/Optional;") if field("icon").is_none() => {
                Ok(empty_optional())
            }
            _ => self.unknown_host_call(r),
        }
    }

    /// `java` and `minecraft` are `BuiltinModMetadata` built by the loader
    /// itself (`ModDiscoverer.createJavaMod`, `MinecraftGameProvider
    /// .getBuiltinMods`): the builder's defaults (`description` `""`,
    /// `UNIVERSAL`, no people, no license, `ContactInformation.EMPTY`, no
    /// custom values, type `builtin`). `java`'s version is Java 25's
    /// `java.specification.version`; its name is `java.vm.name`, which
    /// depends on the JDK's vendor, so it is counted. `minecraft`'s name is
    /// `Minecraft` and its version the game version a caller seeded (counted
    /// without one).
    fn builtin_metadata(&mut self, r: &MethodRef, id: &str) -> Result<Value, NativeError> {
        if r.name == "getId" && r.descriptor == "()Ljava/lang/String;" {
            return Ok(text(id));
        }
        if !matches!(id, "java" | "minecraft") {
            return self.unknown_host_call(r);
        }
        let answer = match (r.name.as_str(), r.descriptor.as_str()) {
            ("getType", "()Ljava/lang/String;") => Some(text("builtin")),
            ("getVersion", "()Lnet/fabricmc/loader/api/Version;") => {
                if id == "java" {
                    Some(text(JAVA_SPECIFICATION_VERSION))
                } else {
                    self.game_version.clone().map(text)
                }
            }
            ("getName", "()Ljava/lang/String;") if id == "minecraft" => Some(text("Minecraft")),
            ("getDescription", "()Ljava/lang/String;") => Some(text("")),
            ("getEnvironment", "()Lnet/fabricmc/loader/api/metadata/ModEnvironment;") => {
                Some(enum_marker(MOD_ENVIRONMENT, "UNIVERSAL"))
            }
            (
                "getAuthors" | "getContributors" | "getLicense" | "getProvides",
                "()Ljava/util/Collection;",
            ) => Some(collection_value("list", &[])),
            ("getContact", "()Lnet/fabricmc/loader/api/metadata/ContactInformation;") => {
                // A pointer into no document: every `get` is empty, as on
                // `ContactInformation.EMPTY`.
                Some(text(format!("{CONTACT_PREFIX}{id}/contact")))
            }
            ("containsCustomValue", "(Ljava/lang/String;)Z") => Some(Value::Int(0)),
            (
                "getCustomValue",
                "(Ljava/lang/String;)Lnet/fabricmc/loader/api/metadata/CustomValue;",
            ) => Some(Value::Ref(None)),
            _ => None,
        };
        match answer {
            Some(value) => Ok(value),
            None => self.unknown_host_call(r),
        }
    }

    fn custom_value_call(
        &mut self,
        r: &MethodRef,
        id: &str,
        pointer: &str,
        value: &serde_json::Value,
        args: &[Value],
    ) -> Result<Value, NativeError> {
        let wanted = match r.name.as_str() {
            "getAsObject" => Some("OBJECT"),
            "getAsArray" => Some("ARRAY"),
            "getAsString" => Some("STRING"),
            "getAsBoolean" => Some("BOOLEAN"),
            _ => None,
        };
        if wanted.is_some_and(|wanted| wanted != cv_type(value)) {
            return self.unknown_host_call(r);
        }
        let marker = |pointer: String| text(format!("{CV_PREFIX}{id}{pointer}"));
        match (r.name.as_str(), r.descriptor.as_str(), value) {
            ("getType", _, _) => Ok(enum_marker(CV_TYPE, cv_type(value))),
            ("getAsObject" | "getAsArray", _, _) => Ok(marker(pointer.to_owned())),
            ("getAsString", _, serde_json::Value::String(string)) => Ok(text(string.clone())),
            ("getAsBoolean", _, serde_json::Value::Bool(flag)) => Ok(Value::Int(i32::from(*flag))),
            ("size", "()I", serde_json::Value::Object(entries)) => {
                Ok(Value::Int(entries.len() as i32))
            }
            ("size", "()I", serde_json::Value::Array(items)) => Ok(Value::Int(items.len() as i32)),
            ("containsKey", "(Ljava/lang/String;)Z", serde_json::Value::Object(entries)) => {
                let key = expect_string(Self::argument(r, args, 0)?);
                Ok(Value::Int(i32::from(entries.contains_key(key))))
            }
            (
                "get",
                "(Ljava/lang/String;)Lnet/fabricmc/loader/api/metadata/CustomValue;",
                serde_json::Value::Object(entries),
            ) => {
                let key = expect_string(Self::argument(r, args, 0)?);
                Ok(if entries.contains_key(key) {
                    marker(child(pointer, key))
                } else {
                    Value::Ref(None)
                })
            }
            (
                "get",
                "(I)Lnet/fabricmc/loader/api/metadata/CustomValue;",
                serde_json::Value::Array(items),
            ) => {
                let index = expect_int(r, Self::argument(r, args, 0)?)?;
                match usize::try_from(index)
                    .ok()
                    .filter(|index| *index < items.len())
                {
                    Some(index) => Ok(marker(child(pointer, &index.to_string()))),
                    // `ArrayList.get` out of range.
                    None => Err(NativeError::UncaughtException {
                        class: "java/lang/IndexOutOfBoundsException".into(),
                        message: Some(format!(
                            "Index {index} out of bounds for length {}",
                            items.len()
                        )),
                        frame: None,
                    }),
                }
            }
            _ => self.unknown_host_call(r),
        }
    }
}
