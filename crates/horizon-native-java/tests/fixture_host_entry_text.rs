//! The host's map entries print as JDK 25 prints them (`host_entry.rs`):
//! `String.valueOf(key) + "=" + String.valueOf(value)`, through `toString()`,
//! string concatenation, `StringBuilder.append` and `String.valueOf`, with a
//! jar key or value printed by its own `toString`. They printed the host's
//! internal encoding.
//!
//! Each expected line is OpenJDK 25.0.5's output for the same probe method
//! (`support/jdk_differential.rs`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
#[path = "support/jdk_differential.rs"]
mod jdk_differential;

fn check(cases: &[(&str, &str)]) {
    jdk_differential::check("host-entry-text", PROBE, cases);
}

const PROBE: &str = r#"package demo;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Objects;
import java.util.stream.Collectors;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.TreeMap;

public final class Probe {
    /// A key with its own `toString`.
    static final class Tag {
        final String name;
        Tag(String name) { this.name = name; }
        @Override public String toString() { return "<" + name + ">"; }
    }

    static Map.Entry<Object, Object> first(Map<Object, Object> map) { return map.entrySet().iterator().next(); }

    public static String hashMapEntry() {
        HashMap<Object, Object> map = new HashMap<>();
        map.put("a", 1);
        return first(map).toString();
    }

    public static String treeMapEntry() {
        TreeMap<Object, Object> map = new TreeMap<>();
        map.put("k", 2.5);
        return first(map).toString();
    }

    public static String linkedEntryWithNulls() {
        LinkedHashMap<Object, Object> map = new LinkedHashMap<>();
        map.put(null, null);
        return first(map).toString();
    }

    public static String mapEntryFactory() { return Map.entry(7L, 'c').toString() + "|" + Map.entry(true, "s"); }

    public static String jarKeyAndValue() {
        HashMap<Object, Object> map = new HashMap<>();
        map.put(new Tag("k"), new Tag("v"));
        return first(map) + "|" + Map.entry(new Tag("a"), 3);
    }

    public static String concatenatedAndAppended() {
        HashMap<Object, Object> map = new HashMap<>();
        map.put("x", "y");
        Map.Entry<Object, Object> entry = first(map);
        return "[" + entry + "]" + new StringBuilder().append(entry) + String.valueOf(entry);
    }

    static HashMap<Object, Object> one() {
        HashMap<Object, Object> map = new HashMap<>();
        map.put("a", 1);
        return map;
    }

    public static String entrySetPrinted() {
        HashMap<Object, Object> m = one();
        return m.entrySet().toString() + "|" + String.valueOf(m.entrySet()) + "|" + ("" + m.entrySet()) + "|"
                + Objects.toString(m.entrySet());
    }

    public static String entriesInLists() {
        HashMap<Object, Object> m = one();
        Map.Entry<Object, Object> entry = first(m);
        return List.of(entry) + "|" + new ArrayList<>(m.entrySet()) + "|" + m.entrySet().stream().toList();
    }

    public static String copiedMapsEntrySets() {
        HashMap<Object, Object> m = one();
        return new TreeMap<>(m).entrySet() + "|" + new LinkedHashMap<>(m).entrySet();
    }

    public static String methodReferences() {
        HashMap<Object, Object> m = one();
        return m.entrySet().stream().map(Object::toString).collect(Collectors.joining(",")) + "|"
                + m.entrySet().stream().map(String::valueOf).toList();
    }

    public static String formatted() { return String.format("<%s>", first(one())); }

    /// Strings that only start like the host's entry encoding stay strings.
    public static String stringsSpelledLikeEntries() {
        String a = "map-entry/x";
        String b = "map-entry/";
        return a + "|" + b + "|" + List.of(a, b) + "|" + a.length() + "|" + String.valueOf(b);
    }

    public static String[] all() {
        return new String[] {"hashMapEntry", "treeMapEntry", "linkedEntryWithNulls", "mapEntryFactory", "jarKeyAndValue",
                "concatenatedAndAppended", "entrySetPrinted", "entriesInLists", "copiedMapsEntrySets", "methodReferences", "formatted",
                "stringsSpelledLikeEntries"};
    }

    public static void main(String[] args) throws Exception {
        for (String name : all()) System.out.println(name + "\t" + Probe.class.getMethod(name).invoke(null));
    }
}
"#;

#[test]
fn host_entry_text_runs_as_jdk_25_does() {
    check(&[
        ("hashMapEntry", "a=1"),
        ("treeMapEntry", "k=2.5"),
        ("linkedEntryWithNulls", "null=null"),
        ("mapEntryFactory", "7=c|true=s"),
        ("jarKeyAndValue", "<k>=<v>|<a>=3"),
        ("concatenatedAndAppended", "[x=y]x=yx=y"),
        ("entrySetPrinted", "[a=1]|[a=1]|[a=1]|[a=1]"),
        ("entriesInLists", "[a=1]|[a=1]|[a=1]"),
        ("copiedMapsEntrySets", "[a=1]|[a=1]"),
        ("methodReferences", "a=1|[a=1]"),
        ("formatted", "<a=1>"),
        (
            "stringsSpelledLikeEntries",
            "map-entry/x|map-entry/|[map-entry/x, map-entry/]|11|map-entry/",
        ),
    ]);
}
