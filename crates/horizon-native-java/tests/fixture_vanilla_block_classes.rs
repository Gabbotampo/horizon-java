//! A vanilla block constant answers `instanceof` for its real class when the
//! game seeds the host with the prepared block class chains: a jar's
//! `Blocks.FIRE instanceof FireBlock` is true, and false for another class.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{collections::BTreeMap, fs, io::Write, path::Path, process::Command, sync::Arc};
use zip::write::SimpleFileOptions;

fn javac() -> std::path::PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| std::path::PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn files_under(root: &Path) -> Vec<std::path::PathBuf> {
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

fn build_jar(
    name: &str,
    sources: BTreeMap<&str, &str>,
    resources: &[(&str, &str)],
    excluded: &[&str],
) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-vanilla-block-classes-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let src = root.join("src");
    let classes = root.join("classes");
    fs::create_dir_all(&classes).unwrap();
    let mut paths = Vec::new();
    for (relative, body) in sources {
        let path = src.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, body).unwrap();
        paths.push(path);
    }
    let status = Command::new(javac())
        .arg("--release")
        .arg("17")
        .arg("-d")
        .arg(&classes)
        .args(paths)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed for {name}");

    let jar_path = root.join(format!("{name}.jar"));
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    for (path, body) in resources {
        zip.start_file(*path, SimpleFileOptions::default()).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    for class in files_under(&classes) {
        let entry = class
            .strip_prefix(&classes)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if excluded.contains(&entry.as_str()) {
            continue;
        }
        zip.start_file(entry, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(class).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    jar_path
}

#[test]
fn vanilla_block_constants_answer_instanceof_from_seeded_classes() {
    let mut sources = BTreeMap::new();
    sources.insert(
        "demo/Probe.java",
        "package demo; public final class Probe { public static void check(String label, boolean value) {} }",
    );
    sources.insert(
        "net/minecraft/world/level/block/Block.java",
        "package net.minecraft.world.level.block; public class Block {}",
    );
    sources.insert(
        "net/minecraft/world/level/block/FireBlock.java",
        "package net.minecraft.world.level.block; public class FireBlock extends Block {}",
    );
    sources.insert(
        "net/minecraft/world/level/block/ChestBlock.java",
        "package net.minecraft.world.level.block; public class ChestBlock extends Block {}",
    );
    sources.insert(
        "net/minecraft/world/level/block/Blocks.java",
        "package net.minecraft.world.level.block; public final class Blocks { public static final Block FIRE = null; }",
    );
    sources.insert(
        "demo/Main.java",
        r#"package demo;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.ChestBlock;
import net.minecraft.world.level.block.FireBlock;
public final class Main {
    public static void main() {
        Probe.check("fire", Blocks.FIRE instanceof FireBlock);
        Probe.check("not-chest", !(Blocks.FIRE instanceof ChestBlock));
    }
}
"#,
    );
    let jar_path = build_jar(
        "vanilla-block-classes",
        sources,
        &[],
        &[
            "demo/Probe.class",
            "net/minecraft/world/level/block/Block.class",
            "net/minecraft/world/level/block/FireBlock.class",
            "net/minecraft/world/level/block/ChestBlock.class",
            "net/minecraft/world/level/block/Blocks.class",
        ],
    );
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let classes = BTreeMap::from([(
        "minecraft:fire".to_owned(),
        vec![
            "FireBlock".to_owned(),
            "BaseFireBlock".to_owned(),
            "Block".to_owned(),
        ],
    )]);
    let mut host = jar.seed_host(StdHost::new().with_game_block_classes(Arc::new(classes)));
    jar.execute("demo.Main", Some("main"), "main", &mut host)
        .unwrap();
    let checks: BTreeMap<String, bool> = host
        .invocations()
        .iter()
        .filter(|call| call.method.owner == "demo/Probe" && call.method.name == "check")
        .filter_map(|call| match call.args.as_slice() {
            [Value::Ref(Some(label)), Value::Int(flag)] => Some((label.clone(), *flag != 0)),
            _ => None,
        })
        .collect();
    assert_eq!(
        checks,
        BTreeMap::from([("fire".to_owned(), true), ("not-chest".to_owned(), true)])
    );
}
