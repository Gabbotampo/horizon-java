//! `AccessibleObject#setAccessible` / `trySetAccessible` / `isAccessible`, each
//! `Field` copy's own `override`, and `Field#get`/`set` through it, run in
//! the interpreter against the JDK's own answers.
//!
//! The `demo/` classes below were compiled with `javac --release 21` and run
//! with `java` (OpenJDK 27) through a `main` that printed each method's
//! result (not committed); the lines are pasted verbatim into `EXPECTED`. No
//! expected value is derived from the runtime. The caller, `demo.sub.Outside`,
//! is in another package than `demo.Holder`, so every private member needs
//! `override`.
//!
//! The counted cases each have a test. Their JDK 27 answers are in each
//! test's doc: the runtime cannot decide them, so they must be counted, not
//! answered.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

const HOLDER: &str = r##"package demo;

public class Holder {
    private int secret = 7;
    private static String name = "holder";
    private final int fixed = 3;
    private static final long LIMIT = 40L;
    public int open = 1;
    Holder link;

    private int hidden() {
        return secret;
    }

    private Holder(int secret) {
        this.secret = secret;
    }

    public Holder() {
    }
}
"##;

const OUTSIDE: &str = r##"package demo.sub;

import demo.Holder;
import java.lang.reflect.AccessibleObject;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.ArrayList;
import java.util.List;

public final class Outside {
    static String failure(Throwable error) {
        return error.getClass().getSimpleName() + ":" + error.getMessage();
    }

    /** Two copies of one field: distinct objects, equal, each with its own flag. */
    public static String copies() throws Exception {
        Holder holder = new Holder();
        Field a = Holder.class.getDeclaredField("secret");
        Field b = Holder.class.getDeclaredField("secret");
        StringBuilder out = new StringBuilder();
        out.append(a == b).append(',').append(a.equals(b)).append(',')
            .append(a.hashCode() == b.hashCode()).append(',').append(a.hashCode()).append(',')
            .append(a.equals(Holder.class.getDeclaredField("open"))).append(',')
            .append(a.equals(null)).append('|');
        out.append(a.isAccessible()).append(',').append(a.canAccess(holder)).append(',');
        a.setAccessible(true);
        out.append(a.isAccessible()).append(',').append(a.canAccess(holder)).append(',')
            .append(b.isAccessible()).append(',').append(b.canAccess(holder)).append('|');
        out.append(b.trySetAccessible()).append(',').append(b.isAccessible()).append(',')
            .append(b.trySetAccessible()).append(',');
        b.setAccessible(false);
        out.append(b.isAccessible()).append(',').append(a.isAccessible()).append(',')
            .append(b.canAccess(holder));
        return out.toString();
    }

    /** Reads and writes through an overridden copy, from another package. */
    public static String overridden() throws Exception {
        Holder holder = new Holder();
        StringBuilder out = new StringBuilder();
        Field secret = Holder.class.getDeclaredField("secret");
        secret.setAccessible(true);
        out.append(secret.getInt(holder)).append(',').append(secret.get(holder)).append(',');
        secret.set(holder, 9);
        out.append(secret.getInt(holder)).append(',');
        secret.setInt(holder, 11);
        out.append(secret.get(holder)).append('|');
        Field name = Holder.class.getDeclaredField("name");
        name.setAccessible(true);
        out.append(name.get(null)).append(',');
        name.set(null, "renamed");
        out.append(name.get(new Holder())).append('|');
        Field fixed = Holder.class.getDeclaredField("fixed");
        fixed.setAccessible(true);
        out.append(fixed.getInt(holder)).append('|');
        Field limit = Holder.class.getDeclaredField("LIMIT");
        limit.setAccessible(true);
        out.append(limit.getLong(null)).append(',');
        try {
            limit.set(null, 41L);
            out.append("set");
        } catch (Exception error) {
            out.append(failure(error));
        }
        out.append(',');
        try {
            limit.setLong(null, 41L);
            out.append("set");
        } catch (Exception error) {
            out.append(failure(error));
        }
        return out.toString();
    }

    /** An instance field read or written with a null object after setAccessible. */
    public static String nullObject() throws Exception {
        Field secret = Holder.class.getDeclaredField("secret");
        secret.setAccessible(true);
        StringBuilder out = new StringBuilder();
        String[] names = {"get", "getInt", "getLong", "set", "setInt", "setLong"};
        for (int index = 0; index < names.length; index++) {
            try {
                switch (index) {
                    case 0: secret.get(null); break;
                    case 1: secret.getInt(null); break;
                    case 2: secret.getLong(null); break;
                    case 3: secret.set(null, 5); break;
                    case 4: secret.setInt(null, 5); break;
                    default: secret.setLong(null, 5L); break;
                }
                out.append(names[index]).append(":ok");
            } catch (Exception error) {
                out.append(names[index]).append(':').append(failure(error));
            }
            out.append(index < names.length - 1 ? "|" : "");
        }
        return out.toString();
    }

    /** The config-loader loop: every declared instance field, made accessible and read. */
    public static String declaredLoop() throws Exception {
        Holder holder = new Holder();
        StringBuilder out = new StringBuilder();
        for (Field field : Holder.class.getDeclaredFields()) {
            if (Modifier.isStatic(field.getModifiers())) {
                continue;
            }
            field.setAccessible(true);
            out.append(field.getName()).append('=').append(field.get(holder)).append(';');
        }
        return out.toString();
    }

    /** Copies kept in a list: equality lookups, and reads after every copy was made accessible. */
    public static String listed() throws Exception {
        Holder holder = new Holder();
        List<Field> fields = new ArrayList<>();
        for (Field field : Holder.class.getDeclaredFields()) {
            if (!Modifier.isStatic(field.getModifiers())) {
                field.setAccessible(true);
                fields.add(field);
            }
        }
        StringBuilder out = new StringBuilder();
        Field probe = Holder.class.getDeclaredField("fixed");
        out.append(fields.contains(probe)).append(',').append(fields.indexOf(probe)).append(',')
            .append(probe.isAccessible()).append('|');
        probe.setAccessible(true);
        for (Field field : fields) {
            out.append(field.getName()).append('=').append(field.get(holder)).append(';');
        }
        return out.toString();
    }

    /** The static array form checks every element before it sets any. */
    public static String array() throws Exception {
        Holder holder = new Holder();
        Field secret = Holder.class.getDeclaredField("secret");
        Field fixed = Holder.class.getDeclaredField("fixed");
        AccessibleObject.setAccessible(new AccessibleObject[] {secret, fixed}, true);
        StringBuilder out = new StringBuilder();
        out.append(secret.isAccessible()).append(',').append(fixed.isAccessible()).append(',')
            .append(secret.getInt(holder) + fixed.getInt(holder)).append(',');
        AccessibleObject.setAccessible(new AccessibleObject[] {secret}, false);
        out.append(secret.isAccessible()).append(',').append(fixed.isAccessible());
        return out.toString();
    }

    /** Methods and constructors: the check passes for the jar's own classes. */
    public static String executables() throws Exception {
        Method hidden = Holder.class.getDeclaredMethod("hidden");
        Constructor<Holder> made = Holder.class.getDeclaredConstructor(int.class);
        StringBuilder out = new StringBuilder();
        out.append(hidden.isAccessible()).append(',').append(made.isAccessible()).append(',');
        hidden.setAccessible(true);
        made.setAccessible(true);
        out.append(Holder.class.getDeclaredMethod("hidden").trySetAccessible()).append(',')
            .append(Holder.class.getDeclaredConstructor(int.class).trySetAccessible());
        return out.toString();
    }

    /** Counted: a private field read from another package without setAccessible. */
    public static String uncheckedGet() throws Exception {
        Field secret = Holder.class.getDeclaredField("secret");
        try {
            return String.valueOf(secret.get(new Holder()));
        } catch (Exception error) {
            return failure(error);
        }
    }

    /** Counted: a read through a field taken back out of a list whose copies disagree. */
    public static String mixedCopies() throws Exception {
        List<Field> fields = new ArrayList<>();
        Field first = Holder.class.getDeclaredField("secret");
        first.setAccessible(true);
        fields.add(first);
        fields.add(Holder.class.getDeclaredField("secret"));
        try {
            return String.valueOf(fields.get(0).get(new Holder()));
        } catch (Exception error) {
            return failure(error);
        }
    }

    /** Counted: setAccessible on a JDK class's private field (the launch's --add-opens decide). */
    public static String jdkField() throws Exception {
        try {
            String.class.getDeclaredField("hash").setAccessible(true);
            return "set";
        } catch (Exception error) {
            return error.getClass().getSimpleName();
        }
    }

    /** Counted: a final instance field written through set() after setAccessible (JEP 500). */
    public static String finalWrite() throws Exception {
        Holder holder = new Holder();
        Field fixed = Holder.class.getDeclaredField("fixed");
        fixed.setAccessible(true);
        try {
            fixed.set(holder, 4);
            return String.valueOf(fixed.get(holder));
        } catch (Exception error) {
            return failure(error);
        }
    }

    /** Counted: isAccessible on a Method after setAccessible (Method objects are not told apart). */
    public static String methodFlag() throws Exception {
        Method hidden = Holder.class.getDeclaredMethod("hidden");
        hidden.setAccessible(true);
        return String.valueOf(Holder.class.getDeclaredMethod("hidden").isAccessible());
    }
}
"##;

/// `demo.sub.Outside.<method>()` on OpenJDK 27.
const EXPECTED: &[(&str, &str)] = &[
    (
        "copies",
        r#"false,true,true,1902428807,false,false|false,false,true,true,false,false|true,true,true,false,true,false"#,
    ),
    (
        "overridden",
        r#"7,7,9,11|holder,renamed|3|40,IllegalAccessException:Can not set static final long field demo.Holder.LIMIT to java.lang.Long,IllegalAccessException:Can not set static final long field demo.Holder.LIMIT to (long)41"#,
    ),
    (
        "nullObject",
        r#"get:NullPointerException:null|getInt:NullPointerException:null|getLong:NullPointerException:null|set:NullPointerException:Cannot invoke "Object.getClass()" because "o" is null|setInt:NullPointerException:null|setLong:IllegalArgumentException:Can not set int field demo.Holder.secret to (long)5"#,
    ),
    ("declaredLoop", r#"secret=7;fixed=3;open=1;link=null;"#),
    (
        "listed",
        r#"true,1,false|secret=7;fixed=3;open=1;link=null;"#,
    ),
    ("array", r#"true,true,10,false,true"#),
    ("executables", r#"false,false,true,true"#),
];

fn fixture(test: &str) -> javac_fixture::Fixture {
    javac_fixture::Fixture::build(
        test,
        &[
            ("demo/Holder.java", HOLDER),
            ("demo/sub/Outside.java", OUTSIDE),
        ],
    )
}

/// Run `String demo.sub.Outside.<method>()` on a fresh host and return its
/// text and the host's unknown calls.
fn run(fixture: &javac_fixture::Fixture, method: &str) -> (String, Vec<(String, usize)>) {
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = match fixture
        .jar
        .invoke_method_with_args(
            "demo/sub/Outside",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .unwrap_or_else(|error| panic!("{method} failed: {error}"))
    {
        Value::Ref(Some(text)) => text,
        other => panic!("{method} did not return a string: {other:?}"),
    };
    (text, host.unknown_host_calls())
}

fn counted(fixture: &javac_fixture::Fixture, method: &str, call: &str) {
    let (_, unknown) = run(fixture, method);
    let names: Vec<&str> = unknown.iter().map(|(name, _)| name.as_str()).collect();
    assert!(
        names.contains(&call),
        "{method}: {call} not counted: {names:?}"
    );
}

/// Every oracle line, answered with no unknown host call left.
#[test]
fn set_accessible_matches_the_jdk() {
    let fixture = fixture("set-accessible");
    for (method, text) in EXPECTED {
        let (answer, unknown) = run(&fixture, method);
        assert_eq!(answer, *text, "Outside.{method}");
        assert!(unknown.is_empty(), "Outside.{method} counted {unknown:?}");
    }
}

/// Counted, not answered: `Field#get` on a private field from another
/// package without `setAccessible`. OpenJDK 25 throws `IllegalAccessException:
/// class demo.sub.Outside cannot access a member of class demo.Holder with
/// modifiers "private"` (Fabric's unnamed module; NeoForge's text names the
/// modules; OpenJDK 27 ends it `with private access`). This read used to be
/// answered `7` without any check.
#[test]
fn failed_access_get_is_counted() {
    let fixture = fixture("set-accessible-unchecked");
    counted(
        &fixture,
        "uncheckedGet",
        "java/lang/reflect/Field#get(Ljava/lang/Object;)Ljava/lang/Object;",
    );
}

/// Counted, not answered: a field read back out of a list holding one copy
/// with `override` and one without. OpenJDK 27 answers `7` (the first copy);
/// the list keeps fields by `Field.equals`, so the runtime cannot tell which
/// copy it hands back.
#[test]
fn read_through_disagreeing_copies_is_counted() {
    let fixture = fixture("set-accessible-mixed");
    counted(
        &fixture,
        "mixedCopies",
        "java/lang/reflect/Field#get(Ljava/lang/Object;)Ljava/lang/Object;",
    );
}

/// Counted, not answered: `setAccessible(true)` on a JDK class's private
/// field. OpenJDK 27 throws `InaccessibleObjectException` (`module java.base
/// does not "opens java.lang" to unnamed module @<hash>`) unless the launch
/// adds `--add-opens`.
#[test]
fn jdk_field_set_accessible_is_counted() {
    let fixture = fixture("set-accessible-jdk");
    counted(
        &fixture,
        "jdkField",
        "java/lang/reflect/Field#setAccessible(Z)V",
    );
}

/// Counted, not answered: `set` on a final instance field after
/// `setAccessible(true)` goes through JDK 27's final-field mutation policy
/// (OpenJDK 27 with default flags writes `4` and prints a warning; with
/// `--illegal-final-field-mutation=deny` it throws).
#[test]
fn final_field_set_after_set_accessible_is_counted() {
    let fixture = fixture("set-accessible-final");
    counted(
        &fixture,
        "finalWrite",
        "java/lang/reflect/Field#set(Ljava/lang/Object;Ljava/lang/Object;)V",
    );
}

/// Counted, not answered: `Method` markers do not tell copies apart, so
/// `isAccessible` after any `setAccessible` on that method is unknown
/// (OpenJDK 27 answers `false` for the fresh copy).
#[test]
fn method_flag_after_set_accessible_is_counted() {
    let fixture = fixture("set-accessible-method");
    counted(
        &fixture,
        "methodFlag",
        "java/lang/reflect/Method#isAccessible()Z",
    );
}
