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
    // `hopti:5` is a handle's text form; a `word/rest` key waits for #156 P17.
    m.put("hopti:5", "s");
    m.put(OptionalInt.of(5), "o");
    out.append(list.get(0)).append(list.get(0) instanceof String).append(list.get(1)).append(list.get(2) instanceof String)
        .append(list.get(3)).append('|').append(m.size()).append(m.get(OptionalInt.of(5))).append(m.get("hopti:5"))
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

  /** Optionals holding text with separators, escapes and non-ASCII read back whole out of collections. */
  public static String nested() {
    List<Object> list = new ArrayList<>();
    list.add(Optional.of("a,b"));
    list.add(Optional.of(Optional.of("x\\,y")));
    list.add(Optional.of("é,\\ü"));
    Optional<?> first = (Optional<?>) list.get(0);
    Optional<?> second = (Optional<?>) ((Optional<?>) list.get(1)).get();
    List<Object> fixed = List.of(Optional.of("k,1"), Optional.of("x\\"));
    Map<Object, String> m = new HashMap<>();
    m.put(Optional.of("z,z"), "v");
    Set<Object> set = new HashSet<>(list);
    return list + "|" + first.get() + "|" + second.get() + "|" + ((Optional<?>) list.get(2)).get()
        + "|" + ((Optional<?>) fixed.get(0)).get() + ((Optional<?>) fixed.get(1)).get()
        + "|" + m.get(Optional.of("z,z")) + m.containsKey(Optional.of("z,z")) + m.keySet()
        + "|" + list.contains(Optional.of("a,b")) + list.indexOf(Optional.of("é,\\ü"))
        + set.contains(Optional.of(Optional.of("x\\,y")))
        + "|" + Arrays.asList(Optional.of("a,b")).contains(Optional.of("a,b"));
  }

  /** A runtime division keeps its own NaN bits. */
  static double div(double a, double b) { return a / b; }

  static double neg(double a) { return -a; }

  /** OptionalDouble compares with Double.compare: every NaN equal, -0.0 not 0.0. */
  public static String nan() {
    double runtimeNan = div(0.0, 0.0);
    double payloadNan = neg(Double.NaN);
    Set<Object> set = new HashSet<>();
    set.add(OptionalDouble.of(Double.NaN));
    set.add(OptionalDouble.of(runtimeNan));
    set.add(OptionalDouble.of(payloadNan));
    return "" + OptionalDouble.of(runtimeNan).equals(OptionalDouble.of(Double.NaN))
        + OptionalDouble.of(payloadNan).equals(OptionalDouble.of(Double.NaN))
        + OptionalDouble.of(payloadNan).hashCode() + "," + OptionalDouble.of(0.0).equals(OptionalDouble.of(-0.0))
        + OptionalDouble.of(-0.0).hashCode() + "," + set.size() + Optional.of(runtimeNan).equals(Optional.of(Double.NaN))
        + Optional.of(0.0).equals(Optional.of(-0.0)) + "," + OptionalDouble.of(payloadNan)
        + Double.isNaN(OptionalDouble.of(payloadNan).getAsDouble());
  }

  static final class Node {
    final String name;
    Node(String name) { this.name = name; }
    @Override public int hashCode() { return 42; }
    @Override public boolean equals(Object o) { return o instanceof Node && ((Node) o).name.equals(name); }
    @Override public String toString() { return "Node(" + name + ")"; }
  }

  /** An optional holding a jar object uses that object's own hashCode, equals and toString. */
  public static String jarHeld() {
    Optional<Node> a = Optional.of(new Node("a"));
    return a.hashCode() + "," + a.equals(Optional.of(new Node("a"))) + a.equals(Optional.of(new Node("b")))
        + "," + a + "," + Objects.hash(a) + "," + Optional.of(List.of(1, 2)).hashCode()
        + "," + Optional.of(new ArrayList<>(List.of(1, 2))).hashCode() + "," + a.toString().length()
        + "," + new ArrayList<>(List.of(Optional.of(new Node("b")), a)).indexOf(Optional.of(new Node("a")))
        + List.of(a).contains(Optional.of(new Node("a"))) + List.of(a).equals(List.of(Optional.of(new Node("a"))));
  }

  /** A HashSet finds an optional holding a jar object by the object's own hashCode and equals. */
  public static String jarHeldSet() {
    Set<Object> set = new HashSet<>();
    set.add(Optional.of(new Node("a")));
    set.add(Optional.of(new Node("a")));
    return set.size() + "" + set.contains(Optional.of(new Node("a")));
  }

  static String castString(Object o) {
    try { return "ok" + ((String) o).length(); } catch (ClassCastException e) { return e.getMessage(); }
  }
  static String castOptional(Object o) {
    try { return "ok" + ((Optional<?>) o).isPresent(); } catch (ClassCastException e) { return e.getMessage(); }
  }
  static String castOptionalInt(Object o) {
    try { return "ok" + ((OptionalInt) o).isPresent(); } catch (ClassCastException e) { return e.getMessage(); }
  }
  static String castComparable(Object o) {
    try { return "ok" + (((Comparable<?>) o) == null); } catch (ClassCastException e) { return e.getMessage(); }
  }

  /** A handle is its class: casts throw the JDK's ClassCastException, instanceof agrees. */
  public static String casts() {
    Object e = Optional.empty();
    Object i = OptionalInt.of(1);
    return castString(e) + "|" + castOptional("optional") + "|" + castOptional(i) + "|" + castOptionalInt(i)
        + "|" + castOptional(e) + "|" + castComparable(e) + "|" + castOptionalInt(OptionalDouble.empty())
        + "|" + (e instanceof Optional) + (e instanceof Object) + (e instanceof Comparable) + (i instanceof Optional)
        + (i instanceof OptionalInt) + "|" + e.getClass().getName() + i.getClass().getName();
  }

  /** IdentityHashMap keys by identity: two equal OptionalInt.of(5) are two keys. */
  public static String identityMap() {
    Map<Object, Integer> m = new IdentityHashMap<>();
    m.put(OptionalInt.of(5), 1);
    m.put(OptionalInt.of(5), 2);
    m.put(OptionalInt.empty(), 3);
    m.put(OptionalInt.empty(), 4);
    return "" + m.size();
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
        .args(["--release", "17", "-nowarn", "-encoding", "UTF-8", "-d"])
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
        // Keys with separators, escapes and non-ASCII text read back whole
        // from lists, maps and sets, nested or not (one encoder/decoder).
        (
            "nested",
            "[Optional[a,b], Optional[Optional[x\\,y]], Optional[é,\\ü]]|a,b|x\\,y|é,\\ü|k,1x\\|vtrue[Optional[z,z]]|true2true|true",
        ),
        // `Double.compare`: a runtime `NaN`, a payload `NaN` and `Double.NaN`
        // are one value; `-0.0` is not `0.0`.
        (
            "nan",
            "truetrue2146959360,false-2147483648,1truefalse,OptionalDouble[NaN]true",
        ),
        // A held jar object's own `hashCode` (42), `equals` and `toString`,
        // also inside a host list (`list_equality.rs`).
        (
            "jarHeld",
            "42,truefalse,Optional[Node(a)],73,994,994,17,1truetrue",
        ),
        // ... and inside a `HashSet` (`hash_collections.rs`).
        ("jarHeldSet", "1true"),
        // A handle is its class: `checkcast` throws Java 25's exception.
        (
            "casts",
            "class java.util.Optional cannot be cast to class java.lang.String (java.util.Optional and java.lang.String are in module java.base of loader 'bootstrap')|class java.lang.String cannot be cast to class java.util.Optional (java.lang.String and java.util.Optional are in module java.base of loader 'bootstrap')|class java.util.OptionalInt cannot be cast to class java.util.Optional (java.util.OptionalInt and java.util.Optional are in module java.base of loader 'bootstrap')|oktrue|okfalse|class java.util.Optional cannot be cast to class java.lang.Comparable (java.util.Optional and java.lang.Comparable are in module java.base of loader 'bootstrap')|class java.util.OptionalDouble cannot be cast to class java.util.OptionalInt (java.util.OptionalDouble and java.util.OptionalInt are in module java.base of loader 'bootstrap')|truetruefalsefalsetrue|java.util.Optionaljava.util.OptionalInt",
        ),
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

/// An `IdentityHashMap` keys by identity: `OptionalInt.of(5)` twice is two
/// keys (Java 25: size `3`). Two equal handles of a kind that is not
/// canonical may be one object or two, so the put is counted, never merged
/// as equal keys silently.
#[test]
fn identity_map_with_two_equal_optionals_is_counted() {
    let jar = jar("identity-map");
    let (_, unknown) = run(&jar, "identityMap");
    assert!(
        unknown
            .iter()
            .any(|(call, _)| call.starts_with("java/util/") && call.contains("#put(")),
        "{unknown:?}"
    );
}
