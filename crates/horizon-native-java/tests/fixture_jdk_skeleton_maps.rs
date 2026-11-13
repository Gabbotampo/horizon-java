//! Jar subclasses of `AbstractMap` and `AbstractMap.SimpleEntry`/
//! `SimpleImmutableEntry`, and jar maps inheriting the `Map` defaults, run
//! the JDK's bodies exactly (`jdk_skeletons/map.rs`), including the
//! `keySet()`/`values()` views.
//!
//! The probe's classes log every call the JDK makes into them; each expected
//! line is OpenJDK 25.0.5's output for the same probe method
//! (`support/jdk_differential.rs`). Before `jdk_skeletons/map.rs`, these
//! bodies were counted unknown calls answering neutral defaults (#97).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
#[path = "support/jdk_differential.rs"]
mod jdk_differential;

fn check(cases: &[(&str, &str)]) {
    jdk_differential::check("jdk-skeleton-maps", PROBE, cases);
}

const PROBE: &str = r#"package demo;

import java.util.AbstractMap;
import java.util.AbstractSet;
import java.util.Collection;
import java.util.ConcurrentModificationException;
import java.util.Iterator;
import java.util.Map;
import java.util.NoSuchElementException;
import java.util.Set;

public final class Probe {
    static final StringBuilder LOG = new StringBuilder();

    static void log(String event) {
        LOG.append(event).append(' ');
    }

    static String take(Object result) {
        String text = LOG.toString().trim() + " => " + result;
        LOG.setLength(0);
        return text;
    }

    static String thrown(Throwable error) {
        return take(error.getClass().getName() + ":" + error.getMessage());
    }

    /// A key whose equals and hashCode log.
    static final class Key {
        final String name;
        Key(String name) { this.name = name; }
        @Override public boolean equals(Object o) { log("eq" + name); return o instanceof Key k && k.name.equals(name); }
        @Override public int hashCode() { log("hash" + name); return name.length(); }
        @Override public String toString() { return name; }
    }

    /// An entry the map hands out: its accessors log.
    static final class Slot extends AbstractMap.SimpleEntry<Object, Object> {
        Slot(Object key, Object value) { super(key, value); }
        Object key() { return super.getKey(); }
        @Override public Object getKey() { log("getKey"); return super.getKey(); }
        @Override public Object getValue() { log("getValue"); return super.getValue(); }
        @Override public Object setValue(Object value) { log("setValue" + value); return super.setValue(value); }
    }

    /// A map whose entries live in arrays, read only through `entrySet()`.
    /// `put` and `remove(Object)` are its own; everything else is the JDK's.
    static class Table extends AbstractMap<Object, Object> {
        Slot[] slots = new Slot[16];
        int count;

        Table(Object... pairs) {
            for (int i = 0; i < pairs.length; i += 2) slots[count++] = new Slot(pairs[i], pairs[i + 1]);
        }

        @Override public Set<Map.Entry<Object, Object>> entrySet() {
            log("entrySet");
            return new AbstractSet<>() {
                @Override public int size() { log("size"); return count; }
                @Override public Iterator<Map.Entry<Object, Object>> iterator() {
                    log("iterator");
                    return new Iterator<>() {
                        int next;
                        @Override public boolean hasNext() { log("hasNext" + next); return next < count; }
                        @Override public Map.Entry<Object, Object> next() {
                            log("next" + next);
                            if (next >= count) throw new NoSuchElementException();
                            return slots[next++];
                        }
                        @Override public void remove() {
                            log("itRemove" + (next - 1));
                            for (int i = next - 1; i < count - 1; i++) slots[i] = slots[i + 1];
                            count--;
                            next--;
                        }
                    };
                }
            };
        }

        @Override public Object put(Object key, Object value) {
            log("put" + key + "=" + value);
            for (int i = 0; i < count; i++) {
                if (slots[i].key().equals(key)) return slots[i].setValue(value);
            }
            slots[count++] = new Slot(key, value);
            return null;
        }
    }

    /// A read-only map: no `put`.
    static final class Frozen extends AbstractMap<Object, Object> {
        @Override public Set<Map.Entry<Object, Object>> entrySet() { log("entrySet"); return Set.of(); }
    }

    // ---- AbstractMap ----

    public static String sizeIsEmpty() { Table t = new Table("a", 1); return take(t.size() + "," + t.isEmpty()); }

    public static String get() {
        Table t = new Table("a", 1, "b", 2, null, 3);
        return take(t.get("b") + "," + t.get(null) + "," + t.get("z"));
    }

    public static String containsKeyValue() {
        Table t = new Table("a", 1, "b", null);
        return take(t.containsKey("b") + "," + t.containsKey("z") + "," + t.containsValue(null) + "," + t.containsValue(1)
                + "," + t.containsValue(9));
    }

    public static String getCallsArgumentEquals() {
        Table t = new Table(new Key("a"), 1, new Key("b"), 2);
        return take(t.get(new Key("b")));
    }

    public static String remove() {
        Table t = new Table("a", 1, "b", 2, "c", 3);
        Object removed = t.remove("b");
        Object absent = t.remove("z");
        return take(removed + "," + absent + ":" + t.count);
    }

    public static String putUnsupported() {
        try {
            new Frozen().put("a", 1);
            return take("no");
        } catch (UnsupportedOperationException e) {
            return thrown(e);
        }
    }

    public static String putAll() {
        Table t = new Table();
        t.putAll(new Table("a", 1, "b", 2));
        return take(t.count);
    }

    public static String clear() {
        Table t = new Table("a", 1, "b", 2);
        t.clear();
        return take(t.count);
    }

    public static String printed() { return take(new Table("a", 1, null, "x").toString()); }

    public static String printedEmpty() { return take(new Table().toString()); }

    public static String printedSelf() {
        Table t = new Table("a", 1);
        t.slots[t.count++] = new Slot(t, t);
        return take(t.toString());
    }

    public static String equalsMaps() {
        Table t = new Table("a", 1, "b", null);
        return take(t.equals(new Table("a", 1, "b", null)) + "," + t.equals(new Table("a", 1)) + ","
                + t.equals(new Table("a", 2, "b", null)) + "," + t.equals("x") + "," + t.equals(t));
    }

    public static String hashCodeMap() { return take(new Table(new Key("ab"), new Key("c"), null, null).hashCode()); }

    // ---- keySet / values views ----

    public static String keySet() {
        Table t = new Table("a", 1, "b", 2);
        Set<Object> keys = t.keySet();
        StringBuilder out = new StringBuilder();
        for (Object k : keys) out.append(k);
        return take(out + "|" + keys.size() + "|" + keys.contains("b") + "|" + (keys == t.keySet()));
    }

    public static String keySetRemove() {
        Table t = new Table("a", 1, "b", 2);
        Iterator<Object> it = t.keySet().iterator();
        it.next();
        it.remove();
        return take(t.count + ":" + t.keySet());
    }

    public static String values() {
        Table t = new Table("a", 1, "b", 2);
        Collection<Object> values = t.values();
        return take(values + "|" + values.size() + "|" + values.contains(2) + "|" + values.isEmpty() + "|"
                + (values == t.values()));
    }

    public static String viewsClear() {
        Table t = new Table("a", 1, "b", 2);
        t.keySet().clear();
        return take(t.count);
    }

    // ---- SimpleEntry / SimpleImmutableEntry ----

    public static String simpleEntry() {
        Map.Entry<Object, Object> e = new AbstractMap.SimpleEntry<>(new Key("k"), new Key("vv"));
        Object old = e.setValue(new Key("w"));
        Map.Entry<Object, Object> copy = new AbstractMap.SimpleEntry<>(e);
        return take(old + "," + e + "," + e.equals(copy) + "," + e.hashCode());
    }

    public static String immutableEntry() {
        Map.Entry<Object, Object> e = new AbstractMap.SimpleImmutableEntry<>(new Key("k"), null);
        try {
            e.setValue(1);
            return take("no");
        } catch (UnsupportedOperationException error) {
            return take(e + "," + e.hashCode() + "," + error.getMessage());
        }
    }

    public static String entryFromJarEntry() {
        Map.Entry<Object, Object> copy = new AbstractMap.SimpleImmutableEntry<>(new Slot("a", 1));
        return take(copy.getKey() + "=" + copy.getValue());
    }

    public static String entryEqualsCallsOther() {
        Slot slot = new Slot("a", 1);
        Map.Entry<Object, Object> plain = new AbstractMap.SimpleEntry<>("a", 1);
        return take(plain.equals(slot) + "," + slot.equals(plain));
    }

    // ---- Map defaults ----

    public static String getOrDefault() {
        Table t = new Table("a", 1, "n", null);
        return take(t.getOrDefault("a", 9) + "," + t.getOrDefault("n", 9) + "," + t.getOrDefault("z", 9));
    }

    public static String forEach() {
        StringBuilder out = new StringBuilder();
        new Table("a", 1, "b", 2).forEach((k, v) -> out.append(k).append(v));
        return take(out);
    }

    public static String replaceAll() {
        Table t = new Table("a", 1, "b", 2);
        t.replaceAll((k, v) -> (Integer) v * 10);
        return take(t);
    }

    public static String putIfAbsent() {
        Table t = new Table("a", 1);
        return take(t.putIfAbsent("a", 5) + "," + t.putIfAbsent("b", 6) + ":" + t);
    }

    public static String removeKeyValue() {
        Table t = new Table("a", 1, "b", 2);
        return take(t.remove("a", 2) + "," + t.remove("a", 1) + ":" + t);
    }

    public static String replace() {
        Table t = new Table("a", 1);
        return take(t.replace("a", 2) + "," + t.replace("z", 3) + "," + t.replace("a", 2, 4) + ","
                + t.replace("a", 9, 5) + ":" + t);
    }

    public static String computeIfAbsent() {
        Table t = new Table("a", 1);
        return take(t.computeIfAbsent("a", k -> 7) + "," + t.computeIfAbsent("b", k -> 8) + ","
                + t.computeIfAbsent("c", k -> null) + ":" + t);
    }

    public static String computeIfPresent() {
        Table t = new Table("a", 1, "b", 2);
        return take(t.computeIfPresent("a", (k, v) -> (Integer) v + 1) + "," + t.computeIfPresent("z", (k, v) -> 0) + ":" + t);
    }

    public static String compute() {
        Table t = new Table("a", 1);
        return take(t.compute("a", (k, v) -> v + "!") + "," + t.compute("b", (k, v) -> "new") + ":" + t);
    }

    public static String merge() {
        Table t = new Table("a", 1);
        return take(t.merge("a", 5, (x, y) -> (Integer) x + (Integer) y) + "," + t.merge("b", 2, (x, y) -> 0) + ":" + t);
    }

    public static String mergeNullValue() {
        try {
            new Table().merge("a", null, (x, y) -> x);
            return take("no");
        } catch (NullPointerException e) {
            return thrown(e);
        }
    }

    public static String[] all() {
        return new String[] {
            "sizeIsEmpty", "get", "containsKeyValue", "getCallsArgumentEquals", "remove", "putUnsupported", "putAll",
            "clear", "printed", "printedEmpty", "printedSelf", "equalsMaps", "hashCodeMap", "keySet", "keySetRemove",
            "values", "viewsClear", "simpleEntry", "immutableEntry", "entryFromJarEntry", "entryEqualsCallsOther",
            "getOrDefault", "forEach", "replaceAll", "putIfAbsent", "removeKeyValue", "replace", "computeIfAbsent",
            "computeIfPresent", "compute", "merge", "mergeNullValue",
        };
    }

    public static void main(String[] args) throws Exception {
        for (String name : all()) {
            System.out.println(name + "\t" + Probe.class.getMethod(name).invoke(null));
        }
    }
}
"#;

#[test]
fn map_views_runs_as_jdk_25_does() {
    check(&[
        (
            "keySet",
            "entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey hasNext2 entrySet size entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey => ab|2|true|true",
        ),
        (
            "keySetRemove",
            "entrySet iterator next0 getKey itRemove0 entrySet iterator hasNext0 next0 getKey hasNext1 => 1:[b]",
        ),
        (
            "values",
            "entrySet iterator hasNext0 next0 getValue hasNext1 next1 getValue hasNext2 entrySet size entrySet iterator hasNext0 next0 getValue hasNext1 next1 getValue entrySet size => [1, 2]|2|true|false|true",
        ),
        (
            "viewsClear",
            "entrySet iterator hasNext0 next0 itRemove0 hasNext0 next0 itRemove0 hasNext0 => 0",
        ),
    ]);
}

#[test]
fn map_entries_runs_as_jdk_25_does() {
    check(&[
        ("simpleEntry", "eqk eqw hashk hashw => vv,k=w,true,0"),
        ("immutableEntry", "hashk => k=null,1,null"),
        ("entryFromJarEntry", "getKey getValue => a=1"),
        ("entryEqualsCallsOther", "getKey getValue => true,true"),
    ]);
}

#[test]
fn map_defaults_runs_as_jdk_25_does() {
    check(&[
        (
            "getOrDefault",
            "entrySet iterator hasNext0 next0 getKey getValue entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey getValue entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey hasNext2 entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey hasNext2 => 1,null,9",
        ),
        (
            "forEach",
            "entrySet iterator hasNext0 next0 getKey getValue hasNext1 next1 getKey getValue hasNext2 => a1b2",
        ),
        (
            "replaceAll",
            "entrySet iterator hasNext0 next0 getKey getValue setValue10 hasNext1 next1 getKey getValue setValue20 hasNext2 => {a=10, b=20}",
        ),
        (
            "putIfAbsent",
            "entrySet iterator hasNext0 next0 getKey getValue entrySet iterator hasNext0 next0 getKey hasNext1 putb=6 entrySet iterator hasNext0 next0 getKey getValue hasNext1 next1 getKey getValue hasNext2 => 1,null:{a=1, b=6}",
        ),
        (
            "removeKeyValue",
            "entrySet iterator hasNext0 next0 getKey getValue entrySet iterator hasNext0 next0 getKey getValue entrySet iterator hasNext0 next0 getKey getValue itRemove0 entrySet iterator hasNext0 next0 getKey getValue hasNext1 => false,true:{b=2}",
        ),
        (
            "replace",
            "entrySet iterator hasNext0 next0 getKey getValue puta=2 setValue2 entrySet iterator hasNext0 next0 getKey hasNext1 entrySet iterator hasNext0 next0 getKey hasNext1 entrySet iterator hasNext0 next0 getKey getValue puta=4 setValue4 entrySet iterator hasNext0 next0 getKey getValue entrySet iterator hasNext0 next0 getKey getValue hasNext1 => 1,null,true,false:{a=4}",
        ),
        (
            "computeIfAbsent",
            "entrySet iterator hasNext0 next0 getKey getValue entrySet iterator hasNext0 next0 getKey hasNext1 putb=8 entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey hasNext2 entrySet iterator hasNext0 next0 getKey getValue hasNext1 next1 getKey getValue hasNext2 => 1,8,null:{a=1, b=8}",
        ),
        (
            "computeIfPresent",
            "entrySet iterator hasNext0 next0 getKey getValue puta=2 setValue2 entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey hasNext2 entrySet iterator hasNext0 next0 getKey getValue hasNext1 next1 getKey getValue hasNext2 => 2,null:{a=2, b=2}",
        ),
        (
            "compute",
            "entrySet iterator hasNext0 next0 getKey getValue puta=1! setValue1! entrySet iterator hasNext0 next0 getKey hasNext1 putb=new entrySet iterator hasNext0 next0 getKey getValue hasNext1 next1 getKey getValue hasNext2 => 1!,new:{a=1!, b=new}",
        ),
        (
            "merge",
            "entrySet iterator hasNext0 next0 getKey getValue puta=6 setValue6 entrySet iterator hasNext0 next0 getKey hasNext1 putb=2 entrySet iterator hasNext0 next0 getKey getValue hasNext1 next1 getKey getValue hasNext2 => 6,2:{a=6, b=2}",
        ),
        ("mergeNullValue", " => java.lang.NullPointerException:null"),
    ]);
}

#[test]
fn abstract_map_runs_as_jdk_25_does() {
    check(&[
        ("sizeIsEmpty", "entrySet size entrySet size => 1,false"),
        (
            "get",
            "entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey getValue entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey hasNext2 next2 getKey getValue entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey hasNext2 next2 getKey hasNext3 => 2,3,null",
        ),
        (
            "containsKeyValue",
            "entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey hasNext2 entrySet iterator hasNext0 next0 getValue hasNext1 next1 getValue entrySet iterator hasNext0 next0 getValue entrySet iterator hasNext0 next0 getValue hasNext1 next1 getValue hasNext2 => true,false,true,true,false",
        ),
        (
            "getCallsArgumentEquals",
            "entrySet iterator hasNext0 next0 getKey eqb hasNext1 next1 getKey eqb getValue => 2",
        ),
        (
            "remove",
            "entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey getValue itRemove1 entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey hasNext2 => 2,null:2",
        ),
        (
            "putUnsupported",
            " => java.lang.UnsupportedOperationException:null",
        ),
        (
            "putAll",
            "entrySet iterator hasNext0 next0 getKey getValue puta=1 hasNext1 next1 getKey getValue putb=2 hasNext2 => 2",
        ),
        (
            "clear",
            "entrySet iterator hasNext0 next0 itRemove0 hasNext0 next0 itRemove0 hasNext0 => 0",
        ),
        (
            "printed",
            "entrySet iterator hasNext0 next0 getKey getValue hasNext1 next1 getKey getValue hasNext2 => {a=1, null=x}",
        ),
        ("printedEmpty", "entrySet iterator hasNext0 => {}"),
        (
            "printedSelf",
            "entrySet iterator hasNext0 next0 getKey getValue hasNext1 next1 getKey getValue hasNext2 => {a=1, (this Map)=(this Map)}",
        ),
        (
            "equalsMaps",
            "entrySet size entrySet size entrySet iterator hasNext0 next0 getKey getValue entrySet iterator hasNext0 next0 getKey getValue hasNext1 next1 getKey getValue entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey getValue entrySet iterator hasNext0 next0 getKey hasNext1 next1 getKey hasNext2 entrySet size entrySet size entrySet size entrySet size entrySet iterator hasNext0 next0 getKey getValue entrySet iterator hasNext0 next0 getKey getValue => true,false,false,false,true",
        ),
        (
            "hashCodeMap",
            "entrySet iterator hasNext0 next0 hashab hashc hasNext1 next1 hasNext2 => 3",
        ),
    ]);
}
