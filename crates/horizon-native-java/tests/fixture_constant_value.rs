//! `getstatic` on a static field whose value is its `ConstantValue` attribute
//! (JVMS §4.7.2), assigned at initialization step 6 (JVMS §5.5), before the
//! superclass and `<clinit>`, against the JDK's own answers.
//!
//! javac inlines a compile-time constant at every use site, so a `getstatic`
//! of one comes from another compiler, or from a class compiled against an
//! older version of the field that was not a constant. The fixture is built
//! the second way: the `V1` sources (fields without constants) are compiled
//! with `javac --release 21`, then the `V2` sources are recompiled over them
//! with only `ConstantValue` attributes and no `<clinit>` assignment. The
//! probe's reads stay `getstatic`. The same classes, built the same way and
//! run with `java` (OpenJDK 27), printed the lines in `EXPECTED` verbatim.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const CONSTS_V1: &str = r##"package demo;

public class Consts {
    public static int i;
    public static short s;
    public static char c;
    public static byte b;
    public static boolean z;
    public static long j;
    public static float f;
    public static double d;
    public static String t;
}
"##;

const BASE: &str = r##"package demo;

public class Base {
    public static int seen = Sub.K;
}
"##;

const SUB_V1: &str = r##"package demo;

public class Sub extends Base {
    public static int K;
}
"##;

const LIMITS_V1: &str = r##"package demo;

public interface Limits {
    int MAX = Integer.parseInt("1");
}
"##;

const PROBE: &str = r##"package demo;

public final class Probe {
    /** Every ConstantValue type, read through getstatic (Consts was not constant when this compiled). */
    public static String constants() {
        return Consts.i + "," + Consts.s + "," + (int) Consts.c + "," + Consts.b + "," + Consts.z
            + "," + Consts.j + "," + Consts.f + "," + Consts.d + "," + Consts.t;
    }

    /** Sub's constant is set before Base's initializer, which runs before Sub's own. */
    public static String beforeSuperclass() {
        return Sub.K + "," + Base.seen;
    }

    /** An interface's constant field. */
    public static String interfaceConstant() {
        return String.valueOf(Limits.MAX);
    }
}
"##;

const CONSTS_V2: &str = r##"package demo;

public class Consts {
    public static final int i = 70001;
    public static final short s = -300;
    public static final char c = 'Z';
    public static final byte b = -5;
    public static final boolean z = true;
    public static final long j = 5000000001L;
    public static final float f = 1.25f;
    public static final double d = 0.1;
    public static final String t = "constant";
}
"##;

const SUB_V2: &str = r##"package demo;

public class Sub extends Base {
    public static final int K = 7;
}
"##;

const LIMITS_V2: &str = r##"package demo;

public interface Limits {
    int MAX = 9;
}
"##;

/// `demo.Probe.<method>()` on OpenJDK 27.
const EXPECTED: &[(&str, &str)] = &[
    (
        "constants",
        "70001,-300,90,-5,true,5000000001,1.25,0.1,constant",
    ),
    ("beforeSuperclass", "7,7"),
    ("interfaceConstant", "9"),
];

fn javac(classes: &Path, sources: &[(&str, &str)], root: &Path, phase: &str) {
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let mut command = Command::new(javac);
    command
        .args(["--release", "21", "-cp"])
        .arg(classes)
        .arg("-d")
        .arg(classes);
    for (path, source) in sources {
        let file = root.join(phase).join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        command.arg(file);
    }
    assert!(
        command.status().unwrap().success(),
        "javac failed ({phase})"
    );
}

/// Compile V1, recompile V2 over it, and pack `demo/` as a Fabric jar.
fn fixture() -> (PathBuf, NativeJar) {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-constant-value-{}-{unique}",
        std::process::id()
    ));
    let classes = root.join("classes");
    fs::create_dir_all(&classes).unwrap();
    javac(
        &classes,
        &[
            ("demo/Consts.java", CONSTS_V1),
            ("demo/Base.java", BASE),
            ("demo/Sub.java", SUB_V1),
            ("demo/Limits.java", LIMITS_V1),
            ("demo/Probe.java", PROBE),
        ],
        &root,
        "v1",
    );
    javac(
        &classes,
        &[
            ("demo/Consts.java", CONSTS_V2),
            ("demo/Sub.java", SUB_V2),
            ("demo/Limits.java", LIMITS_V2),
        ],
        &root,
        "v2",
    );
    let jar_path = root.join("probe.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    zip.start_file("fabric.mod.json", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(br#"{"id":"probe-fixture","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#)
        .unwrap();
    for entry in fs::read_dir(classes.join("demo")).unwrap() {
        let path = entry.unwrap().path();
        let name = format!("demo/{}", path.file_name().unwrap().to_string_lossy());
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(&path).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    let jar = NativeJar::from_path(&jar_path).unwrap();
    (root, jar)
}

/// Every oracle line, answered with no unknown host call. Each method runs on
/// a fresh host, so every class initializes inside it.
#[test]
fn getstatic_reads_constant_values_like_the_jdk() {
    let (root, jar) = fixture();
    for (method, expected) in EXPECTED {
        let mut host = jar.seed_host(StdHost::new());
        let answer = jar
            .invoke_method_with_args(
                "demo/Probe",
                method,
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            )
            .unwrap_or_else(|error| panic!("{method} failed: {error}"));
        assert_eq!(
            answer,
            Value::Ref(Some((*expected).to_owned())),
            "Probe.{method}"
        );
        assert!(
            host.unknown_host_calls().is_empty(),
            "Probe.{method} counted {:?}",
            host.unknown_host_calls()
        );
    }
    let _ = fs::remove_dir_all(root);
}
