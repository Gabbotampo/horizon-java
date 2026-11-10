//! The NeoForge loader statics a mod's bootstrap reads, answered from the
//! host's own role, virtual roots and seeded game version.
//!
//! The stub loader classes are compiled for `javac` but left out of the jar:
//! the probe must reach the host's model exactly as it reaches the real
//! loader's classes. `java` on the same source with a server-side stub prints
//! the same words for the side and path questions; the version is the value
//! the caller seeded, which the real loader reads from the game.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, process::Command};
use zip::write::SimpleFileOptions;

const LOADER_STUBS: &[(&str, &str)] = &[
    (
        "net/neoforged/api/distmarker/Dist.java",
        "package net.neoforged.api.distmarker; public enum Dist { CLIENT, DEDICATED_SERVER; public boolean isClient() { return this == CLIENT; } public boolean isDedicatedServer() { return this == DEDICATED_SERVER; } }",
    ),
    (
        "net/neoforged/fml/loading/FMLEnvironment.java",
        "package net.neoforged.fml.loading; import net.neoforged.api.distmarker.Dist; public class FMLEnvironment { public static Dist dist; public static Dist getDist() { return dist; } public static boolean isProduction() { return true; } }",
    ),
    (
        "net/neoforged/fml/loading/FMLPaths.java",
        "package net.neoforged.fml.loading; import java.nio.file.Path; public enum FMLPaths { GAMEDIR, CONFIGDIR, MODSDIR; public Path get() { return null; } }",
    ),
    (
        "net/neoforged/fml/ModList.java",
        "package net.neoforged.fml; import java.util.Optional; public class ModList { public static ModList get() { return null; } public Optional<ModContainer> getModContainerById(String id) { return Optional.empty(); } public boolean isLoaded(String id) { return false; } }",
    ),
    (
        "net/neoforged/fml/ModContainer.java",
        "package net.neoforged.fml; import net.neoforged.bus.api.IEventBus; import net.neoforged.neoforgespi.language.IModInfo; public class ModContainer { public IEventBus getEventBus() { return null; } public IModInfo getModInfo() { return null; } }",
    ),
    (
        "net/neoforged/bus/api/IEventBus.java",
        "package net.neoforged.bus.api; public interface IEventBus {}",
    ),
    (
        "net/neoforged/neoforgespi/language/IModInfo.java",
        "package net.neoforged.neoforgespi.language; import org.apache.maven.artifact.versioning.ArtifactVersion; public interface IModInfo { ArtifactVersion getVersion(); String getModId(); }",
    ),
    (
        "org/apache/maven/artifact/versioning/ArtifactVersion.java",
        "package org.apache.maven.artifact.versioning; public interface ArtifactVersion { String toString(); }",
    ),
    (
        "net/minecraft/WorldVersion.java",
        "package net.minecraft; public interface WorldVersion { String name(); }",
    ),
    (
        "net/minecraft/DetectedVersion.java",
        "package net.minecraft; public class DetectedVersion { public static WorldVersion tryDetectVersion() { return null; } }",
    ),
];

const PROBE: &str = r#"package demo;

import net.neoforged.api.distmarker.Dist;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.ModList;
import net.neoforged.fml.loading.FMLEnvironment;
import net.neoforged.fml.loading.FMLPaths;

public final class LoaderProbe {
    public static String run() throws Exception {
        Dist dist = FMLEnvironment.getDist();
        java.lang.reflect.Method reflected = FMLEnvironment.class.getMethod("getDist");
        Dist fromReflection = (Dist) reflected.invoke(null);
        ModContainer minecraft = ModList.get().getModContainerById("minecraft").get();
        String bus = minecraft.getEventBus() == null ? "no-bus" : "bus";
        return (dist == Dist.DEDICATED_SERVER) + "|" + dist.isClient() + "|"
            + dist.isDedicatedServer() + "|" + FMLEnvironment.dist.isDedicatedServer() + "|"
            + FMLPaths.CONFIGDIR.get().toString() + "|" + FMLPaths.GAMEDIR.get().toString() + "|"
            + minecraft.getModInfo().getVersion().toString() + "|" + bus + "|"
            + ModList.get().isLoaded("minecraft") + "|"
            + net.minecraft.DetectedVersion.tryDetectVersion().name() + "|"
            + (fromReflection == Dist.DEDICATED_SERVER);
    }
}
"#;

#[test]
fn source_built_loader_probe_reads_host_side_paths_and_version() {
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-neoforge-loader-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let src = root.join("src");
    let classes = root.join("classes");
    let mut sources = Vec::new();
    for (relative, body) in LOADER_STUBS {
        let path = src.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
        sources.push(path);
    }
    fs::create_dir_all(src.join("demo")).unwrap();
    let probe = src.join("demo/LoaderProbe.java");
    fs::write(&probe, PROBE).unwrap();
    sources.push(probe);

    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| std::path::PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    fs::create_dir_all(&classes).unwrap();
    let status = Command::new(javac)
        .arg("--release")
        .arg("17")
        .arg("-d")
        .arg(&classes)
        .args(&sources)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");

    let jar_path = root.join("loader-probe.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    // A loader stub in the jar would answer over the host; keep only the
    // probe, exactly as a mod jar holds its own code and no loader classes.
    let compiled = classes.join("demo");
    for class in files_under(&compiled) {
        let name = class
            .strip_prefix(&classes)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(class).unwrap()).unwrap();
    }
    zip.finish().unwrap();

    let jar = NativeJar::from_path(&jar_path).unwrap();
    let run = |side: &str, version: Option<&str>| {
        let mut seed = StdHost::new().with_environment_type(side);
        if let Some(version) = version {
            seed = seed.with_game_version(version);
        }
        let mut host = jar.seed_host(seed);
        match jar
            .invoke_method(
                "demo.LoaderProbe",
                "run",
                "()Ljava/lang/String;",
                None,
                &mut host,
            )
            .unwrap()
        {
            Value::Ref(Some(text)) => text,
            other => panic!("probe answered {other:?}"),
        }
    };
    assert_eq!(
        run("SERVER", Some("26.3")),
        "true|false|true|true|/horizon/config|/horizon/game|26.3|bus|true|26.3|true"
    );
    assert_eq!(
        run("CLIENT", Some("26.3")),
        "false|true|false|false|/horizon/config|/horizon/game|26.3|bus|true|26.3|false"
    );
    // With no seeded version the host has no release to report: the container
    // is absent, so `Optional.get` throws instead of inventing one.
    let mut host = jar.seed_host(StdHost::new().with_environment_type("SERVER"));
    let error = jar
        .invoke_method(
            "demo.LoaderProbe",
            "run",
            "()Ljava/lang/String;",
            None,
            &mut host,
        )
        .unwrap_err();
    assert!(
        format!("{error}").contains("NoSuchElementException"),
        "an unseeded game version must surface as the empty Optional: {error}"
    );

    let _ = fs::remove_dir_all(root);
}

fn files_under(root: &std::path::Path) -> Vec<std::path::PathBuf> {
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
