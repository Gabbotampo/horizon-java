//! A jar's bundled text resource read back as a `String` through the JDK:
//! `readAllBytes`, then `new String(bytes, UTF_8)`.

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
        "horizon-native-java-resource-text-{name}-{}",
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

/// A jar's bundled text resource (built-in scripts, default configs) read
/// through the class loader, then turned into a `String` with the JDK alone.
#[test]
fn bundled_text_resource_reads_back_as_a_string() {
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
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
public final class Main {
    public static void main() throws Exception {
        InputStream first = Main.class.getClassLoader().getResourceAsStream("assets/demo/hello.sc");
        Probe.check("opened", first != null);
        String jdk = new String(first.readAllBytes(), StandardCharsets.UTF_8);
        Probe.check("jdk", "print('hi')\n".equals(jdk));
    }
}
"#,
    );
    let jar_path = build_jar(
        "bundled-resource",
        sources,
        &[("assets/demo/hello.sc", "print('hi')\n")],
        &["demo/Probe.class"],
    );
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let mut host = jar.seed_host(StdHost::new());
    jar.execute("demo.Main", Some("main"), "main", &mut host)
        .unwrap();
    assert_eq!(
        checks(&host),
        BTreeMap::from([("jdk".to_owned(), true), ("opened".to_owned(), true),])
    );
}
