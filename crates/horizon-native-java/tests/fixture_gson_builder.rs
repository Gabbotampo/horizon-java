//! Gson's fluent builder and formatting style, compiled from real Java at test
//! time.
//!
//! `new GsonBuilder().setPrettyPrinting().create()` is the chain every config
//! bootstrap builds. In Gson every fluent method answers the same builder and
//! only `create()` makes the `Gson`; this fixture proves the runtime keeps the
//! chain alive and that the builder's options reach the written document. The
//! expected values below are the output of the same program run on a real JVM
//! with `gson-2.14.0.jar` on the class path (javac 21, `--release 17`); the
//! `java` output is quoted beside each assertion.

use horizon_native_java::{NativeError, NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const GSON: &str = r#"package com.google.gson;

public class Gson {
    public Gson() {}
    public GsonBuilder newBuilder() { return null; }
    public String toJson(Object src) { return null; }
    public void toJson(Object src, Appendable writer) {}
}
"#;

const GSON_BUILDER: &str = r#"package com.google.gson;

import java.lang.reflect.Type;

public class GsonBuilder {
    public GsonBuilder() {}
    public GsonBuilder setPrettyPrinting() { return this; }
    public GsonBuilder setFormattingStyle(FormattingStyle style) { return this; }
    public GsonBuilder disableHtmlEscaping() { return this; }
    public GsonBuilder serializeNulls() { return this; }
    public GsonBuilder setLenient() { return this; }
    public GsonBuilder excludeFieldsWithModifiers(int... modifiers) { return this; }
    public GsonBuilder registerTypeAdapter(Type type, Object adapter) { return this; }
    public GsonBuilder registerTypeAdapterFactory(Object factory) { return this; }
    public GsonBuilder setFieldNamingPolicy(Object policy) { return this; }
    public Gson create() { return new Gson(); }
}
"#;

// The source `Probe` compiles against declares the constants. The class the
// jar carries does not: the runtime answers a static field only when the jar
// class does not declare it, so the packaged class is the one below, exactly
// as Gson's own constant is the runtime's to model.
const FORMATTING_STYLE_DECL: &str = r#"package com.google.gson;

public class FormattingStyle {
    public static FormattingStyle COMPACT;
    public static FormattingStyle PRETTY;
    public FormattingStyle withNewline(String newline) { return this; }
    public FormattingStyle withIndent(String indent) { return this; }
    public FormattingStyle withSpaceAfterSeparators(boolean space) { return this; }
    public String getNewline() { return null; }
    public String getIndent() { return null; }
    public boolean usesSpaceAfterSeparators() { return false; }
}
"#;

const FORMATTING_STYLE: &str = r#"package com.google.gson;

public class FormattingStyle {
    public FormattingStyle withNewline(String newline) { return this; }
    public FormattingStyle withIndent(String indent) { return this; }
    public FormattingStyle withSpaceAfterSeparators(boolean space) { return this; }
    public String getNewline() { return null; }
    public String getIndent() { return null; }
    public boolean usesSpaceAfterSeparators() { return false; }
}
"#;

const ADAPTER: &str = r#"package com.google.gson;

public interface JsonSerializer {}
"#;

const PROBE: &str = r#"package demo;

import com.google.gson.FormattingStyle;
import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public final class Probe {
    static final class Point {
        int x = 3;
        String name = "p";
        String missing = null;
        transient int hidden = 9;
        static int shared = 7;
    }

    static Map<String, Object> sample() {
        Map<String, Object> map = new LinkedHashMap<>();
        map.put("a", 1);
        map.put("b", List.of(1, 2));
        map.put("empty", new LinkedHashMap<>());
        map.put("none", List.of());
        return map;
    }

    public static String compact() {
        return new GsonBuilder().create().toJson(sample());
    }

    public static String pretty() {
        return new GsonBuilder().setPrettyPrinting().create().toJson(sample());
    }

    public static String html() {
        return new GsonBuilder().create().toJson("a<b&c>d=e'f\"g");
    }

    public static String nohtml() {
        return new GsonBuilder().disableHtmlEscaping().create().toJson("a<b&c>d=e'f\"g");
    }

    public static String prettyStyle() {
        FormattingStyle p = FormattingStyle.PRETTY;
        return p.getNewline() + "|" + p.getIndent() + "|" + p.usesSpaceAfterSeparators();
    }

    public static String compactStyle() {
        FormattingStyle c = FormattingStyle.COMPACT;
        return c.getNewline() + "|" + c.getIndent() + "|" + c.usesSpaceAfterSeparators();
    }

    public static String custom() {
        FormattingStyle tab = FormattingStyle.PRETTY.withIndent("\t").withNewline("\r\n");
        return tab.getNewline() + "|" + tab.getIndent() + "|" + tab.usesSpaceAfterSeparators();
    }

    public static String tabbed() {
        FormattingStyle tab = FormattingStyle.PRETTY.withIndent("\t").withNewline("\r\n");
        return new GsonBuilder().setFormattingStyle(tab).create().toJson(sample());
    }

    public static String nullCheck() {
        try {
            new GsonBuilder().setFormattingStyle(null);
            return "no-npe";
        } catch (NullPointerException error) {
            return error.getClass().getName();
        }
    }

    public static String adapterCheck() {
        try {
            new GsonBuilder().registerTypeAdapter(String.class, "oops");
            return "no-iae";
        } catch (IllegalArgumentException error) {
            return error.getMessage();
        }
    }

    public static String pojo() {
        return new GsonBuilder().setPrettyPrinting().create().toJson(new Point());
    }

    public static String pojoNulls() {
        return new GsonBuilder().serializeNulls().create().toJson(new Point());
    }

    public static String nullValue() {
        return new GsonBuilder().create().toJson(null);
    }

    static final class Adapter implements com.google.gson.JsonSerializer {}

    // A chain with every option a config bootstrap uses, then a document.
    public static String chain() {
        Gson gson = new GsonBuilder()
            .excludeFieldsWithModifiers(0)
            .setLenient()
            .registerTypeAdapter(Integer.class, new Adapter())
            .serializeNulls()
            .create();
        return gson.toJson(sample());
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
            "horizon-native-java-gson-builder-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };
        let classes = fixture.root.join("classes");
        let sources = [
            ("com/google/gson/Gson.java", GSON),
            ("com/google/gson/GsonBuilder.java", GSON_BUILDER),
            (
                "com/google/gson/FormattingStyle.java",
                FORMATTING_STYLE_DECL,
            ),
            ("com/google/gson/JsonSerializer.java", ADAPTER),
            ("demo/Probe.java", PROBE),
        ];
        let mut javac = Command::new(javac_path());
        javac.args(["--release", "17", "-d"]).arg(&classes);
        for (path, source) in sources {
            let file = fixture.root.join("src").join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, source).unwrap();
            javac.arg(file);
        }
        let status = javac.status().unwrap();
        assert!(
            status.success(),
            "javac failed for gson builder fixture {test}"
        );
        // Recompile the fieldless `FormattingStyle` that the jar carries, so
        // the constant read has no jar declaration to shadow the model.
        let replacement = fixture.root.join("replacement");
        let replacement_source = fixture
            .root
            .join("src")
            .join("packaged/FormattingStyle.java");
        fs::create_dir_all(replacement_source.parent().unwrap()).unwrap();
        fs::write(&replacement_source, FORMATTING_STYLE).unwrap();
        let status = Command::new(javac_path())
            .args(["--release", "17", "-d"])
            .arg(&replacement)
            .arg(&replacement_source)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "javac failed for the packaged FormattingStyle of {test}"
        );

        let jar_path = fixture.root.join("gson-builder.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"gson-builder-fixture","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#,
        )
        .unwrap();
        for class in files_under(&classes) {
            let entry = class
                .strip_prefix(&classes)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = if entry == "com/google/gson/FormattingStyle.class" {
                fs::read(replacement.join(&entry)).unwrap()
            } else {
                fs::read(class).unwrap()
            };
            zip.start_file(entry, SimpleFileOptions::default()).unwrap();
            zip.write_all(&bytes).unwrap();
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
fn compact_and_pretty_documents_are_gson_text() {
    let fixture = Fixture::build("documents");
    // java: {"a":1,"b":[1,2],"empty":{},"none":[]}
    assert_eq!(
        fixture.text("compact"),
        r#"{"a":1,"b":[1,2],"empty":{},"none":[]}"#
    );
    // java: {\n  "a": 1,\n  "b": [\n    1,\n    2\n  ],\n  "empty": {},\n  "none": []\n}
    assert_eq!(
        fixture.text("pretty"),
        "{\n  \"a\": 1,\n  \"b\": [\n    1,\n    2\n  ],\n  \"empty\": {},\n  \"none\": []\n}"
    );
}

#[test]
fn html_escaping_follows_the_builder_flag() {
    let fixture = Fixture::build("html");
    // java: "a\u003cb\u0026c\u003ed\u003de\u0027f\"g"
    assert_eq!(
        fixture.text("html"),
        r#""a\u003cb\u0026c\u003ed\u003de\u0027f\"g""#
    );
    // java: "a<b&c>d=e'f\"g"
    assert_eq!(fixture.text("nohtml"), r#""a<b&c>d=e'f\"g""#);
}

#[test]
fn formatting_style_constants_answer_their_real_fields() {
    let fixture = Fixture::build("styles");
    // java: \n|  |true   and   ||false
    assert_eq!(fixture.text("prettyStyle"), "\n|  |true");
    assert_eq!(fixture.text("compactStyle"), "||false");
    // java: \r\n|\t|true
    assert_eq!(fixture.text("custom"), "\r\n|\t|true");
    // java: {\r\n\t"a": 1,\r\n\t"b": [\r\n\t\t1,\r\n\t\t2\r\n\t],\r\n\t"empty": {},...
    assert_eq!(
        fixture.text("tabbed"),
        "{\r\n\t\"a\": 1,\r\n\t\"b\": [\r\n\t\t1,\r\n\t\t2\r\n\t],\r\n\t\"empty\": {},".to_owned()
            + "\r\n\t\"none\": []\r\n}"
    );
}

#[test]
fn null_builder_arguments_raise_the_npe_gson_raises() {
    let fixture = Fixture::build("nulls");
    // java: java.lang.NullPointerException
    assert_eq!(fixture.text("nullCheck"), "java.lang.NullPointerException");
}

#[test]
fn a_jar_class_without_a_supported_adapter_is_an_illegal_argument() {
    let fixture = Fixture::build("adapter");
    // java: Class java.lang.String does not implement any supported type adapter class or interface
    assert_eq!(
        fixture.text("adapterCheck"),
        "Class java.lang.String does not implement any supported type adapter class or interface"
    );
}

#[test]
fn a_jar_object_writes_its_instance_fields() {
    let fixture = Fixture::build("pojo");
    // java: {\n  "x": 3,\n  "name": "p"\n}
    assert_eq!(fixture.text("pojo"), "{\n  \"x\": 3,\n  \"name\": \"p\"\n}");
    // java: {"x":3,"name":"p","missing":null}
    assert_eq!(
        fixture.text("pojoNulls"),
        r#"{"x":3,"name":"p","missing":null}"#
    );
    // java: null
    assert_eq!(fixture.text("nullValue"), "null");
}

#[test]
fn every_fluent_builder_call_returns_the_builder() {
    let fixture = Fixture::build("chain");
    // java: {"a":1,"b":[1,2],"empty":{},"none":[]}
    assert_eq!(
        fixture.text("chain"),
        r#"{"a":1,"b":[1,2],"empty":{},"none":[]}"#
    );
}
