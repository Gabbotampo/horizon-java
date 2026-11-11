//! Source-built fixtures for three answers the runtime used to give without
//! checking: `checkcast` to a jar class, `Enum.valueOf` with a name that is not
//! a constant, and `Thread.start()`.
//!
//! Where the runtime now matches the JVM, the fixture's Java checks its own
//! results and is first run on the real JVM (`java -cp`), so the expected
//! values are OpenJDK's. Where the runtime cannot know (a cast of a game object,
//! a JDK enum it has no table for, a thread body it does not run), the call is
//! counted, and the test pins that count as the runtime's contract, not as JVM
//! behaviour.

use horizon_native_java::{ExecutionReport, NativeError, NativeJar, StdHost, Value};
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
        "horizon-native-java-casts-{tag}-{}",
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

    let id = format!("fixture-casts-{tag}");
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

fn count(host: &StdHost, key: &str) -> usize {
    host.unknown_host_calls()
        .into_iter()
        .find(|(method, _)| method == key)
        .map_or(0, |(_, count)| count)
}

/// A cast to a jar class or interface is checked against the hierarchy: an
/// unrelated jar object, a boxed number, a collection, an optional and an
/// unrelated enum constant throw `ClassCastException`; up- and down-casts
/// along the hierarchy pass. Before the fix every one of these passed.
#[test]
fn a_cast_to_a_jar_class_is_checked_against_the_hierarchy() {
    let jar = build_fixture(
        "checkcast",
        "demo.Casts",
        r#"
package demo;
import java.util.ArrayList;
import java.util.Optional;
public final class Casts {
  interface Api { int answer(); }
  static class Impl implements Api { public int answer() { return 7; } }
  static final class Child extends Impl { }
  static final class Other { }
  enum Color { RED }
  static int casts;
  static void expectCce(Object value, String what) {
    try {
      Impl impl = (Impl) value;
      throw new IllegalStateException(what + " cast to Impl");
    } catch (ClassCastException expected) {
      casts++;
    }
  }
  public void onInitialize() {
    expectCce(new Other(), "Other");
    expectCce(Integer.valueOf(5), "Integer");
    expectCce(new ArrayList<String>(), "ArrayList");
    expectCce(Optional.of("x"), "Optional");
    expectCce(Color.RED, "enum constant");
    if (casts != 5) throw new IllegalStateException("casts " + casts);
    Object child = new Child();
    Impl impl = (Impl) child;
    Api api = (Api) child;
    Child back = (Child) (Object) impl;
    if (back.answer() != 7 || api.answer() != 7) throw new IllegalStateException("answer");
    try {
      Api wrong = (Api) (Object) new Other();
      throw new IllegalStateException("Other cast to Api");
    } catch (ClassCastException e) {
      if (!e.getMessage().startsWith(
          "class demo.Casts$Other cannot be cast to class demo.Casts$Api")) {
        throw new IllegalStateException("message " + e.getMessage());
      }
    }
  }
  public static void main(String[] args) { new Casts().onInitialize(); }
}
"#,
    );
    let (result, host) = execute(&jar);
    result.expect("every ClassCastException is caught by the jar's own handler");
    assert_eq!(host.unknown_host_call_count(), 0);
}

/// A game object cast to a jar interface (a Mixin may add the interface to the
/// game class) is something the runtime cannot check: the cast is counted, not
/// passed silently, and the run continues with the value.
#[test]
fn a_cast_the_runtime_cannot_check_is_counted() {
    let jar_path = build_fixture(
        "uncheckable",
        "demo.Duck",
        r#"
package demo;
public final class Duck {
  public interface Quacks { }
  public static Object cast(Object entity) { return (Quacks) entity; }
  public void onInitialize() { }
  public static void main(String[] args) { }
}
"#,
    );
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let mut host = StdHost::new();
    let entity = Value::Ref(Some("net/minecraft/world/entity/Entity#1".into()));
    let value = jar
        .invoke_method_with_args(
            "demo.Duck",
            "cast",
            "(Ljava/lang/Object;)Ljava/lang/Object;",
            None,
            std::slice::from_ref(&entity),
            &mut host,
        )
        .unwrap();
    assert_eq!(value, entity);
    assert_eq!(
        count(
            &host,
            "demo/Duck$Quacks#<checkcast>(Ljava/lang/Object;)Ldemo/Duck$Quacks;"
        ),
        1
    );
}

/// `valueOf` answers the jar's own constant object (identity holds), throws
/// `IllegalArgumentException` with the JDK's message for a name that is not a
/// constant, and `NullPointerException` for a null name. Before the fix any
/// name answered a fresh marker.
#[test]
fn enum_value_of_answers_the_constant_or_throws() {
    let jar = build_fixture(
        "enum",
        "demo.Enums",
        r#"
package demo;
public final class Enums {
  enum Color { RED, GREEN { } }
  public void onInitialize() {
    if (Color.valueOf("GREEN") != Color.GREEN) throw new IllegalStateException("identity");
    if (Enum.valueOf(Color.class, "RED") != Color.RED) throw new IllegalStateException("Enum.valueOf");
    try {
      Color.valueOf("BLUE");
      throw new IllegalStateException("BLUE");
    } catch (IllegalArgumentException e) {
      if (!e.getMessage().equals("No enum constant demo.Enums.Color.BLUE")) {
        throw new IllegalStateException("message " + e.getMessage());
      }
    }
    try {
      Color.valueOf(null);
      throw new IllegalStateException("null name");
    } catch (NullPointerException e) {
      if (!e.getMessage().equals("Name is null")) {
        throw new IllegalStateException("message " + e.getMessage());
      }
    }
  }
  public static void main(String[] args) { new Enums().onInitialize(); }
}
"#,
    );
    let (result, host) = execute(&jar);
    result.expect("valueOf matches the JDK");
    assert_eq!(host.unknown_host_call_count(), 0);
}

/// `Enum.valueOf` on a JDK enum the runtime has no constant table for cannot
/// check the name, so it is counted. (The JVM throws for `"BLUE"`; the bounded
/// runtime answers a marker and counts the call, so the run is not clean.)
#[test]
fn enum_value_of_on_an_unknown_enum_is_counted() {
    let jar = build_fixture(
        "jdk-enum",
        "demo.JdkEnum",
        r#"
package demo;
public final class JdkEnum {
  public void onInitialize() {
    Thread.State state = Enum.valueOf(Thread.State.class, "RUNNABLE");
  }
  public static void main(String[] args) { new JdkEnum().onInitialize(); }
}
"#,
    );
    let (result, host) = execute(&jar);
    result.unwrap();
    assert_eq!(
        count(
            &host,
            "java/lang/Enum#valueOf(Ljava/lang/Class;Ljava/lang/String;)Ljava/lang/Enum;"
        ),
        1
    );
}

/// A started thread's body does not run in the bounded runtime, so `start()`
/// is counted rather than reported as done. (On the JVM the runnable runs; this
/// pins the runtime's named departure.)
#[test]
fn thread_start_is_counted_not_reported_as_run() {
    let jar = build_fixture(
        "thread",
        "demo.Starts",
        r#"
package demo;
public final class Starts {
  static volatile boolean ran;
  public void onInitialize() {
    new Thread(() -> ran = true, "worker").start();
  }
  public static void main(String[] args) { new Starts().onInitialize(); }
}
"#,
    );
    let (result, host) = execute(&jar);
    result.unwrap();
    assert_eq!(count(&host, "java/lang/Thread#start()V"), 1);

    let jar = NativeJar::from_path(&jar).unwrap();
    let entry = jar.discover_entrypoint("main").unwrap();
    let mut strict = StdHost::new().with_strict_unknown_host_calls();
    assert!(matches!(
        jar.execute(&entry.class, entry.method.as_deref(), "main", &mut strict),
        Err(NativeError::UnknownHostCall(_))
    ));
}
