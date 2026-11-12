//! Reflection on the running jar's fields from a library the jar bundles (a
//! linked jar), against the JDK's own answers.
//!
//! Config libraries a mod bundles (emlib, sewlia config, TXF config in the
//! census) reflect over the mod's own config class. The caller is then
//! another jar's class, and whether `setAccessible` and the access check pass
//! depends on modules: Fabric puts both jars in Knot's unnamed module; FML
//! gives a jar without `module-info.class` an automatic module, open and
//! exported in every package.
//!
//! The `demo/` and `lib/` classes below were compiled with `javac --release
//! 21`, packed as `demo.jar` and `lib.jar`, and run with `java` (OpenJDK 27)
//! twice: on the class path (both jars in the unnamed module, Fabric's case)
//! and on the module path as automatic modules `demo` and `lib` (FML's case
//! for jars without a descriptor). Both runs printed the lines in `EXPECTED`
//! verbatim. No expected value is derived from the runtime.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

const CONFIG: &str = r##"package demo;

import java.util.ArrayList;
import java.util.List;

public class Config {
    private int secret = 5;
    private static String name = "cfg";
    public static boolean enabled = true;
    public static int level = 3;
    private List<String> tags = new ArrayList<>();
    private Integer boxed = 1;
}
"##;

const PROBE: &str = r##"package demo;

public final class Probe {
    public static String readPublic() throws Exception {
        return lib.Loader.readPublic();
    }

    public static String overridden() throws Exception {
        return lib.Loader.overridden(new Config());
    }

    public static String stores() throws Exception {
        return lib.Loader.stores(new Config());
    }

    public static String unchecked() throws Exception {
        return lib.Loader.unchecked(new Config());
    }
}
"##;

const LOADER: &str = r##"package lib;

import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;

/** A config library a mod bundles, reflecting over the mod's own class. */
public final class Loader {
    static String failure(Throwable error) {
        return error.getClass().getSimpleName() + ":" + error.getMessage();
    }

    static Class<?> config() throws Exception {
        return Class.forName("demo.Config");
    }

    /** Public static fields of a public class, read without setAccessible. */
    public static String readPublic() throws Exception {
        Class<?> type = config();
        Field enabled = type.getField("enabled");
        Field level = type.getField("level");
        StringBuilder out = new StringBuilder();
        out.append(enabled.getBoolean(null)).append(',').append(level.get(null)).append(',')
            .append(enabled.canAccess(null)).append(',');
        level.setInt(null, 4);
        out.append(level.getInt(null));
        return out.toString();
    }

    /** Private fields made accessible by the library. */
    public static String overridden(Object config) throws Exception {
        Class<?> type = config();
        Field secret = type.getDeclaredField("secret");
        StringBuilder out = new StringBuilder();
        out.append(secret.canAccess(config)).append(',').append(secret.trySetAccessible())
            .append(',').append(secret.canAccess(config)).append(',');
        out.append(secret.getInt(config)).append(',');
        secret.set(config, 8);
        out.append(secret.get(config)).append('|');
        Field name = type.getDeclaredField("name");
        name.setAccessible(true);
        out.append(name.get(null)).append(',');
        name.set(null, "renamed");
        out.append(name.get(null));
        return out.toString();
    }

    /** Reference stores: JDK collections, strings and boxes into typed fields. */
    public static String stores(Object config) throws Exception {
        Class<?> type = config();
        Field tags = type.getDeclaredField("tags");
        Field name = type.getDeclaredField("name");
        Field boxed = type.getDeclaredField("boxed");
        tags.setAccessible(true);
        name.setAccessible(true);
        boxed.setAccessible(true);
        StringBuilder out = new StringBuilder();
        List<String> list = new ArrayList<>();
        list.add("a");
        tags.set(config, list);
        out.append(((List<?>) tags.get(config)).size()).append(',');
        tags.set(config, List.of("x", "y"));
        out.append(((List<?>) tags.get(config)).size()).append('|');
        Object[] wrong = {"text", new HashSet<String>(), Integer.valueOf(3)};
        for (Object value : wrong) {
            try {
                tags.set(config, value);
                out.append("set");
            } catch (Exception error) {
                out.append(failure(error));
            }
            out.append('|');
        }
        try {
            name.set(null, Integer.valueOf(3));
        } catch (Exception error) {
            out.append(failure(error));
        }
        out.append('|');
        try {
            boxed.set(config, "seven");
        } catch (Exception error) {
            out.append(failure(error));
        }
        return out.toString();
    }

    /** Counted: a private field read without setAccessible from another jar. */
    public static String unchecked(Object config) throws Exception {
        try {
            return String.valueOf(config().getDeclaredField("secret").get(config));
        } catch (Exception error) {
            return failure(error);
        }
    }
}
"##;

/// `demo.Probe.<method>()` on OpenJDK 27, the same on the class path and as
/// automatic modules.
const EXPECTED: &[(&str, &str)] = &[
    ("readPublic", r#"true,3,true,4"#),
    ("overridden", r#"false,true,true,5,8|cfg,renamed"#),
    (
        "stores",
        r#"1,2|IllegalArgumentException:Can not set java.util.List field demo.Config.tags to java.lang.String|IllegalArgumentException:Can not set java.util.List field demo.Config.tags to java.util.HashSet|IllegalArgumentException:Can not set java.util.List field demo.Config.tags to java.lang.Integer|IllegalArgumentException:Can not set static java.lang.String field demo.Config.name to java.lang.Integer|IllegalArgumentException:Can not set java.lang.Integer field demo.Config.boxed to java.lang.String"#,
    ),
];

fn fixture(test: &str, extra: &[(&str, &[u8])]) -> javac_fixture::Fixture {
    javac_fixture::Fixture::build_with_lib(
        test,
        &[
            ("demo/Config.java", CONFIG),
            ("demo/Probe.java", PROBE),
            ("lib/Loader.java", LOADER),
        ],
        extra,
    )
}

/// Run `String demo.Probe.<method>()` with `lib.jar` linked beside the probe
/// jar, and return its text and the host's unknown calls.
fn run(fixture: &javac_fixture::Fixture, method: &str) -> (String, Vec<(String, usize)>) {
    let mut host = fixture.jar.seed_host(StdHost::new());
    host.link_jar("probe-lib", fixture.lib.as_ref().unwrap())
        .unwrap();
    let text = match fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
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

fn names(unknown: &[(String, usize)]) -> Vec<&str> {
    unknown.iter().map(|(name, _)| name.as_str()).collect()
}

/// Every oracle line, answered with no unknown host call left: the probe jar
/// has no `module-info.class`, so its module is open to the library under
/// both loaders.
#[test]
fn bundled_library_reflection_matches_the_jdk() {
    let fixture = fixture("reflect-linked", &[]);
    for (method, text) in EXPECTED {
        let (answer, unknown) = run(&fixture, method);
        assert_eq!(answer, *text, "Probe.{method}");
        assert!(unknown.is_empty(), "Probe.{method} counted {unknown:?}");
    }
}

/// Counted, not answered: a private field read from the library without
/// `setAccessible`. OpenJDK 25 throws `IllegalAccessException`, whose text
/// names the modules only when they are named: `class lib.Loader cannot
/// access a member of class demo.Config with modifiers "private"` on the class
/// path, `class lib.Loader (in module lib) cannot access a member of class
/// demo.Config (in module demo) with modifiers "private"` as automatic modules
/// (OpenJDK 27 ends both `with private access`).
#[test]
fn bundled_library_failed_access_is_counted() {
    let fixture = fixture("reflect-linked-unchecked", &[]);
    let (_, unknown) = run(&fixture, "unchecked");
    assert!(
        names(&unknown)
            .contains(&"java/lang/reflect/Field#get(Ljava/lang/Object;)Ljava/lang/Object;"),
        "get not counted: {unknown:?}"
    );
}

/// Counted, not answered: with a `module-info.class` in the probe jar, FML
/// makes it an explicit module, open to another module only where its
/// descriptor says so (`module demo { exports demo; }` denies `setAccessible`
/// with `InaccessibleObjectException: Unable to make field private int
/// demo.Config.secret accessible: module demo does not "opens demo" to module
/// lib`), while Fabric still passes it. The runtime reads only the entry's
/// name, so a placeholder stands in for a compiled descriptor.
#[test]
fn bundled_library_against_a_module_descriptor_is_counted() {
    let fixture = fixture(
        "reflect-linked-module",
        &[("module-info.class", b"placeholder")],
    );
    let (_, unknown) = run(&fixture, "overridden");
    let names = names(&unknown);
    for counted in [
        "java/lang/reflect/Field#trySetAccessible()Z",
        "java/lang/reflect/Field#setAccessible(Z)V",
    ] {
        assert!(names.contains(&counted), "{counted} not counted: {names:?}");
    }
}
