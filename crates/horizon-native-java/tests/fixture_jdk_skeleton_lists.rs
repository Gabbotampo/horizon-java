//! Jar subclasses of `AbstractList` and `AbstractSequentialList` run the
//! JDK's bodies exactly (`jdk_skeletons/list.rs`): the templates over `get`,
//! `size`, `set`, `add` and `remove`, `AbstractList.Itr`/`ListItr` with
//! `modCount` and `ConcurrentModificationException`, `subList` (nested, and
//! `RandomAccess`), and the `List` defaults.
//!
//! The probe's classes log every call the JDK makes into them; each expected
//! line is OpenJDK 25.0.5's output for the same probe method
//! (`support/jdk_differential.rs`). Before `jdk_skeletons/list.rs`, these
//! bodies were counted unknown calls answering neutral defaults (#97).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
#[path = "support/jdk_differential.rs"]
mod jdk_differential;

fn check(cases: &[(&str, &str)]) {
    jdk_differential::check("jdk-skeleton-lists", PROBE, cases);
}

const PROBE: &str = r#"package demo;

import java.util.AbstractList;
import java.util.AbstractSequentialList;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.ConcurrentModificationException;
import java.util.Iterator;
import java.util.List;
import java.util.ListIterator;
import java.util.NoSuchElementException;
import java.util.RandomAccess;

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

    /// A list over an array with logging `get`/`set`/`add`/`remove`/`size`.
    /// Its `add`/`remove` bump `modCount` as the JDK's contract asks.
    static class Row extends AbstractList<String> {
        String[] items = new String[32];
        int count;

        Row(String... items) {
            for (String item : items) this.items[count++] = item;
        }

        @Override public String get(int index) {
            log("get" + index);
            if (index < 0 || index >= count) throw new IndexOutOfBoundsException("at " + index);
            return items[index];
        }

        @Override public int size() {
            log("size");
            return count;
        }

        @Override public String set(int index, String element) {
            log("set" + index + element);
            String old = items[index];
            items[index] = element;
            return old;
        }

        @Override public void add(int index, String element) {
            log("add" + index + element);
            if (index < 0 || index > count) throw new IndexOutOfBoundsException("add " + index);
            for (int i = count; i > index; i--) items[i] = items[i - 1];
            items[index] = element;
            count++;
            modCount++;
        }

        @Override public String remove(int index) {
            log("remove" + index);
            String old = items[index];
            for (int i = index; i < count - 1; i++) items[i] = items[i + 1];
            count--;
            modCount++;
            return old;
        }

        String dump() {
            StringBuilder out = new StringBuilder();
            for (int i = 0; i < count; i++) out.append(items[i]);
            return out.toString();
        }
    }

    /// A read-only list: only `get` and `size`.
    static final class Fixed extends AbstractList<String> {
        @Override public String get(int index) { log("get" + index); return "f" + index; }
        @Override public int size() { log("size"); return 2; }
    }

    static final class Indexed extends Row implements RandomAccess {
        Indexed(String... items) { super(items); }
    }

    /// A sequential list over an array, read only through its own list
    /// iterator (which logs).
    static final class Chain extends AbstractSequentialList<String> {
        String[] items = new String[16];
        int count;

        Chain(String... items) { for (String item : items) this.items[count++] = item; }

        @Override public int size() { log("size"); return count; }

        String dump() {
            StringBuilder out = new StringBuilder();
            for (int i = 0; i < count; i++) out.append(items[i]);
            return out.toString();
        }

        @Override public ListIterator<String> listIterator(int index) {
            log("listIterator" + index);
            return new ListIterator<>() {
                int cursor = index;
                int last = -1;
                @Override public boolean hasNext() { log("hasNext"); return cursor < count; }
                @Override public String next() {
                    log("next");
                    if (cursor >= count) throw new NoSuchElementException();
                    last = cursor;
                    return items[cursor++];
                }
                @Override public boolean hasPrevious() { return cursor > 0; }
                @Override public String previous() { last = --cursor; return items[cursor]; }
                @Override public int nextIndex() { return cursor; }
                @Override public int previousIndex() { return cursor - 1; }
                @Override public void remove() {
                    log("itRemove");
                    for (int i = last; i < count - 1; i++) items[i] = items[i + 1];
                    count--;
                    cursor = last;
                    last = -1;
                }
                @Override public void set(String s) { log("itSet" + s); items[last] = s; }
                @Override public void add(String s) {
                    log("itAdd" + s);
                    for (int i = count; i > cursor; i--) items[i] = items[i - 1];
                    items[cursor++] = s;
                    count++;
                    last = -1;
                }
            };
        }
    }

    /// An element with its own `hashCode`.
    static final class Tag {
        final int code;
        Tag(int code) { this.code = code; }
        @Override public int hashCode() { log("hash" + code); return code; }
    }

    /// A list of tags.
    static final class Tags extends AbstractList<Object> {
        final Object[] items;
        Tags(Object... items) { this.items = items; }
        @Override public Object get(int index) { return items[index]; }
        @Override public int size() { return items.length; }
    }

    // ---- AbstractList templates ----

    public static String addAppends() {
        Row row = new Row("a");
        boolean added = row.add("b");
        return take(added + ":" + row.dump());
    }

    public static String unsupported() {
        Fixed fixed = new Fixed();
        StringBuilder out = new StringBuilder();
        try { fixed.add("x"); } catch (UnsupportedOperationException e) { out.append("add:" + e.getMessage()); }
        try { fixed.set(0, "x"); } catch (UnsupportedOperationException e) { out.append(",set:" + e.getMessage()); }
        try { fixed.remove(0); } catch (UnsupportedOperationException e) { out.append(",remove:" + e.getMessage()); }
        return take(out);
    }

    public static String indexOf() {
        Row row = new Row("a", "b", null, "b");
        return take(row.indexOf("b") + "," + row.indexOf(null) + "," + row.indexOf("z"));
    }

    public static String lastIndexOf() {
        Row row = new Row("a", "b", null, "b");
        return take(row.lastIndexOf("b") + "," + row.lastIndexOf(null) + "," + row.lastIndexOf("z"));
    }

    public static String contains() { return take(new Row("a", "b").contains("b")); }

    public static String iterate() {
        StringBuilder out = new StringBuilder();
        for (String s : new Row("a", "b")) out.append(s);
        return take(out);
    }

    public static String iteratorPastEnd() {
        Iterator<String> it = new Row("a").iterator();
        it.next();
        try {
            it.next();
            return take("no");
        } catch (NoSuchElementException e) {
            return thrown(e);
        }
    }

    public static String iteratorRemove() {
        Row row = new Row("a", "b", "c");
        Iterator<String> it = row.iterator();
        it.next();
        it.next();
        it.remove();
        String after = it.next();
        return take(after + ":" + row.dump());
    }

    public static String iteratorRemoveTwice() {
        Iterator<String> it = new Row("a", "b").iterator();
        it.next();
        it.remove();
        try {
            it.remove();
            return take("no");
        } catch (IllegalStateException e) {
            return thrown(e);
        }
    }

    public static String concurrentModification() {
        Row row = new Row("a", "b", "c");
        try {
            for (String s : row) {
                if (s.equals("a")) row.add("z");
            }
            return take("no");
        } catch (ConcurrentModificationException e) {
            return thrown(e);
        }
    }

    public static String modificationAfterLast() {
        Row row = new Row("a", "b");
        Iterator<String> it = row.iterator();
        it.next();
        it.next();
        row.add("c");
        return take(it.hasNext());
    }

    public static String listIterator() {
        Row row = new Row("a", "b", "c");
        ListIterator<String> it = row.listIterator(1);
        StringBuilder out = new StringBuilder();
        out.append(it.nextIndex()).append(it.previousIndex()).append(it.hasPrevious());
        out.append(it.next()).append(it.previous()).append(it.previous());
        it.set("A");
        it.add("q");
        out.append(it.nextIndex());
        return take(out + ":" + row.dump());
    }

    public static String listIteratorBadIndex() {
        try {
            new Row("a").listIterator(3);
            return take("no");
        } catch (IndexOutOfBoundsException e) {
            return thrown(e);
        }
    }

    public static String listIteratorSetWithoutNext() {
        try {
            new Row("a").listIterator().set("x");
            return take("no");
        } catch (IllegalStateException e) {
            return thrown(e);
        }
    }

    public static String previousPastStart() {
        try {
            new Row("a").listIterator().previous();
            return take("no");
        } catch (NoSuchElementException e) {
            return thrown(e);
        }
    }

    public static String clear() {
        Row row = new Row("a", "b", "c");
        row.clear();
        return take(row.count);
    }

    public static String addAllAtIndex() {
        Row row = new Row("a", "d");
        boolean changed = row.addAll(1, List.of("b", "c"));
        boolean unchanged = row.addAll(0, List.of());
        return take(changed + "," + unchanged + ":" + row.dump());
    }

    public static String addAllAtBadIndex() {
        try {
            new Row("a").addAll(5, List.of("b"));
            return take("no");
        } catch (IndexOutOfBoundsException e) {
            return thrown(e);
        }
    }

    public static String addAllAppends() {
        Row row = new Row("a");
        boolean changed = row.addAll(List.of("b", "c"));
        return take(changed + ":" + row.dump());
    }

    public static String equalsLists() {
        Row row = new Row("a", "b");
        return take(row.equals(new Row("a", "b")) + "," + row.equals(new Row("a")) + ","
                + row.equals(new Row("a", "z")) + "," + row.equals("ab") + "," + row.equals(row));
    }

    public static String hashCodeList() {
        return take(new Tags(new Tag(7), null, new Tag(-3)).hashCode() + "," + new Tags().hashCode());
    }

    public static String printed() { return take("" + new Row("a", "b")); }

    public static String toArray() {
        Object[] array = new Row("a", "b").toArray();
        return take(array.length + ":" + array[0] + array[1]);
    }

    // ---- subList ----

    public static String subList() {
        Row row = new Row("a", "b", "c", "d");
        List<String> sub = row.subList(1, 3);
        return take(sub.size() + ":" + sub.get(0) + sub.get(1) + ":" + sub);
    }

    public static String subListWrites() {
        Row row = new Row("a", "b", "c", "d");
        List<String> sub = row.subList(1, 3);
        sub.set(0, "B");
        sub.add("x");
        sub.remove(0);
        return take(sub.size() + ":" + row.dump());
    }

    public static String subListClear() {
        Row row = new Row("a", "b", "c", "d");
        row.subList(1, 3).clear();
        return take(row.dump());
    }

    public static String subListOfSubList() {
        Row row = new Row("a", "b", "c", "d", "e");
        List<String> inner = row.subList(1, 4).subList(1, 2);
        inner.add("q");
        return take(inner + ":" + row.dump());
    }

    public static String subListRanges() {
        Row row = new Row("a", "b");
        StringBuilder out = new StringBuilder();
        try { row.subList(-1, 1); } catch (IndexOutOfBoundsException e) { out.append(e.getMessage()); }
        try { row.subList(0, 3); } catch (IndexOutOfBoundsException e) { out.append("|" + e.getMessage()); }
        try { row.subList(2, 1); } catch (IllegalArgumentException e) { out.append("|" + e.getMessage()); }
        try { row.subList(0, 1).get(1); } catch (IndexOutOfBoundsException e) { out.append("|" + e.getMessage()); }
        return take(out);
    }

    public static String subListComodification() {
        Row row = new Row("a", "b", "c");
        List<String> sub = row.subList(0, 2);
        row.add("z");
        try {
            sub.size();
            return take("no");
        } catch (ConcurrentModificationException e) {
            return thrown(e);
        }
    }

    public static String subListIterate() {
        Row row = new Row("a", "b", "c");
        StringBuilder out = new StringBuilder();
        for (String s : row.subList(1, 3)) out.append(s);
        return take(out);
    }

    public static String subListRandomAccess() {
        List<String> sub = new Indexed("a", "b", "c").subList(0, 2);
        return take((sub instanceof RandomAccess) + "," + (new Row("a").subList(0, 1) instanceof RandomAccess));
    }

    // ---- List defaults ----

    public static String sort() {
        Row row = new Row("c", "a", "b");
        row.sort(Comparator.naturalOrder());
        return take(row.dump());
    }

    public static String replaceAll() {
        Row row = new Row("a", "b");
        row.replaceAll(s -> s + s);
        return take(row.dump());
    }

    public static String firstLast() {
        Row row = new Row("a", "b", "c");
        String text = row.getFirst() + row.getLast() + row.removeFirst() + row.removeLast();
        row.addFirst("0");
        row.addLast("9");
        return take(text + ":" + row.dump());
    }

    public static String firstOfEmpty() {
        try {
            new Row().getFirst();
            return take("no");
        } catch (NoSuchElementException e) {
            return thrown(e);
        }
    }

    public static String streamRandomAccess() {
        return take(new Indexed("a", "bb", "c").stream().filter(s -> s.length() == 1).count());
    }

    public static String streamSequential() {
        return take(new Row("a", "bb", "c").stream().filter(s -> s.length() == 1).count());
    }

    // ---- AbstractSequentialList ----

    public static String sequentialGetSet() {
        Chain chain = new Chain("a", "b");
        String got = chain.get(1);
        String old = chain.set(0, "A");
        return take(got + old + ":" + chain.dump());
    }

    public static String sequentialAddRemove() {
        Chain chain = new Chain("a", "c");
        chain.add(1, "b");
        String removed = chain.remove(0);
        return take(removed + ":" + chain.dump());
    }

    public static String sequentialBadIndex() {
        Chain chain = new Chain("a");
        try {
            chain.get(1);
            return take("no");
        } catch (IndexOutOfBoundsException e) {
            return thrown(e);
        }
    }

    public static String sequentialAddAll() {
        Chain chain = new Chain("a", "d");
        boolean changed = chain.addAll(1, List.of("b", "c"));
        return take(changed + ":" + chain.dump());
    }

    public static String sequentialIterate() {
        StringBuilder out = new StringBuilder();
        for (String s : new Chain("x", "y")) out.append(s);
        return take(out);
    }

    public static String[] all() {
        return new String[] {
            "addAppends", "unsupported", "indexOf", "lastIndexOf", "contains", "iterate", "iteratorPastEnd",
            "iteratorRemove", "iteratorRemoveTwice", "concurrentModification", "modificationAfterLast",
            "listIterator", "listIteratorBadIndex", "listIteratorSetWithoutNext", "previousPastStart", "clear",
            "addAllAtIndex", "addAllAtBadIndex", "addAllAppends", "equalsLists", "hashCodeList", "printed",
            "toArray", "subList", "subListWrites", "subListClear", "subListOfSubList", "subListRanges",
            "subListComodification", "subListIterate", "subListRandomAccess", "replaceAll", "firstLast",
            "firstOfEmpty", "streamRandomAccess", "streamSequential", "sequentialGetSet", "sequentialAddRemove",
            "sequentialBadIndex", "sequentialAddAll", "sequentialIterate",
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
fn sub_list_runs_as_jdk_25_does() {
    check(&[
        ("subList", "size get1 get2 size get1 get2 => 2:bc:[b, c]"),
        ("subListWrites", "size set1B add3x remove1 => 2:acxd"),
        ("subListClear", "size size get1 remove1 get1 remove1 => ad"),
        (
            "subListOfSubList",
            "size add3q size get2 get3 => [c, q]:abcqde",
        ),
        (
            "subListRanges",
            "size size size size => fromIndex = -1|toIndex = 3|fromIndex(2) > toIndex(1)|Index 1 out of bounds for length 1",
        ),
        (
            "subListComodification",
            "size size add3z => java.util.ConcurrentModificationException:null",
        ),
        ("subListIterate", "size size get1 get2 => bc"),
        ("subListRandomAccess", "size size => true,false"),
    ]);
}

#[test]
fn list_defaults_runs_as_jdk_25_does() {
    check(&[
        (
            "replaceAll",
            "size size get0 set0aa size get1 set1bb size => aabb",
        ),
        (
            "firstLast",
            "size get0 size size get2 size remove0 size size remove1 add00 size add29 => acac:0b9",
        ),
        (
            "firstOfEmpty",
            "size => java.util.NoSuchElementException:null",
        ),
        ("streamRandomAccess", "size get0 get1 get2 => 2"),
        (
            "streamSequential",
            "size size get0 size get1 size get2 size => 2",
        ),
    ]);
}

#[test]
fn abstract_sequential_list_runs_as_jdk_25_does() {
    check(&[
        (
            "sequentialGetSet",
            "listIterator1 next listIterator0 next itSetA => ba:Ab",
        ),
        (
            "sequentialAddRemove",
            "listIterator1 itAddb listIterator0 next itRemove => a:bc",
        ),
        (
            "sequentialBadIndex",
            "listIterator1 next => java.lang.IndexOutOfBoundsException:Index: 1",
        ),
        (
            "sequentialAddAll",
            "listIterator1 itAddb itAddc => true:abcd",
        ),
        (
            "sequentialIterate",
            "listIterator0 hasNext next hasNext next hasNext => xy",
        ),
    ]);
}

#[test]
fn abstract_list_runs_as_jdk_25_does() {
    check(&[
        ("addAppends", "size add1b => true:ab"),
        ("unsupported", "size => add:null,set:null,remove:null"),
        (
            "indexOf",
            "size size get0 size get1 size size get0 size get1 size get2 size size get0 size get1 size get2 size get3 size => 1,2,-1",
        ),
        (
            "lastIndexOf",
            "size size get3 size size get3 get2 size size get3 get2 get1 get0 => 3,2,-1",
        ),
        ("contains", "size get0 size get1 => true"),
        ("iterate", "size get0 size get1 size => ab"),
        (
            "iteratorPastEnd",
            "get0 get1 => java.util.NoSuchElementException:java.lang.IndexOutOfBoundsException: at 1",
        ),
        ("iteratorRemove", "get0 get1 remove1 get1 => c:ac"),
        (
            "iteratorRemoveTwice",
            "get0 remove0 => java.lang.IllegalStateException:null",
        ),
        (
            "concurrentModification",
            "size get0 size add3z size => java.util.ConcurrentModificationException:null",
        ),
        ("modificationAfterLast", "get0 get1 size add2c size => true"),
        (
            "listIterator",
            "size get1 get1 get0 set0A add0q => 10truebba1:qAbc",
        ),
        (
            "listIteratorBadIndex",
            "size size => java.lang.IndexOutOfBoundsException:Index: 3, Size: 1",
        ),
        (
            "listIteratorSetWithoutNext",
            "size => java.lang.IllegalStateException:null",
        ),
        (
            "previousPastStart",
            "size get-1 => java.util.NoSuchElementException:java.lang.IndexOutOfBoundsException: at -1",
        ),
        (
            "clear",
            "size size get0 remove0 get0 remove0 get0 remove0 => 0",
        ),
        ("addAllAtIndex", "size add1b add2c size => true,false:abcd"),
        (
            "addAllAtBadIndex",
            "size size => java.lang.IndexOutOfBoundsException:Index: 5, Size: 1",
        ),
        ("addAllAppends", "size add1b size add2c => true:abc"),
        (
            "equalsLists",
            "size size size size get0 get0 size size get1 get1 size size size size size size size get0 get0 size size size size size size size get0 get0 size size get1 get1 => true,false,false,false,true",
        ),
        ("hashCodeList", "hash7 hash-3 => 36515,1"),
        ("printed", "size get0 size get1 size => [a, b]"),
        ("toArray", "size size get0 size get1 size => 2:ab"),
    ]);
}
