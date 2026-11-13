//! The independent review's cases for #122, as JDK 25 differential tests:
//! `Map.forEach`/`replaceAll` throw `ConcurrentModificationException(cause)`
//! with the entry's `IllegalStateException` as its cause (the jar's own
//! object when the jar threw it), `SimpleEntry` equals host entries both ways,
//! `AbstractMap.equals` catches a jar `ClassCastException`/`NullPointerException`
//! subclass and propagates any other, and `values()` equals itself by identity.
//!
//! Each expected line is OpenJDK 25.0.5's output for the same probe method
//! (`support/jdk_differential.rs`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
#[path = "support/jdk_differential.rs"]
mod jdk_differential;

fn check(cases: &[(&str, &str)]) {
    jdk_differential::check("jdk-skeleton-review-122", PROBE, cases);
}

const PROBE: &str = r#"package demo;

import java.util.AbstractMap;
import java.util.AbstractSet;
import java.util.Collection;
import java.util.ConcurrentModificationException;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;
import java.util.Set;

/// The independent review's #122 cases, rewritten as probe methods.
public final class Probe {
    static final StringBuilder LOG = new StringBuilder();

    static void log(String event) { LOG.append(event).append(' '); }

    static String take(Object result) {
        String text = LOG.toString().trim() + " => " + result;
        LOG.setLength(0);
        return text;
    }

    static final class MyCCE extends ClassCastException { MyCCE(String m) { super(m); } }
    static final class MyNPE extends NullPointerException { MyNPE(String m) { super(m); } }
    static final class MyISE extends IllegalStateException { MyISE(String m) { super(m); } }

    /// An entry whose `getValue` can throw.
    static final class Slot implements Map.Entry<Object, Object> {
        final Object key;
        Object value;
        RuntimeException onGet;
        Slot(Object key, Object value) { this.key = key; this.value = value; }
        @Override public Object getKey() { log("gk" + key); return key; }
        @Override public Object getValue() {
            log("gv" + value);
            if (onGet != null) throw onGet;
            return value;
        }
        @Override public Object setValue(Object v) { Object old = value; value = v; return old; }
    }

    /// A map over an array of slots; `get` logs and can throw.
    static final class AM extends AbstractMap<Object, Object> {
        final Slot[] slots;
        RuntimeException onGet;
        AM(Object... pairs) {
            slots = new Slot[pairs.length / 2];
            for (int i = 0; i < slots.length; i++) slots[i] = new Slot(pairs[2 * i], pairs[2 * i + 1]);
        }
        @Override public Object get(Object key) {
            log("G" + key);
            if (onGet != null) throw onGet;
            return super.get(key);
        }
        @Override public Set<Map.Entry<Object, Object>> entrySet() {
            log("es");
            return new AbstractSet<>() {
                @Override public int size() { return slots.length; }
                @Override public Iterator<Map.Entry<Object, Object>> iterator() {
                    return new Iterator<>() {
                        int next;
                        @Override public boolean hasNext() { return next < slots.length; }
                        @Override public Map.Entry<Object, Object> next() { return slots[next++]; }
                    };
                }
            };
        }
    }

    static final MyISE THROWN = new MyISE("mine");

    static String cme(boolean replace, RuntimeException onGet) {
        AM map = new AM(1, "a");
        map.slots[0].onGet = onGet;
        try {
            if (replace) map.replaceAll((k, v) -> v);
            else map.forEach((k, v) -> {});
            return take("no");
        } catch (ConcurrentModificationException e) {
            return take(e.getMessage() + "/" + e.getCause() + "/" + (e.getCause() == THROWN));
        }
    }

    public static String forEachChainsCause() { return cme(false, new IllegalStateException("c")); }

    public static String replaceAllChainsCause() { return cme(true, new IllegalStateException("c")); }

    public static String forEachChainsTheJarsOwnException() { return cme(false, THROWN); }

    public static String simpleEntryVersusHostEntries() {
        HashMap<Object, Object> h = new HashMap<>();
        h.put("k", "a");
        Map.Entry<Object, Object> he = h.entrySet().iterator().next();
        AbstractMap.SimpleEntry<Object, Object> e = new AbstractMap.SimpleEntry<>("k", "a");
        return take(e.equals(he) + "|" + he.equals(e) + "|" + e.equals(Map.entry("k", "a")) + "|"
                + Map.entry("k", "b").equals(new AbstractMap.SimpleEntry<>("k", "b")) + "|"
                + e.equals(Map.entry("k", "z")));
    }

    static String equalsThrowing(RuntimeException error) {
        AM other = new AM(1, "a", 2, null);
        other.onGet = error;
        try {
            return take(new AM(1, "a", 2, null).equals(other));
        } catch (RuntimeException e) {
            return take(e.getClass().getName() + ":" + e.getMessage());
        }
    }

    public static String mapEqualsCatchesJarCce() { return equalsThrowing(new MyCCE("c")); }

    public static String mapEqualsCatchesJarNpe() { return equalsThrowing(new MyNPE("n")); }

    public static String mapEqualsPropagatesJarIse() { return equalsThrowing(new MyISE("i")); }

    public static String valuesEqualsItself() {
        Collection<Object> values = new AM(1, "a").values();
        return take(values.equals(values) + "," + values.equals(new AM(1, "a").values()));
    }

    public static String[] all() {
        return new String[] {"forEachChainsCause", "replaceAllChainsCause", "forEachChainsTheJarsOwnException",
                "simpleEntryVersusHostEntries", "mapEqualsCatchesJarCce", "mapEqualsCatchesJarNpe",
                "mapEqualsPropagatesJarIse", "valuesEqualsItself"};
    }

    public static void main(String[] args) throws Exception {
        for (String name : all()) System.out.println(name + "\t" + Probe.class.getMethod(name).invoke(null));
    }
}
"#;

#[test]
fn review_122_runs_as_jdk_25_does() {
    check(&[
        (
            "forEachChainsCause",
            "es gk1 gva => java.lang.IllegalStateException: c/java.lang.IllegalStateException: c/false",
        ),
        (
            "replaceAllChainsCause",
            "es gk1 gva => java.lang.IllegalStateException: c/java.lang.IllegalStateException: c/false",
        ),
        (
            "forEachChainsTheJarsOwnException",
            "es gk1 gva => demo.Probe$MyISE: mine/demo.Probe$MyISE: mine/true",
        ),
        (
            "simpleEntryVersusHostEntries",
            " => true|true|true|true|false",
        ),
        ("mapEqualsCatchesJarCce", "es es es gk1 gva G1 => false"),
        ("mapEqualsCatchesJarNpe", "es es es gk1 gva G1 => false"),
        (
            "mapEqualsPropagatesJarIse",
            "es es es gk1 gva G1 => demo.Probe$MyISE:i",
        ),
        ("valuesEqualsItself", " => true,false"),
    ]);
}
