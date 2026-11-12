//! `String`'s regex methods and `Pattern`'s static helpers, run through the
//! interpreter, against the JDK's own answers.
//!
//! The probe repeats the calls the census jars stop on: Cupboard's
//! `CupboardConfig#splitLongDesc` (`replaceFirst` with lookbehinds and a `$1`
//! replacement, then `split("\\R")`) and maxhenkel's
//! `CommentedProperties#removeNewLines` (`split("\\r?\\n")`), plus group
//! references, splits, a replacement that throws, and a pattern outside the
//! modelled subset. Compiled with `javac --release 21` and run with `java`
//! (OpenJDK 27), it printed the lines in `EXPECTED` verbatim.

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::write::SimpleFileOptions;

const PROBE: &str = r##"package demo;

import java.util.regex.Pattern;
import java.util.regex.Matcher;

public final class Probe {
    /** Cupboard's CupboardConfig#splitLongDesc shape. */
    public static String splitLongDesc() {
        String desc = "Whether to log errors, e.g. missing files. Uses the latest. format";
        String out = "";
        for (String line : desc.replaceFirst("((?<!latest)(?<!e\\.g)\\.(?!g\\.)|,)\\h*", "$1\n").split("\\R")) {
            out += "[" + line + "]";
        }
        return out + "|" + desc.replaceAll("((?<!latest)(?<!e\\.g)\\.(?!g\\.)|,)\\h*", "$1\n").split("\\R").length;
    }

    /** maxhenkel's CommentedProperties#removeNewLines shape. */
    public static String removeNewLines() {
        String[] lines = "first\r\nsecond\nthird\r\n\n".split("\\r?\\n");
        return lines.length + ":" + String.join("+", lines);
    }

    public static String groupsAndNames() {
        return "item-12 and block-7".replaceAll("(?<name>[a-z]+)-(\\d+)", "${name}=$2")
            + "|" + "a1b22c333".replaceAll("\\d+?", "<$0>")
            + "|" + "Hello World".replaceFirst("(?i)WORLD", "\\$x")
            + "|" + "x".replaceAll("", "-");
    }

    public static String splits() {
        return String.join(",", "a1b2c3".split("\\d"))
            + "|" + "a,b,,c,,".split(",", -1).length
            + "|" + String.join("/", "boo:and:foo".split("o", 3))
            + "|" + String.join("/", "CamelCaseString".split("(?=\\p{Lu})"));
    }

    public static String matching() {
        return "minecraft:stone".matches("[a-z_]+:[a-z_/]+")
            + "," + Pattern.matches("\\d+(\\.\\d+)*", "1.21.10")
            + "," + "a.b".matches(Pattern.quote("a.b"))
            + "," + "a$b".replaceAll("\\$", Matcher.quoteReplacement("\\$"));
    }

    /** A replacement naming a missing group throws like Java. */
    public static String badReplacement() {
        String out = "";
        try {
            out += "ab".replaceAll("(a)", "$2");
        } catch (IndexOutOfBoundsException e) {
            out += e.getClass().getSimpleName() + ":" + e.getMessage();
        }
        try {
            out += "ab".replaceFirst("a", "x\\");
        } catch (IllegalArgumentException e) {
            out += "|" + e.getMessage();
        }
        // Nothing matched: the replacement is never read.
        return out + "|" + "ab".replaceAll("z", "$9");
    }

    static String none() {
        return null;
    }

    /** A null split pattern: JDK 27's own NPE text, caught by the caller. */
    public static String nullSplitPattern() {
        String pattern = none();
        try {
            return String.valueOf("a,b".split(pattern).length);
        } catch (NullPointerException e) {
            return e.getMessage();
        }
    }

    /** A StringBuilder input is read as its text. */
    public static String builderInput() {
        return String.valueOf(Pattern.matches("a+b", new StringBuilder("aa").append('b')));
    }

    /** A jar's own CharSequence: its text is not tracked, so the call is counted. */
    public static String jarCharSequence() {
        return String.valueOf(Pattern.matches("a+", new Seq("aaa")));
    }

    static final class Seq implements CharSequence {
        private final String text;
        Seq(String text) { this.text = text; }
        public int length() { return text.length(); }
        public char charAt(int index) { return text.charAt(index); }
        public CharSequence subSequence(int start, int end) { return text.subSequence(start, end); }
        public String toString() { return text; }
    }

    /** A property outside the modelled subset: counted, never guessed. */
    public static String unsupported() {
        return String.valueOf("abc".replaceAll("\\p{IsAlphabetic}", "x"));
    }

    /** A 2,999-operand `&&` class: JDK 27 overflows its stack on it. */
    public static String hugeClass() {
        StringBuilder pattern = new StringBuilder("[");
        for (int i = 0; i < 2999; i++) {
            pattern.append("a-z&&");
        }
        pattern.append("a-z]");
        return String.valueOf("a".matches(pattern.toString()));
    }
}
"##;

/// `demo.Probe.<method>()` on OpenJDK 27.
const EXPECTED: &[(&str, &str)] = &[
    (
        "splitLongDesc",
        "[Whether to log errors,][e.g. missing files. Uses the latest. format]|3",
    ),
    ("removeNewLines", "3:first+second+third"),
    (
        "groupsAndNames",
        "item=12 and block=7|a<1>b<2><2>c<3><3><3>|Hello $x|-x-",
    ),
    ("splits", "a,b,c|6|b//:and:foo|Camel/Case/String"),
    ("matching", "true,true,true,a\\$b"),
    (
        "badReplacement",
        "IndexOutOfBoundsException:No group 2|character to be escaped is missing|ab",
    ),
    ("builderInput", "true"),
    (
        "nullSplitPattern",
        "Cannot invoke \"String.length()\" because \"regex\" is null",
    ),
];

fn fixture() -> (PathBuf, NativeJar) {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-regex-{}-{unique}",
        std::process::id()
    ));
    let classes = root.join("classes");
    let source = root.join("src/demo/Probe.java");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::create_dir_all(&classes).unwrap();
    fs::write(&source, PROBE).unwrap();
    let javac = std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into());
    let status = Command::new(javac)
        .args(["--release", "21", "-d"])
        .arg(&classes)
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");
    let jar_path = root.join("probe.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    zip.start_file("fabric.mod.json", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(br#"{"id":"regex-fixture","version":"1.0","entrypoints":{"main":"demo.Probe"}}"#)
        .unwrap();
    for class in ["Probe", "Probe$Seq"] {
        zip.start_file(format!("demo/{class}.class"), SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&fs::read(classes.join(format!("demo/{class}.class"))).unwrap())
            .unwrap();
    }
    zip.finish().unwrap();
    let jar = NativeJar::from_path(&jar_path).unwrap();
    (root, jar)
}

fn call(jar: &NativeJar, method: &str) -> (Value, Vec<(String, usize)>) {
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
    (answer, host.unknown_host_calls())
}

/// Every oracle line, answered with no unknown host call.
#[test]
fn regex_calls_answer_like_the_jdk() {
    let (root, jar) = fixture();
    for (method, expected) in EXPECTED {
        let (answer, unknown) = call(&jar, method);
        assert_eq!(
            answer,
            Value::Ref(Some((*expected).to_owned())),
            "Probe.{method}"
        );
        assert!(unknown.is_empty(), "Probe.{method} counted {unknown:?}");
    }
    let _ = fs::remove_dir_all(root);
}

/// `\\p{IsAlphabetic}` needs Unicode tables: the call is counted, not
/// answered (the JDK prints `xxx`).
#[test]
fn a_pattern_outside_the_subset_is_counted() {
    let (root, jar) = fixture();
    let (_, unknown) = call(&jar, "unsupported");
    assert_eq!(
        unknown,
        [(
            "java/lang/String#replaceAll(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;"
                .to_owned(),
            1
        )]
    );
    let _ = fs::remove_dir_all(root);
}

/// `Pattern.matches` on a jar's own `CharSequence`: the JDK answers `true`
/// (it reads the object's characters), the runtime does not track them, so
/// the call is counted instead of matching the object's marker.
#[test]
fn an_untracked_char_sequence_is_counted() {
    let (root, jar) = fixture();
    let (_, unknown) = call(&jar, "jarCharSequence");
    assert_eq!(
        unknown,
        [(
            "java/util/regex/Pattern#matches(Ljava/lang/String;Ljava/lang/CharSequence;)Z"
                .to_owned(),
            1
        )]
    );
    let _ = fs::remove_dir_all(root);
}

/// A class of 2,999 `&&` operands, built by the jar at run time. JDK 27's
/// answer depends on its stack (`StackOverflowError` from ~1,941 operands
/// on a 512 KB `-Xint` thread, `true` on a cold 1 MB tiered one), and this
/// runtime's recursion over it once aborted the process. The call is
/// counted, on the test harness's 2 MB thread, in any build.
#[test]
fn an_oversized_class_is_counted_not_a_crash() {
    let (root, jar) = fixture();
    let (_, unknown) = call(&jar, "hugeClass");
    assert_eq!(
        unknown,
        [(
            "java/lang/String#matches(Ljava/lang/String;)Z".to_owned(),
            1
        )]
    );
    let _ = fs::remove_dir_all(root);
}
