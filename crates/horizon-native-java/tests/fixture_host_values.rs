//! Host objects are typed handles (`Value::Host`, #156): the optionals.
//!
//! `Optional`, `OptionalInt`, `OptionalLong` and `OptionalDouble` used to be
//! text a jar `String` could spell (`optional-int/5`, `optional`), and the
//! empty primitive optionals equalled any string (`OptionalInt.empty()
//! .equals("x")` was `true`). Every expected string is what this class
//! printed on Java 25 (OpenJDK 25.0.5, `javac --release 17`, two runs alike).

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, path::PathBuf, process::Command};
use zip::write::SimpleFileOptions;

const SOURCE: &str = r#"
package demo;

import java.util.*;

public final class HostValues {
  /** Strings spelled like the optionals' old markers, or their handle encodings, stay Strings. */
  public static String spelled() {
    Object[] texts = {"optional-int/5", "optional-int", "optional-long/7", "optional-double/1.5", "optional-double", "optional", "hopti:5", "hopt:i5"};
    StringBuilder out = new StringBuilder();
    for (Object t : texts) {
      out.append(t instanceof OptionalInt).append(t instanceof OptionalLong).append(t instanceof OptionalDouble)
          .append(t instanceof Optional).append(t.equals(OptionalInt.of(5))).append(OptionalInt.empty().equals(t))
          .append(Optional.empty().equals(t))
          // `word/rest` text is still classified by is_plain_string (#156 P17).
          .append(t.toString().contains("/") ? "" : String.valueOf(t.hashCode())).append(',');
    }
    List<Object> list = new ArrayList<>(List.of("hopti:5", OptionalInt.of(5), "hopt:i5", Optional.of(5)));
    Map<Object, String> m = new HashMap<>();
    m.put("optional-int/5", "s");
    m.put(OptionalInt.of(5), "o");
    out.append(list.get(0)).append(list.get(0) instanceof String).append(list.get(1)).append(list.get(2) instanceof String)
        .append(list.get(3)).append('|').append(m.size()).append(m.get(OptionalInt.of(5))).append(m.get("optional-int/5"))
        .append('|').append(Optional.of("optional-int/5").get()).append(OptionalInt.of(5).equals("optional-int/5"))
        .append('|').append("" + "optional-long/7" + "optional");
    return out.toString();
  }

  /** The real optionals behave as the JDK's. */
  public static String real() {
    OptionalInt i = OptionalInt.of(5);
    OptionalLong l = OptionalLong.of(7);
    OptionalDouble d = OptionalDouble.of(1.5);
    OptionalDouble nan = OptionalDouble.of(Double.NaN);
    return i + "|" + l + "|" + d + "|" + OptionalInt.empty() + OptionalLong.empty() + OptionalDouble.empty() + "|"
        + i.getAsInt() + l.getAsLong() + d.getAsDouble() + "|" + i.hashCode() + "," + l.hashCode() + "," + d.hashCode()
        + "," + OptionalInt.empty().hashCode() + "," + OptionalLong.of(-1L << 40).hashCode() + "|" + i.equals(OptionalInt.of(5))
        + l.equals(OptionalLong.of(8)) + nan.equals(OptionalDouble.of(Double.NaN)) + OptionalDouble.of(0.0).equals(OptionalDouble.of(-0.0))
        + OptionalInt.empty().equals(OptionalInt.empty()) + i.equals(OptionalLong.of(5)) + "|" + ((Object) i).getClass().getSimpleName()
        + ((Object) d).getClass().getSimpleName() + "|" + i.orElse(9) + OptionalInt.empty().orElse(9) + "|" + i.isPresent()
        + OptionalLong.empty().isPresent() + "|" + Optional.of(OptionalInt.of(3)).get().getAsInt() + "|" + Optional.of(OptionalDouble.of(2.5));
  }

  /** The empty optionals are singletons: one object. */
  public static String canonical() {
    return "" + (Optional.empty() == Optional.empty()) + (OptionalInt.empty() == OptionalInt.empty())
        + (OptionalLong.empty() == OptionalLong.empty()) + (OptionalDouble.empty() == OptionalDouble.empty())
        + ((Object) Optional.empty() != Optional.of(1)) + (OptionalInt.of(5) != OptionalInt.of(6));
  }

  /** The same handle flowing to both sides is one object (provable). */
  public static String sameHandle() {
    OptionalInt a = OptionalInt.of(5);
    OptionalInt b = a;
    Optional<String> c = Optional.of("x");
    Object d = c;
    return "" + (a == b) + (c == d) + (a != b);
  }

  /** Two OptionalInt.of(5) are equal values but different objects. */
  public static String byValue() {
    return "" + (OptionalInt.of(5) == OptionalInt.of(5));
  }
}
"#;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

/// Compile the fixture into a jar; each test passes its own tag so parallel
/// tests never share a build root.
fn jar(tag: &str) -> NativeJar {
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-host-values-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let source = root.join("src/demo/HostValues.java");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::create_dir_all(root.join("classes")).unwrap();
    fs::write(&source, SOURCE).unwrap();
    let status = Command::new(javac())
        .args(["--release", "17", "-nowarn", "-d"])
        .arg(root.join("classes"))
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");
    let jar_path = root.join("host-values.jar");
    let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
    for class in fs::read_dir(root.join("classes/demo")).unwrap() {
        let class = class.unwrap().path();
        let name = format!("demo/{}", class.file_name().unwrap().to_string_lossy());
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(&fs::read(&class).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    let jar = NativeJar::from_path(&jar_path).unwrap();
    let _ = fs::remove_dir_all(&root);
    jar
}

/// Run one method on a fresh host: its answer and the unknown calls it made.
fn run(jar: &NativeJar, name: &str) -> (Value, Vec<(String, usize)>) {
    let mut host = StdHost::new();
    let value = jar
        .invoke_method_with_args(
            "demo.HostValues",
            name,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap_or_else(|error| panic!("{name} failed: {error}"));
    (value, host.unknown_host_calls())
}

/// No string is an optional: not one spelled like the old markers, nor one
/// spelled like a handle's encoding in a collection; and the real optionals
/// answer as Java 25 does.
#[test]
fn no_string_spells_an_optional_and_the_optionals_answer_as_java_25() {
    let jar = jar("exact");
    for (name, expected) in [
        (
            "spelled",
            "falsefalsefalsefalsefalsefalsefalse,falsefalsefalsefalsefalsefalsefalse1821091842,falsefalsefalsefalsefalsefalsefalse,falsefalsefalsefalsefalsefalsefalse,falsefalsefalsefalsefalsefalsefalse-2021912866,falsefalsefalsefalsefalsefalsefalse-79017120,falsefalsefalsefalsefalsefalsefalse1095930937,falsefalsefalsefalsefalsefalsefalse1095887227,hopti:5trueOptionalInt[5]trueOptional[5]|2os|optional-int/5false|optional-long/7optional",
        ),
        (
            "real",
            "OptionalInt[5]|OptionalLong[7]|OptionalDouble[1.5]|OptionalInt.emptyOptionalLong.emptyOptionalDouble.empty|571.5|5,7,1073217536,0,-256|truefalsetruefalsetruefalse|OptionalIntOptionalDouble|59|truefalse|3|Optional[OptionalDouble[2.5]]",
        ),
        // The empties are the kinds' singletons (canonical: one object).
        ("canonical", "truetruetruetruetruetrue"),
        // The same handle on both sides: provably one object, not counted.
        ("sameHandle", "truetruefalse"),
    ] {
        let (value, unknown) = run(&jar, name);
        assert_eq!(
            value,
            Value::Ref(Some(expected.into())),
            "{name}: {unknown:?}"
        );
        assert_eq!(unknown, Vec::<(String, usize)>::new(), "{name}");
    }
}

/// `OptionalInt.of(5)` twice: equal values, different objects (Java 25:
/// `false`). Equal keys never answer identity for a by-value kind: counted.
#[test]
fn identity_of_two_equal_optionals_is_counted() {
    let jar = jar("identity");
    let (_, unknown) = run(&jar, "byValue");
    assert!(
        unknown
            .iter()
            .any(|(call, _)| call
                == "java/lang/Object#if_acmp(Ljava/lang/Object;Ljava/lang/Object;)Z"),
        "{unknown:?}"
    );
}
