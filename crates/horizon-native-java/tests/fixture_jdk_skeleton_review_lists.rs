//! The independent review's cases for #120, as JDK 25 differential tests:
//! `AbstractList.Itr.next` and `ListItr.previous` turn the list's
//! `IndexOutOfBoundsException` into `NoSuchElementException(cause)` whose
//! `getCause()` is that exception, the jar's own object when the jar threw it.
//!
//! Each expected line is OpenJDK 25.0.5's output for the same probe method
//! (`support/jdk_differential.rs`).

#[path = "support/javac_fixture.rs"]
mod javac_fixture;
#[path = "support/jdk_differential.rs"]
mod jdk_differential;

fn check(cases: &[(&str, &str)]) {
    jdk_differential::check("jdk-skeleton-review-120", PROBE, cases);
}

const PROBE: &str = r#"package demo;

import java.util.AbstractList;
import java.util.Iterator;
import java.util.ListIterator;
import java.util.NoSuchElementException;

/// The independent review's #120 cases, rewritten as probe methods.
public final class Probe {
    static final StringBuilder LOG = new StringBuilder();

    static void log(String event) { LOG.append(event).append(' '); }

    static String take(Object result) {
        String text = LOG.toString().trim() + " => " + result;
        LOG.setLength(0);
        return text;
    }

    static final class MyIOOBE extends IndexOutOfBoundsException { MyIOOBE(String m) { super(m); } }

    static final MyIOOBE THROWN = new MyIOOBE("mine");

    /// `size()` claims 2; `get` reads its array (`get(2)` overruns it).
    static final class RL extends AbstractList<Object> {
        final Object[] items = {"x", "y"};
        boolean own;
        @Override public Object get(int index) {
            log("g" + index);
            if (own && index >= items.length) throw THROWN;
            return items[index];
        }
        @Override public int size() { return 2; }
    }

    static String nseeCause(RL list, boolean backwards) {
        try {
            if (backwards) {
                ListIterator<Object> it = list.listIterator();
                it.previous();
            } else {
                Iterator<Object> it = list.iterator();
                it.next();
                it.next();
                it.next();
            }
            return take("no");
        } catch (NoSuchElementException e) {
            Throwable cause = e.getCause();
            return take(e.getMessage() + "/" + cause + "/" + (cause == THROWN));
        }
    }

    public static String nextPastEndChainsCause() { return nseeCause(new RL(), false); }

    public static String previousAtStartChainsCause() { return nseeCause(new RL(), true); }

    public static String nextChainsTheJarsOwnException() {
        RL list = new RL();
        list.own = true;
        return nseeCause(list, false);
    }

    /// Not in `all()`: `ArrayList.listIterator()` is a counted host gap, so
    /// this case has no JDK answer to match (Java: `true`).
    public static String equalsHostListStops() {
        return take(new RL().equals(new java.util.ArrayList<>(java.util.List.of("x", "y"))));
    }

    public static String[] all() {
        return new String[] {"nextPastEndChainsCause", "previousAtStartChainsCause", "nextChainsTheJarsOwnException"};
    }

    public static void main(String[] args) throws Exception {
        for (String name : all()) System.out.println(name + "\t" + Probe.class.getMethod(name).invoke(null));
    }
}
"#;

#[test]
fn review_120_runs_as_jdk_25_does() {
    check(&[
        (
            "nextPastEndChainsCause",
            "g0 g1 g2 => java.lang.ArrayIndexOutOfBoundsException: Index 2 out of bounds for length 2/java.lang.ArrayIndexOutOfBoundsException: Index 2 out of bounds for length 2/false",
        ),
        (
            "previousAtStartChainsCause",
            "g-1 => java.lang.ArrayIndexOutOfBoundsException: Index -1 out of bounds for length 2/java.lang.ArrayIndexOutOfBoundsException: Index -1 out of bounds for length 2/false",
        ),
        (
            "nextChainsTheJarsOwnException",
            "g0 g1 g2 => demo.Probe$MyIOOBE: mine/demo.Probe$MyIOOBE: mine/true",
        ),
    ]);
}

/// `AbstractList.equals` against a host `ArrayList` walks both
/// `listIterator()`s; the host's own list iterator (#133,
/// `host_list_iterator.rs`) answers it, so the body runs to Java's `true`
/// with nothing counted. Before #133 the host's `listIterator()` was a counted
/// gap and the body stopped at its neutral `false`.
#[test]
fn equals_against_a_host_list_walks_its_list_iterator() {
    check(&[("equalsHostListStops", "g0 g1 => true")]);
}
