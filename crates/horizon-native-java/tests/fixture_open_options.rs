//! `StandardOpenOption`s handed to `Files`' writers, and closing the stream
//! `Files.newOutputStream` opens (`open_options.rs`), compiled from real Java at
//! test time.
//!
//! The expected values are the same program run on a real JVM (Temurin
//! 25.0.5+8, javac `--release 17`, in an empty working directory), through a
//! `main` that prints each method:
//!
//! ```text
//! writes=one|two|three|four|five
//! refusals=IllegalArgumentException:READ not allowed;IllegalArgumentException:APPEND + TRUNCATE_EXISTING not allowed;NullPointerException:null;IllegalArgumentException:READ not allowed;IllegalArgumentException:READ not allowed;IllegalArgumentException:APPEND + TRUNCATE_EXISTING not allowed
//! constants=10|READ|DSYNC|4|SYNC|true|IllegalArgumentException:No enum constant java.nio.file.StandardOpenOption.create|NullPointerException:Name is null
//! appends=false
//! ```
//!
//! `appends` is the one the runtime does not answer from the JDK: an `APPEND`
//! open's result depends on whether the file exists, which this runtime does
//! not know for a file it did not write, so the open is counted and answers
//! the neutral `null`.

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

import java.io.OutputStream;
import java.io.PrintWriter;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.OpenOption;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;

import static java.nio.file.StandardOpenOption.APPEND;
import static java.nio.file.StandardOpenOption.CREATE;
import static java.nio.file.StandardOpenOption.READ;
import static java.nio.file.StandardOpenOption.SYNC;
import static java.nio.file.StandardOpenOption.TRUNCATE_EXISTING;
import static java.nio.file.StandardOpenOption.WRITE;

public final class Probe {
    interface Body {
        void run() throws Exception;
    }

    static String thrown(Body body) {
        try {
            body.run();
            return "none";
        } catch (Exception error) {
            return error.getClass().getSimpleName() + ":" + error.getMessage();
        }
    }

    static Path file() throws Exception {
        Path dir = Path.of("open-options-probe");
        Files.createDirectories(dir);
        return Path.of("open-options-probe/config.properties");
    }

    // Sound Physics Remastered's `CommentedPropertyConfig.saveSync` shape.
    public static String writes() throws Exception {
        Path path = file();
        StringBuilder out = new StringBuilder();
        OutputStream stream = Files.newOutputStream(path, CREATE, SYNC, TRUNCATE_EXISTING);
        PrintWriter writer = new PrintWriter(stream);
        writer.print("one");
        writer.flush();
        stream.close();
        out.append(Files.readString(path));
        try (PrintWriter again = new PrintWriter(Files.newOutputStream(path, TRUNCATE_EXISTING,
                CREATE, WRITE))) {
            again.print("two");
        }
        out.append('|').append(Files.readString(path));
        Files.writeString(path, "three", CREATE, TRUNCATE_EXISTING);
        out.append('|').append(Files.readString(path));
        try (Writer buffered = Files.newBufferedWriter(path, StandardCharsets.UTF_8, CREATE,
                TRUNCATE_EXISTING)) {
            buffered.write("four");
        }
        out.append('|').append(Files.readString(path));
        Files.writeString(path, "five");
        out.append('|').append(Files.readString(path));
        return out.toString();
    }

    public static String refusals() throws Exception {
        Path path = file();
        return String.join(";",
            thrown(() -> Files.newOutputStream(path, READ)),
            thrown(() -> Files.newOutputStream(path, CREATE, APPEND, TRUNCATE_EXISTING)),
            thrown(() -> Files.newOutputStream(path, CREATE, (OpenOption) null)),
            // `READ` is refused while the array is copied, before the null is seen.
            thrown(() -> Files.newOutputStream(path, (OpenOption) null, READ)),
            thrown(() -> Files.writeString(path, "x", READ)),
            thrown(() -> Files.newBufferedWriter(path, APPEND, TRUNCATE_EXISTING)));
    }

    public static String constants() {
        StandardOpenOption[] all = StandardOpenOption.values();
        return all.length + "|" + all[0] + "|" + all[all.length - 1] + "|" + CREATE.ordinal()
            + "|" + SYNC.name() + "|" + (StandardOpenOption.valueOf("TRUNCATE_EXISTING")
            == TRUNCATE_EXISTING) + "|" + thrown(() -> StandardOpenOption.valueOf("create"))
            + "|" + thrown(() -> StandardOpenOption.valueOf(null));
    }

    // The JVM appends to the file that exists; this runtime does not know
    // whether a file it did not write exists, so the open is counted.
    public static String appends() throws Exception {
        Path path = file();
        Files.writeString(path, "five");
        OutputStream stream = Files.newOutputStream(path, CREATE, APPEND);
        return String.valueOf(stream == null);
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
            "horizon-native-java-open-options-{test}-{}-{unique}",
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
            "javac failed for the open-options fixture {test}"
        );

        let jar_path = fixture.root.join("open-options.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"open-options-fixture","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
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
fn create_and_truncate_writes_reach_the_file_through_every_writer() {
    let fixture = Fixture::build("writes");
    // java: writes=one|two|three|four|five
    assert_eq!(answered(&fixture, "writes"), "one|two|three|four|five");
}

#[test]
fn refused_option_sets_throw_what_the_jdk_throws() {
    let fixture = Fixture::build("refusals");
    assert_eq!(
        answered(&fixture, "refusals"),
        [
            "IllegalArgumentException:READ not allowed",
            "IllegalArgumentException:APPEND + TRUNCATE_EXISTING not allowed",
            "NullPointerException:null",
            "IllegalArgumentException:READ not allowed",
            "IllegalArgumentException:READ not allowed",
            "IllegalArgumentException:APPEND + TRUNCATE_EXISTING not allowed",
        ]
        .join(";")
    );
}

#[test]
fn the_open_options_are_the_jdk_enum() {
    let fixture = Fixture::build("constants");
    assert_eq!(
        answered(&fixture, "constants"),
        "10|READ|DSYNC|4|SYNC|true|IllegalArgumentException:No enum constant \
         java.nio.file.StandardOpenOption.create|NullPointerException:Name is null"
    );
}

/// Not JDK behaviour: the JVM opens the stream (`false`); the runtime counts
/// an `APPEND` open instead of guessing whether the file exists.
#[test]
fn an_append_open_is_counted_not_answered_as_a_truncation() {
    let fixture = Fixture::build("appends");
    let (text, counted) = fixture.text("appends");
    assert_eq!(text, "true");
    assert_eq!(
        counted,
        vec![(
            "java/nio/file/Files#newOutputStream(Ljava/nio/file/Path;[Ljava/nio/file/OpenOption;)Ljava/io/OutputStream;"
                .to_owned(),
            1
        )]
    );
}
