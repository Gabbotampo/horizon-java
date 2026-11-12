//! `CountDownLatch` and a constructed `ThreadPoolExecutor`, compiled from real
//! Java at test time (`concurrent_sync.rs`).
//!
//! The expected values are the same program run on a real JVM (Temurin
//! 25.0.5+8, javac `--release 17`), through a `main` that prints each method:
//!
//! ```text
//! latch=2|1|false|false|0|true|true|awaited
//! latchErrors=IllegalArgumentException:count < 0|NullPointerException:Cannot invoke "java.util.concurrent.TimeUnit.toNanos(long)" because "unit" is null
//! coreTimeout=false|true|false|IllegalArgumentException:Core threads must have nonzero keep alive times|false|true
//! poolErrors=IllegalArgumentException:corePoolSize must be non-negative;IllegalArgumentException:maximumPoolSize must be positive;IllegalArgumentException:maximumPoolSize must be greater than or equal to corePoolSize;IllegalArgumentException:keepAliveTime must be non-negative;IllegalArgumentException:corePoolSize must be non-negative;NullPointerException:unit;NullPointerException:workQueue;NullPointerException:threadFactory;NullPointerException:handler;NullPointerException:handler;none
//! gatedTask=0|true
//! blockingWait=false
//! ```
//!
//! `blockingWait` is the one the runtime does not answer from the JDK: the JVM
//! blocks for the timeout on a latch nothing else can count down, and this
//! runtime, with one thread, cannot tell that from a latch another thread
//! would have counted down. It counts the call and answers the neutral `false`.

use horizon_native_java::{NativeError, NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const PROBE: &str = r#"package demo;

import java.util.concurrent.BlockingQueue;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.RejectedExecutionHandler;
import java.util.concurrent.ThreadFactory;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;

public final class Probe {
    static final class Flag {
        String value = "unset";
    }

    static String thrown(Runnable body) {
        try {
            body.run();
            return "none";
        } catch (RuntimeException error) {
            return error.getClass().getSimpleName() + ":" + error.getMessage();
        }
    }

    public static String latch() throws Exception {
        CountDownLatch latch = new CountDownLatch(2);
        StringBuilder out = new StringBuilder();
        out.append(latch.getCount());
        latch.countDown();
        out.append('|').append(latch.getCount());
        out.append('|').append(latch.await(0, TimeUnit.SECONDS));
        out.append('|').append(latch.await(-5, TimeUnit.MILLISECONDS));
        latch.countDown();
        latch.countDown();
        out.append('|').append(latch.getCount());
        out.append('|').append(latch.await(60, TimeUnit.SECONDS));
        out.append('|').append(latch.await(Long.MAX_VALUE, TimeUnit.DAYS));
        latch.await();
        out.append("|awaited");
        return out.toString();
    }

    public static String latchErrors() {
        CountDownLatch done = new CountDownLatch(0);
        return thrown(() -> new CountDownLatch(-1)) + "|" + thrown(() -> {
            try {
                done.await(1, null);
            } catch (InterruptedException error) {
                throw new IllegalStateException(error);
            }
        });
    }

    public static String coreTimeout() {
        ThreadPoolExecutor pool =
            new ThreadPoolExecutor(1, 2, 30L, TimeUnit.SECONDS, new LinkedBlockingQueue<>());
        StringBuilder out = new StringBuilder();
        out.append(pool.allowsCoreThreadTimeOut());
        pool.allowCoreThreadTimeOut(true);
        out.append('|').append(pool.allowsCoreThreadTimeOut());
        pool.allowCoreThreadTimeOut(false);
        out.append('|').append(pool.allowsCoreThreadTimeOut());
        ThreadPoolExecutor still =
            new ThreadPoolExecutor(0, 1, 0L, TimeUnit.SECONDS, new LinkedBlockingQueue<>());
        still.allowCoreThreadTimeOut(false);
        out.append('|').append(thrown(() -> still.allowCoreThreadTimeOut(true)));
        out.append('|').append(still.allowsCoreThreadTimeOut());
        // One nanosecond is a nonzero keep-alive.
        ThreadPoolExecutor brief =
            new ThreadPoolExecutor(0, 1, 1L, TimeUnit.NANOSECONDS, new LinkedBlockingQueue<>());
        brief.allowCoreThreadTimeOut(true);
        out.append('|').append(brief.allowsCoreThreadTimeOut());
        return out.toString();
    }

    public static String poolErrors() {
        BlockingQueue<Runnable> queue = new LinkedBlockingQueue<>();
        ThreadFactory factory = Thread::new;
        RejectedExecutionHandler handler = new ThreadPoolExecutor.AbortPolicy();
        return String.join(";",
            thrown(() -> new ThreadPoolExecutor(-1, 1, 1L, TimeUnit.SECONDS, queue)),
            thrown(() -> new ThreadPoolExecutor(0, 0, 1L, TimeUnit.SECONDS, queue)),
            thrown(() -> new ThreadPoolExecutor(2, 1, 1L, TimeUnit.SECONDS, queue)),
            thrown(() -> new ThreadPoolExecutor(1, 1, -1L, TimeUnit.SECONDS, queue)),
            // The size checks come before the null checks.
            thrown(() -> new ThreadPoolExecutor(-1, 1, 1L, null, null)),
            thrown(() -> new ThreadPoolExecutor(1, 1, 1L, null, queue)),
            thrown(() -> new ThreadPoolExecutor(1, 1, 1L, TimeUnit.SECONDS, null)),
            thrown(() -> new ThreadPoolExecutor(1, 1, 1L, TimeUnit.SECONDS, queue,
                (ThreadFactory) null)),
            thrown(() -> new ThreadPoolExecutor(1, 1, 1L, TimeUnit.SECONDS, queue,
                (RejectedExecutionHandler) null)),
            thrown(() -> new ThreadPoolExecutor(1, 1, 1L, TimeUnit.SECONDS, queue, factory,
                null)),
            thrown(() -> new ThreadPoolExecutor(1, 1, 1L, TimeUnit.SECONDS, queue, factory,
                handler)));
    }

    // Serilum's `collective`: a pool with core-thread timeout, and an update check
    // that waits on a latch the mod's own init has already counted down.
    public static String gatedTask() throws Exception {
        CountDownLatch loaded = new CountDownLatch(1);
        ThreadPoolExecutor pool = new ThreadPoolExecutor(10, 10, 30L, TimeUnit.SECONDS,
            new LinkedBlockingQueue<>(), runnable -> {
                Thread thread = new Thread(runnable, "update checker");
                thread.setDaemon(true);
                return thread;
            });
        pool.allowCoreThreadTimeOut(true);
        loaded.countDown();
        Flag flag = new Flag();
        ExecutorService service = pool;
        String before = String.valueOf(loaded.getCount());
        service.submit(() -> {
            try {
                flag.value = String.valueOf(loaded.await(60, TimeUnit.SECONDS));
            } catch (InterruptedException error) {
                flag.value = "interrupted";
            }
        }).get();
        service.shutdown();
        return before + "|" + flag.value;
    }

    // A wait no other code could end: the JVM would block here.
    public static String blockingWait() throws Exception {
        CountDownLatch never = new CountDownLatch(1);
        return String.valueOf(never.await(1, TimeUnit.MILLISECONDS));
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
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "horizon-native-java-concurrent-sync-{test}-{}-{unique}",
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
            "javac failed for the latch fixture {test}"
        );

        let jar_path = fixture.root.join("concurrent-sync.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"concurrent-sync-fixture","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
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

    fn call(&self, method: &str, host: &mut StdHost) -> Result<Value, NativeError> {
        self.jar.invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            host,
        )
    }

    /// The method's text, and the unknown host calls it counted.
    fn text(&self, method: &str) -> (String, Vec<(String, usize)>) {
        let mut host = StdHost::new();
        match self.call(method, &mut host).unwrap() {
            Value::Ref(Some(text)) => (text, host.unknown_host_calls()),
            other => panic!("{method} did not return a string: {other:?}"),
        }
    }
}

/// A method JDK 25 answers in full: its text, with nothing counted.
fn answered(fixture: &Fixture, method: &str) -> String {
    let (text, counted) = fixture.text(method);
    assert!(counted.is_empty(), "{method} counted {counted:?}");
    text
}

#[test]
fn a_latch_counts_down_and_answers_its_waits() {
    let fixture = Fixture::build("latch");
    // java: latch=2|1|false|false|0|true|true|awaited
    assert_eq!(
        answered(&fixture, "latch"),
        "2|1|false|false|0|true|true|awaited"
    );
    assert_eq!(
        answered(&fixture, "latchErrors"),
        "IllegalArgumentException:count < 0|NullPointerException:Cannot invoke \
         \"java.util.concurrent.TimeUnit.toNanos(long)\" because \"unit\" is null"
    );
}

#[test]
fn a_constructed_pool_checks_its_arguments_and_core_timeout() {
    let fixture = Fixture::build("pool");
    assert_eq!(
        answered(&fixture, "coreTimeout"),
        "false|true|false|IllegalArgumentException:Core threads must have nonzero keep \
         alive times|false|true"
    );
    assert_eq!(
        answered(&fixture, "poolErrors"),
        [
            "IllegalArgumentException:corePoolSize must be non-negative",
            "IllegalArgumentException:maximumPoolSize must be positive",
            "IllegalArgumentException:maximumPoolSize must be greater than or equal to corePoolSize",
            "IllegalArgumentException:keepAliveTime must be non-negative",
            "IllegalArgumentException:corePoolSize must be non-negative",
            "NullPointerException:unit",
            "NullPointerException:workQueue",
            "NullPointerException:threadFactory",
            "NullPointerException:handler",
            "NullPointerException:handler",
            "none",
        ]
        .join(";")
    );
}

#[test]
fn a_task_gated_on_a_counted_down_latch_runs() {
    let fixture = Fixture::build("gated");
    // java: gatedTask=0|true (collective's update-check shape)
    assert_eq!(answered(&fixture, "gatedTask"), "0|true");
}

#[test]
fn a_wait_that_would_block_is_counted_not_guessed() {
    let fixture = Fixture::build("blocking");
    let (text, counted) = fixture.text("blockingWait");
    // The JVM blocks for the timeout and prints `false`; the runtime cannot
    // know no other thread would count down, so the answer is the neutral
    // `false` and the call is counted.
    assert_eq!(text, "false");
    assert_eq!(
        counted,
        vec![(
            "java/util/concurrent/CountDownLatch#await(JLjava/util/concurrent/TimeUnit;)Z"
                .to_owned(),
            1
        )]
    );
}
