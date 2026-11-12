//! `StringBuilder`, `StringBuffer`, `Character`, `Character.UnicodeBlock`,
//! `Double.toString`/`Float.toString` and modified-UTF-8 string constants
//! from a javac-built class, so the descriptors, the `invokedynamic` concat,
//! the exception tables and the constant pool are javac's own.
//!
//! Every expected string is what the same class printed on Java 25
//! (Eclipse Adoptium 25.0.5+8-LTS; `javac --release 17`, then `java`), the
//! runtime Minecraft 26.3 ships.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, path::PathBuf, process::Command};
use zip::write::SimpleFileOptions;

const SOURCE: &str = r#"
package demo;

public final class TextEntrypoint {
  record Point(int x, int y) {}

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

  /** Capitalises each word in place, the way collective's string helpers do. */
  public static String capitalized() {
    StringBuilder sb = new StringBuilder("hello wörld ßtraße éa");
    for (int i = 0; i < sb.length(); i++) {
      if (i == 0 || sb.charAt(i - 1) == ' ') {
        sb.setCharAt(i, Character.toUpperCase(sb.charAt(i)));
      }
    }
    return sb.toString();
  }

  public static String edits() {
    StringBuilder sb = new StringBuilder("ab😀cd");
    sb.insert(2, '-').append(true).append(7L).deleteCharAt(0);
    sb.replace(0, 2, "XY").reverse();
    sb.setLength(sb.length() - 1);
    sb.append(new char[] {'!', '?'}, 1, 1).insert(0, 3.5f).appendCodePoint(0x1F601);
    int at = sb.indexOf("😀");
    int last = sb.lastIndexOf("e");
    CharSequence view = sb;
    return sb + "|" + at + "|" + last + "|" + sb.codePointAt(at) + "|" + view.charAt(1)
        + "|" + view.subSequence(0, 3) + "|" + sb.substring(4) + "|" + sb.codePointCount(0, sb.length());
  }

  public static String caught() {
    StringBuilder sb = new StringBuilder("abc");
    String out = "";
    try {
      sb.charAt(9);
    } catch (StringIndexOutOfBoundsException e) {
      out += e.getMessage();
    }
    try {
      sb.delete(5, 6);
    } catch (StringIndexOutOfBoundsException e) {
      out += "|" + e.getMessage();
    }
    try {
      sb.append("xyz", 2, 9);
    } catch (IndexOutOfBoundsException e) {
      out += "|" + e.getClass().getSimpleName() + ": " + e.getMessage();
    }
    return out;
  }

  public static String jarObjects() {
    StringBuilder sb = new StringBuilder();
    sb.append(new Point(1, 2)).append(';');
    Object boxed = 42;
    sb.append(boxed).append(new StringBuilder("inner"));
    StringBuffer buffer = new StringBuffer("buf");
    buffer.append(sb).insert(0, 'b').setCharAt(1, 'U');
    return buffer.toString();
  }

  /** A setCharAt that would split a surrogate pair: counted, not approximated. */
  public static String loneSurrogate() {
    StringBuilder sb = new StringBuilder("😀");
    sb.setCharAt(0, 'x');
    return sb.toString();
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
fn floats_and_doubles_print_as_java_25_prints_them() {
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
fn characters_and_unicode_blocks_answer_from_java_25s_tables() {
    let jar = jar("characters");
    let mut host = StdHost::new();
    // `isLetter(0x10EC5)` is `false`: U+10EC5 is assigned in Unicode 17.0
    // (JDK 26+, which answers `true`), not in Java 25's Unicode 16.0.
    assert_eq!(
        call(&jar, &mut host, "characters"),
        text("true 12 15 453 false b false 26 true 105")
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

#[test]
fn builders_answer_as_java_25() {
    let jar = jar("builders");
    let mut host = StdHost::new();
    assert_eq!(
        call(&jar, &mut host, "capitalized"),
        text("Hello W\u{f6}rld \u{df}tra\u{df}e \u{c9}a")
    );
    assert_eq!(
        call(&jar, &mut host, "edits"),
        text("3.57eurtdc\u{1f600}Y?\u{1f601}|10|4|128512|.|3.5|eurtdc\u{1f600}Y?\u{1f601}|14")
    );
    // A jar record's `toString` (#70), a box, a builder argument, then a
    // `StringBuffer` built from all of it.
    assert_eq!(
        call(&jar, &mut host, "jarObjects"),
        text("bUufPoint[x=1, y=2];42inner")
    );
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());
}

#[test]
fn index_checks_throw_the_jdk_exceptions_and_messages() {
    let jar = jar("caught");
    let mut host = StdHost::new();
    assert_eq!(
        call(&jar, &mut host, "caught"),
        text(
            "Index 9 out of bounds for length 3|Range [5, 3) out of bounds for length 3|\
             IndexOutOfBoundsException: Range [2, 9) out of bounds for length 3"
        )
    );
    assert_eq!(host.unknown_host_calls(), Vec::<(String, usize)>::new());
}

#[test]
fn a_lone_surrogate_is_counted_and_the_builder_is_not_read_stale() {
    let jar = jar("lone");
    let mut host = StdHost::new();
    // The JDK returns "x\uDE00", which a Rust string cannot hold: the
    // `setCharAt` is counted, and so is every later read of that builder,
    // instead of answering the "\u{1f600}" it held before.
    let answer = call(&jar, &mut host, "loneSurrogate");
    assert_eq!(
        host.unknown_host_calls(),
        vec![
            ("java/lang/StringBuilder#setCharAt(IC)V".to_owned(), 1),
            (
                "java/lang/StringBuilder#toString()Ljava/lang/String;".to_owned(),
                1
            ),
        ]
    );
    assert_eq!(answer, Value::Ref(None));
}
