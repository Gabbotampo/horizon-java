//! `java.lang.ref` references and the inherited `AbstractCollection.isEmpty`.
//!
//! respackopts keeps its server set in jfronny commons' `WeakSet`: a
//! `HashSet` of `WeakReference`s plus a `ReferenceQueue`, with `isEmpty`
//! inherited from `java.util.AbstractCollection` (`size() == 0`). The runtime
//! had neither the reference classes nor the inherited body, so the listener
//! died on `Set.isEmpty` (`docs/in-game-survey.md`, "Top causes" §5).
//!
//! The expected strings are what `java` prints for the same `Probe` methods
//! without a GC between the calls. The one place this runtime differs from a
//! JVM is named in `references`: objects are never collected, so a weak
//! referent is never cleared and nothing is ever enqueued. That is the same
//! observable answer as a JVM run with no collection, which is what the
//! fixture measures.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const PROBE: &str = r##"package demo;

import java.lang.ref.ReferenceQueue;
import java.lang.ref.WeakReference;
import java.util.AbstractSet;
import java.util.HashSet;
import java.util.Iterator;
import java.util.Set;

public class Probe {
    static String reference() {
        Object payload = "payload";
        ReferenceQueue<Object> queue = new ReferenceQueue<>();
        WeakReference<Object> reference = new WeakReference<>(payload, queue);
        StringBuilder out = new StringBuilder();
        out.append(reference.get() == payload).append(':');
        out.append(reference.isEnqueued()).append(':');
        out.append(queue.poll() == null).append(':');
        reference.clear();
        out.append(reference.get() == null);
        return out.toString();
    }

    static class Bag<E> extends AbstractSet<E> {
        private final Set<E> delegate = new HashSet<>();

        @Override
        public Iterator<E> iterator() {
            return delegate.iterator();
        }

        @Override
        public int size() {
            return delegate.size();
        }

        @Override
        public boolean add(E value) {
            return delegate.add(value);
        }
    }

    /** The receiver is typed as `Set`: `invokeinterface Set.isEmpty`. */
    static String asSet() {
        Set<String> bag = new Bag<>();
        StringBuilder out = new StringBuilder();
        out.append(bag.isEmpty()).append(':');
        bag.add("x");
        out.append(bag.isEmpty());
        return out.toString();
    }

    /** The receiver is typed as the subclass: `invokevirtual Bag.isEmpty`. */
    static String asSubclass() {
        Bag<String> bag = new Bag<>();
        StringBuilder out = new StringBuilder();
        out.append(bag.isEmpty()).append(':');
        bag.add("x");
        out.append(bag.isEmpty());
        return out.toString();
    }
}
"##;

const ENTRY: &str = r##"package demo;

public class Entry {
    public void onInitialize() {
        Probe.asSet();
    }
}
"##;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

struct Fixture {
    root: PathBuf,
    jar: NativeJar,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn build(test: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "horizon-native-java-references-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };
        let classes = fixture.root.join("classes");
        let mut javac = Command::new(javac());
        javac.args(["--release", "21", "-d"]).arg(&classes);
        for (path, source) in [("demo/Probe.java", PROBE), ("demo/Entry.java", ENTRY)] {
            let file = fixture.root.join("src").join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, source).unwrap();
            javac.arg(file);
        }
        assert!(javac.status().unwrap().success(), "javac failed");

        let jar_path = fixture.root.join("references.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"references-fixture","version":"1.0","entrypoints":{"main":"demo.Entry"}}"#,
        )
        .unwrap();
        for class in files_under(&classes.join("demo")) {
            let entry = class
                .strip_prefix(&classes)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(entry, SimpleFileOptions::default()).unwrap();
            zip.write_all(&fs::read(class).unwrap()).unwrap();
        }
        zip.finish().unwrap();

        fixture.jar = NativeJar::from_path(&jar_path).unwrap();
        fixture
    }

    fn text(&self, method: &str) -> String {
        let mut host = self.jar.seed_host(StdHost::new());
        match self
            .jar
            .invoke_method_with_args(
                "demo/Probe",
                method,
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            )
            .unwrap_or_else(|error| panic!("{method} failed: {error}"))
        {
            Value::Ref(Some(text)) => text,
            other => panic!("{method} did not return a string: {other:?}"),
        }
    }
}

#[test]
fn a_weak_reference_answers_its_referent_and_an_empty_queue() {
    let fixture = Fixture::build("reference");
    assert_eq!(fixture.text("reference"), "true:false:true:true");
}

#[test]
fn an_inherited_is_empty_is_size_equals_zero_for_an_interface_call() {
    let fixture = Fixture::build("as-set");
    assert_eq!(fixture.text("asSet"), "true:false");
}

#[test]
fn an_inherited_is_empty_is_size_equals_zero_for_a_subclass_call() {
    let fixture = Fixture::build("as-subclass");
    assert_eq!(fixture.text("asSubclass"), "true:false");
}
