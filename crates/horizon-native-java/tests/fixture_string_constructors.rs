//! `new String(...)` is counted, never answered with the allocation marker,
//! until a model builds its text (the `char[]` and code-point constructors
//! now do: `string_constructors.rs`).
//!
//! On `main` before this test, every `String` constructor left the
//! `java/lang/String#n` marker where Java has the text, silently:
//! `new String(new char[] {'a', 'b'})` had length 18 and `equals("ab")` was
//! `false`. Until a model builds the text, each overload must be an unknown
//! host call. The comment on each method is what Java 25 (OpenJDK 25.0.5,
//! `javac --release 17`) returns for it, the answer a future model must give.

use horizon_native_java::{NativeJar, StdHost};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zip::write::SimpleFileOptions;

const SOURCE: &str = r#"
package demo;

import java.nio.charset.StandardCharsets;
import java.util.function.Function;
import java.util.function.Supplier;

@SuppressWarnings("deprecation")
public final class Ctors {
  // Java 25: "ab|2|true"
  public static String chars() { String s = new String(new char[] {'a', 'b'}); return s + "|" + s.length() + "|" + s.equals("ab"); }
  // "bc"
  public static String charsRange() { return new String(new char[] {'a', 'b', 'c', 'd'}, 1, 2); }
  // "a" + U+1F600 + "b"
  public static String codePoints() { return new String(new int[] {0x61, 0x1F600, 0x62}, 0, 3); }
  // "hi"
  public static String bytes() { return new String(new byte[] {104, 105}); }
  // "b"
  public static String bytesRange() { return new String(new byte[] {97, 98, 99}, 1, 1); }
  // U+00E9
  public static String bytesCharset() { return new String(new byte[] {(byte) 0xC3, (byte) 0xA9}, StandardCharsets.UTF_8); }
  // "bc"
  public static String bytesRangeCharset() { return new String(new byte[] {97, 98, 99}, 1, 2, StandardCharsets.US_ASCII); }
  // "ab"
  public static String bytesName() throws Exception { return new String(new byte[] {97, 98}, "UTF-8"); }
  // "ab"
  public static String bytesRangeName() throws Exception { return new String(new byte[] {97, 98, 99}, 0, 2, "ISO-8859-1"); }
  // "ab"
  public static String hibyte() { return new String(new byte[] {97, 98}, 0); }
  // "b"
  public static String hibyteRange() { return new String(new byte[] {97, 98}, 0, 1, 1); }
  // "0"
  public static String empty() { return "" + new String().length(); }
  // "xy"
  public static String copy() { return new String("xy"); }
  // "sb"
  public static String builder() { return new String(new StringBuilder("sb")); }
  // "bf"
  public static String buffer() { return new String(new StringBuffer("bf")); }
  // "q"
  public static String reference() { Function<char[], String> f = String::new; return f.apply(new char[] {'q'}); }
  // "0"
  public static String supplier() { Supplier<String> f = String::new; return "" + f.get().length(); }
  // "mh"
  public static String handle() throws Throwable {
    var h = java.lang.invoke.MethodHandles.lookup().findConstructor(
        String.class, java.lang.invoke.MethodType.methodType(void.class, String.class));
    return (String) h.invoke("mh");
  }
}
"#;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

/// The build directory of one [`jar`] call, removed when it drops (also when
/// the build panics).
struct BuildDir(PathBuf);

impl Drop for BuildDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Compile and pack the fixture. Both tests call this at once on parallel
/// threads of one process, so each call builds in its own directory: with a
/// name made of the process id alone, one call's cleanup deleted the other's
/// half-built jar (a flaky `NotFound` on CI).
fn jar() -> NativeJar {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = BuildDir(std::env::temp_dir().join(format!(
        "horizon-native-java-string-ctors-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let root = &dir.0;
    let _ = fs::remove_dir_all(root);
    let source = root.join("src/demo/Ctors.java");
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
    let jar_path = root.join("ctors.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    for class in fs::read_dir(root.join("classes/demo")).unwrap() {
        let class = class.unwrap().path();
        let name = format!("demo/{}", class.file_name().unwrap().to_string_lossy());
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(&class).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    NativeJar::from_path(&jar_path).unwrap()
}

/// The `char[]` and code-point constructors are modelled
/// (`string_constructors.rs`): they answer the Java 25 values the comments
/// above record, with nothing counted.
#[test]
fn char_and_code_point_constructors_answer_java_25() {
    let jar = jar();
    let cases = [
        ("chars", "ab|2|true"),
        ("charsRange", "bc"),
        ("codePoints", "a\u{1F600}b"),
        ("reference", "q"),
    ];
    for (name, expected) in cases {
        let mut host = StdHost::new();
        let answer = jar
            .invoke_method_with_args(
                "demo.Ctors",
                name,
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            )
            .unwrap();
        assert_eq!(
            answer,
            horizon_native_java::Value::Ref(Some(expected.to_owned())),
            "{name}"
        );
        assert_eq!(
            host.unknown_host_call_count(),
            0,
            "{name}: {:?}",
            host.unknown_host_calls()
        );
    }
}

#[test]
fn every_string_constructor_is_counted_until_it_is_modelled() {
    let jar = jar();
    // (method, the constructor descriptor it must be counted under)
    let cases = [
        ("bytes", "([B)V"),
        ("bytesRange", "([BII)V"),
        ("bytesCharset", "([BLjava/nio/charset/Charset;)V"),
        ("bytesRangeCharset", "([BIILjava/nio/charset/Charset;)V"),
        ("bytesName", "([BLjava/lang/String;)V"),
        ("bytesRangeName", "([BIILjava/lang/String;)V"),
        ("hibyte", "([BI)V"),
        ("hibyteRange", "([BIII)V"),
        ("empty", "()V"),
        ("copy", "(Ljava/lang/String;)V"),
        ("builder", "(Ljava/lang/StringBuilder;)V"),
        ("buffer", "(Ljava/lang/StringBuffer;)V"),
        ("supplier", "()V"),
        ("handle", "(Ljava/lang/String;)V"),
    ];
    for (name, descriptor) in cases {
        let mut host = StdHost::new();
        let answer = jar.invoke_method_with_args(
            "demo.Ctors",
            name,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        let counted = format!("java/lang/String#<init>{descriptor}");
        assert!(
            host.unknown_host_calls()
                .iter()
                .any(|(call, count)| call == &counted && *count >= 1),
            "{name} answered {answer:?} without counting {counted}: {:?}",
            host.unknown_host_calls()
        );
    }
}
