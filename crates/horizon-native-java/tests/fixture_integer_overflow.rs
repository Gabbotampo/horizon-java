//! Java's integer arithmetic wraps where Rust's checked arithmetic rejects:
//! `iadd`/`isub`/`imul`, their `long` twins and `iinc` keep the low-order
//! bits of the result (JVMS §6.5), and signed division has one defined
//! overflow result, `MIN_VALUE / -1` is `MIN_VALUE` with remainder 0.

#[path = "support/scratch.rs"]
mod scratch;

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, path::PathBuf, process::Command, sync::OnceLock};
use zip::write::SimpleFileOptions;

const SOURCE: &str = r#"
package demo;
public final class IntegerOverflowEntrypoint {
  public static int intDivide(int value) { return value / -1; }
  public static int intRemainder(int value) { return value % -1; }
  public static long longDivide(long value) { return value / -1L; }
  public static long longRemainder(long value) { return value % -1L; }
  public static int intAdd(int left, int right) { return left + right; }
  public static int intSubtract(int left, int right) { return left - right; }
  public static int intMultiply(int left, int right) { return left * right; }
  public static long longAdd(long left, long right) { return left + right; }
  public static long longSubtract(long left, long right) { return left - right; }
  public static long longMultiply(long left, long right) { return left * right; }
  public static int increment(int value) { value += 100; return value; }
  public static int textHash(String text) {
    int hash = 0;
    for (int i = 0; i < text.length(); i++) {
      hash = 31 * hash + text.charAt(i);
    }
    return hash;
  }
  public void onInitialize() {
    if (intDivide(Integer.MIN_VALUE) != Integer.MIN_VALUE
        || intRemainder(Integer.MIN_VALUE) != 0
        || longDivide(Long.MIN_VALUE) != Long.MIN_VALUE
        || longRemainder(Long.MIN_VALUE) != 0L) {
      throw new IllegalStateException("signed division overflow semantics changed");
    }
  }
}
"#;

/// The fixture jar's bytes, built once for every test; its scratch
/// directory is gone again as soon as they are read.
fn fixture() -> &'static [u8] {
    static FIXTURE: OnceLock<Vec<u8>> = OnceLock::new();
    FIXTURE.get_or_init(|| fs::read(&*build_fixture()).unwrap())
}

fn build_fixture() -> scratch::Scratch {
    let root = scratch::Scratch::new("integer-overflow");
    let source_path = root.join("src/demo/IntegerOverflowEntrypoint.java");
    fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    fs::create_dir_all(root.join("classes")).unwrap();
    fs::write(&source_path, SOURCE).unwrap();
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let status = Command::new(javac)
        .args(["--release", "17", "-d"])
        .arg(root.join("classes"))
        .arg(&source_path)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");

    let jar_path = root.join("fixture-integer-overflow.jar");
    let file = fs::File::create(&jar_path).unwrap();
    let mut jar = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    jar.start_file("fabric.mod.json", options).unwrap();
    jar.write_all(
        br#"{"id":"fixture-integer-overflow","version":"1.0","entrypoints":{"main":"demo.IntegerOverflowEntrypoint"}}"#,
    )
    .unwrap();
    jar.start_file("demo/IntegerOverflowEntrypoint.class", options)
        .unwrap();
    jar.write_all(&fs::read(root.join("classes/demo/IntegerOverflowEntrypoint.class")).unwrap())
        .unwrap();
    jar.finish().unwrap();
    root.into_path(jar_path)
}

fn call(jar: &NativeJar, name: &str, descriptor: &str, args: &[Value]) -> Value {
    jar.invoke_method_with_args(
        "demo/IntegerOverflowEntrypoint",
        name,
        descriptor,
        None,
        args,
        &mut StdHost::new(),
    )
    .unwrap()
}

#[test]
fn signed_division_overflow_matches_java() {
    let jar = NativeJar::from_bytes(fixture()).unwrap();
    assert_eq!(
        call(&jar, "intDivide", "(I)I", &[Value::Int(i32::MIN)]),
        Value::Int(i32::MIN)
    );
    assert_eq!(
        call(&jar, "intRemainder", "(I)I", &[Value::Int(i32::MIN)]),
        Value::Int(0)
    );
    assert_eq!(
        call(&jar, "longDivide", "(J)J", &[Value::Long(i64::MIN)]),
        Value::Long(i64::MIN)
    );
    assert_eq!(
        call(&jar, "longRemainder", "(J)J", &[Value::Long(i64::MIN)]),
        Value::Long(0)
    );

    let entry = jar.discover_entrypoint("main").unwrap();
    jar.execute(
        &entry.class,
        entry.method.as_deref(),
        "main",
        &mut StdHost::new(),
    )
    .unwrap();
}

/// The wrapping operators, and a `String.hashCode`-style loop whose
/// accumulator overflows on any text longer than a few characters.
#[test]
fn signed_add_subtract_multiply_and_increment_wrap_like_java() {
    let jar = NativeJar::from_bytes(fixture()).unwrap();
    let ints = |name: &str, left: i32, right: i32| {
        call(&jar, name, "(II)I", &[Value::Int(left), Value::Int(right)])
    };
    assert_eq!(ints("intAdd", i32::MAX, 1), Value::Int(i32::MIN));
    assert_eq!(ints("intSubtract", i32::MIN, 1), Value::Int(i32::MAX));
    assert_eq!(ints("intMultiply", 65_536, 65_536), Value::Int(0));
    assert_eq!(ints("intMultiply", i32::MIN, -1), Value::Int(i32::MIN));
    let longs = |name: &str, left: i64, right: i64| {
        call(
            &jar,
            name,
            "(JJ)J",
            &[Value::Long(left), Value::Long(right)],
        )
    };
    assert_eq!(longs("longAdd", i64::MAX, 1), Value::Long(i64::MIN));
    assert_eq!(longs("longSubtract", i64::MIN, 1), Value::Long(i64::MAX));
    assert_eq!(
        longs("longMultiply", 6_364_136_223_846_793_005, 3),
        Value::Long(6_364_136_223_846_793_005_i64.wrapping_mul(3))
    );
    assert_eq!(
        call(&jar, "increment", "(I)I", &[Value::Int(i32::MAX - 50)]),
        Value::Int((i32::MAX - 50).wrapping_add(100))
    );
    let text = "horizon:overflowing_identifier";
    let expected = text.encode_utf16().fold(0_i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(i32::from(unit))
    });
    assert_eq!(
        call(
            &jar,
            "textHash",
            "(Ljava/lang/String;)I",
            &[Value::Ref(Some(text.into()))]
        ),
        Value::Int(expected)
    );
}
