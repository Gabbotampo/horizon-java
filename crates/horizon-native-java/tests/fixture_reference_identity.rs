//! `==` on boxes and strings answers Java's object identity, or is counted.
//!
//! The interpreter holds a box as its value and a string as its text, so `==`
//! used to answer "same object" for any two equal values:
//! `Integer a = 1000, b = 1000; a == b` was `true`. Every expected string here
//! is what the same class printed on Java 25 (OpenJDK 25.0.5, `javac
//! --release 17`, run twice with the same output). Comparisons whose answer
//! depends on an identity the interpreter does not keep (a box read back from
//! a list, one producer inside a loop, two parameters) must be counted
//! instead.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, path::PathBuf, process::Command};
use zip::write::SimpleFileOptions;

const SOURCE: &str = r#"
package demo;

import java.util.ArrayList;
import java.util.List;

@SuppressWarnings({"removal", "deprecation"})
public final class Identity {
  public static String boxes() {
    Integer a = 1000, b = 1000, c = 127, d = 127, e = 128, f = 128, g = -128, h = -128, i = -129, j = -129;
    return "" + (a == b) + (a != b) + (c == d) + (e == f) + (g == h) + (i == j)
        + (Integer.valueOf(1000) == Integer.valueOf(1000)) + (Integer.valueOf("5") == c - 122);
  }

  public static String otherBoxes() {
    Long x = 200L, y = 200L, p = 100L, q = 100L;
    Short s = 5, t = 5, u = 300, v = 300;
    Character ca = 'a', cb = 'a', cc = 200, cd = 200;
    Byte b1 = -100, b2 = -100;
    Boolean t1 = true, t2 = Boolean.TRUE, t3 = Boolean.valueOf("TRUE");
    Double d1 = 1.0, d2 = 1.0, z = 0.0, nz = -0.0, nan = Double.NaN;
    Float f1 = 1f, f2 = 1f;
    return "" + (x == y) + (p == q) + (s == t) + (u == v) + (ca == cb) + (cc == cd) + (b1 == b2)
        + (t1 == t2) + (t2 == t3) + (d1 == d2) + (f1 == f2) + (z == nz) + (nan == nan);
  }

  public static String sameReference() {
    Integer a = 1000;
    Integer b = a;
    Object o = b;
    Double d = 2.5;
    Object e = d;
    Integer n = new Integer(5), m = new Integer(5), k = 5;
    return "" + (a == b) + (o == a) + (e == d) + (n == m) + (n == k) + (n == n);
  }

  public static String mixedTypes() {
    Object i = 1, s = (short) 1, l = 1L, c = (char) 1;
    return "" + (i == s) + (i == l) + (i == c) + (s == c);
  }

  /** Switches, a handler, wide locals and a loop before the producers: still exact. */
  public static String acrossShapes() {
    long wide = 3L;
    double real = 2.0;
    for (int i = 0; i < 3; i++) {
      wide += i;
    }
    Integer a = 1000;
    Integer b = a;
    switch ((int) wide) {
      case 5: real += 1; break;
      case 6: wide *= 2; break;
      default: wide = 0;
    }
    switch ((int) wide) {
      case 1: real = 0; break;
      case 100: real = 1; break;
      case 12: wide++; break;
      default: break;
    }
    try {
      if (real > 1) {
        throw new IllegalStateException("x");
      }
    } catch (IllegalStateException ex) {
      wide += (long) real;
    }
    Integer c = 1000;
    return "" + (a == b) + (a == c) + wide;
  }

  public static String strings() {
    String a = "ab", b = "ab";
    String c = new StringBuilder("ab").toString();
    String d = a.substring(0, 1) + "b";
    return "" + (a == b) + (a == "a" + "b") + (c == a) + (d == a) + (d == c) + (c.intern() == a) + (d.intern() == b);
  }

  // ---- not provable: counted ----
  public static String fromList() {
    List<Integer> l = new ArrayList<>();
    l.add(1000);
    l.add(1000);
    return "" + (l.get(0) == l.get(1));
  }

  public static String inLoop() {
    Integer prev = 0;
    String out = "";
    for (int i = 0; i < 2; i++) {
      Integer cur = 1000;
      out += (cur == prev);
      prev = cur;
    }
    return out;
  }

  public static String stringFromList() {
    List<String> l = new ArrayList<>();
    l.add("ab");
    return "" + (l.get(0) == "ab");
  }

  public static String params() {
    return compare(1000, 1000);
  }

  static String compare(Integer a, Integer b) {
    return "" + (a == b);
  }
}
"#;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

/// Compile the fixture into a jar; each test passes its own tag so parallel
/// tests never share a build root.
fn jar(tag: &str) -> NativeJar {
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-identity-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let source = root.join("src/demo/Identity.java");
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
    let jar_path = root.join("identity.jar");
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

/// Run one method on a fresh host: its answer and the unknown calls it made.
fn run(jar: &NativeJar, name: &str) -> (Value, Vec<(String, usize)>) {
    let mut host = StdHost::new();
    let value = jar
        .invoke_method_with_args(
            "demo.Identity",
            name,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap_or_else(|error| panic!("{name} failed: {error}"));
    (value, host.unknown_host_calls())
}

fn text(value: &str) -> Value {
    Value::Ref(Some(value.to_owned()))
}

#[test]
fn provable_identities_answer_as_java_25() {
    let jar = jar("exact");
    for (name, expected) in [
        ("boxes", "falsetruetruefalsetruefalsefalsetrue"),
        ("sameReference", "truetruetruefalsefalsetrue"),
        ("mixedTypes", "falsefalsefalsefalse"),
        ("acrossShapes", "truefalse15"),
        ("strings", "truetruefalsefalsefalsetruetrue"),
    ] {
        let (value, unknown) = run(&jar, name);
        assert_eq!(value, text(expected), "{name}");
        assert_eq!(unknown, Vec::<(String, usize)>::new(), "{name}");
    }
    // `Byte.valueOf` itself is not modelled (counted), but every comparison
    // in the method is answered as Java 25 does.
    let (value, unknown) = run(&jar, "otherBoxes");
    assert_eq!(
        value,
        text("falsetruetruefalsetruefalsetruetruetruefalsefalsefalsetrue")
    );
    assert!(
        unknown.iter().all(|(call, _)| !call.contains("if_acmp")),
        "{unknown:?}"
    );
}

#[test]
fn identities_the_interpreter_does_not_keep_are_counted() {
    let jar = jar("counted");
    for (name, family) in [
        ("fromList", "java/lang/Object"),
        ("inLoop", "java/lang/Object"),
        ("params", "java/lang/Object"),
        ("stringFromList", "java/lang/String"),
    ] {
        let (_, unknown) = run(&jar, name);
        let counted = format!("{family}#if_acmp(Ljava/lang/Object;Ljava/lang/Object;)Z");
        assert!(
            unknown.iter().any(|(call, _)| *call == counted),
            "{name} did not count {counted}: {unknown:?}"
        );
    }
}
