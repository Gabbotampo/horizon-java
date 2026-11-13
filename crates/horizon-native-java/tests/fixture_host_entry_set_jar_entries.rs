//! A host map's entry set asked `contains`/`remove` with a jar `Map.Entry`.
//!
//! JDK 25 reads the jar entry's accessors in an order that depends on the
//! map (`HashMap.EntrySet.contains`: `getKey`, then the node's `equals`:
//! `getKey`, `getValue`; `TreeMap`'s: `getValue`, `getKey`; `remove`:
//! `getKey`, `getValue`). The host knows neither the map kind nor how to run
//! the jar entry, and used to answer `false` silently. The call is now
//! counted (`jar_operands.rs`): these tests pin that honesty contract, which
//! is **not JVM behaviour**; JDK 25's answers are quoted on each.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"package demo;

import java.util.HashMap;
import java.util.Map;
import java.util.TreeMap;

public final class Probe {
    static final StringBuilder LOG = new StringBuilder();

    static String take(Object result) {
        String text = LOG.toString().trim() + " => " + result;
        LOG.setLength(0);
        return text;
    }

    /// A jar entry whose accessors log.
    static final class Pair implements Map.Entry<Object, Object> {
        final Object key;
        final Object value;
        Pair(Object key, Object value) { this.key = key; this.value = value; }
        @Override public Object getKey() { LOG.append("gk "); return key; }
        @Override public Object getValue() { LOG.append("gv "); return value; }
        @Override public Object setValue(Object v) { throw new UnsupportedOperationException(); }
    }

    public static String hashMapEntrySetContainsJarEntry() {
        HashMap<Object, Object> map = new HashMap<>();
        map.put("a", "x");
        return take(map.entrySet().contains(new Pair("a", "x")) + "," + map.entrySet().contains(new Pair("a", "y")));
    }

    public static String treeMapEntrySetContainsJarEntry() {
        TreeMap<Object, Object> map = new TreeMap<>();
        map.put("a", "x");
        return take(map.entrySet().contains(new Pair("a", "x")));
    }

    public static String hashMapEntrySetRemoveJarEntry() {
        HashMap<Object, Object> map = new HashMap<>();
        map.put("a", "x");
        return take(map.entrySet().remove(new Pair("a", "x")) + ":" + map.size());
    }

    public static String[] all() {
        return new String[] {"hashMapEntrySetContainsJarEntry", "treeMapEntrySetContainsJarEntry",
                "hashMapEntrySetRemoveJarEntry"};
    }

    public static void main(String[] args) throws Exception {
        for (String name : all()) System.out.println(name + "\t" + Probe.class.getMethod(name).invoke(null));
    }
}
"#;

/// The text `method` returns and the unknown host calls it counted.
fn run(method: &str) -> (String, Vec<(String, usize)>) {
    let fixture = Fixture::build("host-entry-set", &[("demo/Probe.java", PROBE)]);
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = fixture.text_on(&mut host, method);
    (text, host.unknown_host_calls())
}

fn counted(name: &str, times: usize) -> Vec<(String, usize)> {
    vec![(format!("java/util/Set#{name}(Ljava/lang/Object;)Z"), times)]
}

/// JDK 25: `gk gk gv gk gk gv => true,false`. Counted twice, not `false`.
#[test]
fn a_hash_map_entry_set_contains_a_jar_entry_is_counted() {
    let (_, unknown) = run("hashMapEntrySetContainsJarEntry");
    assert_eq!(unknown, counted("contains", 2));
}

/// JDK 25: `gv gk => true`.
#[test]
fn a_tree_map_entry_set_contains_a_jar_entry_is_counted() {
    let (_, unknown) = run("treeMapEntrySetContainsJarEntry");
    assert_eq!(unknown, counted("contains", 1));
}

/// JDK 25: `gk gv => true:0`.
#[test]
fn a_hash_map_entry_set_remove_of_a_jar_entry_is_counted() {
    let (_, unknown) = run("hashMapEntrySetRemoveJarEntry");
    assert_eq!(unknown, counted("remove", 1));
}
