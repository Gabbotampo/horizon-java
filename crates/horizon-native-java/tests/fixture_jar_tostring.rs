//! A jar object's text is its own `toString()`, on every path that prints it
//! (#69): `toString()` itself, string concatenation, `String.valueOf`,
//! `Objects.toString`, `StringBuilder`/`StringBuffer.append(Object)`,
//! `PrintStream.println(Object)`, `String.format("%s")` and a record's
//! generated `toString`.
//!
//! The expected strings are what this same source printed on OpenJDK 27
//! (`javac --release 21`, `java -cp`). `Object.toString()`'s identity hash is
//! not reproducible, so those cases pin the runtime's contract instead: the
//! call is counted as an unknown `Object#toString`, and the object's marker
//! never reaches the text. A jar object inside a host collection is counted
//! the same way (the host cannot run jar code), which is also the runtime's
//! contract, not JVM behaviour.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};
use javac_fixture::Fixture;

const SOURCE: &str = r#"
package demo;

import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;

public final class Probe {
  enum Mode { A, B }

  enum Fancy {
    X, Y;

    @Override
    public String toString() {
      return "fancy-" + name().toLowerCase(Locale.ROOT);
    }
  }

  enum Op {
    PLUS {
      @Override
      int apply(int a, int b) { return a + b; }
    },
    TIMES {
      @Override
      int apply(int a, int b) { return a * b; }

      @Override
      public String toString() { return "*"; }
    };

    abstract int apply(int a, int b);
  }

  record Point(int x, int y) {}

  record Tagged(String tag, Mode mode, Fancy fancy) {}

  static final class Named {
    final String n;
    Named(String n) { this.n = n; }
    @Override
    public String toString() { return "named:" + n; }
  }

  static class Base {
    @Override
    public String toString() { return "base"; }
  }

  static final class Sub extends Base {}

  static final class Plain {}

  static final class Wrapped {
    @Override
    public String toString() { return "w:" + super.toString(); }
  }

  public static String enumConcatObject() { Object m = Mode.B; return "" + m; }
  public static String enumConcatTyped() { Mode m = Mode.B; return "[" + m + "]"; }
  public static String enumValueOf() { Object m = Mode.B; return String.valueOf(m); }
  public static String enumBuilder() { Object m = Mode.B; return new StringBuilder().append(m).append('|').append(Mode.A).toString(); }
  public static String enumBuffer() { return new StringBuffer().append((Object) Mode.A).toString(); }
  public static String enumToString() { Object m = Mode.B; return m.toString(); }
  public static String enumTypedToString() { return Mode.A.toString(); }
  public static String enumObjectsToString() { return Objects.toString(Mode.B) + "," + Objects.toString(null, "dflt") + "," + Objects.toString(Mode.A, "dflt"); }
  public static String enumFormat() { return String.format("%s/%S/%4s", Mode.B, Mode.A, Mode.B); }
  public static String overrideConcat() { Object f = Fancy.Y; return f + "|" + Fancy.X.toString() + "|" + String.valueOf(f) + "|" + Fancy.X.name(); }
  public static String overrideBuilder() { return new StringBuilder().append(Fancy.X).toString(); }
  public static String overrideFormat() { return String.format("%s", Fancy.Y); }
  public static String constantBody() { return Op.PLUS + "," + Op.TIMES + "," + Op.PLUS.name() + "," + Op.TIMES.name() + "," + Op.TIMES.apply(3, 4); }
  public static String recordText() { return "" + new Point(1, -2); }
  public static String recordWithEnum() { return new Tagged("t", Mode.A, Fancy.Y).toString(); }
  public static String namedText() { Object n = new Named("x"); return n + "|" + String.valueOf(n) + "|" + new StringBuilder().append(n) + "|" + Objects.toString(n) + "|" + String.format("%s", n); }
  public static String inherited() { Object s = new Sub(); return "" + s + "," + s.toString(); }
  public static String switchOnName() {
    Object m = Mode.B;
    switch (m.toString()) {
      case "A": return "was A";
      case "B": return "was B";
      default: return "other";
    }
  }
  public static String printed() { System.out.println(Mode.B); System.out.println((Object) Fancy.X); System.out.print(new Named("p")); return "done"; }
  public static String inList() { return "" + List.of(Mode.A, Fancy.X); }
  public static String plainConcat() { return "" + new Plain(); }
  public static String plainToString() { return new Plain().toString(); }
  public static String plainBuilder() { return new StringBuilder().append(new Plain()).toString(); }
  public static String plainValueOf() { return String.valueOf(new Plain()); }
  public static String plainFormat() { return String.format("%s", new Plain()); }
  public static String bareObject() { return "" + new Object(); }
  public static String superToString() { return new Wrapped().toString(); }
}
"#;

const OBJECT_TO_STRING: &str = "java/lang/Object#toString()Ljava/lang/String;";

struct Run {
    value: Value,
    unknown: Vec<(String, usize)>,
    stdout: String,
}

fn run(fixture: &Fixture, method: &str) -> Run {
    let mut host = fixture.jar.seed_host(StdHost::new());
    let value = fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap_or_else(|error| panic!("{method} failed: {error}"));
    Run {
        value,
        unknown: host.unknown_host_calls(),
        stdout: host.take_stdout(),
    }
}

fn fixture(test: &str) -> Fixture {
    Fixture::build(test, &[("demo/Probe.java", SOURCE)])
}

/// Each method's text is the JVM's, and nothing was counted.
fn assert_exact(fixture: &Fixture, cases: &[(&str, &str)]) {
    for (method, expected) in cases {
        let run = run(fixture, method);
        assert_eq!(
            run.value,
            Value::Ref(Some((*expected).to_owned())),
            "{method}"
        );
        assert!(run.unknown.is_empty(), "{method}: {:?}", run.unknown);
    }
}

#[test]
fn enum_constants_print_their_name_on_every_path() {
    let fixture = fixture("tostring-enum");
    assert_exact(
        &fixture,
        &[
            ("enumConcatObject", "B"),
            ("enumConcatTyped", "[B]"),
            ("enumValueOf", "B"),
            ("enumBuilder", "B|A"),
            ("enumBuffer", "A"),
            ("enumToString", "B"),
            ("enumTypedToString", "A"),
            ("enumObjectsToString", "B,dflt,A"),
            ("enumFormat", "B/A/   B"),
            ("switchOnName", "was B"),
        ],
    );
}

#[test]
fn overrides_run_their_own_body() {
    let fixture = fixture("tostring-override");
    assert_exact(
        &fixture,
        &[
            ("overrideConcat", "fancy-y|fancy-x|fancy-y|X"),
            ("overrideBuilder", "fancy-x"),
            ("overrideFormat", "fancy-y"),
            // A constant with a body is an instance of its own subclass.
            ("constantBody", "PLUS,*,PLUS,TIMES,12"),
            ("namedText", "named:x|named:x|named:x|named:x|named:x"),
            ("inherited", "base,base"),
        ],
    );
}

#[test]
fn records_print_their_simple_name_and_component_text() {
    let fixture = fixture("tostring-record");
    assert_exact(
        &fixture,
        &[
            ("recordText", "Point[x=1, y=-2]"),
            ("recordWithEnum", "Tagged[tag=t, mode=A, fancy=fancy-y]"),
        ],
    );
}

#[test]
fn println_of_an_object_writes_its_text() {
    let fixture = fixture("tostring-println");
    let run = run(&fixture, "printed");
    assert_eq!(run.value, Value::Ref(Some("done".into())));
    assert_eq!(run.stdout, "B\nfancy-x\nnamed:p");
    assert!(run.unknown.is_empty(), "{:?}", run.unknown);
}

/// `Object.toString()` is `Class.getName() + "@" + identity hash`: the JVM
/// printed `demo.Probe$Plain@330bedb4`, a different hash every run. The
/// runtime counts the call and never prints the object's marker instead.
#[test]
fn identity_hash_text_is_counted_not_invented() {
    let fixture = fixture("tostring-identity");
    for method in [
        "plainConcat",
        "plainToString",
        "plainBuilder",
        "plainValueOf",
        "plainFormat",
        "bareObject",
        "superToString",
    ] {
        let run = run(&fixture, method);
        assert!(
            run.unknown
                .iter()
                .any(|(call, count)| call == OBJECT_TO_STRING && *count == 1),
            "{method} was not counted: {:?}",
            run.unknown
        );
        if let Value::Ref(Some(text)) = &run.value {
            assert!(!text.contains('#'), "{method} printed a marker: {text}");
        }
    }
}

/// The host prints a collection's elements and cannot run jar code: a jar
/// element is counted (the JVM printed `[A, fancy-x]`). Runtime contract, not
/// JVM behaviour.
#[test]
fn jar_objects_inside_host_collections_are_counted() {
    let fixture = fixture("tostring-collection");
    let run = run(&fixture, "inList");
    assert!(
        run.unknown
            .iter()
            .any(|(call, count)| call == OBJECT_TO_STRING && *count == 2),
        "{:?}",
        run.unknown
    );
}
