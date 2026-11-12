//! `ServiceLoader` builds its providers the way JDK 27 does (`service_loader.rs`).
//!
//! The expected text is OpenJDK 27's output for the same classes and
//! `META-INF/services/demo.Api`, run twice: on the class path
//! (`java -cp classes run.Run`, Fabric's Knot) and as an automatic module
//! (`java -p demo.jar --add-modules demo -cp runcp run.Run`, how NeoForge loads a
//! mod). Each `Probe` method below prints one of those lines, and the line is
//! pasted verbatim. The provider order is `Good, NoCtor, Throws, NotApi, Hidden`.
//!
//! The one deliberate difference is in `walk`: the runtime does not carry an
//! exception's cause across frames, so the JVM's `<- cause` suffix is absent
//! there (the message itself is the JDK's).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const API: &str = "package demo;\npublic interface Api { String name(); }\n";
const GOOD: &str = r#"package demo;
public class Good implements Api {
    public static int built;
    private final int serial;
    public Good() { serial = ++built; }
    public String name() { return "good" + serial; }
}
"#;
const NO_CTOR: &str = r#"package demo;
public class NoCtor implements Api {
    public NoCtor(int x) { }
    public String name() { return "noctor"; }
}
"#;
const THROWS: &str = r#"package demo;
public class Throws implements Api {
    public Throws() { throw new IllegalStateException("boom"); }
    public String name() { return "throws"; }
}
"#;
const NOT_API: &str = "package demo;\npublic class NotApi { }\n";
const HIDDEN: &str = r#"package demo;
class Hidden implements Api {
    public Hidden() { }
    public String name() { return "hidden"; }
}
"#;

const PROBE: &str = r#"package demo;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.ServiceLoader;

public final class Probe {
    static String t(Runnable r) {
        try {
            r.run();
            return "ok";
        } catch (Throwable e) {
            return e.getClass().getName() + ": " + e.getMessage();
        }
    }

    /// Seven `next()` calls on one iterator: each failure consumes its provider.
    public static String walk() {
        Good.built = 0;
        Iterator<Api> it = ServiceLoader.load(Api.class).iterator();
        List<String> out = new ArrayList<>();
        for (int i = 0; i < 7; i++) {
            boolean more = it.hasNext();
            out.add(more + ":" + t(() -> it.next().name()));
        }
        return out.toString();
    }

    /// Instances are cached per loader, built afresh by a new loader, and a
    /// `Provider` builds a new one on every `get()`.
    public static String identity() {
        Good.built = 0;
        ServiceLoader<Api> l = ServiceLoader.load(Api.class);
        Api first = l.iterator().next();
        String again = l.iterator().next().name();
        String found = l.findFirst().get().name();
        String fresh = ServiceLoader.load(Api.class).findFirst().get().name();
        ServiceLoader.Provider<Api> p = ServiceLoader.load(Api.class).stream().findFirst().orElseThrow();
        return first.name() + " " + again + " " + found + " " + fresh + " " + (first == l.iterator().next())
            + " " + first.getClass().getName() + " get " + p.get().name() + " " + p.get().name()
            + " type " + p.type().getName() + " built=" + Good.built;
    }

    public static String stream() {
        return t(() -> ServiceLoader.load(Api.class).stream().map(p -> p.type().getName()).toList());
    }

    public static String reload() {
        ServiceLoader<Api> l = ServiceLoader.load(Api.class);
        Iterator<Api> it = l.iterator();
        it.next();
        l.reload();
        return t(() -> it.hasNext()) + " / " + l.iterator().hasNext();
    }
}
"#;

const SERVICES: &[u8] =
    b"demo.Good\ndemo.NoCtor\ndemo.Throws\ndemo.NotApi\ndemo.Hidden\n# comment\n";

fn fixture() -> Fixture {
    Fixture::build_with_lib(
        "service-loader",
        &[
            ("demo/Probe.java", PROBE),
            ("demo/Api.java", API),
            ("demo/Good.java", GOOD),
            ("demo/NoCtor.java", NO_CTOR),
            ("demo/Throws.java", THROWS),
            ("demo/NotApi.java", NOT_API),
            ("demo/Hidden.java", HIDDEN),
        ],
        &[("META-INF/services/demo.Api", SERVICES)],
    )
}

/// Fabric's Knot: the class path.
fn class_path_host(fixture: &Fixture) -> StdHost {
    fixture
        .jar
        .seed_host(StdHost::new())
        .with_fabric_entrypoint("main", "demo.Probe", "probe-fixture")
}

/// NeoForge: the mod is an automatic named module. The fixture jar is a
/// Fabric jar, so its rows are seeded by hand instead of by `seed_host`
/// (which marks the run as Fabric's).
fn module_host() -> StdHost {
    [
        "demo/Good",
        "demo/NoCtor",
        "demo/Throws",
        "demo/NotApi",
        "demo/Hidden",
    ]
    .into_iter()
    .fold(StdHost::new(), |host, provider| {
        host.with_service_provider("demo/Api", provider)
    })
}

const SCE: &str = "java.util.ServiceConfigurationError: demo.Api:";

#[test]
fn class_path_iteration_builds_each_provider_and_reports_each_failure() {
    let fixture = fixture();
    let mut host = class_path_host(&fixture);
    assert_eq!(
        fixture.text_on(&mut host, "walk"),
        format!(
            "[true:ok, \
             true:{SCE} demo.NoCtor Unable to get public no-arg constructor, \
             true:{SCE} Provider demo.Throws could not be instantiated, \
             true:{SCE} demo.NotApi not a subtype, \
             true:{SCE} Provider demo.Hidden could not be instantiated, \
             false:java.util.NoSuchElementException: null, \
             false:java.util.NoSuchElementException: null]"
        )
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );
}

#[test]
fn a_module_refuses_a_non_public_provider_in_the_lookup() {
    let fixture = fixture();
    let mut host = module_host();
    assert_eq!(
        fixture.text_on(&mut host, "walk"),
        format!(
            "[true:ok, \
             true:{SCE} demo.NoCtor Unable to get public no-arg constructor, \
             true:{SCE} Provider demo.Throws could not be instantiated, \
             true:{SCE} demo.NotApi not a subtype, \
             true:{SCE} class demo.Hidden is not public, \
             false:java.util.NoSuchElementException: null, \
             false:java.util.NoSuchElementException: null]"
        )
    );
}

#[test]
fn providers_are_cached_per_loader_and_built_per_get() {
    let fixture = fixture();
    let mut host = class_path_host(&fixture);
    // OpenJDK 27: one loader iterated twice and `findFirst` share `good1`; a
    // new loader builds `good2`; each `Provider.get` builds another.
    assert_eq!(
        fixture.text_on(&mut host, "identity"),
        "good1 good1 good1 good2 true demo.Good get good3 good4 type demo.Good built=4"
    );
    assert_eq!(
        fixture.text_on(&mut host, "stream"),
        format!("{SCE} demo.NoCtor Unable to get public no-arg constructor")
    );
    assert_eq!(
        fixture.text_on(&mut host, "reload"),
        "java.util.ConcurrentModificationException: null / true"
    );
}
