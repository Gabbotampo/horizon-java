//! The host's map entries (a `HashMap` or `LinkedHashMap` entry, a `TreeMap`
//! entry, `Map.entry`) are `Map.Entry` to `instanceof` and `checkcast`, as
//! in Java. They answered `false`, so a JDK body's `instanceof Map.Entry`
//! (`SimpleEntry.equals`, `AbstractMap.equals`) rejected every host entry.
//!
//! Each expected line is OpenJDK 25.0.5's output for the same probe method
//! (`support/jdk_differential.rs`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
#[path = "support/jdk_differential.rs"]
mod jdk_differential;

fn check(cases: &[(&str, &str)]) {
    jdk_differential::check("entry-instanceof", PROBE, cases);
}

const PROBE: &str = r#"package demo;

import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.TreeMap;

public final class Probe {
    static String check(Object o) {
        return (o instanceof Map.Entry) + ":" + (o instanceof Iterable) + ":" + ((Map.Entry<?, ?>) o).getKey();
    }

    public static String hashMapEntry() {
        HashMap<Object, Object> map = new HashMap<>();
        map.put("k", 1);
        return check(map.entrySet().iterator().next());
    }

    public static String linkedEntry() {
        LinkedHashMap<Object, Object> map = new LinkedHashMap<>();
        map.put("k", 1);
        return check(map.entrySet().iterator().next());
    }

    public static String treeEntry() {
        TreeMap<Object, Object> map = new TreeMap<>();
        map.put("k", 1);
        return check(map.entrySet().iterator().next());
    }

    public static String mapEntry() { return check(Map.entry("k", 1)); }

    public static String[] all() { return new String[] {"hashMapEntry", "linkedEntry", "treeEntry", "mapEntry"}; }

    public static void main(String[] args) throws Exception {
        for (String name : all()) System.out.println(name + "\t" + Probe.class.getMethod(name).invoke(null));
    }
}
"#;

#[test]
fn host_map_entries_runs_as_jdk_25_does() {
    check(&[
        ("hashMapEntry", "true:false:k"),
        ("linkedEntry", "true:false:k"),
        ("treeEntry", "true:false:k"),
        ("mapEntry", "true:false:k"),
    ]);
}
