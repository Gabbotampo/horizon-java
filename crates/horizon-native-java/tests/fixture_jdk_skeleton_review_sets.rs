//! The independent review's cases for #119, as JDK 25 differential tests:
//! `AbstractSet.equals` catches a jar subclass of `ClassCastException` or
//! `NullPointerException` thrown by `containsAll` (answering `false`) and
//! propagates any other jar exception as itself, and a jar entry set equals
//! a `HashMap` entry set by asking the jar entries for their key and value.
//!
//! Each expected line is OpenJDK 25.0.5's output for the same probe method
//! (`support/jdk_differential.rs`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
#[path = "support/jdk_differential.rs"]
mod jdk_differential;

fn check(cases: &[(&str, &str)]) {
    jdk_differential::check("jdk-skeleton-review-119", PROBE, cases);
}

const PROBE: &str = r#"package demo;

import java.util.AbstractSet;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;

/// The independent review's #119 cases, rewritten as probe methods.
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

    /// A set over an array whose `contains` logs and can throw.
    static final class Picky extends AbstractSet<Object> {
        final Object[] items;
        RuntimeException throwInContains;
        Picky(Object... items) { this.items = items.clone(); }
        @Override public int size() { log("sz"); return items.length; }
        @Override public Iterator<Object> iterator() {
            log("it");
            return new Iterator<>() {
                int next;
                @Override public boolean hasNext() { return next < items.length; }
                @Override public Object next() { return items[next++]; }
            };
        }
        @Override public boolean contains(Object o) {
            log("ct" + (o instanceof Map.Entry ? "entry" : o));
            if (throwInContains != null) throw throwInContains;
            for (Object item : items) if (o.equals(item)) return true;
            return false;
        }
    }

    /// A jar `Map.Entry` whose accessors log.
    static final class Pair implements Map.Entry<Object, Object> {
        final Object key;
        final Object value;
        Pair(Object key, Object value) { this.key = key; this.value = value; }
        @Override public Object getKey() { log("gk" + key); return key; }
        @Override public Object getValue() { log("gv" + value); return value; }
        @Override public Object setValue(Object value) { throw new UnsupportedOperationException(); }
    }

    static String equalsThrowing(RuntimeException error) {
        Picky picky = new Picky("x", "y");
        picky.throwInContains = error;
        try {
            return take(picky.equals(new LinkedHashSet<>(List.of("x", "y"))));
        } catch (RuntimeException e) {
            return take(e.getClass().getName() + ":" + e.getMessage());
        }
    }

    public static String equalsCatchesJarCce() { return equalsThrowing(new MyCCE("mycce")); }

    public static String equalsCatchesJarNpe() { return equalsThrowing(new MyNPE("mynpe")); }

    public static String equalsPropagatesJarIse() { return equalsThrowing(new MyISE("ise")); }

    public static String entrySetEqualsHashMapEntrySet() {
        HashMap<Object, Object> map = new HashMap<>();
        map.put(1, "a");
        Picky entries = new Picky(new Pair(1, "a"));
        return take(entries.equals(map.entrySet()));
    }

    static final NullPointerException SAVED = new NullPointerException();

    /// A jar NPE that `AbstractSet.equals` swallows must not come back from
    /// a later, unrelated host-raised NPE.
    public static String swallowedThrowStaysSwallowed() {
        Picky picky = new Picky("x", "y");
        picky.throwInContains = SAVED;
        boolean equal = picky.equals(new LinkedHashSet<>(List.of("x", "y")));
        try {
            java.util.Objects.requireNonNull(null);
            return take("no");
        } catch (NullPointerException e) {
            return take(equal + "|" + (e == SAVED) + "|" + e.getMessage());
        }
    }

    public static String[] all() {
        return new String[] {"equalsCatchesJarCce", "equalsCatchesJarNpe", "equalsPropagatesJarIse",
                "entrySetEqualsHashMapEntrySet", "swallowedThrowStaysSwallowed"};
    }

    public static void main(String[] args) throws Exception {
        for (String name : all()) System.out.println(name + "\t" + Probe.class.getMethod(name).invoke(null));
    }
}
"#;

#[test]
fn review_119_runs_as_jdk_25_does() {
    check(&[
        ("equalsCatchesJarCce", "sz ctx => false"),
        ("equalsCatchesJarNpe", "sz ctx => false"),
        ("equalsPropagatesJarIse", "sz ctx => demo.Probe$MyISE:ise"),
        (
            "entrySetEqualsHashMapEntrySet",
            "sz ctentry gk1 gva => true",
        ),
        ("swallowedThrowStaysSwallowed", "sz ctx => false|false|null"),
    ]);
}
