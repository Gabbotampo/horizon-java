//! `java.util.concurrent` executors, compiled from real Java at test time.
//!
//! The fixture compiles against the JDK's own executor types and packages only
//! its own classes: the runtime answers `Executors` and `ExecutorService` from
//! its own model. The expected values below are the same program run on a real
//! JVM (OpenJDK 27, javac 21, `--release 17`):
//!
//! ```text
//! submit=[42|true|false|false]
//! execute=[1]
//! runnable-submit=[null|true]
//! callable=[null|2]
//! with-result=[ok]
//! shutdown=[true|true|true]
//! rejected=[java.util.concurrent.RejectedExecutionException]
//! fixed=[fixed]
//! scheduled=[later]
//! cached=[7]
//! zero-pool=[java.lang.IllegalArgumentException]
//! ```
//!
//! Every expected value is deterministic on a real JVM because each result is
//! read through `Future.get()` or after `shutdown()`; the runtime's departure
//! (a task runs inline, before `execute` returns) cannot change these answers.

use horizon_native_java::{NativeError, NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use zip::write::SimpleFileOptions;

#[path = "support/unique_dir.rs"]
mod unique_dir;
use unique_dir::unique_suffix;

const PROBE: &str = r#"package demo;

import java.util.concurrent.Callable;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;

public final class Probe {
    static final class Counter {
        int value;
        void inc() { value++; }
    }

    static final class Flag {
        boolean value;
        void set() { value = true; }
    }

    public static String roundTrip() throws Exception {
        Counter runs = new Counter();
        Flag flag = new Flag();
        ExecutorService pool = Executors.newSingleThreadExecutor();
        pool.execute(() -> runs.inc());
        Future<Integer> answer = pool.submit(() -> 41 + 1);
        String submit = answer.get() + "|" + answer.isDone() + "|" + answer.isCancelled()
            + "|" + answer.cancel(true);
        String execute = String.valueOf(runs.value);
        Future<?> done = pool.submit(() -> flag.set());
        String runnable = done.get() + "|" + flag.value;
        Runnable task = () -> runs.inc();
        Callable<Object> callable = Executors.callable(task);
        String called = callable.call() + "|" + runs.value;
        String withResult = Executors.callable((Runnable) () -> {}, "ok").call() + "";
        pool.shutdown();
        String shutdown = pool.isShutdown() + "|" + pool.awaitTermination(1, TimeUnit.SECONDS)
            + "|" + pool.isTerminated();
        String rejected;
        try {
            pool.execute(() -> {});
            rejected = "no-reject";
        } catch (RejectedExecutionException error) {
            rejected = error.getClass().getName();
        }
        return submit + ";" + execute + ";" + runnable + ";" + called + ";" + withResult + ";"
            + shutdown + ";" + rejected;
    }

    public static String factories() throws Exception {
        ExecutorService fixed = Executors.newFixedThreadPool(2);
        String one = String.valueOf(fixed.submit(() -> "fixed").get());
        fixed.shutdownNow();
        ScheduledExecutorService scheduled = Executors.newSingleThreadScheduledExecutor();
        String two = String.valueOf(scheduled.schedule(() -> "later", 5, TimeUnit.SECONDS).get());
        scheduled.shutdown();
        ExecutorService cached = Executors.newCachedThreadPool();
        String three = String.valueOf(cached.submit(() -> 7).get());
        cached.shutdown();
        return one + "|" + two + "|" + three;
    }

    public static String emptyPool() {
        try {
            Executors.newFixedThreadPool(0);
            return "no-iae";
        } catch (IllegalArgumentException error) {
            return error.getClass().getName();
        }
    }
}
"#;

fn javac_path() -> PathBuf {
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
        let unique = unique_suffix();
        let root = std::env::temp_dir().join(format!(
            "horizon-native-java-executors-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };
        let classes = fixture.root.join("classes");
        let file = fixture.root.join("src").join("demo/Probe.java");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, PROBE).unwrap();
        let status = Command::new(javac_path())
            .args(["--release", "17", "-d"])
            .arg(&classes)
            .arg(&file)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "javac failed for the executors fixture {test}"
        );

        let jar_path = fixture.root.join("executors.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"executors-fixture","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
        )
        .unwrap();
        for class in files_under(&classes) {
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

    fn call(&self, method: &str) -> Result<Value, NativeError> {
        self.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut StdHost::new(),
        )
    }

    fn text(&self, method: &str) -> String {
        match self.call(method).unwrap() {
            Value::Ref(Some(text)) => text,
            other => panic!("{method} did not return a string: {other:?}"),
        }
    }
}

#[test]
fn tasks_run_and_their_futures_answer() {
    let fixture = Fixture::build("round-trip");
    // java: submit=[42|true|false|false]; execute=[1]; runnable-submit=[null|true];
    //       callable=[null|2]; with-result=[ok]; shutdown=[true|true|true];
    //       rejected=[java.util.concurrent.RejectedExecutionException]
    assert_eq!(
        fixture.text("roundTrip"),
        "42|true|false|false;1;null|true;null|2;ok;true|true|true;\
         java.util.concurrent.RejectedExecutionException"
    );
}

#[test]
fn the_pool_factories_answer_a_working_service() {
    let fixture = Fixture::build("factories");
    // java: fixed|later|7
    assert_eq!(fixture.text("factories"), "fixed|later|7");
    // java: java.lang.IllegalArgumentException
    assert_eq!(
        fixture.text("emptyPool"),
        "java.lang.IllegalArgumentException"
    );
}
