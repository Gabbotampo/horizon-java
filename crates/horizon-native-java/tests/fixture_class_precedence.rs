//! Class precedence between a jar's copy of a loader class and the host's
//! model of it (`class_precedence.rs`).
//!
//! The rule (2026-11-13): a class the run's loader does not provide runs from
//! the jar that ships it. Forge Config API Port is the real case: it ships
//! NeoForge's `net/neoforged/fml/config` and `ModConfigSpec` classes for
//! Fabric mods. On Fabric that copy is the only definition, so a JVM runs it;
//! on NeoForge, FML and NeoForge define those classes first, so the host's
//! NeoForge model keeps answering.
//!
//! The copies below are small classes of this test's own, under the real
//! names, so which definition ran is visible in the answer: the copy counts
//! its registrations and records its constructor, while the host model counts
//! the calls it has no answer for.

use horizon_native_java::{NativeJar, RunLoader, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use zip::write::SimpleFileOptions;

const PROBE: &str = r#"package demo;

import net.neoforged.fml.config.ConfigTracker;
import net.neoforged.fml.config.ConfigValues;
import net.neoforged.neoforge.common.ModConfigSpec;

public final class Probe {
    public static String track() {
        Object result = ConfigTracker.INSTANCE.registerConfig("COMMON", "spec");
        return result + ":" + ConfigTracker.INSTANCE.count();
    }

    public static String build() {
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();
        builder.push("general");
        return builder.path();
    }

    /// A subclass of the jar's copy, whose `super` calls resolve one to the
    /// copy's own body and one to the JDK `AbstractCollection` it extends.
    static final class Sub extends ConfigValues {
        @Override public String describe() {
            return "sub>" + super.describe() + ">" + super.toString();
        }
    }

    public static String skeleton() {
        return new Sub().describe() + "|" + new ConfigValues().isEmpty();
    }
}
"#;

/// A loader-namespace copy that is also a JDK `AbstractCollection`: the jar
/// copy (`class_precedence.rs`) and the JDK skeleton (`jdk_skeletons/`)
/// both sit in its dispatch chain.
const VALUES_COPY: &str = r#"package net.neoforged.fml.config;

import java.util.AbstractCollection;
import java.util.Iterator;

public class ConfigValues extends AbstractCollection<String> {
    private final String[] items = {"a", "b"};

    @Override public int size() { return items.length; }

    @Override public Iterator<String> iterator() {
        return new Iterator<>() {
            int next;
            @Override public boolean hasNext() { return next < items.length; }
            @Override public String next() { return items[next++]; }
        };
    }

    public String describe() { return "copy:" + size(); }
}
"#;

const TRACKER_COPY: &str = r#"package net.neoforged.fml.config;

public class ConfigTracker {
    public static final ConfigTracker INSTANCE = new ConfigTracker();
    private int registered;

    public Object registerConfig(Object type, Object spec) {
        registered++;
        return "jar-copy";
    }

    public int count() { return registered; }
}
"#;

const SPEC_COPY: &str = r#"package net.neoforged.neoforge.common;

public class ModConfigSpec {
    public static class Builder {
        private final StringBuilder path = new StringBuilder("root");

        public Builder() { path.append(">init"); }

        public Builder push(String name) {
            path.append('/').append(name);
            return this;
        }

        public String path() { return path.toString(); }
    }
}
"#;

const FABRIC_META: (&str, &[u8]) = (
    "fabric.mod.json",
    br#"{"id":"probe-fixture","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
);
const NEOFORGE_META: (&str, &[u8]) = (
    "META-INF/neoforge.mods.toml",
    b"modLoader=\"javafml\"\nloaderVersion=\"[1,)\"\nlicense=\"MIT\"\n[[mods]]\nmodId=\"probefixture\"\nversion=\"1.0\"\n",
);
const PORT_META: (&str, &[u8]) = (
    "fabric.mod.json",
    br#"{"id":"configport-fixture","version":"1.0"}"#,
);

/// The compiled classes of this test, in a scratch folder removed on drop.
struct Classes {
    root: PathBuf,
}

impl Drop for Classes {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Classes {
    fn compile(test: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "horizon-native-java-precedence-{test}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let javac = std::env::var_os("JAVA_HOME")
            .map(|home| PathBuf::from(home).join("bin/javac"))
            .unwrap_or_else(|| "javac".into());
        let mut command = Command::new(javac);
        command
            .args(["--release", "21", "-d"])
            .arg(root.join("classes"));
        for (path, source) in [
            ("demo/Probe.java", PROBE),
            ("net/neoforged/fml/config/ConfigTracker.java", TRACKER_COPY),
            (
                "net/neoforged/neoforge/common/ModConfigSpec.java",
                SPEC_COPY,
            ),
            ("net/neoforged/fml/config/ConfigValues.java", VALUES_COPY),
        ] {
            let file = root.join("src").join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, source).unwrap();
            command.arg(file);
        }
        assert!(
            command.status().unwrap().success(),
            "javac failed for {test}"
        );
        Self { root }
    }

    /// A jar of `metadata` and every class file under each of `packages`.
    fn jar(&self, name: &str, metadata: (&str, &[u8]), packages: &[&str]) -> NativeJar {
        let classes = self.root.join("classes");
        let path = self.root.join(format!("{name}.jar"));
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        zip.start_file(metadata.0, SimpleFileOptions::default())
            .unwrap();
        zip.write_all(metadata.1).unwrap();
        for package in packages {
            for file in files_under(&classes.join(package)) {
                let entry = file.strip_prefix(&classes).unwrap().to_string_lossy();
                zip.start_file(entry.replace('\\', "/"), SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(&fs::read(file).unwrap()).unwrap();
            }
        }
        zip.finish().unwrap();
        NativeJar::from_path(&path).unwrap()
    }
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
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

/// Run `demo.Probe.<method>()` and return its text and the host's unknown
/// calls.
fn run(jar: &NativeJar, links: &[&NativeJar], method: &str) -> (String, Vec<(String, usize)>) {
    let mut host = jar.seed_host(StdHost::new());
    for (index, link) in links.iter().enumerate() {
        host.link_jar(format!("link-{index}"), link).unwrap();
    }
    let text = match jar
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
        // The host model's neutral answer for a call it counts.
        Value::Ref(None) => "null".to_owned(),
        other => panic!("{method} did not return a string: {other:?}"),
    };
    (text, host.unknown_host_calls())
}

#[test]
fn a_fabric_run_runs_a_linked_jars_copy_of_neoforge_classes() {
    let classes = Classes::compile("fabric-linked");
    let probe = classes.jar("probe", FABRIC_META, &["demo"]);
    assert_eq!(probe.run_loader(), RunLoader::Fabric);
    let port = classes.jar("port", PORT_META, &["net"]);
    assert_eq!(
        run(&probe, &[&port], "track"),
        ("jar-copy:1".to_owned(), Vec::new())
    );
    // The constructor and the builder calls are the copy's, not the
    // `ModConfigSpec$Builder` model's.
    assert_eq!(
        run(&probe, &[&port], "build"),
        ("root>init/general".to_owned(), Vec::new())
    );
}

#[test]
fn a_fabric_run_runs_its_own_copy_of_neoforge_classes() {
    let classes = Classes::compile("fabric-own");
    let probe = classes.jar("probe", FABRIC_META, &["demo", "net"]);
    assert_eq!(
        run(&probe, &[], "track"),
        ("jar-copy:1".to_owned(), Vec::new())
    );
    assert_eq!(
        run(&probe, &[], "build"),
        ("root>init/general".to_owned(), Vec::new())
    );
}

#[test]
fn a_neoforge_run_keeps_the_host_model_over_a_jars_copy() {
    let classes = Classes::compile("neoforge-linked");
    let probe = classes.jar("probe", NEOFORGE_META, &["demo"]);
    assert_eq!(probe.run_loader(), RunLoader::NeoForge);
    let port = classes.jar("port", PORT_META, &["net"]);
    let (text, unknown) = run(&probe, &[&port], "track");
    assert_ne!(text, "jar-copy:1");
    assert!(
        unknown.iter().any(|(call, _)| call
            == "net/neoforged/fml/config/ConfigTracker#registerConfig(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"),
        "{unknown:?}"
    );
    let (text, _) = run(&probe, &[&port], "build");
    assert_ne!(text, "root>init/general");
}

#[test]
fn a_fabric_run_without_a_copy_keeps_the_host_model() {
    let classes = Classes::compile("fabric-none");
    let probe = classes.jar("probe", FABRIC_META, &["demo"]);
    let (text, unknown) = run(&probe, &[], "track");
    assert_ne!(text, "jar-copy:1");
    assert!(
        unknown.iter().any(|(call, _)| call
            == "net/neoforged/fml/config/ConfigTracker#registerConfig(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"),
        "{unknown:?}"
    );
}

/// A `super` call on a subclass of a jar's loader-namespace copy goes where
/// the JVM resolves it: `super.describe()` to the copy's own body (the jar
/// copy path), `super.toString()` past the copy to `AbstractCollection`'s
/// body (the JDK skeleton path), and the copy's constructor chains into
/// `AbstractCollection()`. Expected text: OpenJDK 25.0.5 running the same
/// classes (`java -cp classes demo.Probe`-style driver calling `skeleton()`).
#[test]
fn a_jar_copy_extending_a_jdk_skeleton_runs_each_body_where_the_jvm_resolves_it() {
    let classes = Classes::compile("fabric-skeleton");
    let probe = classes.jar("probe", FABRIC_META, &["demo"]);
    let port = classes.jar("port", PORT_META, &["net"]);
    assert_eq!(
        run(&probe, &[&port], "skeleton"),
        ("sub>copy:2>[a, b]|false".to_owned(), Vec::new())
    );
}
