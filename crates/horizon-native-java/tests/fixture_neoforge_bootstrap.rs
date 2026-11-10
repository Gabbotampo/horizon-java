//! The NeoForge loader statics a mod's constructor reads before the loader's
//! own classes exist on the runtime's class path: the game event bus, the
//! config type enum and tracker, the loader's own file record, the attachment
//! registry and the data-component deferred register.
//!
//! The loader stubs are compiled for `javac` but left out of the jar, so every
//! call reaches the host's model exactly as it reaches the real FML classes.
//! Expected values are the loader's API contract (one bus, six config types,
//! an empty tracker before any config loads, the metadata version of a loaded
//! id and null for an absent one) and plain Java reference/identity facts;
//! the fixture also requires zero unknown host calls, which is the regression
//! the test exists for.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const LOADER_STUBS: &[(&str, &str)] = &[
    (
        "net/neoforged/bus/api/IEventBus.java",
        "package net.neoforged.bus.api; public interface IEventBus { <T> void addListener(java.util.function.Consumer<T> listener); }",
    ),
    (
        "net/neoforged/neoforge/common/NeoForge.java",
        "package net.neoforged.neoforge.common; import net.neoforged.bus.api.IEventBus; public class NeoForge { public static IEventBus EVENT_BUS; }",
    ),
    (
        "net/neoforged/fml/config/ModConfig.java",
        "package net.neoforged.fml.config; public class ModConfig { public enum Type { CLIENT, SERVER, COMMON, STARTUP, LOCAL, SYNCED } }",
    ),
    (
        "net/neoforged/fml/config/ConfigTracker.java",
        "package net.neoforged.fml.config; public class ConfigTracker { public static ConfigTracker INSTANCE; public java.util.concurrent.ConcurrentHashMap<String, Object> configsByMod; public java.util.EnumMap<ModConfig.Type, Object> configSets; public java.util.Map<String, Object> fileMap; }",
    ),
    (
        "net/neoforged/fml/loading/FMLLoader.java",
        "package net.neoforged.fml.loading; public class FMLLoader { public static FMLLoader getCurrent() { return null; } public LoadingModList getLoadingModList() { return null; } }",
    ),
    (
        "net/neoforged/fml/loading/LoadingModList.java",
        "package net.neoforged.fml.loading; import net.neoforged.fml.loading.moddiscovery.ModFileInfo; public class LoadingModList { public ModFileInfo getModFileById(String id) { return null; } }",
    ),
    (
        "net/neoforged/fml/loading/moddiscovery/ModFileInfo.java",
        "package net.neoforged.fml.loading.moddiscovery; public class ModFileInfo { public String versionString() { return null; } }",
    ),
    (
        "net/minecraft/resources/ResourceKey.java",
        "package net.minecraft.resources; public class ResourceKey<T> { public static <T> ResourceKey<T> create(Object registry, Object location) { return null; } }",
    ),
    (
        "net/minecraft/core/Registry.java",
        "package net.minecraft.core; public interface Registry<T> {}",
    ),
    (
        "net/neoforged/neoforge/registries/NeoForgeRegistries.java",
        "package net.neoforged.neoforge.registries; import net.minecraft.core.Registry; public class NeoForgeRegistries { public static Registry<Object> ATTACHMENT_TYPES; }",
    ),
    (
        "net/neoforged/neoforge/registries/DeferredRegister.java",
        "package net.neoforged.neoforge.registries; import net.minecraft.resources.ResourceKey; import net.neoforged.bus.api.IEventBus; public class DeferredRegister<T> { public static DataComponents createDataComponents(ResourceKey<?> key, String namespace) { return null; } public static class DataComponents extends DeferredRegister<Object> { public void register(IEventBus bus) {} } }",
    ),
];

const PROBE: &str = r#"package demo;

import java.util.List;
import net.minecraft.core.Registry;
import net.minecraft.resources.ResourceKey;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.config.ConfigTracker;
import net.neoforged.fml.config.ModConfig;
import net.neoforged.fml.loading.FMLLoader;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.registries.DeferredRegister;
import net.neoforged.neoforge.registries.NeoForgeRegistries;

public final class BootstrapProbe {
    public static String eventBus() {
        IEventBus bus = NeoForge.EVENT_BUS;
        return (bus != null) + "|" + (bus == NeoForge.EVENT_BUS);
    }

    public static String configType() {
        ModConfig.Type type = ModConfig.Type.CLIENT;
        return (type != null) + "|" + (type == ModConfig.Type.CLIENT) + "|" + (type == ModConfig.Type.SERVER);
    }

    public static String tracker() {
        return ConfigTracker.INSTANCE.configsByMod.size() + "|"
            + ConfigTracker.INSTANCE.configSets.size() + "|"
            + ConfigTracker.INSTANCE.fileMap.size();
    }

    public static String loader() {
        String version = FMLLoader.getCurrent().getLoadingModList().getModFileById("demo").versionString();
        String absent = FMLLoader.getCurrent().getLoadingModList().getModFileById("absent") == null ? "null" : "present";
        return version + "|" + absent;
    }

    public static String registries() {
        Registry<Object> attachments = NeoForgeRegistries.ATTACHMENT_TYPES;
        DeferredRegister.DataComponents components = DeferredRegister.createDataComponents(
            ResourceKey.create(null, null), "demo");
        components.register(NeoForge.EVENT_BUS);
        return (attachments != null) + "|" + (components != null);
    }
}
"#;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

struct Fixture {
    root: PathBuf,
    jar: NativeJar,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn build(test: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "horizon-native-java-neoforge-bootstrap-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };
        let classes = fixture.root.join("classes");
        let mut sources = Vec::new();
        for (relative, body) in LOADER_STUBS {
            let path = fixture.root.join("src").join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, body).unwrap();
            sources.push(path);
        }
        let probe = fixture.root.join("src/demo/BootstrapProbe.java");
        fs::create_dir_all(probe.parent().unwrap()).unwrap();
        fs::write(&probe, PROBE).unwrap();
        sources.push(probe);
        let mut javac = Command::new(javac());
        javac
            .args(["--release", "21", "-d"])
            .arg(&classes)
            .args(sources);
        let status = javac.status().unwrap();
        assert!(
            status.success(),
            "javac failed for neoforge bootstrap fixture {test}"
        );

        let jar_path = fixture.root.join("bootstrap.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        for class in files_under(&classes.join("demo")) {
            let entry = class
                .strip_prefix(&classes)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(entry, SimpleFileOptions::default()).unwrap();
            zip.write_all(&fs::read(class).unwrap()).unwrap();
        }
        zip.finish().unwrap();

        fixture.jar = NativeJar::from_path(&jar_path).unwrap();
        fixture
    }

    fn text(&self, method: &str) -> String {
        // "demo" is the jar's own id, discovered with version 1.0 in the real
        // loader; an id the loader does not hold answers null.
        let mut host = self.jar.seed_host(StdHost::new().with_mod("demo", "1.0"));
        let text = match self
            .jar
            .invoke_method_with_args(
                "demo/BootstrapProbe",
                method,
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            )
            .unwrap_or_else(|error| panic!("{method} failed: {error}"))
        {
            Value::Ref(Some(text)) => text,
            other => panic!("{method} did not return a string: {other:?}"),
        };
        assert_eq!(
            host.unknown_host_calls(),
            Vec::<(String, usize)>::new(),
            "{method} left unmodelled host calls"
        );
        text
    }
}

#[test]
fn the_game_bus_exists_and_is_one_object() {
    let fixture = Fixture::build("bus");
    assert_eq!(fixture.text("eventBus"), "true|true");
}

#[test]
fn config_types_are_distinct_stable_constants() {
    let fixture = Fixture::build("type");
    assert_eq!(fixture.text("configType"), "true|true|false");
}

#[test]
fn the_config_tracker_starts_empty_instead_of_null() {
    let fixture = Fixture::build("tracker");
    assert_eq!(fixture.text("tracker"), "0|0|0");
}

#[test]
fn a_mod_reads_its_own_version_and_an_absent_id_is_null() {
    let fixture = Fixture::build("loader");
    assert_eq!(fixture.text("loader"), "1.0|null");
}

#[test]
fn the_attachment_registry_and_data_component_register_exist() {
    let fixture = Fixture::build("registries");
    assert_eq!(fixture.text("registries"), "true|true");
}
