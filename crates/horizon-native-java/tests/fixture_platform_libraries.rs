//! Platform library surface a mod's bootstrap reaches through jars the game
//! ships: Fabric API's per-pack-type `ResourceLoader`, Guava's
//! `Maps.newConcurrentMap` and Netty's `PlatformDependent` OS predicates.
//!
//! Stubs are compiled for `javac` and left out of the jar so every call
//! reaches the host. Expected values are the platform's documented answers:
//! `ResourceLoader.get` is `IMPL_MAP.computeIfAbsent(type, ...)` on the real
//! loader (so one instance per pack type, never null), a concurrent map is a
//! mutable map, and the host reports `os.name=Linux`, so Netty's `isLinux` is
//! true and the other two false.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use zip::write::SimpleFileOptions;

#[path = "support/unique_dir.rs"]
mod unique_dir;
use unique_dir::unique_suffix;

const STUBS: &[(&str, &str)] = &[
    (
        "net/fabricmc/fabric/api/resource/v1/ResourceLoader.java",
        "package net.fabricmc.fabric.api.resource.v1; import net.minecraft.resources.Identifier; import net.minecraft.server.packs.PackType; import net.minecraft.server.packs.resources.PreparableReloadListener; public interface ResourceLoader { static ResourceLoader get(PackType type) { return null; } void registerReloadListener(Identifier id, PreparableReloadListener listener); void addListenerOrdering(Identifier first, Identifier second); }",
    ),
    (
        "net/minecraft/server/packs/PackType.java",
        "package net.minecraft.server.packs; public enum PackType { CLIENT_RESOURCES, SERVER_DATA }",
    ),
    (
        "net/minecraft/resources/Identifier.java",
        "package net.minecraft.resources; public class Identifier { public Identifier(String id) {} }",
    ),
    (
        "net/minecraft/server/packs/resources/PreparableReloadListener.java",
        "package net.minecraft.server.packs.resources; public interface PreparableReloadListener {}",
    ),
    (
        "com/google/common/collect/Maps.java",
        "package com.google.common.collect; public class Maps { public static <K, V> java.util.concurrent.ConcurrentMap<K, V> newConcurrentMap() { return null; } }",
    ),
    (
        "io/netty/util/internal/PlatformDependent.java",
        "package io.netty.util.internal; public class PlatformDependent { public static boolean isOsx() { return false; } public static boolean isWindows() { return false; } public static boolean isLinux() { return false; } }",
    ),
];

const PROBE: &str = r#"package demo;

import com.google.common.collect.Maps;
import io.netty.util.internal.PlatformDependent;
import java.util.concurrent.ConcurrentMap;
import net.fabricmc.fabric.api.resource.v1.ResourceLoader;
import net.minecraft.resources.Identifier;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.resources.PreparableReloadListener;

public final class PlatformProbe {
    static final class Listener implements PreparableReloadListener {}

    public static String resourceLoaders() {
        ResourceLoader server = ResourceLoader.get(PackType.SERVER_DATA);
        server.registerReloadListener(new Identifier("demo:test"), new Listener());
        return (server != null) + "|"
            + (server == ResourceLoader.get(PackType.SERVER_DATA)) + "|"
            + (ResourceLoader.get(PackType.CLIENT_RESOURCES) != null);
    }

    public static String concurrentMap() {
        ConcurrentMap<String, String> map = Maps.newConcurrentMap();
        map.put("k", "v");
        return (map != null) + "|" + map.get("k") + "|" + map.size();
    }

    public static String nettyOs() {
        return PlatformDependent.isOsx() + "|" + PlatformDependent.isWindows() + "|" + PlatformDependent.isLinux();
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
        let unique = unique_suffix();
        let root = std::env::temp_dir().join(format!(
            "horizon-native-java-platform-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };
        let classes = fixture.root.join("classes");
        let mut sources = Vec::new();
        for (relative, body) in STUBS {
            let path = fixture.root.join("src").join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, body).unwrap();
            sources.push(path);
        }
        let probe = fixture.root.join("src/demo/PlatformProbe.java");
        fs::create_dir_all(probe.parent().unwrap()).unwrap();
        fs::write(&probe, PROBE).unwrap();
        sources.push(probe);
        let status = Command::new(javac())
            .args(["--release", "21", "-d"])
            .arg(&classes)
            .args(sources)
            .status()
            .unwrap();
        assert!(status.success(), "javac failed for platform fixture {test}");

        let jar_path = fixture.root.join("platform.jar");
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
        let mut host = self.jar.seed_host(StdHost::new());
        let text = match self
            .jar
            .invoke_method_with_args(
                "demo/PlatformProbe",
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
fn a_resource_loader_exists_once_per_pack_type() {
    let fixture = Fixture::build("resources");
    assert_eq!(fixture.text("resourceLoaders"), "true|true|true");
}

#[test]
fn a_guava_concurrent_map_is_a_mutable_map() {
    let fixture = Fixture::build("concurrent-map");
    assert_eq!(fixture.text("concurrentMap"), "true|v|1");
}

#[test]
fn netty_reads_the_linux_platform_the_host_reports() {
    let fixture = Fixture::build("netty");
    assert_eq!(fixture.text("nettyOs"), "false|false|true");
}
