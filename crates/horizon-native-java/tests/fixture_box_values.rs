//! Boxes are their own value, so a `String` spelled like one stays a `String`.
//!
//! The runtime used to hold a boxed `Integer`/`Short`/`Byte`/`Character`/
//! `Boolean` as text (`integer:5`, `boolean:true`), so a real `String` with
//! that text was read as the box: `"" + "integer:5"` printed `5`,
//! `"boolean:true"` was an `instanceof Boolean`, and `"true"` formatted under
//! `%b` was refused as ambiguous. Every expected string is what this class
//! printed on Java 25 (OpenJDK 25.0.5, `javac --release 17`, run twice).

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, path::PathBuf, process::Command};
use zip::write::SimpleFileOptions;

const SOURCE: &str = r#"
package demo;

public final class Spelled {
  /** Strings spelled like the runtime's old box markers are Strings. */
  public static String text() {
    String a = "integer:5", b = "integer:05", t = "boolean:true", c = "character:97", d = "double:1.0";
    Object o = a;
    return "" + a + "|" + String.valueOf((Object) c) + "|" + new StringBuilder().append((Object) d)
        + "|" + t.length() + "|" + a.equals(b) + "|" + (a == b);
  }

  public static String types() {
    Object a = "integer:5", t = "true", f = "boolean:false";
    return (a instanceof Integer) + "|" + (a instanceof String) + "|" + (t instanceof Boolean)
        + "|" + (f instanceof Boolean) + "|" + a.getClass().getName() + "|" + t.getClass().getName();
  }

  public static String format() {
    return String.format("%s|%b|%s|%b", "false", "false", "integer:7", "boolean:false");
  }

  /** Real boxes still print, class and compare as boxes. */
  public static String boxes() {
    Object i = Integer.valueOf(5), s = Short.valueOf((short) -2), c = Character.valueOf('a'), b = Boolean.TRUE;
    Integer x = 7;
    return "" + i + s + c + b + "|" + i.getClass().getName() + "|" + c.getClass().getName() + "|"
        + (b instanceof Boolean) + (i instanceof Number) + (c instanceof Number) + "|" + (x + 1) + "|"
        + i.equals(5) + "|" + String.format("%s|%b|%c", i, b, c);
  }
}
"#;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn jar() -> NativeJar {
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-spelled-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let source = root.join("src/demo/Spelled.java");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::create_dir_all(root.join("classes")).unwrap();
    fs::write(&source, SOURCE).unwrap();
    let status = Command::new(javac())
        .args(["--release", "17", "-nowarn", "-d"])
        .arg(root.join("classes"))
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");
    let jar_path = root.join("spelled.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    for class in fs::read_dir(root.join("classes/demo")).unwrap() {
        let class = class.unwrap().path();
        let name = format!("demo/{}", class.file_name().unwrap().to_string_lossy());
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(&class).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let _ = fs::remove_dir_all(&root);
    jar
}

#[test]
fn strings_spelled_like_boxes_are_strings_and_boxes_are_boxes() {
    let jar = jar();
    for (name, expected) in [
        ("text", "integer:5|character:97|double:1.0|12|false|false"),
        (
            "types",
            "false|true|false|false|java.lang.String|java.lang.String",
        ),
        ("format", "false|true|integer:7|true"),
        (
            "boxes",
            "5-2atrue|java.lang.Integer|java.lang.Character|truetruefalse|8|true|5|true|a",
        ),
    ] {
        let mut host = StdHost::new();
        let value = jar
            .invoke_method_with_args(
                "demo.Spelled",
                name,
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            )
            .unwrap_or_else(|error| panic!("{name} failed: {error}"));
        assert_eq!(value, Value::Ref(Some(expected.into())), "{name}");
        assert_eq!(
            host.unknown_host_calls(),
            Vec::<(String, usize)>::new(),
            "{name}"
        );
    }
}
