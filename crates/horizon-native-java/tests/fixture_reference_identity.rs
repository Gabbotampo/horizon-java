//! `==` on boxes, strings and the JDK's shared empty collections answers
//! Java's object identity, or is counted.
//!
//! The interpreter holds a box as its value and a string as its text, so `==`
//! used to answer "same object" for any two equal values:
//! `Integer a = 1000, b = 1000; a == b` was `true`. Every expected string here
//! is what the same class printed on Java 25 (OpenJDK 25.0.5, `javac
//! --release 17`; two runs and an `-Xint` run, the same output). Comparisons
//! whose answer depends on an identity the interpreter does not keep (a box
//! read back from a list, a fresh string that `intern()` may have made the
//! pool's, an array element, one producer inside a loop, two parameters) must
//! be counted instead, whatever they answer.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};
use zip::write::SimpleFileOptions;

/// `MANY` becomes 4000 `if (a == b) n++;` lines (a 64 KB class, as in the
/// review that found one analysis per comparison).
const SOURCE: &str = r#"

package demo;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Set;

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
    return "" + (a == b) + (a == "a" + "b") + (d == c) + (c.intern() == a) + (d.intern() == b);
  }

  /** Empty builder text is the "" literal (JDK 25's toString: length() == 0 ? "" : new String). */
  public static String emptyBuilders() {
    StringBuilder sb = new StringBuilder();
    String a = sb.toString(), b = sb.toString();
    StringBuffer sf = new StringBuffer();
    String c = sf.toString(), d = sf.toString();
    return "" + (a == "") + (a == b) + (c == "") + (c == d) + (new StringBuilder("").toString() == "");
  }

  /** A concatenation is a new string even when empty; against a literal, unknown. */
  public static String emptyConcat() {
    String e = "";
    return "" + (e + e == "") + ("" + e == e);
  }

  /** Different texts that once parsed as one box marker. */
  public static String spelledLikeBoxes() {
    String a = "integer:5", b = "integer:05", c = "integer:+5", d = "double:1.0", e = "double:1", f = "long:1", g = "long:01";
    String p = "integer:";
    return "" + (a == b) + (a == c) + (d == e) + (f == g) + ("integer:5" == (p + "05").intern());
  }

  /** The JDK's shared empty collections. */
  public static String sharedEmpties() {
    return "" + (List.of() == List.of()) + (Collections.emptyList() == Collections.emptyList())
        + (Map.of() == Map.of()) + (Set.of() == Set.of()) + (List.of() == Collections.emptyList())
        + (new ArrayList<>() == new ArrayList<>());
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

  /** intern() puts a fresh string itself in the pool (s.intern() == s). */
  public static String internedFresh() {
    String s = new StringBuilder("qz7").append("w1x").toString();
    String t = s.intern();
    return "" + (s == t) + (t == s);
  }

  public static String internedConcat() {
    return internConcat("k3");
  }

  static String internConcat(String a) {
    String s = a + "p9v";
    return "" + (s.intern() == s);
  }

  public static String internedThenLiteral() {
    String s = new StringBuilder("hh").append("q8r").toString();
    s.intern();
    return "" + (s == "hhq8r");
  }

  public static String freshVsLiteral() {
    String c = new StringBuilder("ab").toString();
    String d = "ab".substring(0, 1) + "b";
    return "" + (c == "ab") + (d == "ab");
  }

  public static String arrayStrings() {
    String[] ss = {"aa", new StringBuilder("a").toString() + "a"};
    return "" + (ss[0] == ss[1]);
  }

  public static String objectStrings() {
    List<String> l = new ArrayList<>();
    l.add("ll");
    l.add(new StringBuilder("l").append("l").toString());
    Object c = l.get(0), d = l.get(1);
    return "" + (c == d);
  }

  public static String markerShapedLiteral() {
    return "" + ("a/B#1" == new StringBuilder("a/B#").append(1).toString());
  }

  /** 4000 comparisons of one reference in a loop: the analysis runs once. */
  public static String manyCompares() {
    Integer a = 1000;
    Integer b = a;
    int n = 0;
    for (int i = 0; i < 2; i++) {
MANY    }
    return "" + n;
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
    let many = "      if (a == b) n++;\n".repeat(4000);
    fs::write(&source, SOURCE.replace("MANY", &many)).unwrap();
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
        ("strings", "truetruefalsetruetrue"),
        ("emptyBuilders", "truetruetruetruetrue"),
        ("spelledLikeBoxes", "falsefalsefalsefalsefalse"),
        ("sharedEmpties", "truetruetruetruefalsefalse"),
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
    // (method, Java 25's answer, the family it is counted under)
    for (name, java, family) in [
        ("fromList", "false", "java/lang/Object"),
        ("inLoop", "falsefalse", "java/lang/Object"),
        ("params", "false", "java/lang/Object"),
        ("stringFromList", "true", "java/lang/String"),
        ("internedFresh", "truetrue", "java/lang/String"),
        ("internedConcat", "true", "java/lang/String"),
        ("internedThenLiteral", "true", "java/lang/String"),
        ("freshVsLiteral", "falsefalse", "java/lang/String"),
        ("arrayStrings", "false", "java/lang/String"),
        ("objectStrings", "false", "java/lang/String"),
        ("markerShapedLiteral", "false", "java/lang/String"),
        ("emptyConcat", "falsefalse", "java/lang/String"),
    ] {
        let (value, unknown) = run(&jar, name);
        let counted = format!("{family}#if_acmp(Ljava/lang/Object;Ljava/lang/Object;)Z");
        assert!(
            unknown.iter().any(|(call, _)| *call == counted),
            "{name} answered {value:?} (Java 25: {java}) without counting {counted}: {unknown:?}"
        );
    }
}

/// The producer analysis runs once per method, not once per comparison, and
/// is kept across calls: 8000 comparisons in a 64 KB method answer exactly
/// and quickly (one review measured 23 s per call before this).
#[test]
fn a_method_full_of_comparisons_is_analysed_once() {
    let jar = jar("many");
    let started = Instant::now();
    for _ in 0..2 {
        let (value, unknown) = run(&jar, "manyCompares");
        assert_eq!(value, text("8000"));
        assert_eq!(unknown, Vec::<(String, usize)>::new());
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "two calls took {elapsed:?}"
    );
}
