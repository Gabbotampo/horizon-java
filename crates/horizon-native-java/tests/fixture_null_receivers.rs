//! Source-built fixtures for JVMS §6.5's null checks: an instance call,
//! `getfield`, `putfield`, `monitorenter` or `athrow` on `null` throws a
//! catchable `NullPointerException`, and an empty `Optional` is an object, not
//! a null, so calls on it never throw.
//!
//! Every fixture's Java checks its own results and throws
//! `IllegalStateException` on a mismatch, and every fixture is first run on the
//! real JVM (`java -cp`): the expected values are whatever OpenJDK does, and the
//! bounded runtime has to agree with it.

use horizon_native_java::{ExecutionReport, NativeError, NativeJar, StdHost};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use zip::write::SimpleFileOptions;

fn jdk_tool(name: &str) -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin").join(name))
        .unwrap_or_else(|| name.into())
}

/// Compile one entrypoint source (nested classes included), run its `main` on
/// the JVM as the oracle, and package every class as a Fabric jar.
fn build_fixture(tag: &str, entrypoint: &str, source: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-null-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let class = entrypoint.replace('.', "/");
    let source_path = root.join("src").join(format!("{class}.java"));
    let classes = root.join("classes");
    fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    fs::create_dir_all(&classes).unwrap();
    fs::write(&source_path, source).unwrap();

    let status = Command::new(jdk_tool("javac"))
        .args(["--release", "17", "-d"])
        .arg(&classes)
        .arg(&source_path)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed for {entrypoint}");

    // The oracle: the same class on a real JVM must pass its own checks.
    let oracle = Command::new(jdk_tool("java"))
        .arg("-cp")
        .arg(&classes)
        .arg(entrypoint)
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "the JVM disagrees with the fixture's expectations: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );

    let id = format!("fixture-null-{tag}");
    let jar_path = root.join(format!("{id}.jar"));
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    let options = SimpleFileOptions::default();
    zip.start_file("fabric.mod.json", options).unwrap();
    zip.write_all(
        format!(r#"{{"id":"{id}","version":"1.0","entrypoints":{{"main":"{entrypoint}"}}}}"#)
            .as_bytes(),
    )
    .unwrap();
    let package = classes.join(Path::new(&class).parent().unwrap());
    for entry in fs::read_dir(&package).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap();
        let parent = Path::new(&class).parent().unwrap().to_str().unwrap();
        zip.start_file(format!("{parent}/{name}"), options).unwrap();
        zip.write_all(&fs::read(&path).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    jar_path
}

fn execute(jar_path: &Path) -> (Result<ExecutionReport, NativeError>, StdHost) {
    let jar = NativeJar::from_path(jar_path).unwrap();
    let entry = jar.discover_entrypoint("main").unwrap();
    let mut host = StdHost::new();
    let result = jar.execute(&entry.class, entry.method.as_deref(), "main", &mut host);
    (result, host)
}

/// A call on a `null` receiver throws before any body runs: a jar class's
/// body, a jar interface, `Object`'s members and a JDK class alike, and the
/// jar's own `catch` sees it. Before the fix the concrete body ran with
/// `this == null` and answered 7, and the interface call answered 0.
#[test]
fn an_instance_call_on_null_throws_a_catchable_npe() {
    let jar = build_fixture(
        "invoke",
        "demo.NullInvoke",
        r#"
package demo;
public final class NullInvoke {
  interface Api { int answer(); }
  static final class Impl implements Api {
    static int ran;
    public int answer() { ran++; return 7; }
  }
  static void expectNpe(Runnable action, String what) {
    try {
      action.run();
    } catch (NullPointerException expected) {
      return;
    }
    throw new IllegalStateException(what + " did not throw");
  }
  static Impl impl() { return null; }
  static Api api() { return null; }
  static String text() { return null; }

  public void onInitialize() {
    expectNpe(() -> impl().answer(), "concrete call");
    expectNpe(() -> api().answer(), "interface call");
    expectNpe(() -> api().hashCode(), "hashCode");
    expectNpe(() -> api().toString(), "toString");
    expectNpe(() -> text().length(), "String.length");
    if (Impl.ran != 0) throw new IllegalStateException("the body ran on null");
    try {
      impl().answer();
      throw new IllegalStateException("no throw");
    } catch (NullPointerException e) {
      if (!e.getMessage().startsWith("Cannot invoke \"demo.NullInvoke$Impl.answer()\"")) {
        throw new IllegalStateException("message " + e.getMessage());
      }
    }
  }

  public static void main(String[] args) { new NullInvoke().onInitialize(); }
}
"#,
    );
    let (result, host) = execute(&jar);
    result.expect("every NPE is caught by the jar's own handler");
    assert_eq!(host.unknown_host_call_count(), 0);
}

/// `getfield`, `putfield`, `synchronized` and `throw` on `null` all throw.
/// Before the fix the write was dropped and `synchronized (null)` ran its body.
#[test]
fn field_access_monitor_and_throw_on_null_throw_a_catchable_npe() {
    let jar = build_fixture(
        "fields",
        "demo.NullFields",
        r#"
package demo;
public final class NullFields {
  static final class Box { int v; }
  static Box box() { return null; }
  static Object lock() { return null; }
  static RuntimeException failure() { return null; }

  public void onInitialize() {
    int caught = 0;
    try { int v = box().v; } catch (NullPointerException e) { caught++; }
    try { box().v = 2; } catch (NullPointerException e) { caught++; }
    boolean entered = false;
    try { synchronized (lock()) { entered = true; } } catch (NullPointerException e) { caught++; }
    try { throw failure(); } catch (NullPointerException e) { caught++; }
    if (caught != 4) throw new IllegalStateException("caught " + caught);
    if (entered) throw new IllegalStateException("synchronized (null) ran its body");
  }

  public static void main(String[] args) { new NullFields().onInitialize(); }
}
"#,
    );
    let (result, _) = execute(&jar);
    result.expect("every NPE is caught by the jar's own handler");
}

/// An NPE nothing catches fails the entrypoint with the real throwable, so the
/// run is not reported as clean.
#[test]
fn an_uncaught_npe_fails_the_entrypoint() {
    let jar = build_fixture(
        "uncaught",
        "demo.NullUncaught",
        r#"
package demo;
public final class NullUncaught {
  static final class Impl { int answer() { return 7; } }
  static Impl impl() { return null; }
  public void onInitialize() {
    impl().answer();
  }
  public static void main(String[] args) {
    try {
      new NullUncaught().onInitialize();
    } catch (NullPointerException expected) {
      return;
    }
    throw new IllegalStateException("the JVM did not throw");
  }
}
"#,
    );
    match execute(&jar).0 {
        Err(NativeError::UncaughtException { class, .. }) => {
            assert_eq!(class, "java/lang/NullPointerException");
        }
        other => panic!("expected an uncaught NPE, got {other:?}"),
    }
}

/// The empty `Optional`, `OptionalInt`, `OptionalLong` and `OptionalDouble`
/// are objects: their members answer without throwing, `== null` is false, the
/// JDK's singletons compare identical, and an empty optional nested in a
/// present one stays empty.
#[test]
fn empty_optionals_are_objects_not_nulls() {
    let jar = build_fixture(
        "optional",
        "demo.EmptyOptionals",
        r#"
package demo;
import java.util.Optional;
import java.util.OptionalDouble;
import java.util.OptionalInt;
import java.util.OptionalLong;
import java.util.stream.Stream;
public final class EmptyOptionals {
  static void check(boolean ok, String what) {
    if (!ok) throw new IllegalStateException(what);
  }
  public void onInitialize() {
    Optional<String> empty = Optional.empty();
    check(empty != null, "empty is null");
    check(!empty.isPresent(), "isPresent");
    check(empty.isEmpty(), "isEmpty");
    check(empty.orElse("d").equals("d"), "orElse");
    check(empty.orElseGet(() -> "g").equals("g"), "orElseGet");
    check(!empty.map(String::length).isPresent(), "map");
    check(!empty.filter(s -> true).isPresent(), "filter");
    int[] ran = {0};
    empty.ifPresent(s -> ran[0]++);
    check(ran[0] == 0, "ifPresent ran");
    check(empty.toString().equals("Optional.empty"), "toString");
    check(("" + empty).equals("Optional.empty"), "concat");
    check(("" + OptionalInt.empty()).equals("OptionalInt.empty"), "OptionalInt concat");
    check(String.valueOf(OptionalLong.empty()).equals("OptionalLong.empty"), "OptionalLong valueOf");
    check(empty.equals(Optional.ofNullable(null)), "equals");
    check(empty == (Object) Optional.empty(), "singleton");
    check(Optional.of("x").filter(s -> false).isEmpty(), "filtered out");
    check(!Optional.of(Optional.empty()).get().isPresent(), "nested empty");
    check(Optional.ofNullable(null).isEmpty(), "ofNullable(null)");
    check(!Stream.of().findFirst().isPresent(), "findFirst on an empty stream");
    try {
      empty.get();
      throw new IllegalStateException("get on empty");
    } catch (java.util.NoSuchElementException expected) {
    }

    OptionalInt noInt = OptionalInt.empty();
    check(noInt != null && !noInt.isPresent() && noInt.orElse(5) == 5, "OptionalInt");
    OptionalLong noLong = OptionalLong.empty();
    check(noLong != null && !noLong.isPresent() && noLong.orElse(6L) == 6L, "OptionalLong");
    OptionalDouble noDouble = OptionalDouble.empty();
    check(noDouble != null && !noDouble.isPresent() && noDouble.orElse(1.5) == 1.5,
        "OptionalDouble");
  }
  public static void main(String[] args) { new EmptyOptionals().onInitialize(); }
}
"#,
    );
    let (result, host) = execute(&jar);
    result.expect("an empty optional's members never throw NullPointerException");
    assert_eq!(host.unknown_host_call_count(), 0);
}
