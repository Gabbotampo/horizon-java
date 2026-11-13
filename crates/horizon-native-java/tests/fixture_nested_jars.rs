//! A jar's bundled libraries are on its class path (`META-INF/jars/*.jar`).
//!
//! Fabric's Jar-in-Jar puts a bundle's classes on the mod's class path, and a
//! loader's dependency graph sees the bundled jars' own mod ids. Before this,
//! `respackopts` called `dev.jfronny.commons.ref.WeakSet` from its bundled
//! `libjf-base` dependency and the runtime answered "class not on the class
//! path" (`docs/in-game-survey.md`, "Top causes" §5).

use horizon_native_java::{NativeJar, StdHost, dependencies};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const OUTER: &str = r##"package demo;

public class Entry implements net.fabricmc.api.ModInitializer {
    @Override
    public void onInitialize() {
        System.out.println(Inner.tag());
    }
}
"##;

const INNER: &str = r##"package demo;

public class Inner {
    public static String tag() {
        return "from-inner";
    }
}
"##;

const MOD_INITIALIZER: &str = r##"package net.fabricmc.api;

public interface ModInitializer {
    void onInitialize();
}
"##;

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

/// Compile `sources` under `root/src` into `root/classes`.
fn compile(root: &Path, classpath: Option<&Path>, sources: &[(&str, &str)]) {
    let classes = root.join("classes");
    let mut javac = Command::new(javac());
    javac.args(["--release", "21", "-d"]).arg(&classes);
    if let Some(classpath) = classpath {
        javac.arg("-cp").arg(classpath);
    }
    for (path, source) in sources {
        let file = root.join("src").join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        javac.arg(file);
    }
    assert!(javac.status().unwrap().success(), "javac failed");
}

/// A jar holding every class under `root/classes` whose name starts with
/// `prefix`, plus `metadata`.
fn jar_from(root: &Path, prefix: &str, metadata: Option<&str>) -> Vec<u8> {
    let path = root.join(format!("{}.jar", prefix.replace('/', "-")));
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    if let Some(metadata) = metadata {
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(metadata.as_bytes()).unwrap();
    }
    let classes = root.join("classes");
    for class in files_under(&classes.join(prefix)) {
        let entry = class
            .strip_prefix(&classes)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        zip.start_file(entry, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(class).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    fs::read(&path).unwrap()
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
            "horizon-native-java-nested-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };

        // The bundled library compiles on its own, exactly like a published
        // library jar.
        let inner_root = fixture.root.join("inner");
        compile(&inner_root, None, &[("demo/Inner.java", INNER)]);
        let inner = jar_from(
            &inner_root,
            "demo",
            Some(r#"{"schemaVersion":1,"id":"inner-lib","version":"1.0"}"#),
        );

        let outer_root = fixture.root.join("outer");
        compile(
            &outer_root,
            Some(&inner_root.join("classes")),
            &[
                ("demo/Entry.java", OUTER),
                ("net/fabricmc/api/ModInitializer.java", MOD_INITIALIZER),
            ],
        );
        let classes = outer_root.join("classes");
        let jar_path = outer_root.join("outer.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"schemaVersion":1,"id":"outer-fixture","version":"1.0","entrypoints":{"main":"demo.Entry"},"depends":{"inner-lib":">=1"},"jars":[{"file":"META-INF/jars/inner.jar"}]}"#,
        )
        .unwrap();
        zip.start_file("META-INF/jars/inner.jar", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&inner).unwrap();
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

    /// Run the entrypoint and return what it printed.
    fn printed(&self) -> String {
        let mut host = self
            .jar
            .seed_host(StdHost::new().with_environment_type("SERVER"));
        let entries = self.jar.discover_entrypoints_on("main", "SERVER");
        for entry in &entries {
            self.jar
                .execute_with_budget(
                    &entry.class,
                    entry.method.as_deref(),
                    "main",
                    &mut host,
                    &horizon_native_java::ExecutionBudget::within(std::time::Duration::from_secs(
                        30,
                    )),
                )
                .unwrap();
        }
        host.take_stdout()
    }
}

#[test]
fn a_bundled_jar_s_class_is_on_the_outer_jar_s_class_path() {
    let fixture = Fixture::build("classpath");
    assert_eq!(fixture.printed().trim(), "from-inner");
}

#[test]
fn a_bundle_provides_the_mod_ids_of_its_bundled_jars() {
    let fixture = Fixture::build("plan");
    let jars = [("outer.jar", &fixture.jar)];
    let plan = dependencies::plan(&jars);
    // `outer-fixture`’s nested jar declares the id `inner-lib`; nothing else
    // is missing.
    assert!(plan.missing.is_empty(), "missing: {:?}", plan.missing);
}
