//! TimSort and `PriorityQueue` as JDK 25 runs them (`tim_sort.rs`,
//! `priority_queue.rs`), with natural order running a jar's `compareTo`
//! (`jar_compare.rs`).
//!
//! Every expected value is OpenJDK 25.0.5's output for the same sources
//! (`javac --release 21`, then `java -cp` calling each method), pasted
//! verbatim. `RandomSort` is generated (`gen_sort.py 1 120`): random
//! sorts and priority-queue operations over `Comparable` records that count
//! their `compareTo` calls, printing the counts and the results.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"package demo;

import java.util.*;
import java.util.stream.*;

public final class Probe {
    record Ver(int n, String tag) implements Comparable<Ver> {
        public int compareTo(Ver o) { return Integer.compare(n, o.n); }
    }
    static int cmpCalls;
    record CVer(int n) implements Comparable<CVer> {
        public int compareTo(CVer o) { cmpCalls++; return Integer.compare(n, o.n); }
    }
    record ThrowCmp(int n) implements Comparable<ThrowCmp> {
        public int compareTo(ThrowCmp o) { throw new IllegalStateException("cmp"); }
    }
    enum Mode { LOW, MID, HIGH }

    /// The reviewer's call counts on #98 (`14` and `5` there).
    public static String compareCounts() {
        cmpCalls = 0;
        List<CVer> l = new ArrayList<>();
        for (int n : new int[] {5, 3, 7, 1, 6, 2, 4}) l.add(new CVer(n));
        l.sort(null);
        int a = cmpCalls; cmpCalls = 0;
        String s = Stream.of(new CVer(4), new CVer(2), new CVer(3), new CVer(1)).sorted().map(c -> "" + c.n()).collect(Collectors.joining());
        return a + "|" + cmpCalls + "|" + s;
    }
    /// A `compareTo` that throws reaches the jar's handler (it aborted the run).
    public static String compareThrows() {
        List<ThrowCmp> l = new ArrayList<>(List.of(new ThrowCmp(1), new ThrowCmp(2)));
        try { l.sort(null); return "sorted"; } catch (IllegalStateException e) { return "threw:" + e.getMessage(); }
    }
    /// `PriorityQueue` polls the least first (it polled in insertion order).
    public static String priorityOrder() {
        PriorityQueue<Ver> q = new PriorityQueue<>();
        q.add(new Ver(2, "b")); q.add(new Ver(1, "a")); q.add(new Ver(9, "e")); q.add(new Ver(5, "f"));
        StringBuilder sb = new StringBuilder();
        for (Ver v : q) sb.append(v.tag());
        sb.append('/');
        while (!q.isEmpty()) sb.append(q.poll().tag());
        PriorityQueue<Integer> r = new PriorityQueue<>(Comparator.reverseOrder());
        r.addAll(List.of(3, 9, 1, 7));
        return sb + "/" + r.peek() + "/" + r;
    }
    /// #98's natural-order cases: `sorted()`, `List.sort(null)`,
    /// `Collections.sort`, `naturalOrder()`, and enums by ordinal.
    public static String naturalOrder() {
        String streamed = Stream.of(new Ver(3, "c"), new Ver(1, "a"), new Ver(2, "b")).sorted().map(v -> v.tag()).collect(Collectors.joining());
        List<Ver> list = new ArrayList<>(List.of(new Ver(9, "i"), new Ver(7, "g"), new Ver(8, "h")));
        list.sort(null);
        int direct = Comparator.<Ver>naturalOrder().compare(new Ver(5, "x"), new Ver(4, "y"));
        String modes = Stream.of(Mode.HIGH, Mode.LOW, Mode.MID).sorted().map(Mode::name).collect(Collectors.joining(","));
        return streamed + "|" + list.get(0).tag() + list.get(1).tag() + list.get(2).tag() + "|" + direct + "|" + modes;
    }
    /// A string compared with a record: the JDK throws `ClassCastException`
    /// from a body the host does not hold; counted.
    public static String mixedNaturalOrder() {
        List<Object> mixed = new ArrayList<>(List.of("s", new Ver(1, "a")));
        try { mixed.sort(null); } catch (ClassCastException e) { return "cce"; }
        return "sorted";
    }

    public static String random_m0() { return RandomSort.m0(); }
    public static String random_m1() { return RandomSort.m1(); }
    public static String random_m2() { return RandomSort.m2(); }
    public static String random_m3() { return RandomSort.m3(); }
    public static String random_m4() { return RandomSort.m4(); }
    public static String random_m5() { return RandomSort.m5(); }
    public static String random_m6() { return RandomSort.m6(); }
    public static String random_m7() { return RandomSort.m7(); }
    public static String random_m8() { return RandomSort.m8(); }
    public static String random_m9() { return RandomSort.m9(); }
    public static String random_m10() { return RandomSort.m10(); }
    public static String random_m11() { return RandomSort.m11(); }
    public static String random_m12() { return RandomSort.m12(); }
    public static String random_m13() { return RandomSort.m13(); }
}
"#;

const RANDOM_SORT: &str = r#"package demo;

import java.util.*;
import java.util.stream.*;

/// Seeded random sort and priority-queue programs (`gen_sort.py 1 120`, the first 14).
final class RandomSort {
    static int calls;
    static int budget = Integer.MAX_VALUE;
    /** Logs (counts) every compareTo; orders by n, ties keep their tag. */
    record C(int n, String tag) implements Comparable<C> {
        public int compareTo(C o) {
            calls++;
            if (--budget < 0) throw new IllegalStateException("budget");
            return Integer.compare(n, o.n);
        }
        @Override public String toString() { return n + tag; }
    }
    static final Comparator<C> DESC = (a, b) -> { calls++; if (--budget < 0) throw new IllegalStateException("budget"); return Integer.compare(b.n(), a.n()); };
    static String it(Iterable<?> c) {
        StringBuilder sb = new StringBuilder();
        for (Object o : c) sb.append(o).append(',');
        return sb.toString();
    }

    public static String m0() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(0, "a"));
        src.add(new C(2, "b"));
        src.add(new C(0, "c"));
        src.add(new C(3, "d"));
        src.add(new C(3, "e"));
        StringBuilder log = new StringBuilder();
        String s; try { s = it(src.stream().sorted().collect(Collectors.toList())); } catch (IllegalStateException e) { s = "threw"; }
        log.append(s).append('|').append(calls);
        return log.toString();
    }
    public static String m1() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(1, "a"));
        src.add(new C(199, "b"));
        src.add(new C(221, "c"));
        StringBuilder log = new StringBuilder();
        List<C> l = new LinkedList<>(src);
        try { budget = 4; l.sort(null); log.append("ok"); } catch (IllegalStateException e) { log.append("threw"); }
        log.append('|').append(calls).append('|').append(it(l));
        return log.toString();
    }
    public static String m2() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(1, "a"));
        src.add(new C(1, "b"));
        src.add(new C(41, "c"));
        src.add(new C(34, "d"));
        src.add(new C(0, "e"));
        src.add(new C(60, "f"));
        src.add(new C(56, "g"));
        src.add(new C(24, "h"));
        src.add(new C(43, "i"));
        src.add(new C(13, "j"));
        src.add(new C(27, "k"));
        src.add(new C(46, "l"));
        src.add(new C(1, "m"));
        src.add(new C(33, "n"));
        src.add(new C(14, "o"));
        src.add(new C(48, "p"));
        src.add(new C(28, "q"));
        src.add(new C(60, "r"));
        src.add(new C(31, "s"));
        src.add(new C(35, "t"));
        src.add(new C(14, "u"));
        src.add(new C(22, "v"));
        src.add(new C(14, "w"));
        src.add(new C(43, "x"));
        src.add(new C(14, "y"));
        src.add(new C(48, "z"));
        src.add(new C(29, "a"));
        src.add(new C(60, "b"));
        src.add(new C(18, "c"));
        src.add(new C(59, "d"));
        src.add(new C(1, "e"));
        StringBuilder log = new StringBuilder();
        String s; try { budget = 143; s = it(src.stream().sorted().collect(Collectors.toList())); } catch (IllegalStateException e) { s = "threw"; }
        log.append(s).append('|').append(calls);
        return log.toString();
    }
    public static String m3() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(917, "a"));
        src.add(new C(760, "b"));
        src.add(new C(738, "c"));
        src.add(new C(340, "d"));
        src.add(new C(123, "e"));
        StringBuilder log = new StringBuilder();
        PriorityQueue<C> q = new PriorityQueue<>(DESC);
        for (C c : src) q.add(c);
        log.append(calls).append('|').append(it(q)).append('|');
        log.append(q.contains(src.get(2))).append(':').append(calls).append(';');
        log.append(q.contains(src.get(3))).append(':').append(calls).append(';');
        log.append(q.peek()).append(':').append(calls).append(';');
        log.append(q.contains(src.get(4))).append(':').append(calls).append(';');
        log.append(q.contains(src.get(0))).append(':').append(calls).append(';');
        log.append(q.poll()).append(':').append(calls).append(';');
        log.append(q.contains(src.get(4))).append(':').append(calls).append(';');
        log.append('|').append(it(q));
        StringBuilder drained = new StringBuilder(); while (!q.isEmpty()) drained.append(q.poll()).append(',');
        log.append('|').append(drained).append(calls);
        return log.toString();
    }
    public static String m4() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(1, "a"));
        src.add(new C(5, "b"));
        src.add(new C(12, "c"));
        src.add(new C(57, "d"));
        src.add(new C(132, "e"));
        src.add(new C(172, "f"));
        src.add(new C(204, "g"));
        src.add(new C(204, "h"));
        src.add(new C(210, "i"));
        src.add(new C(232, "j"));
        src.add(new C(237, "k"));
        src.add(new C(275, "l"));
        src.add(new C(352, "m"));
        src.add(new C(354, "n"));
        src.add(new C(361, "o"));
        src.add(new C(365, "p"));
        src.add(new C(373, "q"));
        src.add(new C(392, "r"));
        src.add(new C(414, "s"));
        src.add(new C(423, "t"));
        src.add(new C(424, "u"));
        src.add(new C(436, "v"));
        src.add(new C(470, "w"));
        src.add(new C(492, "x"));
        src.add(new C(496, "y"));
        src.add(new C(514, "z"));
        src.add(new C(516, "a"));
        src.add(new C(524, "b"));
        src.add(new C(526, "c"));
        src.add(new C(531, "d"));
        src.add(new C(551, "e"));
        src.add(new C(552, "f"));
        src.add(new C(553, "g"));
        src.add(new C(561, "h"));
        src.add(new C(561, "i"));
        src.add(new C(567, "j"));
        src.add(new C(574, "k"));
        src.add(new C(583, "l"));
        src.add(new C(591, "m"));
        src.add(new C(623, "n"));
        src.add(new C(627, "o"));
        src.add(new C(638, "p"));
        src.add(new C(675, "q"));
        src.add(new C(746, "r"));
        src.add(new C(758, "s"));
        src.add(new C(789, "t"));
        src.add(new C(796, "u"));
        src.add(new C(802, "v"));
        src.add(new C(805, "w"));
        src.add(new C(828, "x"));
        src.add(new C(832, "y"));
        src.add(new C(840, "z"));
        src.add(new C(867, "a"));
        src.add(new C(877, "b"));
        src.add(new C(880, "c"));
        src.add(new C(890, "d"));
        src.add(new C(907, "e"));
        src.add(new C(931, "f"));
        src.add(new C(942, "g"));
        src.add(new C(960, "h"));
        src.add(new C(963, "i"));
        src.add(new C(972, "j"));
        src.add(new C(975, "k"));
        src.add(new C(977, "l"));
        src.add(new C(980, "m"));
        StringBuilder log = new StringBuilder();
        List<C> l = new LinkedList<>(src);
        try { l.sort(null); log.append("ok"); } catch (IllegalStateException e) { log.append("threw"); }
        log.append('|').append(calls).append('|').append(it(l));
        return log.toString();
    }
    public static String m5() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(14, "a"));
        src.add(new C(17, "b"));
        src.add(new C(18, "c"));
        src.add(new C(21, "d"));
        src.add(new C(24, "e"));
        src.add(new C(31, "f"));
        src.add(new C(33, "g"));
        src.add(new C(36, "h"));
        src.add(new C(48, "i"));
        src.add(new C(60, "j"));
        src.add(new C(71, "k"));
        src.add(new C(72, "l"));
        src.add(new C(72, "m"));
        src.add(new C(78, "n"));
        src.add(new C(85, "o"));
        src.add(new C(93, "p"));
        src.add(new C(111, "q"));
        src.add(new C(112, "r"));
        src.add(new C(116, "s"));
        src.add(new C(128, "t"));
        src.add(new C(149, "u"));
        src.add(new C(163, "v"));
        src.add(new C(164, "w"));
        src.add(new C(171, "x"));
        src.add(new C(172, "y"));
        src.add(new C(181, "z"));
        src.add(new C(185, "a"));
        src.add(new C(189, "b"));
        src.add(new C(192, "c"));
        src.add(new C(214, "d"));
        src.add(new C(217, "e"));
        src.add(new C(225, "f"));
        src.add(new C(228, "g"));
        src.add(new C(230, "h"));
        src.add(new C(255, "i"));
        src.add(new C(259, "j"));
        src.add(new C(261, "k"));
        src.add(new C(261, "l"));
        src.add(new C(264, "m"));
        src.add(new C(275, "n"));
        src.add(new C(279, "o"));
        src.add(new C(287, "p"));
        src.add(new C(297, "q"));
        src.add(new C(301, "r"));
        src.add(new C(305, "s"));
        src.add(new C(313, "t"));
        src.add(new C(317, "u"));
        src.add(new C(319, "v"));
        src.add(new C(328, "w"));
        src.add(new C(329, "x"));
        src.add(new C(351, "y"));
        src.add(new C(352, "z"));
        src.add(new C(395, "a"));
        src.add(new C(404, "b"));
        src.add(new C(406, "c"));
        src.add(new C(431, "d"));
        src.add(new C(436, "e"));
        src.add(new C(436, "f"));
        src.add(new C(442, "g"));
        src.add(new C(456, "h"));
        src.add(new C(461, "i"));
        src.add(new C(463, "j"));
        src.add(new C(465, "k"));
        src.add(new C(485, "l"));
        src.add(new C(508, "m"));
        src.add(new C(518, "n"));
        src.add(new C(522, "o"));
        src.add(new C(528, "p"));
        src.add(new C(536, "q"));
        src.add(new C(540, "r"));
        src.add(new C(557, "s"));
        src.add(new C(563, "t"));
        src.add(new C(564, "u"));
        src.add(new C(589, "v"));
        src.add(new C(598, "w"));
        src.add(new C(620, "x"));
        src.add(new C(639, "y"));
        src.add(new C(645, "z"));
        src.add(new C(646, "a"));
        src.add(new C(650, "b"));
        src.add(new C(663, "c"));
        src.add(new C(664, "d"));
        src.add(new C(672, "e"));
        src.add(new C(675, "f"));
        src.add(new C(689, "g"));
        src.add(new C(691, "h"));
        src.add(new C(694, "i"));
        src.add(new C(711, "j"));
        src.add(new C(719, "k"));
        src.add(new C(721, "l"));
        src.add(new C(728, "m"));
        src.add(new C(736, "n"));
        src.add(new C(747, "o"));
        src.add(new C(755, "p"));
        src.add(new C(772, "q"));
        src.add(new C(773, "r"));
        src.add(new C(815, "s"));
        src.add(new C(816, "t"));
        src.add(new C(816, "u"));
        src.add(new C(816, "v"));
        src.add(new C(817, "w"));
        src.add(new C(822, "x"));
        src.add(new C(836, "y"));
        src.add(new C(836, "z"));
        src.add(new C(852, "a"));
        src.add(new C(861, "b"));
        src.add(new C(871, "c"));
        src.add(new C(879, "d"));
        src.add(new C(881, "e"));
        src.add(new C(888, "f"));
        src.add(new C(896, "g"));
        src.add(new C(921, "h"));
        src.add(new C(953, "i"));
        src.add(new C(966, "j"));
        src.add(new C(974, "k"));
        src.add(new C(982, "l"));
        src.add(new C(988, "m"));
        src.add(new C(991, "n"));
        src.add(new C(998, "o"));
        src.add(new C(999, "p"));
        StringBuilder log = new StringBuilder();
        List<C> l = new LinkedList<>(src);
        try { budget = 427; l.sort(DESC); log.append("ok"); } catch (IllegalStateException e) { log.append("threw"); }
        log.append('|').append(calls).append('|').append(it(l));
        return log.toString();
    }
    public static String m6() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(8, "a"));
        src.add(new C(0, "b"));
        src.add(new C(9, "c"));
        src.add(new C(3, "d"));
        src.add(new C(9, "e"));
        StringBuilder log = new StringBuilder();
        String s; try { s = it(src.stream().sorted().collect(Collectors.toList())); } catch (IllegalStateException e) { s = "threw"; }
        log.append(s).append('|').append(calls);
        return log.toString();
    }
    public static String m7() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(17, "a"));
        src.add(new C(18, "b"));
        src.add(new C(101, "c"));
        src.add(new C(106, "d"));
        src.add(new C(160, "e"));
        src.add(new C(198, "f"));
        src.add(new C(205, "g"));
        src.add(new C(210, "h"));
        src.add(new C(288, "i"));
        src.add(new C(303, "j"));
        src.add(new C(333, "k"));
        src.add(new C(335, "l"));
        src.add(new C(355, "m"));
        src.add(new C(399, "n"));
        src.add(new C(411, "o"));
        src.add(new C(443, "p"));
        src.add(new C(504, "q"));
        src.add(new C(511, "r"));
        src.add(new C(516, "s"));
        src.add(new C(576, "t"));
        src.add(new C(587, "u"));
        src.add(new C(605, "v"));
        src.add(new C(626, "w"));
        src.add(new C(681, "x"));
        src.add(new C(690, "y"));
        src.add(new C(801, "z"));
        src.add(new C(830, "a"));
        src.add(new C(878, "b"));
        src.add(new C(892, "c"));
        src.add(new C(918, "d"));
        src.add(new C(921, "e"));
        src.add(new C(960, "f"));
        StringBuilder log = new StringBuilder();
        try { Collections.sort(src, DESC); log.append("ok"); } catch (IllegalStateException e) { log.append("threw"); }
        log.append('|').append(calls).append('|').append(it(src));
        return log.toString();
    }
    public static String m8() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(954, "a"));
        src.add(new C(936, "b"));
        src.add(new C(857, "c"));
        src.add(new C(690, "d"));
        src.add(new C(560, "e"));
        src.add(new C(388, "f"));
        src.add(new C(352, "g"));
        src.add(new C(98, "h"));
        StringBuilder log = new StringBuilder();
        PriorityQueue<C> q = new PriorityQueue<>();
        for (C c : src) q.add(c);
        log.append(calls).append('|').append(it(q)).append('|');
        log.append(q.peek()).append(':').append(calls).append(';');
        log.append(q.add(new C(10, "z"))).append(':').append(calls).append(';');
        log.append(q.poll()).append(':').append(calls).append(';');
        log.append('|').append(it(q));
        StringBuilder drained = new StringBuilder(); while (!q.isEmpty()) drained.append(q.poll()).append(',');
        log.append('|').append(drained).append(calls);
        return log.toString();
    }
    public static String m9() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(40, "a"));
        src.add(new C(74, "b"));
        src.add(new C(106, "c"));
        src.add(new C(138, "d"));
        src.add(new C(328, "e"));
        src.add(new C(416, "f"));
        src.add(new C(500, "g"));
        src.add(new C(564, "h"));
        src.add(new C(593, "i"));
        src.add(new C(618, "j"));
        src.add(new C(732, "k"));
        src.add(new C(788, "l"));
        src.add(new C(798, "m"));
        src.add(new C(888, "n"));
        src.add(new C(908, "o"));
        src.add(new C(966, "p"));
        src.add(new C(977, "q"));
        StringBuilder log = new StringBuilder();
        String s; try { budget = 19; s = it(src.stream().sorted().collect(Collectors.toList())); } catch (IllegalStateException e) { s = "threw"; }
        log.append(s).append('|').append(calls);
        return log.toString();
    }
    public static String m10() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(39, "a"));
        src.add(new C(37, "b"));
        src.add(new C(50, "c"));
        src.add(new C(59, "d"));
        src.add(new C(24, "e"));
        src.add(new C(4, "f"));
        src.add(new C(36, "g"));
        src.add(new C(35, "h"));
        src.add(new C(14, "i"));
        src.add(new C(36, "j"));
        src.add(new C(5, "k"));
        src.add(new C(60, "l"));
        src.add(new C(17, "m"));
        src.add(new C(23, "n"));
        src.add(new C(57, "o"));
        src.add(new C(18, "p"));
        src.add(new C(36, "q"));
        src.add(new C(34, "r"));
        src.add(new C(59, "s"));
        src.add(new C(7, "t"));
        src.add(new C(29, "u"));
        src.add(new C(57, "v"));
        src.add(new C(17, "w"));
        src.add(new C(6, "x"));
        src.add(new C(50, "y"));
        src.add(new C(2, "z"));
        src.add(new C(52, "a"));
        src.add(new C(18, "b"));
        src.add(new C(0, "c"));
        src.add(new C(39, "d"));
        src.add(new C(42, "e"));
        StringBuilder log = new StringBuilder();
        try { src.sort(null); log.append("ok"); } catch (IllegalStateException e) { log.append("threw"); }
        log.append('|').append(calls).append('|').append(it(src));
        return log.toString();
    }
    public static String m11() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(1, "a"));
        src.add(new C(1, "b"));
        src.add(new C(4, "c"));
        StringBuilder log = new StringBuilder();
        String s; try { s = it(src.stream().sorted(DESC).collect(Collectors.toList())); } catch (IllegalStateException e) { s = "threw"; }
        log.append(s).append('|').append(calls);
        return log.toString();
    }
    public static String m12() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(105, "a"));
        src.add(new C(162, "b"));
        src.add(new C(445, "c"));
        src.add(new C(761, "d"));
        src.add(new C(865, "e"));
        StringBuilder log = new StringBuilder();
        String s; try { budget = 18; s = it(src.stream().sorted(DESC).collect(Collectors.toList())); } catch (IllegalStateException e) { s = "threw"; }
        log.append(s).append('|').append(calls);
        return log.toString();
    }
    public static String m13() {
        calls = 0; budget = Integer.MAX_VALUE;
        List<C> src = new ArrayList<>();
        src.add(new C(993, "a"));
        src.add(new C(993, "b"));
        src.add(new C(947, "c"));
        src.add(new C(935, "d"));
        src.add(new C(912, "e"));
        src.add(new C(888, "f"));
        src.add(new C(805, "g"));
        src.add(new C(803, "h"));
        src.add(new C(796, "i"));
        src.add(new C(743, "j"));
        src.add(new C(728, "k"));
        src.add(new C(704, "l"));
        src.add(new C(677, "m"));
        src.add(new C(667, "n"));
        src.add(new C(632, "o"));
        src.add(new C(615, "p"));
        src.add(new C(610, "q"));
        src.add(new C(555, "r"));
        src.add(new C(554, "s"));
        src.add(new C(488, "t"));
        src.add(new C(480, "u"));
        src.add(new C(466, "v"));
        src.add(new C(460, "w"));
        src.add(new C(408, "x"));
        src.add(new C(400, "y"));
        src.add(new C(369, "z"));
        src.add(new C(364, "a"));
        src.add(new C(327, "b"));
        src.add(new C(325, "c"));
        src.add(new C(324, "d"));
        src.add(new C(322, "e"));
        src.add(new C(320, "f"));
        src.add(new C(314, "g"));
        src.add(new C(302, "h"));
        src.add(new C(265, "i"));
        src.add(new C(256, "j"));
        src.add(new C(252, "k"));
        src.add(new C(220, "l"));
        src.add(new C(212, "m"));
        src.add(new C(212, "n"));
        src.add(new C(203, "o"));
        src.add(new C(187, "p"));
        src.add(new C(114, "q"));
        src.add(new C(102, "r"));
        src.add(new C(83, "s"));
        src.add(new C(65, "t"));
        src.add(new C(64, "u"));
        src.add(new C(40, "v"));
        src.add(new C(27, "w"));
        src.add(new C(10, "x"));
        StringBuilder log = new StringBuilder();
        List<C> l = new LinkedList<>(src);
        try { l.sort(DESC); log.append("ok"); } catch (IllegalStateException e) { log.append("threw"); }
        log.append('|').append(calls).append('|').append(it(l));
        return log.toString();
    }
}
"#;

fn fixture(test: &str) -> Fixture {
    Fixture::build(
        test,
        &[
            ("demo/Probe.java", PROBE),
            ("demo/RandomSort.java", RANDOM_SORT),
        ],
    )
}

/// Runs each `(method, JDK 25 answer)` and checks that nothing was counted.
fn assert_exact(test: &str, cases: &[(&str, &str)]) {
    let fixture = fixture(test);
    for (method, expected) in cases {
        let mut host = fixture.jar.seed_host(StdHost::new());
        let text = fixture.text_on(&mut host, method);
        assert_eq!(&text, expected, "{method}");
        assert_eq!(
            host.unknown_host_call_count(),
            0,
            "{method}: {:?}",
            host.unknown_host_calls()
        );
    }
}

/// The reviewer's programs on #98: `List.sort(null)` of 7 records makes 13
/// `compareTo` calls and `sorted()` of 4 makes 6 (TimSort's binary
/// insertion; the merge sort made 14 and 5), a `compareTo` that throws
/// reaches the jar's `catch`, and natural order runs the record's
/// `compareTo` (enums by ordinal).
#[test]
fn sorts_call_compare_to_as_tim_sort_does() {
    assert_exact(
        "sorts-call-compare-to-as-tim-sort-does",
        &[
            ("compareCounts", "13|6|1234"),
            ("compareThrows", "threw:cmp"),
            ("naturalOrder", "abc|ghi|1|LOW,MID,HIGH"),
        ],
    );
}

/// `PriorityQueue` iterates its heap array and polls the least element
/// (it was a FIFO list), with natural order or a comparator.
#[test]
fn a_priority_queue_is_a_heap() {
    assert_exact(
        "a-priority-queue-is-a-heap",
        &[("priorityOrder", "abef/abfe/9/[9, 7, 1, 3]")],
    );
}

/// Generated programs: TimSort over 2–120 elements (random, sorted,
/// reversed, runs, few distinct values; merges and galloping), a
/// comparator that throws after a random number of calls (the list keeps
/// `ArrayList.sort`'s part-merged array), and random `PriorityQueue`
/// operations: every answer and every call count matches JDK 25.
#[test]
fn random_sort_programs_match_jdk_25() {
    assert_exact(
        "random-sort-programs-match-jdk-25",
        &[
            ("random_m0", "0a,0c,2b,3d,3e,|8"),
            ("random_m1", "ok|2|1a,199b,221c,"),
            (
                "random_m2",
                "0e,1a,1b,1m,1e,13j,14o,14u,14w,14y,18c,22v,24h,27k,28q,29a,31s,33n,34d,35t,41c,43i,43x,46l,48p,48z,56g,59d,60f,60r,60b,|113",
            ),
            (
                "random_m3",
                "4|917a,760b,738c,340d,123e,|true:4;true:4;917a:4;true:4;true:4;917a:7;true:7;|760b,340d,738c,123e,|760b,738c,340d,123e,10",
            ),
            (
                "random_m4",
                "ok|64|1a,5b,12c,57d,132e,172f,204g,204h,210i,232j,237k,275l,352m,354n,361o,365p,373q,392r,414s,423t,424u,436v,470w,492x,496y,514z,516a,524b,526c,531d,551e,552f,553g,561h,561i,567j,574k,583l,591m,623n,627o,638p,675q,746r,758s,789t,796u,802v,805w,828x,832y,840z,867a,877b,880c,890d,907e,931f,942g,960h,963i,972j,975k,977l,980m,",
            ),
            (
                "random_m5",
                "ok|385|999p,998o,991n,988m,982l,974k,966j,953i,921h,896g,888f,881e,879d,871c,861b,852a,836y,836z,822x,817w,816t,816u,816v,815s,773r,772q,755p,747o,736n,728m,721l,719k,711j,694i,691h,689g,675f,672e,664d,663c,650b,646a,645z,639y,620x,598w,589v,564u,563t,557s,540r,536q,528p,522o,518n,508m,485l,465k,463j,461i,456h,442g,436e,436f,431d,406c,404b,395a,352z,351y,329x,328w,319v,317u,313t,305s,301r,297q,287p,279o,275n,264m,261k,261l,259j,255i,230h,228g,225f,217e,214d,192c,189b,185a,181z,172y,171x,164w,163v,149u,128t,116s,112r,111q,93p,85o,78n,72l,72m,71k,60j,48i,36h,33g,31f,24e,21d,18c,17b,14a,",
            ),
            ("random_m6", "0b,3d,8a,9c,9e,|7"),
            (
                "random_m7",
                "ok|31|960f,921e,918d,892c,878b,830a,801z,690y,681x,626w,605v,587u,576t,516s,511r,504q,443p,411o,399n,355m,335l,333k,303j,288i,210h,205g,198f,160e,106d,101c,18b,17a,",
            ),
            (
                "random_m8",
                "13|98h,352g,388f,690d,857c,936b,560e,954a,|98h:13;true:16;10z:21;|98h,352g,388f,690d,857c,936b,560e,954a,|98h,352g,388f,560e,690d,857c,936b,954a,36",
            ),
            (
                "random_m9",
                "40a,74b,106c,138d,328e,416f,500g,564h,593i,618j,732k,788l,798m,888n,908o,966p,977q,|16",
            ),
            (
                "random_m10",
                "ok|116|0c,2z,4f,5k,6x,7t,14i,17m,17w,18p,18b,23n,24e,29u,34r,35h,36g,36j,36q,37b,39a,39d,42e,50c,50y,52a,57o,57v,59d,59s,60l,",
            ),
            ("random_m11", "4c,1a,1b,|4"),
            ("random_m12", "865e,761d,445c,162b,105a,|4"),
            (
                "random_m13",
                "ok|49|993a,993b,947c,935d,912e,888f,805g,803h,796i,743j,728k,704l,677m,667n,632o,615p,610q,555r,554s,488t,480u,466v,460w,408x,400y,369z,364a,327b,325c,324d,322e,320f,314g,302h,265i,256j,252k,220l,212m,212n,203o,187p,114q,102r,83s,65t,64u,40v,27w,10x,",
            ),
        ],
    );
}

/// A string compared with a record: the JDK throws `ClassCastException`
/// from the record's bridge; the runtime counts the cast it cannot make.
#[test]
fn a_platform_value_compared_with_a_jar_object_is_counted() {
    let fixture = fixture("a-platform-value-compared-with-a-jar-obj");
    let cases: &[(&str, &[(&str, usize)])] = &[(
        "mixedNaturalOrder",
        &[(
            "demo/Probe$Ver#<checkcast>(Ljava/lang/Object;)Ldemo/Probe$Ver;",
            1,
        )],
    )];
    for (method, expected) in cases {
        let mut host = fixture.jar.seed_host(StdHost::new());
        fixture.text_on(&mut host, method);
        let counted = host.unknown_host_calls();
        for (call, count) in *expected {
            assert!(
                counted.iter().any(|(c, n)| c == call && n == count),
                "{method}: {counted:?}"
            );
        }
    }
}
