//! `Registry.iterator()` over the values a jar registered.
//!
//! `entity-view-distance`'s `main` walks `BuiltInRegistries.ENTITY_TYPE` with a
//! for-each loop; the registry marker had no iterator, so the loop's
//! `Iterator.hasNext` failed the whole entrypoint ("malformed
//! java/util/Iterator marker", `docs/in-game-survey.md`, "Top causes" §5).
//!
//! The expected values are Java's own semantics for an iterable that yields
//! the registered values in registration order — the fixture's registry is the
//! game classes stubbed out (compiled, left out of the jar), so the host is
//! what answers `iterator()`. The game's own registry entries are not
//! materialised (no objects exist for them), which is why the empty case is
//! also pinned: a registry with no host-recorded entry iterates zero times
//! instead of failing.

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

const PROBE: &str = r##"package demo;

import java.util.ArrayList;
import java.util.List;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;

public class Probe {
    static String walk() {
        List<String> seen = new ArrayList<>();
        for (String value : BuiltInRegistries.ITEM) {
            seen.add(value);
        }
        return String.join(",", seen);
    }

    static String registered() {
        int empty = 0;
        for (String ignored : BuiltInRegistries.ITEM) {
            empty++;
        }
        Registry.register(BuiltInRegistries.ITEM,
                Identifier.fromNamespaceAndPath("demo", "first"), "first");
        Registry.register(BuiltInRegistries.ITEM,
                Identifier.fromNamespaceAndPath("demo", "second"), "second");
        return empty + ":" + walk();
    }
}
"##;

const ENTRY: &str = r##"package demo;

public class Entry {
    public void onInitialize() {
        Probe.registered();
    }
}
"##;

/// The game classes the fixture compiles against; left out of the jar so the
/// host answers them.
const STUBS: &[(&str, &str)] = &[
    (
        "net/minecraft/core/Registry.java",
        r#"package net.minecraft.core;

public interface Registry<T> extends Iterable<T> {
    static <T> T register(Registry<T> registry, net.minecraft.resources.Identifier id, T value) {
        return value;
    }
}
"#,
    ),
    (
        "net/minecraft/core/DefaultedRegistry.java",
        r#"package net.minecraft.core;

public interface DefaultedRegistry<T> extends Registry<T> {
}
"#,
    ),
    (
        "net/minecraft/core/registries/BuiltInRegistries.java",
        r#"package net.minecraft.core.registries;

import net.minecraft.core.DefaultedRegistry;

public final class BuiltInRegistries {
    public static final DefaultedRegistry<String> ITEM = null;
}
"#,
    ),
    (
        "net/minecraft/resources/Identifier.java",
        r#"package net.minecraft.resources;

public final class Identifier {
    public static Identifier fromNamespaceAndPath(String namespace, String path) {
        return null;
    }
}
"#,
    ),
];

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
            "horizon-native-java-registry-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };
        let classes = fixture.root.join("classes");
        let mut javac = Command::new(javac());
        javac.args(["--release", "21", "-d"]).arg(&classes);
        let mut sources = vec![("demo/Probe.java", PROBE), ("demo/Entry.java", ENTRY)];
        sources.extend(STUBS.iter().copied());
        for (path, source) in sources {
            let file = fixture.root.join("src").join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, source).unwrap();
            javac.arg(file);
        }
        let status = javac.status().unwrap();
        assert!(status.success(), "javac failed for fixture {test}");

        let jar_path = fixture.root.join("registry.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"registry-fixture","version":"1.0","entrypoints":{"main":"demo.Entry"}}"#,
        )
        .unwrap();
        // Only the fixture's own classes are jarred; the game stubs stay out.
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
        match self
            .jar
            .invoke_method_with_args(
                "demo/Probe",
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
        }
    }
}

#[test]
fn a_registry_iterates_the_values_registered_through_the_host() {
    let fixture = Fixture::build("registered");
    assert_eq!(fixture.text("registered"), "0:first,second");
}
