//! `ItemLike.asItem()` on a mod's own items and blocks, whose superclasses are
//! game classes the runtime doesn't hold: an `Item` answers itself, and a
//! `Block` answers the `BlockItem` built for it (vanilla's `Item.BY_BLOCK`), or
//! `Items.AIR` when there is none, never null.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{collections::BTreeMap, fs, io::Write, path::Path, process::Command};
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
        "horizon-native-java-item-like-{name}-{}",
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

fn checks(host: &StdHost) -> BTreeMap<String, bool> {
    host.invocations()
        .iter()
        .filter(|invocation| {
            invocation.method.owner == "demo/Probe" && invocation.method.name == "check"
        })
        .filter_map(|invocation| match invocation.args.as_slice() {
            [Value::Ref(Some(label)), Value::Int(flag)] => Some((label.clone(), *flag != 0)),
            _ => None,
        })
        .collect()
}

#[test]
fn item_like_as_item_answers_like_vanilla() {
    let mut sources = BTreeMap::new();
    sources.insert(
        "demo/Probe.java",
        r#"package demo;
public final class Probe {
    public static void check(String label, boolean value) {}
}
"#,
    );
    // Compile-time stubs of the game classes, excluded from the jar.
    sources.insert(
        "net/minecraft/world/level/ItemLike.java",
        r#"package net.minecraft.world.level;
public interface ItemLike { net.minecraft.world.item.Item asItem(); }
"#,
    );
    sources.insert(
        "net/minecraft/world/item/Item.java",
        r#"package net.minecraft.world.item;
public class Item implements net.minecraft.world.level.ItemLike {
    public Item(Object properties) {}
    public Item asItem() { return this; }
}
"#,
    );
    sources.insert(
        "net/minecraft/world/item/BlockItem.java",
        r#"package net.minecraft.world.item;
public class BlockItem extends Item {
    public BlockItem(net.minecraft.world.level.block.Block block, Object properties) { super(properties); }
}
"#,
    );
    sources.insert(
        "net/minecraft/world/level/block/Block.java",
        r#"package net.minecraft.world.level.block;
public class Block implements net.minecraft.world.level.ItemLike {
    public Block(Object properties) {}
    public net.minecraft.world.item.Item asItem() { return null; }
}
"#,
    );
    sources.insert(
        "demo/CabinetBlock.java",
        r#"package demo;
public class CabinetBlock extends net.minecraft.world.level.block.Block {
    public CabinetBlock() { super(null); }
}
"#,
    );
    sources.insert(
        "demo/Main.java",
        r#"package demo;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.ItemLike;
public final class Main {
    public static void main() {
        Item plain = new Item(null);
        ItemLike asLike = plain;
        Probe.check("item", asLike.asItem() == plain);
        CabinetBlock cabinet = new CabinetBlock();
        BlockItem cabinetItem = new BlockItem(cabinet, null);
        Probe.check("block", cabinet.asItem() == cabinetItem);
        CabinetBlock lonely = new CabinetBlock();
        Probe.check("no-item", lonely.asItem() != null);
    }
}
"#,
    );
    let jar_path = build_jar(
        "item-like",
        sources,
        &[],
        &[
            "demo/Probe.class",
            "net/minecraft/world/level/ItemLike.class",
            "net/minecraft/world/item/Item.class",
            "net/minecraft/world/item/BlockItem.class",
            "net/minecraft/world/level/block/Block.class",
        ],
    );
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    jar.execute("demo.Main", Some("main"), "main", &mut host)
        .unwrap();
    assert_eq!(
        checks(&host),
        BTreeMap::from([
            ("block".to_owned(), true),
            ("item".to_owned(), true),
            ("no-item".to_owned(), true),
        ])
    );
}
