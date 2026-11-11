//! Unmodifiable collections, `UUID.randomUUID`, `Runtime` and
//! `FabricLoader.isModLoaded` answer what the platform answers.
//!
//! The stubs exist only for `javac`: only `demo/` ships, so every call reaches
//! the host's model.

/// The expected strings, one `method=text` line per `Probe` method: the
/// output of `java` (OpenJDK 27, Guava 33.6.0 from the 26.3 server libraries)
/// running the same `Probe`, pasted verbatim. On that JVM `processors` printed
/// `16`, this machine's `nproc`; it is compared with the host's count below.
const EXPECTED: &str = "\
lists=UnsupportedOperationException|UnsupportedOperationException|UnsupportedOperationException|UnsupportedOperationException|b|2|true|[a, b]\n\
setsAndMaps=UnsupportedOperationException|IllegalArgumentException:duplicate element: a|UnsupportedOperationException|IllegalArgumentException:duplicate key: k|1|true|{k=1}|[a]\n\
views=UnsupportedOperationException|2|y|UnsupportedOperationException|UnsupportedOperationException\n\
copies=1|1|2|UnsupportedOperationException|UnsupportedOperationException|a\n\
equality=true|true|true|true|false|true\n\
uuids=false|36|4|true|4\n\
";

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{NativeJar, StdHost};
use std::io::Write;

const GUAVA_STUB: &str = r#"package com.google.common.collect;

public abstract class ImmutableList<E> extends java.util.AbstractList<E> {
    public static <E> ImmutableList<E> copyOf(java.util.Collection<? extends E> c) { return null; }
}
"#;

const LOADER_STUB: &str = r#"package net.fabricmc.loader.api;

public interface FabricLoader {
    static FabricLoader getInstance() { return null; }
    boolean isModLoaded(String id);
}
"#;

const PROBE: &str = r##"package demo;

import com.google.common.collect.ImmutableList;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import net.fabricmc.loader.api.FabricLoader;

public final class Probe {
    static String t(Runnable action) {
        try {
            action.run();
            return "ok";
        } catch (RuntimeException e) {
            return e.getClass().getSimpleName() + (e.getMessage() == null ? "" : ":" + e.getMessage());
        }
    }

    public static String lists() {
        List<String> list = List.of("a", "b");
        return t(() -> list.add("c")) + "|" + t(() -> list.set(0, "z")) + "|"
            + t(() -> list.remove("a")) + "|" + t(() -> list.clear()) + "|" + list.get(1) + "|"
            + list.size() + "|" + list.contains("a") + "|" + list;
    }

    public static String setsAndMaps() {
        Set<String> set = Set.of("a");
        Map<String, Integer> map = Map.of("k", 1);
        return t(() -> set.add("b")) + "|" + t(() -> Set.of("a", "a")) + "|"
            + t(() -> map.put("j", 2)) + "|" + t(() -> Map.of("k", 1, "k", 2)) + "|"
            + map.get("k") + "|" + set.contains("a") + "|" + map + "|" + set;
    }

    public static String views() {
        List<String> base = new ArrayList<>();
        base.add("x");
        List<String> view = Collections.unmodifiableList(base);
        String before = t(() -> view.add("y"));
        base.add("y");
        return before + "|" + view.size() + "|" + view.get(1) + "|"
            + t(() -> Collections.emptyList().add("q")) + "|"
            + t(() -> Collections.singletonList("s").add("q"));
    }

    public static String copies() {
        List<String> source = new ArrayList<>();
        source.add("a");
        List<String> copy = List.copyOf(source);
        ImmutableList<String> guava = ImmutableList.copyOf(source);
        source.add("b");
        return copy.size() + "|" + guava.size() + "|" + source.size() + "|"
            + t(() -> guava.add("c")) + "|" + t(() -> copy.add("c")) + "|" + guava.get(0);
    }

    public static String equality() {
        List<String> grown = new ArrayList<>();
        grown.add("a");
        return List.of("a").equals(List.of("a")) + "|" + List.of("a").equals(grown) + "|"
            + Set.of("a", "b").equals(Set.of("b", "a")) + "|" + Map.of("k", 1).equals(Map.of("k", 1))
            + "|" + List.of("a").equals(List.of("b")) + "|" + grown.equals(List.of("a"));
    }

    public static String uuids() {
        String a = UUID.randomUUID().toString();
        String b = UUID.randomUUID().toString();
        return a.equals(b) + "|" + a.length() + "|" + a.charAt(14) + "|"
            + ("89ab".indexOf(a.charAt(19)) >= 0) + "|" + b.charAt(14);
    }

    public static String processors() {
        return String.valueOf(Runtime.getRuntime().availableProcessors());
    }

    public static String memory() {
        return String.valueOf(Runtime.getRuntime().maxMemory());
    }

    public static String mods() {
        FabricLoader loader = FabricLoader.getInstance();
        return loader.isModLoaded("sibling") + "|" + loader.isModLoaded("fabric-api") + "|"
            + loader.isModLoaded("absent") + "|" + loader.isModLoaded("minecraft");
    }
}
"##;

fn fixture() -> javac_fixture::Fixture {
    javac_fixture::Fixture::build(
        "collections",
        &[
            ("com/google/common/collect/ImmutableList.java", GUAVA_STUB),
            ("net/fabricmc/loader/api/FabricLoader.java", LOADER_STUB),
            ("demo/Probe.java", PROBE),
        ],
    )
}

fn check(methods: &[&str]) {
    let fixture = fixture();
    for method in methods {
        let expected = EXPECTED
            .lines()
            .find_map(|line| line.strip_prefix(method)?.strip_prefix('='))
            .unwrap_or_else(|| panic!("no expected line for {method}"));
        assert_eq!(fixture.text(method), expected, "{method}");
    }
}

#[test]
fn unmodifiable_collections_refuse_mutation() {
    check(&["lists", "setsAndMaps", "views"]);
}

#[test]
fn copies_do_not_follow_their_source_and_equality_reads_contents() {
    // `ImmutableList.copyOf(source)` used to answer `source` itself.
    check(&["copies", "equality"]);
}

#[test]
fn random_uuids_are_distinct_version_4() {
    check(&["uuids"]);
}

#[test]
fn runtime_reports_the_host_processors_and_counts_the_heap() {
    let fixture = fixture();
    let host_processors = std::thread::available_parallelism().unwrap().get();
    assert_eq!(fixture.text("processors"), host_processors.to_string());
    // There is no Java heap: `maxMemory` is counted as unknown rather than
    // answered with an invented size.
    let mut host = fixture.jar.seed_host(StdHost::new());
    fixture.text_on(&mut host, "memory");
    assert!(
        host.unknown_host_calls()
            .iter()
            .any(|(call, _)| call == "java/lang/Runtime#maxMemory()J"),
        "{:?}",
        host.unknown_host_calls()
    );
}

/// A jar holding only `fabric.mod.json` for mod `id`.
fn metadata_jar(id: &str) -> NativeJar {
    let mut bytes = Vec::new();
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
    zip.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    write!(zip, r#"{{"id":"{id}","version":"2.0"}}"#).unwrap();
    zip.finish().unwrap();
    NativeJar::from_bytes(&bytes).unwrap()
}

#[test]
fn is_mod_loaded_answers_from_the_jars_present() {
    // Fabric Loader's `isModLoaded` is `modMap.containsKey(id)` (`javap -c`
    // of fabric-loader 0.19.5), and its mod map holds every jar it loaded,
    // declared as a dependency or not. `fabric-api` is the façade Horizon
    // runs in that jar's place for a Fabric mod.
    let fixture = fixture();
    let mut host = fixture.jar.seed_host(StdHost::new());
    host.link_jar("sibling.jar", &metadata_jar("sibling"))
        .unwrap();
    assert_eq!(fixture.text_on(&mut host, "mods"), "true|true|false|true");
}
