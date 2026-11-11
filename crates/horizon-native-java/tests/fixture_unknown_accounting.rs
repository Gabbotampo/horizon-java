//! Every call the runtime cannot answer from a model is counted, so a run that
//! leaned on a neutral answer never reads `ran-clean`:
//!
//! - a jar's `native` method (on the JVM: `UnsatisfiedLinkError: 'int
//!   demo.Bodiless.nat(int)'` without its library, measured with OpenJDK 27);
//! - an abstract jar member reached on an object the jar does not own;
//! - an instance call on a host object no model answers;
//! - the 65th and later distinct unknown method (the table used to stop
//!   counting at 64 keys).
//!
//! Before this, each of these answered `0`/`null` and added nothing to
//! `unknown_host_call_count()`.

use horizon_native_java::{Host, MethodRef, NativeError, NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use zip::write::SimpleFileOptions;

/// Compile one source (nested classes included) and package its package's
/// classes as a Fabric jar.
fn build_fixture(tag: &str, entrypoint: &str, source: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-unknown-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let class = entrypoint.replace('.', "/");
    let source_path = root.join("src").join(format!("{class}.java"));
    let classes = root.join("classes");
    fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    fs::create_dir_all(&classes).unwrap();
    fs::write(&source_path, source).unwrap();
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let status = Command::new(javac)
        .args(["--release", "17", "-d"])
        .arg(&classes)
        .arg(&source_path)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed for {entrypoint}");

    let id = format!("fixture-unknown-{tag}");
    let jar_path = root.join(format!("{id}.jar"));
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    let options = SimpleFileOptions::default();
    zip.start_file("fabric.mod.json", options).unwrap();
    zip.write_all(
        format!(r#"{{"id":"{id}","version":"1.0","entrypoints":{{"main":"{entrypoint}"}}}}"#)
            .as_bytes(),
    )
    .unwrap();
    let package = Path::new(&class).parent().unwrap();
    for entry in fs::read_dir(classes.join(package)).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap();
        zip.start_file(format!("{}/{name}", package.display()), options)
            .unwrap();
        zip.write_all(&fs::read(&path).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    jar_path
}

fn count(host: &StdHost, key: &str) -> usize {
    host.unknown_host_calls()
        .into_iter()
        .find(|(method, _)| method == key)
        .map_or(0, |(_, count)| count)
}

const BODILESS: &str = r#"
package demo;
public final class Bodiless {
  static native int nat(int x);
  interface Api { int answer(); }
  public static int viaApi(Api api) { return api.answer(); }
  public void onInitialize() {
    nat(5);
  }
}
"#;

/// A `native` method has no body to run: the call is counted as an unknown
/// host call, and a strict host refuses it.
#[test]
fn a_native_method_is_a_counted_unknown_not_a_silent_zero() {
    let jar = NativeJar::from_path(&build_fixture("native", "demo.Bodiless", BODILESS)).unwrap();
    let entry = jar.discover_entrypoint("main").unwrap();
    let mut host = StdHost::new();
    jar.execute(&entry.class, entry.method.as_deref(), "main", &mut host)
        .unwrap();
    assert_eq!(count(&host, "demo/Bodiless#nat(I)I"), 1);
    assert_eq!(host.unknown_host_call_count(), 1);

    let mut strict = StdHost::new().with_strict_unknown_host_calls();
    match jar.execute(&entry.class, entry.method.as_deref(), "main", &mut strict) {
        Err(NativeError::UnknownHostCall(r)) => assert_eq!(r.name, "nat"),
        other => panic!("a strict host must refuse the native call, got {other:?}"),
    }
}

/// An abstract jar member reached on an object the jar does not own (a host
/// marker the jar was handed) has no body in the jar: the host answers it, and
/// an unmodelled one is counted.
#[test]
fn an_abstract_call_on_a_host_object_is_a_counted_unknown() {
    let jar = NativeJar::from_path(&build_fixture("abstract", "demo.Bodiless", BODILESS)).unwrap();
    let mut host = StdHost::new();
    let value = jar
        .invoke_method_with_args(
            "demo.Bodiless",
            "viaApi",
            "(Ldemo/Bodiless$Api;)I",
            None,
            &[Value::Ref(Some("game/object#1".into()))],
            &mut host,
        )
        .unwrap();
    assert_eq!(value, Value::Int(0));
    assert_eq!(count(&host, "demo/Bodiless$Api#answer()I"), 1);
}

/// An instance call on a host object no model answers is counted; the
/// constructor that made the object is not (its marker is the object).
#[test]
fn an_unmodelled_host_instance_call_is_counted() {
    let jar = NativeJar::from_path(&build_fixture(
        "instance",
        "demo.Checksum",
        r#"
package demo;
import java.util.zip.Adler32;
public final class Checksum {
  public void onInitialize() {
    Adler32 sum = new Adler32();
    sum.update(1);
    sum.update(2);
    long value = sum.getValue();
  }
}
"#,
    ))
    .unwrap();
    let entry = jar.discover_entrypoint("main").unwrap();
    let mut host = StdHost::new();
    jar.execute(&entry.class, entry.method.as_deref(), "main", &mut host)
        .unwrap();
    assert_eq!(count(&host, "java/util/zip/Adler32#update(I)V"), 2);
    assert_eq!(count(&host, "java/util/zip/Adler32#getValue()J"), 1);
    assert_eq!(count(&host, "java/util/zip/Adler32#<init>()V"), 0);
    assert_eq!(host.unknown_host_call_count(), 3);
}

/// The unknown-call table counts every distinct method, not the first 64.
#[test]
fn every_distinct_unknown_call_is_counted() {
    let mut host = StdHost::new();
    for i in 0..100 {
        let r = MethodRef {
            owner: "demo/Unmodelled".into(),
            name: format!("call{i}"),
            descriptor: "()V".into(),
        };
        host.invoke(&r, &[]).unwrap();
        host.invoke(&r, &[]).unwrap();
    }
    assert_eq!(host.unknown_host_calls().len(), 100);
    assert_eq!(host.unknown_host_call_count(), 200);
}
