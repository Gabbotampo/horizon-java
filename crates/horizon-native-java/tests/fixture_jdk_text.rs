//! `Double.toString`/`Float.toString`, modified-UTF-8 string constants,
//! `Character` and `Character.UnicodeBlock` from a javac-built class, so the
//! descriptors, the `invokedynamic` concat and the constant pool are javac's
//! own.
//!
//! Every expected string is what the same class printed on OpenJDK 27
//! (`javac --release 17`, then `java`).

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, path::PathBuf, process::Command};
use zip::write::SimpleFileOptions;

const SOURCE: &str = r#"
package demo;

public final class TextEntrypoint {
  /** String constants javac writes as modified UTF-8: a surrogate pair, a NUL. */
  public static String constants() {
    String literal = "é😀\u0000c";
    return literal.length() + "|" + (int) literal.charAt(2) + "|" + (int) literal.charAt(3) + "|" + literal;
  }

  public static String numbers() {
    StringBuilder sb = new StringBuilder();
    sb.append(-219738.625f).append(' ').append(Double.MIN_VALUE).append(' ').append(1e23)
        .append(' ').append(2e-3f).append(' ').append(1.0E7).append(' ').append(0.1 + 0.2);
    return sb.toString();
  }

  public static String characters() {
    return Character.isLetter('ß') + " " + Character.getNumericValue('Ⅻ') + " "
        + Character.digit('f', 16) + " " + (int) Character.toTitleCase('ǆ') + " "
        + Character.isWhitespace(' ') + " " + Character.forDigit(11, 16) + " "
        + Character.isLetter(0x10EC5) + " " + Character.getType('$') + " "
        + Character.isUpperCase('Ⅰ') + " " + Character.toLowerCase(0x130);
  }

  public static String blocks() {
    int latin = 0;
    String text = "abc é日";
    for (int i = 0; i < text.length(); i++) {
      if (Character.UnicodeBlock.of(text.charAt(i)) == Character.UnicodeBlock.BASIC_LATIN) {
        latin++;
      }
    }
    return latin + " " + Character.UnicodeBlock.of('é') + " " + Character.UnicodeBlock.of(0x10FFFF);
  }
}
"#;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

/// Compile the fixture into a jar of its own; each test passes its own tag so
/// parallel tests never share a build root.
fn jar(tag: &str) -> NativeJar {
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-text-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let source = root.join("src/demo/TextEntrypoint.java");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::create_dir_all(root.join("classes")).unwrap();
    fs::write(&source, SOURCE).unwrap();
    let status = Command::new(javac())
        .args(["--release", "17", "-d"])
        .arg(root.join("classes"))
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");
    let jar_path = root.join("text.jar");
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

fn call(jar: &NativeJar, host: &mut StdHost, name: &str) -> Value {
    jar.invoke_method_with_args(
        "demo.TextEntrypoint",
        name,
        "()Ljava/lang/String;",
        None,
        &[],
        host,
    )
    .unwrap_or_else(|error| panic!("{name} failed: {error}"))
}

fn text(value: &str) -> Value {
    Value::Ref(Some(value.to_owned()))
}

#[test]
fn string_constants_decode_as_javac_encoded_them() {
    let jar = jar("constants");
    let mut host = StdHost::new();
    // `é😀\0c`: five UTF-16 units, the pair D83D DE00 and a NUL, which the
    // constant pool holds as modified UTF-8 (each surrogate three bytes, NUL
    // as C0 80).
    assert_eq!(
        call(&jar, &mut host, "constants"),
        text("5|56832|0|\u{e9}\u{1f600}\u{0}c")
    );
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());
}

#[test]
fn floats_and_doubles_print_as_jdk_27_prints_them() {
    let jar = jar("numbers");
    let mut host = StdHost::new();
    // `-219738.625f` is a tie between two shortest candidates (the JDK keeps
    // the even digit) and `Double.MIN_VALUE` has a one-digit shortest form
    // (the JDK prints two digits, the closer one).
    assert_eq!(
        call(&jar, &mut host, "numbers"),
        text("-219738.62 4.9E-324 1.0E23 0.002 1.0E7 0.30000000000000004")
    );
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());
}

#[test]
fn characters_and_unicode_blocks_answer_from_jdk_27s_tables() {
    let jar = jar("characters");
    let mut host = StdHost::new();
    assert_eq!(
        call(&jar, &mut host, "characters"),
        text("true 12 15 453 false b true 26 true 105")
    );
    let blocks = call(&jar, &mut host, "blocks");
    assert_eq!(
        blocks,
        text("4 LATIN_1_SUPPLEMENT SUPPLEMENTARY_PRIVATE_USE_AREA_B"),
        "unknown calls: {:?}",
        host.unknown_host_calls()
    );
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());
}
