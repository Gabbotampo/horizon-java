//! Jar subclasses of `AbstractCollection`, `AbstractSet` and `AbstractQueue`,
//! and jar classes inheriting the `Iterable`, `Collection` and `Iterator`
//! defaults, run the JDK's template bodies exactly (`jdk_skeletons/`).
//!
//! The probe's classes log every call the JDK makes into them; each expected
//! line is OpenJDK 25.0.5's output for the same probe method
//! (`support/jdk_differential.rs`). Before `jdk_skeletons/`, these bodies
//! were counted unknown calls answering neutral defaults (#97).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
#[path = "support/jdk_differential.rs"]
mod jdk_differential;

fn check(cases: &[(&str, &str)]) {
    jdk_differential::check("jdk-skeleton-collections", PROBE, cases);
}

const PROBE: &str = r#"package demo;

import java.util.AbstractCollection;
import java.util.AbstractQueue;
import java.util.AbstractSet;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.LinkedHashSet;
import java.util.List;
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

    /// A bag over an array, logging every call the JDK templates make into it.
    static class Bag extends AbstractCollection<Object> {
        Object[] items;
        int count;
        final boolean removable;

        Bag(boolean removable, Object... items) {
            this.items = new Object[16];
            for (int i = 0; i < items.length; i++) this.items[i] = items[i];
            this.count = items.length;
            this.removable = removable;
        }

        @Override public Iterator<Object> iterator() {
            log("iterator");
            return new Iterator<>() {
                int next;
                int last = -1;

                @Override public boolean hasNext() {
                    log("hasNext" + next);
                    return next < count;
                }

                @Override public Object next() {
                    log("next" + next);
                    if (next >= count) throw new NoSuchElementException("past " + next);
                    last = next;
                    return items[next++];
                }

                @Override public void remove() {
                    if (!removable) {
                        Iterator.super.remove();
                        return;
                    }
                    log("remove" + last);
                    for (int i = last; i < count - 1; i++) items[i] = items[i + 1];
                    count--;
                    next = last;
                    last = -1;
                }
            };
        }

        @Override public int size() {
            log("size");
            return count;
        }

        @Override public boolean add(Object element) {
            log("add" + element);
            if ("full".equals(element)) return false;
            items[count++] = element;
            return true;
        }
    }

    /// No `add` override: `AbstractCollection.add` throws.
    static final class ReadOnly extends AbstractCollection<Object> {
        @Override public Iterator<Object> iterator() { log("iterator"); return List.<Object>of(1, 2).iterator(); }
        @Override public int size() { log("size"); return 2; }
    }

    /// An element whose `equals` logs who asked.
    static final class Key {
        final String name;
        Key(String name) { this.name = name; }
        @Override public boolean equals(Object other) {
            log("eq" + name + "?" + other);
            return other instanceof Key key && key.name.equals(name);
        }
        @Override public int hashCode() { log("hash" + name); return name.length(); }
        @Override public String toString() { return "K" + name; }
    }

    static final class Trio extends AbstractSet<Object> {
        final Bag bag;
        Trio(Object... items) { bag = new Bag(true, items); }
        @Override public Iterator<Object> iterator() { return bag.iterator(); }
        @Override public int size() { return bag.size(); }
        @Override public boolean remove(Object o) { log("setRemove" + o); return super.remove(o); }
    }

    static final class Line extends AbstractQueue<String> {
        final ArrayList<String> items = new ArrayList<>();
        final int capacity;
        Line(int capacity) { this.capacity = capacity; }
        @Override public boolean offer(String s) { log("offer" + s); if (items.size() >= capacity) return false; items.add(s); return true; }
        @Override public String poll() { log("poll"); return items.isEmpty() ? null : items.remove(0); }
        @Override public String peek() { log("peek"); return items.isEmpty() ? null : items.get(0); }
        @Override public Iterator<String> iterator() { log("iterator"); return items.iterator(); }
        @Override public int size() { log("size"); return items.size(); }
    }

    static final class Gen implements Iterable<String> {
        @Override public Iterator<String> iterator() { log("iterator"); return List.of("x", "y").iterator(); }
    }

    // ---- AbstractCollection ----

    public static String isEmpty() { return take(new Bag(true, "a").isEmpty()); }

    public static String contains() { return take(new Bag(true, "a", "b", "c").contains("b")); }

    public static String containsNull() {
        Bag bag = new Bag(true, "a", null);
        return take(bag.contains(null) + "," + bag.contains("z"));
    }

    public static String containsCallsArgumentEquals() {
        Bag bag = new Bag(true, new Key("a"), new Key("b"));
        return take(bag.contains(new Key("b")));
    }

    public static String toArray() {
        Object[] array = new Bag(true, "a", "b").toArray();
        return take(array.length + ":" + array[0] + array[1] + ":" + array.getClass().getComponentType().getName());
    }

    /// `size()` says 1 but the iterator yields 3: `finishToArray` grows it.
    public static String toArrayGrows() {
        Bag bag = new Bag(true, "a", "b", "c") {
            @Override public int size() { log("size"); return 1; }
        };
        Object[] array = bag.toArray();
        return take(array.length + ":" + String.join("", (String) array[0], (String) array[1], (String) array[2]));
    }

    /// `size()` says 5 but the iterator yields 2: `Arrays.copyOf` trims it.
    public static String toArrayShrinks() {
        Bag bag = new Bag(true, "a", "b") {
            @Override public int size() { log("size"); return 5; }
        };
        return take(bag.toArray().length);
    }

    public static String toArrayTyped() {
        String[] array = new Bag(true, "a", "b").toArray(new String[0]);
        return take(array.length + ":" + array[1] + ":" + array.getClass().getComponentType().getName());
    }

    public static String toArrayIntoLarger() {
        String[] given = {"p", "q", "r", "s"};
        String[] array = new Bag(true, "a", "b").toArray(given);
        return take((array == given) + ":" + String.join(",", String.valueOf(array[0]), String.valueOf(array[1]),
                String.valueOf(array[2]), String.valueOf(array[3])));
    }

    public static String toArrayGenerator() {
        String[] array = new Bag(true, "a", "b").toArray(String[]::new);
        return take(array.length + ":" + array[0] + array[1]);
    }

    public static String remove() {
        Bag bag = new Bag(true, "a", "b", "c");
        boolean removed = bag.remove("b");
        return take(removed + ":" + bag.count + ":" + bag.items[1]);
    }

    public static String removeAbsent() { return take(new Bag(true, "a").remove("z")); }

    public static String removeUnsupported() {
        try {
            new Bag(false, "a", "b").remove("b");
            return take("no");
        } catch (UnsupportedOperationException e) {
            return thrown(e);
        }
    }

    public static String addUnsupported() {
        try {
            new ReadOnly().add(3);
            return take("no");
        } catch (UnsupportedOperationException e) {
            return thrown(e);
        }
    }

    public static String containsAll() {
        Bag bag = new Bag(true, "a", "b");
        return take(bag.containsAll(List.of("b", "a")) + "," + bag.containsAll(List.of("a", "z", "b")));
    }

    public static String addAll() {
        Bag bag = new Bag(true, "a");
        boolean changed = bag.addAll(List.of("b", "full", "c"));
        boolean unchanged = bag.addAll(List.of("full"));
        return take(changed + "," + unchanged + ":" + bag.count);
    }

    public static String addAllFromJarCollection() {
        Bag bag = new Bag(true, "a");
        bag.addAll(new Bag(true, "q"));
        return take(bag.count);
    }

    public static String removeAll() {
        Bag bag = new Bag(true, "a", "b", "c", "b");
        boolean changed = bag.removeAll(List.of("b"));
        return take(changed + ":" + bag.count + ":" + bag);
    }

    public static String removeAllNull() {
        try {
            new Bag(true, "a").removeAll(null);
            return take("no");
        } catch (NullPointerException e) {
            return thrown(e);
        }
    }

    public static String retainAll() {
        Bag bag = new Bag(true, "a", "b", "c");
        boolean changed = bag.retainAll(Set.of("b"));
        return take(changed + ":" + bag);
    }

    public static String clear() {
        Bag bag = new Bag(true, "a", "b");
        bag.clear();
        return take(bag.count);
    }

    public static String printed() { return take(new Bag(true, "a", null, 3, new Key("k"))); }

    public static String printedEmpty() { return take(new Bag(true).toString()); }

    public static String printedSelf() {
        Bag bag = new Bag(true, "a");
        bag.items[1] = bag;
        bag.count = 2;
        return take(bag.toString());
    }

    public static String concatenated() { return take("<" + new Bag(true, "a", "b") + ">"); }

    public static String iteratorThrows() {
        Bag bag = new Bag(true, "a") {
            @Override public Iterator<Object> iterator() {
                log("iterator");
                return new Iterator<>() {
                    @Override public boolean hasNext() { log("hasNext"); return true; }
                    @Override public Object next() { throw new IllegalStateException("boom"); }
                };
            }
        };
        try {
            bag.contains("x");
            return take("no");
        } catch (IllegalStateException e) {
            return thrown(e);
        }
    }

    // ---- Collection / Iterable / Iterator defaults ----

    public static String removeIf() {
        Bag bag = new Bag(true, "a", "bb", "c", "dd");
        boolean changed = bag.removeIf(s -> ((String) s).length() == 2);
        return take(changed + ":" + bag);
    }

    public static String removeIfNone() { return take(new Bag(true, "a").removeIf(s -> false)); }

    public static String removeIfNull() {
        try {
            new Bag(true, "a").removeIf(null);
            return take("no");
        } catch (NullPointerException e) {
            return thrown(e);
        }
    }

    public static String forEach() {
        StringBuilder seen = new StringBuilder();
        new Bag(true, "a", "b").forEach(seen::append);
        return take(seen);
    }

    public static String iterableForEach() {
        StringBuilder seen = new StringBuilder();
        new Gen().forEach(seen::append);
        return take(seen);
    }

    public static String forEachRemaining() {
        StringBuilder seen = new StringBuilder();
        Iterator<Object> it = new Bag(true, "a", "b", "c").iterator();
        it.next();
        it.forEachRemaining(seen::append);
        return take(seen);
    }

    public static String iteratorRemoveDefault() {
        Iterator<Object> it = new Bag(false, "a").iterator();
        it.next();
        try {
            it.remove();
            return take("no");
        } catch (UnsupportedOperationException e) {
            return thrown(e);
        }
    }

    public static String stream() {
        long count = new Bag(true, "a", "bb", "c").stream().filter(s -> ((String) s).length() == 1).count();
        return take(count);
    }

    // ---- AbstractSet ----

    public static String setEquals() {
        Trio trio = new Trio("a", "b");
        return take(trio.equals(new LinkedHashSet<>(List.of("b", "a"))) + "," + trio.equals(List.of("a", "b")) + "," + trio.equals(trio)
                + "," + trio.equals(Set.of("a")));
    }

    public static String setHashCode() { return take(new Trio(new Key("ab"), null, new Key("c")).hashCode()); }

    public static String setRemoveAllSmallArgument() {
        Trio trio = new Trio("a", "b", "c");
        boolean changed = trio.removeAll(List.of("c", "z"));
        return take(changed + ":" + trio);
    }

    public static String setRemoveAllLargeArgument() {
        Trio trio = new Trio("a", "b");
        boolean changed = trio.removeAll(List.of("b", "x", "y"));
        return take(changed + ":" + trio);
    }

    // ---- AbstractQueue ----

    public static String queueAdd() {
        Line line = new Line(1);
        line.add("a");
        try {
            line.add("b");
            return take("no");
        } catch (IllegalStateException e) {
            return thrown(e);
        }
    }

    public static String queueRemoveElement() {
        Line line = new Line(3);
        line.add("a");
        String first = line.element() + line.remove();
        try {
            line.remove();
            return take("no");
        } catch (NoSuchElementException e) {
            return take(first + "|" + e.getClass().getName() + ":" + e.getMessage());
        }
    }

    public static String queueElementEmpty() {
        try {
            new Line(1).element();
            return take("no");
        } catch (NoSuchElementException e) {
            return thrown(e);
        }
    }

    public static String queueClearAddAll() {
        Line line = new Line(5);
        boolean changed = line.addAll(List.of("a", "b"));
        line.clear();
        return take(changed + ":" + line.items.size());
    }

    public static String queueAddAllSelf() {
        Line line = new Line(5);
        try {
            line.addAll(line);
            return take("no");
        } catch (IllegalArgumentException e) {
            return thrown(e);
        }
    }

    public static String queueAddAllNull() {
        try {
            new Line(5).addAll(null);
            return take("no");
        } catch (NullPointerException e) {
            return thrown(e);
        }
    }

    public static String[] all() {
        return new String[] {
            "isEmpty", "contains", "containsNull", "containsCallsArgumentEquals", "toArray", "toArrayGrows",
            "toArrayShrinks", "toArrayTyped", "toArrayIntoLarger", "toArrayGenerator", "remove", "removeAbsent",
            "removeUnsupported", "addUnsupported", "containsAll", "addAll", "addAllFromJarCollection", "removeAll",
            "removeAllNull", "retainAll", "clear", "printed", "printedEmpty", "printedSelf", "concatenated",
            "iteratorThrows", "removeIf", "removeIfNone", "removeIfNull", "forEach", "iterableForEach",
            "forEachRemaining", "iteratorRemoveDefault", "stream", "setEquals", "setHashCode",
            "setRemoveAllSmallArgument", "setRemoveAllLargeArgument", "queueAdd", "queueRemoveElement",
            "queueElementEmpty", "queueClearAddAll", "queueAddAllSelf", "queueAddAllNull",
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
fn interface_defaults_runs_as_jdk_25_does() {
    check(&[
        (
            "toArrayGenerator",
            "size iterator hasNext0 next0 hasNext1 next1 hasNext2 => 2:ab",
        ),
        (
            "removeUnsupported",
            "iterator hasNext0 next0 hasNext1 next1 => java.lang.UnsupportedOperationException:remove",
        ),
        (
            "removeIf",
            "iterator hasNext0 next0 hasNext1 next1 remove1 hasNext1 next1 hasNext2 next2 remove2 hasNext2 iterator hasNext0 next0 hasNext1 next1 hasNext2 => true:[a, c]",
        ),
        ("removeIfNone", "iterator hasNext0 next0 hasNext1 => false"),
        ("removeIfNull", " => java.lang.NullPointerException:null"),
        (
            "forEach",
            "iterator hasNext0 next0 hasNext1 next1 hasNext2 => ab",
        ),
        ("iterableForEach", "iterator => xy"),
        (
            "forEachRemaining",
            "iterator next0 hasNext1 next1 hasNext2 next2 hasNext3 => bc",
        ),
        (
            "iteratorRemoveDefault",
            "iterator next0 => java.lang.UnsupportedOperationException:remove",
        ),
        (
            "stream",
            "iterator size hasNext0 next0 hasNext1 next1 hasNext2 next2 hasNext3 => 2",
        ),
    ]);
}

#[test]
fn abstract_set_runs_as_jdk_25_does() {
    check(&[
        (
            "setEquals",
            "size iterator hasNext0 next0 hasNext1 next1 iterator hasNext0 next0 size => true,false,true,false",
        ),
        (
            "setHashCode",
            "iterator hasNext0 next0 hashab hasNext1 next1 hasNext2 next2 hashc hasNext3 => 3",
        ),
        (
            "setRemoveAllSmallArgument",
            "size setRemovec iterator hasNext0 next0 hasNext1 next1 hasNext2 next2 remove2 setRemovez iterator hasNext0 next0 hasNext1 next1 hasNext2 iterator hasNext0 next0 hasNext1 next1 hasNext2 => true:[a, b]",
        ),
        (
            "setRemoveAllLargeArgument",
            "size iterator hasNext0 next0 hasNext1 next1 remove1 hasNext1 iterator hasNext0 next0 hasNext1 => true:[a]",
        ),
    ]);
}

#[test]
fn abstract_queue_runs_as_jdk_25_does() {
    check(&[
        (
            "queueAdd",
            "offera offerb => java.lang.IllegalStateException:Queue full",
        ),
        (
            "queueRemoveElement",
            "offera peek poll poll => aa|java.util.NoSuchElementException:null",
        ),
        (
            "queueElementEmpty",
            "peek => java.util.NoSuchElementException:null",
        ),
        ("queueClearAddAll", "offera offerb poll poll poll => true:0"),
        (
            "queueAddAllSelf",
            " => java.lang.IllegalArgumentException:null",
        ),
        ("queueAddAllNull", " => java.lang.NullPointerException:null"),
    ]);
}

#[test]
fn abstract_collection_runs_as_jdk_25_does() {
    check(&[
        ("isEmpty", "size => false"),
        ("contains", "iterator hasNext0 next0 hasNext1 next1 => true"),
        (
            "containsNull",
            "iterator hasNext0 next0 hasNext1 next1 iterator hasNext0 next0 hasNext1 next1 hasNext2 => true,false",
        ),
        (
            "containsCallsArgumentEquals",
            "iterator hasNext0 next0 eqb?Ka hasNext1 next1 eqb?Kb => true",
        ),
        (
            "toArray",
            "size iterator hasNext0 next0 hasNext1 next1 hasNext2 => 2:ab:java.lang.Object",
        ),
        (
            "toArrayGrows",
            "size iterator hasNext0 next0 hasNext1 hasNext1 next1 hasNext2 next2 hasNext3 => 3:abc",
        ),
        (
            "toArrayShrinks",
            "size iterator hasNext0 next0 hasNext1 next1 hasNext2 => 2",
        ),
        (
            "toArrayTyped",
            "size iterator hasNext0 next0 hasNext1 next1 hasNext2 => 2:b:java.lang.String",
        ),
        (
            "toArrayIntoLarger",
            "size iterator hasNext0 next0 hasNext1 next1 hasNext2 => true:a,b,null,s",
        ),
        (
            "remove",
            "iterator hasNext0 next0 hasNext1 next1 remove1 => true:2:c",
        ),
        ("removeAbsent", "iterator hasNext0 next0 hasNext1 => false"),
        (
            "addUnsupported",
            " => java.lang.UnsupportedOperationException:null",
        ),
        (
            "containsAll",
            "iterator hasNext0 next0 hasNext1 next1 iterator hasNext0 next0 iterator hasNext0 next0 iterator hasNext0 next0 hasNext1 next1 hasNext2 => true,false",
        ),
        ("addAll", "addb addfull addc addfull => true,false:3"),
        (
            "addAllFromJarCollection",
            "iterator hasNext0 next0 addq hasNext1 => 2",
        ),
        (
            "removeAll",
            "iterator hasNext0 next0 hasNext1 next1 remove1 hasNext1 next1 hasNext2 next2 remove2 hasNext2 iterator hasNext0 next0 hasNext1 next1 hasNext2 => true:2:[a, c]",
        ),
        ("removeAllNull", " => java.lang.NullPointerException:null"),
        (
            "retainAll",
            "iterator hasNext0 next0 remove0 hasNext0 next0 hasNext1 next1 remove1 hasNext1 iterator hasNext0 next0 hasNext1 => true:[b]",
        ),
        (
            "clear",
            "iterator hasNext0 next0 remove0 hasNext0 next0 remove0 hasNext0 => 0",
        ),
        ("printed", " => [a, null, 3, Kk]"),
        ("printedEmpty", "iterator hasNext0 => []"),
        (
            "printedSelf",
            "iterator hasNext0 next0 hasNext1 next1 hasNext2 => [a, (this Collection)]",
        ),
        (
            "concatenated",
            "iterator hasNext0 next0 hasNext1 next1 hasNext2 => <[a, b]>",
        ),
        (
            "iteratorThrows",
            "iterator hasNext => java.lang.IllegalStateException:boom",
        ),
    ]);
}
