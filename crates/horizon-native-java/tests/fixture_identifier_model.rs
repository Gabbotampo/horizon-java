//! `Identifier` factories and members answer what 26.3 answers.
//!
//! The stubs exist only for `javac`: only `demo/` ships, so the calls reach
//! the host's model as they do for a real mod.

/// The expected strings, one `method=text` line per `Probe` method: the
/// output of `java` (OpenJDK 27) running the same `Probe` against the real
/// 26.3 `server-game.jar` and its libraries, pasted verbatim, not this
/// interpreter's answers.
const EXPECTED: &str = "\
parseDefaults=minecraft:foo|minecraft:foo|a:|minecraft:\n\
rejects=IE:Non [a-z0-9_.-] character in namespace of identifier: Bad Mod:X Y|IE:Non [a-z0-9/._-] character in path of location: ok:UP|IE:Non [a-z0-9/._-] character in path of location: a:X Y|IE:Non [a-z0-9/._-] character in path of location: minecraft:UPPER\n\
tries=null|null|a:x\n\
separators=a:b|a:b|minecraft:x\n\
paths=a:z|a:p/x|a:x/s|IE:Non [a-z0-9/._-] character in path of location: a:Z\n\
text=a:x/y|a:x/y|a:x/y|x/y|a.x/y|p.a.x/y|p.a.x/y.s|a.x/y|x/y|a_x_y\n\
identity=true|3127|false|true\n\
order=true|true|0|true\n\
predicates=true|false|false|true|true\n\
operator=a:block/x|IE:Non [a-z0-9/._-] character in path of location: a:Bad\n\
edges=IE:Non [a-z0-9/._-] character in path of location: a:b:c|IE:Non [a-z0-9_.-] character in namespace of identifier: A:b|null|minecraft:x|NullPointerException\n\
objects=a:x|[a, b]|[1, 2]|{k=1}|Optional[v]|00000000-0000-0001-0000-000000000002|7|a:x|a:x|a:x[a, b]|list/a\n\
builders=[a:x][a:x][q][z]\n\
";

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

const IDENTIFIER_STUB: &str = r#"package net.minecraft.resources;

import java.util.function.UnaryOperator;

public final class Identifier implements Comparable<Identifier> {
    public static Identifier fromNamespaceAndPath(String n, String p) { return null; }
    public static Identifier parse(String id) { return null; }
    public static Identifier withDefaultNamespace(String p) { return null; }
    public static Identifier tryParse(String id) { return null; }
    public static Identifier tryBuild(String n, String p) { return null; }
    public static Identifier bySeparator(String id, char s) { return null; }
    public static Identifier tryBySeparator(String id, char s) { return null; }
    public static boolean isAllowedInIdentifier(char c) { return false; }
    public static boolean isValidPath(String p) { return false; }
    public static boolean isValidNamespace(String n) { return false; }
    public static boolean validPathChar(char c) { return false; }
    public Identifier withPath(String p) { return null; }
    public Identifier withPath(UnaryOperator<String> m) { return null; }
    public Identifier withPrefix(String p) { return null; }
    public Identifier withSuffix(String s) { return null; }
    public String toDebugFileName() { return null; }
    public String toLanguageKey() { return null; }
    public String toShortLanguageKey() { return null; }
    public String toShortString() { return null; }
    public String toLanguageKey(String p) { return null; }
    public String toLanguageKey(String p, String s) { return null; }
    public int compareTo(Identifier o) { return 0; }
    @Override public int hashCode() { return 0; }
    @Override public boolean equals(Object o) { return false; }
}
"#;

const EXCEPTION_STUB: &str = r#"package net.minecraft;

public class IdentifierException extends RuntimeException {
    public IdentifierException(String message) { super(message); }
}
"#;

const PROBE: &str = r##"package demo;

import java.util.function.UnaryOperator;
import net.minecraft.resources.Identifier;

public final class Probe {
    static String thrown(Runnable check) {
        try {
            check.run();
            return "ok";
        } catch (net.minecraft.IdentifierException e) {
            return "IE:" + e.getMessage();
        } catch (Throwable t) {
            return t.getClass().getSimpleName();
        }
    }

    public static String parseDefaults() {
        return Identifier.parse(":foo") + "|" + Identifier.parse("foo") + "|"
            + Identifier.parse("a:") + "|" + Identifier.parse("");
    }

    public static String rejects() {
        return thrown(() -> Identifier.parse("Bad Mod:X Y")) + "|"
            + thrown(() -> Identifier.parse("ok:UP")) + "|"
            + thrown(() -> Identifier.fromNamespaceAndPath("a", "X Y")) + "|"
            + thrown(() -> Identifier.withDefaultNamespace("UPPER"));
    }

    public static String tries() {
        return Identifier.tryParse("Bad Mod:X Y") + "|" + Identifier.tryBuild("a", "X Y")
            + "|" + Identifier.tryParse("a:x");
    }

    public static String separators() {
        return Identifier.bySeparator("a/b", '/') + "|" + Identifier.tryBySeparator("a/b", '/')
            + "|" + Identifier.bySeparator("/x", '/');
    }

    public static String paths() {
        Identifier id = Identifier.parse("a:x");
        return id.withPath("z") + "|" + id.withPrefix("p/") + "|"
            + id.withSuffix("/s") + "|" + thrown(() -> id.withPath("Z"));
    }

    public static String text() {
        Identifier mod = Identifier.parse("a:x/y");
        Identifier mc = Identifier.parse("minecraft:x/y");
        return "" + mod + "|" + new StringBuilder().append(mod) + "|" + mod.toShortString()
            + "|" + mc.toShortString() + "|" + mod.toLanguageKey() + "|"
            + mod.toLanguageKey("p") + "|" + mod.toLanguageKey("p", "s") + "|"
            + mod.toShortLanguageKey() + "|" + mc.toShortLanguageKey() + "|"
            + Identifier.parse("a:x/y").toDebugFileName();
    }

    public static String identity() {
        Identifier a = Identifier.parse("a:x");
        Identifier b = Identifier.parse("a:x");
        return a.equals(b) + "|" + a.hashCode() + "|" + a.equals(null) + "|"
            + Identifier.parse("minecraft:").equals(Identifier.parse(""));
    }

    public static String order() {
        Identifier x = Identifier.parse("b:a");
        Identifier y = Identifier.parse("a:b");
        Identifier samePath = Identifier.parse("b:a");
        return (x.compareTo(y) < 0) + "|" + (y.compareTo(x) > 0) + "|"
            + x.compareTo(samePath) + "|"
            + (Identifier.parse("a:a").compareTo(Identifier.parse("b:a")) < 0);
    }

    public static String operator() {
        Identifier id = Identifier.parse("a:x");
        return id.withPath(p -> "block/" + p) + "|" + thrown(() -> id.withPath(p -> "Bad"));
    }

    public static String edges() {
        return thrown(() -> Identifier.parse("a:b:c")) + "|"
            + thrown(() -> Identifier.bySeparator("A/b", '/')) + "|"
            + Identifier.tryBySeparator("a/B", '/') + "|" + Identifier.tryParse("x") + "|"
            + thrown(() -> Identifier.parse("a:x").compareTo(null));
    }

    public static String objects() {
        Object id = Identifier.parse("a:x");
        java.util.List<String> list = java.util.List.of("a", "b");
        java.util.List<Integer> grown = new java.util.ArrayList<>();
        grown.add(1);
        grown.add(2);
        java.util.Map<String, Integer> map = new java.util.HashMap<>();
        map.put("k", 1);
        java.util.Optional<String> some = java.util.Optional.of("v");
        java.util.UUID uuid = java.util.UUID.fromString("00000000-0000-0001-0000-000000000002");
        Object box = Integer.valueOf(7);
        String looksLikeMarker = "list/a";
        return "" + id + "|" + list + "|" + grown + "|" + map + "|" + some + "|" + uuid + "|" + box + "|" + String.valueOf(id)
            + "|" + java.util.Objects.toString(id) + "|"
            + new StringBuilder().append(id).append(list) + "|" + looksLikeMarker;
    }

    public static String builders() {
        Identifier id = Identifier.parse("a:x");
        StringBuilder b = new StringBuilder();
        b.append(id);
        StringBuilder c = new StringBuilder().append("q");
        return "[" + b.toString() + "][" + b + "][" + c + "][" + new StringBuilder().append("z") + "]";
    }

    public static String predicates() {
        return Identifier.isValidNamespace("minecraft") + "|" + Identifier.isValidNamespace("..")
            + "|" + Identifier.isValidPath("A") + "|" + Identifier.isAllowedInIdentifier(':')
            + "|" + Identifier.validPathChar('/');
    }
}
"##;

fn check(methods: &[&str]) {
    let fixture = javac_fixture::Fixture::build(
        "identifier",
        &[
            ("net/minecraft/resources/Identifier.java", IDENTIFIER_STUB),
            ("net/minecraft/IdentifierException.java", EXCEPTION_STUB),
            ("demo/Probe.java", PROBE),
        ],
    );
    for (method, expected) in methods.iter().map(|m| (*m, expected(m))) {
        assert_eq!(fixture.text(method), expected, "{method}");
    }
}

/// The JVM's line for `method` in the module doc above.
fn expected(method: &str) -> &'static str {
    EXPECTED
        .lines()
        .find_map(|line| line.strip_prefix(method)?.strip_prefix('='))
        .unwrap_or_else(|| panic!("no expected line for {method}"))
}

#[test]
fn factories_validate_and_default_the_namespace_like_vanilla() {
    check(&[
        "parseDefaults",
        "rejects",
        "tries",
        "separators",
        "edges",
        "predicates",
    ]);
}

#[test]
fn path_operations_validate_the_new_path_and_run_the_operator() {
    check(&["paths", "operator"]);
}

#[test]
fn text_identity_and_order_follow_the_namespace_and_path() {
    check(&["text", "identity", "order"]);
}

#[test]
fn model_objects_and_builders_concatenate_as_their_to_string() {
    // An `Object`-typed id, collections, an optional and a UUID print what
    // `String.valueOf` prints; a `String` that looks like a marker stays the
    // text it is; javac lowers `"" + builder` to `String.valueOf(builder)`.
    check(&["objects", "builders"]);
}
