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

import java.util.HashMap;
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

    public static String[] all() {
        return new String[] {"hashMapEntry", "treeMapEntry", "linkedEntryWithNulls", "mapEntryFactory", "jarKeyAndValue",
                "concatenatedAndAppended"};
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
    ]);
}
