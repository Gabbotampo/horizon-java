//! A source-built jar pins which classes count as NeoForge `@Mod`
//! entrypoints and which side each one loads on.
//!
//! Real jars exposed both mistakes: KotlinLangForge's type-alias facade keeps
//! the `@Mod` descriptor only inside `kotlin.Metadata`'s strings, and Balm
//! ships a `@Mod(dist = CLIENT)` class beside its unrestricted one. The
//! expectations below are FML's rules, not the scanner's implementation: an
//! annotation member is what makes a mod class, and `dist` limits the side.

use horizon_native_java::NativeJar;
use std::{fs, io::Write, process::Command};
use zip::write::SimpleFileOptions;

#[test]
fn source_built_jar_discovers_annotated_mod_classes_and_their_side() {
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-neoforge-entrypoints-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let src = root.join("src");
    let classes = root.join("classes");
    fs::create_dir_all(src.join("demo")).unwrap();
    fs::create_dir_all(src.join("net/neoforged/api/distmarker")).unwrap();
    fs::create_dir_all(src.join("net/neoforged/fml/common")).unwrap();

    fs::write(
        src.join("net/neoforged/api/distmarker/Dist.java"),
        "package net.neoforged.api.distmarker; public enum Dist { CLIENT, DEDICATED_SERVER }",
    )
    .unwrap();
    fs::write(
        src.join("net/neoforged/fml/common/Mod.java"),
        r#"package net.neoforged.fml.common;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import net.neoforged.api.distmarker.Dist;
@Retention(RetentionPolicy.RUNTIME)
public @interface Mod { String value(); Dist[] dist() default {}; }
"#,
    )
    .unwrap();
    fs::write(
        src.join("demo/NeoBoth.java"),
        "package demo; @net.neoforged.fml.common.Mod(\"demo\") public final class NeoBoth { public NeoBoth() {} }",
    )
    .unwrap();
    fs::write(
        src.join("demo/NeoClient.java"),
        r#"package demo;
@net.neoforged.fml.common.Mod(value = "demo", dist = net.neoforged.api.distmarker.Dist.CLIENT)
public final class NeoClient { public NeoClient() {} }
"#,
    )
    .unwrap();
    fs::write(
        src.join("demo/NeoDedicated.java"),
        r#"package demo;
@net.neoforged.fml.common.Mod(value = "demo", dist = net.neoforged.api.distmarker.Dist.DEDICATED_SERVER)
public final class NeoDedicated { public NeoDedicated() {} }
"#,
    )
    .unwrap();
    // Kotlin's `Metadata` keeps every mentioned type as a string, including
    // the `@Mod` descriptor. A class with no annotation is not a mod class.
    fs::write(
        src.join("demo/TypeAliasFacade.java"),
        r#"package demo;
public final class TypeAliasFacade {
    static final String MENTIONED = "Lnet/neoforged/fml/common/Mod;";
    private TypeAliasFacade() {}
}
"#,
    )
    .unwrap();

    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| std::path::PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    fs::create_dir_all(&classes).unwrap();
    let sources = [
        src.join("net/neoforged/api/distmarker/Dist.java"),
        src.join("net/neoforged/fml/common/Mod.java"),
        src.join("demo/NeoBoth.java"),
        src.join("demo/NeoClient.java"),
        src.join("demo/NeoDedicated.java"),
        src.join("demo/TypeAliasFacade.java"),
    ];
    let status = Command::new(javac)
        .arg("--release")
        .arg("17")
        .arg("-d")
        .arg(&classes)
        .args(sources)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");

    let jar_path = root.join("entrypoints.jar");
    let file = fs::File::create(&jar_path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file("META-INF/neoforge.mods.toml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"[[mods]]\nmodId = \"demo\"\nversion = \"1.0\"\n")
        .unwrap();
    for class in files_under(&classes) {
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
    let discovered = |side: &str| {
        let mut classes = jar
            .discover_entrypoints_on("neoforge", side)
            .into_iter()
            .map(|entry| entry.class)
            .collect::<Vec<_>>();
        classes.sort();
        classes
    };
    assert_eq!(
        discovered("SERVER"),
        ["demo.NeoBoth", "demo.NeoDedicated"],
        "a client-only @Mod class must not load on a dedicated server"
    );
    assert_eq!(
        discovered("CLIENT"),
        ["demo.NeoBoth", "demo.NeoClient"],
        "a dedicated-server-only @Mod class must not load on a client"
    );
    assert_eq!(
        discovered("BOTH"),
        ["demo.NeoBoth", "demo.NeoClient", "demo.NeoDedicated"],
        "the side-independent query keeps every annotated @Mod class"
    );
    assert!(
        !discovered("SERVER").contains(&"demo.TypeAliasFacade".to_owned()),
        "a descriptor in a string constant is not an annotation"
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
