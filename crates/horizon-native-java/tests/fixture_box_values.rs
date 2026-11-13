//! Boxes are their own value, so a `String` spelled like one stays a `String`.
//!
//! The runtime used to hold a boxed `Integer`/`Short`/`Byte`/`Character`/
//! `Boolean` as text (`integer:5`, `boolean:true`), so a real `String` with
//! that text was read as the box: `"" + "integer:5"` printed `5`,
//! `"boolean:true"` was an `instanceof Boolean`, and `"true"` formatted under
//! `%b` was refused as ambiguous. Every expected string is what this class
//! printed on Java 25 (OpenJDK 25.0.5, `javac --release 17`, run twice).

use horizon_native_java::{NativeJar, StdHost, Value};
use std::{fs, io::Write, path::PathBuf, process::Command};
use zip::write::SimpleFileOptions;

const SOURCE: &str = r#"
package demo;

public final class Spelled {
  /** Strings spelled like the runtime's old box markers are Strings. */
  public static String text() {
    String a = "integer:5", b = "integer:05", t = "boolean:true", c = "character:97", d = "double:1.0";
    Object o = a;
    return "" + a + "|" + String.valueOf((Object) c) + "|" + new StringBuilder().append((Object) d)
        + "|" + t.length() + "|" + a.equals(b) + "|" + (a == b);
  }

  public static String types() {
    Object a = "integer:5", t = "true", f = "boolean:false";
    return (a instanceof Integer) + "|" + (a instanceof String) + "|" + (t instanceof Boolean)
        + "|" + (f instanceof Boolean) + "|" + a.getClass().getName() + "|" + t.getClass().getName();
  }

  public static String format() {
    return String.format("%s|%b|%s|%b", "false", "false", "integer:7", "boolean:false");
  }

  /** A String spelled like a box hashes as a String; a box as its class does. */
  public static String hashes() {
    Object t = "true", b = "boolean:true", i = "integer:7";
    return t.hashCode() + "|" + b.hashCode() + "|" + i.hashCode() + "|" + Boolean.TRUE.hashCode() + "|"
        + Character.valueOf('a').hashCode() + "|" + Short.valueOf((short) -3).hashCode() + "|"
        + java.util.Objects.hash("true", Boolean.FALSE, 7) + "|" + java.util.Objects.equals("integer:7", Integer.valueOf(7));
  }

  /** Real boxes still print, class and compare as boxes. */
  public static String boxes() {
    Object i = Integer.valueOf(5), s = Short.valueOf((short) -2), c = Character.valueOf('a'), b = Boolean.TRUE;
    Integer x = 7;
    return "" + i + s + c + b + "|" + i.getClass().getName() + "|" + c.getClass().getName() + "|"
        + (b instanceof Boolean) + (i instanceof Number) + (c instanceof Number) + "|" + (x + 1) + "|"
        + i.equals(5) + "|" + String.format("%s|%b|%c", i, b, c);
  }
}
"#;

/// Boxes through `Optional`, `Object.toString`, `Float` against `Double`,
/// boxes of different classes, `IntStream.boxed()` keys and map entries.
const BOXES2: &str = r#"
package demo;

import java.util.*;
import java.util.function.Function;
import java.util.stream.*;

public final class Boxes2 {
  static double dbl = 2.5;

  /** Boxes through Optional come back as the same boxes. */
  public static String optionals() {
    Optional<Integer> o = Optional.of(5);
    Map<String, Integer> map = new HashMap<>();
    map.put("a", 3);
    Object got = Optional.of(5).get();
    return (o.get() + 1) + "|" + got.getClass().getName() + "|" + Optional.of(5).orElse(7) + "|"
        + Optional.of('a').get() + "|" + Optional.of('a') + "|" + (Optional.of(true).get() ? "y" : "n") + "|"
        + Optional.of(5).filter(x -> x > 3).isPresent() + "|"
        + Optional.ofNullable(map.get("a")).map(v -> v + 1).orElse(0) + "|"
        + Stream.of(4, 1, 3).max(Integer::compare).get() + "|"
        + ((Object) Stream.of(4, 1, 3).max(Integer::compare).get()).getClass().getName() + "|"
        + Optional.of(5).equals(Optional.of("BI5")) + "|" + Optional.of(5).equals(Optional.of(5)) + "|"
        + Optional.of(5L).get() + "|" + Optional.of(1.5) + "|" + Optional.of(5).hashCode();
  }

  /** Object.toString on a box is the box's own toString. */
  public static String boxText() {
    Function<Object, String> f = Object::toString;
    Object o = 5;
    return Stream.of(1, 2).map(Object::toString).collect(Collectors.joining(",")) + "|"
        + Stream.of(true, false).map(Object::toString).collect(Collectors.joining(",")) + "|"
        + Stream.of((short) 3).map(Object::toString).collect(Collectors.joining(",")) + "|"
        + Stream.of('a', 'b').map(Object::toString).collect(Collectors.joining()) + "|"
        + f.apply(5) + f.apply('c') + f.apply(true) + o.toString() + "|" + f.apply(2.5f) + f.apply(2.5) + f.apply(7L);
  }

  /** Float and Double are different boxes. */
  public static String floatsAndDoubles() throws Exception {
    Object d = 1.0, fl = 1.0f;
    Map<Object, String> m = new HashMap<>();
    m.put(5.0, "d");
    m.put(5f, "f");
    Object field = Boxes2.class.getDeclaredField("dbl").get(null);
    Float a = 0.1f;
    Double b = 0.1;
    return d.getClass().getName() + "|" + fl.getClass().getName() + "|" + ((Object) 1.0f).equals(1.0) + "|"
        + ((Object) 1.0).equals(1.0f) + "|" + ("" + (Object) 0.1f) + "|" + m.size() + "|"
        + field.getClass().getName() + "|" + a.hashCode() + "|" + b.hashCode() + "|" + a + "|" + b + "|"
        + a.compareTo(0.2f) + "|" + Float.valueOf(Float.NaN).equals(Float.NaN) + "|" + Float.valueOf(0f).equals(-0f)
        + "|" + (fl instanceof Float) + (fl instanceof Double) + (d instanceof Double) + (d instanceof Float);
  }

  /** A box never equals another class's box of the same value. */
  public static String crossEquals() {
    Object i = 5, l = 5L, s = (short) 5, c = 'a', z = true;
    return ((Long) 5L).equals(5) + "|" + ((Integer) 5).equals(5L) + "|" + l.equals(i) + "|" + i.equals(s)
        + "|" + Objects.equals(i, l) + "|" + Objects.equals(c, 97) + "|" + z.equals(1) + "|" + ((Double) 5.0).equals(5);
  }

  /** IntStream.boxed() Integers and valueOf Integers are one key. */
  public static String boxedKeys() {
    Map<Integer, String> m = new HashMap<>();
    IntStream.range(0, 3).boxed().forEach(i -> m.put(i, "v" + i));
    List<Integer> list = IntStream.range(0, 3).boxed().collect(Collectors.toList());
    Set<Integer> set = new HashSet<>(list);
    return m.get(1) + "|" + m.containsKey(0) + "|" + list.get(1).equals(Integer.valueOf(1)) + "|"
        + Integer.valueOf(1).equals(list.get(1)) + "|" + set.contains(2) + "|" + list.indexOf(2) + "|"
        + Objects.equals(list.get(0), 0) + "|" + list.get(2).hashCode();
  }

  public static String entries() {
    Map<Integer, String> m = new LinkedHashMap<>();
    m.put(1, "a");
    return Map.entry(1, "a").toString() + "|" + m.entrySet().iterator().next() + "|" + Map.entry('c', 2L);
  }
}
"#;

/// Reflection's boxing, `Byte` boxes, `NaN` keys, entry hashes and
/// floating-point narrowing.
const BOXES3: &str = r#"
package demo;

import java.util.*;
import java.util.stream.*;

public final class Boxes3 {
  static float rf() { return 1.5f; }
  static char rc() { return 'q'; }
  static boolean rz() { return true; }
  static short rs() { return 3; }
  static byte rb() { return -2; }
  static double rd() { return 2.5; }
  static void rv() {}

  /** Reflection boxes a primitive by its declared type. */
  public static String reflection() throws Exception {
    StringBuilder out = new StringBuilder();
    for (String name : new String[] {"rf", "rc", "rz", "rs", "rb", "rd"}) {
      Object v = Boxes3.class.getDeclaredMethod(name).invoke(null);
      out.append(v).append(v.getClass().getSimpleName()).append(',');
    }
    out.append(Boxes3.class.getDeclaredMethod("rf").invoke(null).equals(1.5f)).append(',');
    out.append(Boxes3.class.getDeclaredMethod("rv").invoke(null)).append('|');
    Object[] arrays = {new float[] {1.5f}, new char[] {'x'}, new boolean[] {true}, new short[] {4}, new byte[] {5}, new double[] {0.5}, new int[] {6}, new long[] {7}};
    for (Object array : arrays) {
      Object v = java.lang.reflect.Array.get(array, 0);
      out.append(v).append(v.getClass().getSimpleName()).append(',');
    }
    return out.toString();
  }

  /** Every byte is a cached Byte. */
  public static String bytes() {
    Byte b = 5;
    Object[] a = {(byte) 4};
    byte by = 1;
    by++;
    Byte c = by, d = 2;
    return b + "|" + a[0].getClass().getSimpleName() + "|" + Optional.of((byte) 1).get() + "|" + (c == d) + "|"
        + c.equals(d) + "|" + (b + 1) + "|" + Byte.valueOf((byte) -128).hashCode();
  }

  /** NaN is one key: Double/Float.equals compare canonical bits. */
  public static String nanKeys() {
    Set<Double> s = new HashSet<>();
    s.add(Double.NaN);
    s.add(Double.NaN);
    Map<Double, String> m = new HashMap<>();
    m.put(Double.NaN, "a");
    m.put(Double.NaN, "b");
    return s.size() + "|" + m.size() + "|" + m.get(Double.NaN) + "|"
        + new HashSet<>(Arrays.asList(Float.NaN, Float.NaN)).size() + "|" + new HashSet<>(List.of(Double.NaN)).contains(Double.NaN)
        + "|" + Stream.of(Double.NaN, Double.NaN).distinct().count();
  }

  public static String entryHashes() {
    Map<Integer, String> t = new TreeMap<>();
    t.put(3, "b");
    return Map.entry(1.5f, 'c').hashCode() + "|" + Map.entry(1, "a").hashCode() + "|"
        + t.entrySet().iterator().next().hashCode();
  }

  public static String narrowing() {
    Float f = 3.99e10f;
    Double d = -3.99e10;
    Float n = Float.NaN;
    return ((Number) f).shortValue() + "," + ((Number) f).byteValue() + "," + f.shortValue() + "," + f.byteValue()
        + "," + d.shortValue() + "," + d.byteValue() + "," + n.shortValue() + "," + ((Number) 1e300).byteValue()
        + "," + Long.valueOf(70000L).shortValue();
  }
}
"#;

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

/// Compile `source` (class `demo.<class>`) into a jar of its own.
fn jar(class: &str, source_text: &str) -> NativeJar {
    let root = std::env::temp_dir().join(format!(
        "horizon-native-java-boxes-{class}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let source = root.join(format!("src/demo/{class}.java"));
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::create_dir_all(root.join("classes")).unwrap();
    fs::write(&source, source_text).unwrap();
    let status = Command::new(javac())
        .args(["--release", "17", "-nowarn", "-d"])
        .arg(root.join("classes"))
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");
    let jar_path = root.join("boxes.jar");
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

#[test]
fn strings_spelled_like_boxes_are_strings_and_boxes_are_boxes() {
    let jar = jar("Spelled", SOURCE);
    for (name, expected) in [
        ("text", "integer:5|character:97|double:1.0|12|false|false"),
        (
            "types",
            "false|true|false|false|java.lang.String|java.lang.String",
        ),
        ("format", "false|true|integer:7|true"),
        (
            "hashes",
            "3569038|-1072581216|492450043|1231|97|-3|-865053633|false",
        ),
        (
            "boxes",
            "5-2atrue|java.lang.Integer|java.lang.Character|truetruefalse|8|true|5|true|a",
        ),
    ] {
        let mut host = StdHost::new();
        let value = jar
            .invoke_method_with_args(
                "demo.Spelled",
                name,
                "()Ljava/lang/String;",
                None,
                &[],
                &mut host,
            )
            .unwrap_or_else(|error| panic!("{name} failed: {error}"));
        assert_eq!(value, Value::Ref(Some(expected.into())), "{name}");
        assert_eq!(
            host.unknown_host_calls(),
            Vec::<(String, usize)>::new(),
            "{name}"
        );
    }
}

/// Every expected string is what `Boxes2` printed on Java 25 (OpenJDK
/// 25.0.5, two runs alike); each line was wrong or an error before.
#[test]
fn boxes_keep_their_class_through_optionals_text_keys_and_equality() {
    let jar = jar("Boxes2", BOXES2);
    for (name, expected) in [
        (
            "optionals",
            "6|java.lang.Integer|5|a|Optional[a]|y|true|4|4|java.lang.Integer|false|true|5|Optional[1.5]|5",
        ),
        ("boxText", "1,2|true,false|3|ab|5ctrue5|2.52.57"),
        (
            "floatsAndDoubles",
            "java.lang.Double|java.lang.Float|false|false|0.1|2|java.lang.Double|1036831949|-1507852285|0.1|0.1|-1|true|false|truefalsetruefalse",
        ),
        (
            "crossEquals",
            "false|false|false|false|false|false|false|false",
        ),
        ("boxedKeys", "v1|true|true|true|true|2|true|2"),
        ("entries", "1=a|1=a|c=2"),
    ] {
        let mut host = StdHost::new();
        let value = jar.invoke_method_with_args(
            "demo.Boxes2",
            name,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        assert_eq!(
            value.as_ref().ok(),
            Some(&Value::Ref(Some(expected.into()))),
            "{name}: {value:?}, unknown calls {:?}",
            host.unknown_host_calls()
        );
    }
}

/// Every expected string is what `Boxes3` printed on Java 25 (OpenJDK
/// 25.0.5, two runs alike).
#[test]
fn reflection_bytes_nan_keys_entry_hashes_and_narrowing_answer_as_java_25() {
    let jar = jar("Boxes3", BOXES3);
    for (name, expected) in [
        (
            "reflection",
            "1.5Float,qCharacter,trueBoolean,3Short,-2Byte,2.5Double,true,null|1.5Float,xCharacter,trueBoolean,4Short,5Byte,0.5Double,6Integer,7Long,",
        ),
        ("bytes", "5|Byte|1|true|true|6|-128"),
        ("nanKeys", "1|1|b|1|true|1"),
        ("entryHashes", "1069547619|96|97"),
        ("narrowing", "-1,-1,-1,-1,0,0,0,-1,4464"),
    ] {
        let mut host = StdHost::new();
        let value = jar.invoke_method_with_args(
            "demo.Boxes3",
            name,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        );
        assert_eq!(
            value.as_ref().ok(),
            Some(&Value::Ref(Some(expected.into()))),
            "{name}: {value:?}, unknown calls {:?}",
            host.unknown_host_calls()
        );
    }
}
