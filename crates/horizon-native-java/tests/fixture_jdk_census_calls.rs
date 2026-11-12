//! JDK calls the unmodelled-call census ranks by `sole`, run from a compiled
//! jar so the descriptors are `javac`'s, not hand-written.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{collections::BTreeMap, fs, io::Write, path::Path, process::Command};
use zip::write::SimpleFileOptions;

fn javac() -> std::path::PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| std::path::PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn files_under(root: &Path) -> Vec<std::path::PathBuf> {
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

fn build_jar(
    name: &str,
    sources: BTreeMap<&str, &str>,
    resources: &[(&str, &str)],
    excluded: &[&str],
) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-jdk-census-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let src = root.join("src");
    let classes = root.join("classes");
    fs::create_dir_all(&classes).unwrap();
    let mut paths = Vec::new();
    for (relative, body) in sources {
        let path = src.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, body).unwrap();
        paths.push(path);
    }
    let status = Command::new(javac())
        .arg("--release")
        .arg("17")
        .arg("-d")
        .arg(&classes)
        .args(paths)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed for {name}");

    let jar_path = root.join(format!("{name}.jar"));
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    for (path, body) in resources {
        zip.start_file(*path, SimpleFileOptions::default()).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    for class in files_under(&classes) {
        let entry = class
            .strip_prefix(&classes)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if excluded.contains(&entry.as_str()) {
            continue;
        }
        zip.start_file(entry, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(class).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    jar_path
}

fn checks(host: &StdHost) -> BTreeMap<String, bool> {
    host.invocations()
        .iter()
        .filter(|invocation| {
            invocation.method.owner == "demo/Probe" && invocation.method.name == "check"
        })
        .filter_map(|invocation| match invocation.args.as_slice() {
            [Value::Ref(Some(label)), Value::Int(flag)] => Some((label.clone(), *flag != 0)),
            _ => None,
        })
        .collect()
}

/// Three JDK calls the unmodelled-call census ranks highest by `sole`
/// (the jars each one alone keeps from `ran-clean`): `Properties.setProperty`,
/// `String.formatted` and a `StackWalker.Option` constant.
#[test]
fn census_jdk_calls_answer_like_the_jdk() {
    let mut sources = BTreeMap::new();
    sources.insert(
        "demo/Probe.java",
        r#"package demo;
public final class Probe {
    public static void check(String label, boolean value) {}
}
"#,
    );
    sources.insert(
        "demo/Main.java",
        r#"package demo;
import java.util.Properties;
public final class Main {
    public static void main() {
        Properties properties = new Properties();
        Object first = properties.setProperty("mode", "fast");
        Object second = properties.setProperty("mode", "safe");
        Probe.check("set-new", first == null);
        Probe.check("set-old", "fast".equals(second));
        Probe.check("get", "safe".equals(properties.getProperty("mode")));
        Probe.check("formatted", "%s-%d".formatted("tick", 20).equals("tick-20"));
        StackWalker.Option option = StackWalker.Option.RETAIN_CLASS_REFERENCE;
        Probe.check("option-same", option == StackWalker.Option.RETAIN_CLASS_REFERENCE);
        Probe.check("option-name", "RETAIN_CLASS_REFERENCE".equals(option.name()));
        Probe.check("walker", StackWalker.getInstance(option) != null);
    }
}
"#,
    );
    let jar_path = build_jar("census-jdk-calls", sources, &[], &["demo/Probe.class"]);
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    jar.execute("demo.Main", Some("main"), "main", &mut host)
        .unwrap();
    assert_eq!(
        checks(&host),
        BTreeMap::from(
            [
                "set-new",
                "set-old",
                "get",
                "formatted",
                "option-same",
                "option-name",
                "walker",
            ]
            .map(|label| (label.to_owned(), true))
        )
    );
    // Only the probe's own `check`, whose class the jar leaves out on purpose.
    let unknown: Vec<_> = host
        .unknown_host_calls()
        .into_iter()
        .filter(|(call, _)| !call.starts_with("demo/Probe#"))
        .collect();
    assert_eq!(unknown, Vec::new());
}

/// A `formatted` call the formatter refuses (`%a` is not modelled) is left to
/// the host and counted under its own name, not as `String.format`.
#[test]
fn refused_formatted_is_counted_as_formatted() {
    let mut sources = BTreeMap::new();
    sources.insert(
        "demo/Main.java",
        r#"package demo;
public final class Main {
    public static String text;
    public static void main() {
        text = "%a".formatted(1.0);
    }
}
"#,
    );
    let jar_path = build_jar("census-refused-formatted", sources, &[], &[]);
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    jar.execute("demo.Main", Some("main"), "main", &mut host)
        .unwrap();
    let calls: Vec<String> = host
        .unknown_host_calls()
        .into_iter()
        .map(|(call, _)| call)
        .collect();
    assert!(
        calls.contains(
            &"java/lang/String#formatted([Ljava/lang/Object;)Ljava/lang/String;".to_owned()
        ),
        "{calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|call| call.starts_with("java/lang/String#format(")),
        "{calls:?}"
    );
}
