//! Virtual dispatch selects the receiver's own body before any host model
//! (`jvm_dispatch.rs`), in the JVM's order: the receiver's class and its
//! superclasses, a JDK skeleton's own declaration, then a jar interface's
//! default; a package-private method is not overridden from another package.
//!
//! Every expected value is OpenJDK 25.0.5's output for the same `Probe`
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"package demo;

import demo.a.Base;
import demo.b.Sub;
import java.util.AbstractList;
import java.util.AbstractMap;
import java.util.AbstractSet;
import java.util.Collection;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;

public final class Probe {
    /// A set whose `isEmpty` override disagrees with its `size()`, so the
    /// answer shows which body ran.
    static final class Shy extends AbstractSet<String> {
        int asked;
        @Override public Iterator<String> iterator() { return List.of("a", "b").iterator(); }
        @Override public int size() { return 2; }
        @Override public boolean isEmpty() { asked++; return true; }
    }

    /// A map that answers `containsKey` itself.
    static final class OnlyX extends AbstractMap<String, Integer> {
        @Override public Set<Map.Entry<String, Integer>> entrySet() { return Set.of(); }
        @Override public boolean containsKey(Object key) { return "x".equals(key); }
    }

    /// A jar default the JDK superclass does not declare (`getFirst`), and
    /// one it does (`isEmpty`, declared by `AbstractCollection`).
    interface Ends<E> extends List<E> {
        @SuppressWarnings("unchecked")
        @Override default E getFirst() { return (E) "from-default"; }
        @Override default boolean isEmpty() { return true; }
    }

    static final class Pair extends AbstractList<String> implements Ends<String> {
        @Override public String get(int index) { return index == 0 ? "left" : "right"; }
        @Override public int size() { return 2; }
    }

    public static String overrideBeatsTemplate() {
        Shy shy = new Shy();
        Set<String> set = shy;
        Collection<String> collection = shy;
        return set.isEmpty() + "|" + collection.isEmpty() + "|" + shy.isEmpty() + "|" + shy.asked;
    }

    public static String mapOverride() {
        Map<String, Integer> map = new OnlyX();
        return map.containsKey("x") + "|" + map.containsKey("y");
    }

    public static String defaultsAfterSuperclass() {
        List<String> list = new Pair();
        Pair pair = new Pair();
        return list.getFirst() + "|" + list.isEmpty() + "|" + pair.getFirst() + "|" + pair.isEmpty();
    }

    public static String packagePrivate() {
        Sub sub = new Sub();
        return Base.tagOf(sub) + "|" + sub.callTag() + "|" + Base.nameOf(sub);
    }
}
"#;

const BASE: &str = r#"package demo.a;

public class Base {
    String tag() { return "base"; }
    public String name() { return "base-name"; }
    public static String tagOf(Base base) { return base.tag(); }
    public static String nameOf(Base base) { return base.name(); }
}
"#;

const SUB: &str = r#"package demo.b;

public class Sub extends demo.a.Base {
    // Not an override: `Base.tag` is package-private in another package.
    String tag() { return "sub"; }
    @Override public String name() { return "sub-name"; }
    public String callTag() { return tag(); }
}
"#;

fn fixture(test: &str) -> Fixture {
    Fixture::build(
        test,
        &[
            ("demo/Probe.java", PROBE),
            ("demo/a/Base.java", BASE),
            ("demo/b/Sub.java", SUB),
        ],
    )
}

/// Runs `method` and checks that nothing was answered as an unknown host
/// call: each answer here is a jar body or a modelled JDK member.
fn exact(fixture: &Fixture, method: &str) -> String {
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = fixture.text_on(&mut host, method);
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{method}: {:?}",
        host.unknown_host_calls()
    );
    text
}

/// `Set.isEmpty`/`Collection.isEmpty` on a jar `AbstractSet` that overrides
/// it ran `AbstractCollection`'s `size() == 0` template instead.
#[test]
fn an_override_beats_the_skeleton_template() {
    let fixture = fixture("jvm-dispatch-override");
    assert_eq!(exact(&fixture, "overrideBeatsTemplate"), "true|true|true|3");
}

/// `Map.containsKey` on a jar map went to the host's map model.
#[test]
fn a_jar_map_answers_its_own_contains_key() {
    let fixture = fixture("jvm-dispatch-map");
    assert_eq!(exact(&fixture, "mapOverride"), "true|false");
}

/// `AbstractList` does not declare `getFirst`, so the jar interface's default
/// is selected; it does declare (through `AbstractCollection`) `isEmpty`, so
/// the superclass beats the jar default.
#[test]
fn a_jar_default_is_selected_only_when_the_jdk_superclass_does_not_declare_it() {
    let fixture = fixture("jvm-dispatch-defaults");
    assert_eq!(
        exact(&fixture, "defaultsAfterSuperclass"),
        "from-default|false|from-default|false"
    );
}

/// `Sub.tag()` does not override the package-private `Base.tag()`, so a call
/// through `Base` runs `Base`'s body; the public `name()` is overridden.
#[test]
fn a_package_private_method_is_not_overridden_from_another_package() {
    let fixture = fixture("jvm-dispatch-package");
    assert_eq!(exact(&fixture, "packagePrivate"), "base|sub|sub-name");
}
