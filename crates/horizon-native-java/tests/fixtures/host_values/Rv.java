package demo;

import java.util.*;
import java.util.function.*;

public final class Rv {
  static String ex(Throwable e) { return "EXC " + e.getClass().getName() + ": " + e.getMessage(); }

  static final class Node {
    final String name;
    Node(String name) { this.name = name; }
    @Override public int hashCode() { return 42; }
    @Override public boolean equals(Object o) { return o instanceof Node && ((Node) o).name.equals(name); }
    @Override public String toString() { return "Node(" + name + ")"; }
  }
  record Rec(String s, int n) {}

  // ---------- A1 neighbours: keys with separators through every container ----------
  public static String a1TreeMap() {
    try {
      TreeMap<Optional<String>, Integer> m = new TreeMap<>(Comparator.comparing((Optional<String> o) -> o.get()));
      m.put(Optional.of("b,\\c"), 1);
      m.put(Optional.of("a,b"), 2);
      m.put(Optional.of("hopt:ra"), 3);
      StringBuilder sb = new StringBuilder();
      for (Optional<String> k : m.keySet()) sb.append(k.get()).append('|').append(k.get().length()).append(';');
      return sb + "" + m.firstKey().get() + m.get(Optional.of("a,b")) + m;
    } catch (Throwable e) { return ex(e); }
  }
  public static String a1LinkedHashMap() {
    try {
      Map<Object, Object> m = new LinkedHashMap<>();
      m.put(Optional.of("a,b"), Optional.of("v\\,w"));
      m.put(Optional.of("c\\"), "x,y");
      m.put(Optional.of(""), Optional.empty());
      StringBuilder sb = new StringBuilder();
      for (Map.Entry<Object, Object> e : m.entrySet()) {
        Optional<?> k = (Optional<?>) e.getKey();
        sb.append(k.get()).append('=').append(e.getValue()).append(';');
      }
      return sb + "|" + m + "|" + m.get(Optional.of("a,b")) + m.containsKey(Optional.of("c\\")) + m.containsValue(Optional.of("v\\,w"));
    } catch (Throwable e) { return ex(e); }
  }
  public static String a1Nested3() {
    try {
      Optional<Optional<Optional<String>>> o = Optional.of(Optional.of(Optional.of("a\\,b,c\\\\")));
      List<Object> l = new ArrayList<>();
      l.add(o);
      l.add(List.of(o));
      @SuppressWarnings("unchecked")
      Optional<Optional<Optional<String>>> back = (Optional<Optional<Optional<String>>>) l.get(0);
      @SuppressWarnings("unchecked")
      Optional<Optional<Optional<String>>> back2 = (Optional<Optional<Optional<String>>>) ((List<Object>) l.get(1)).get(0);
      return back.get().get().get() + "|" + back2.get().get().get() + "|" + back.equals(o) + back2.equals(o) + "|" + l
          + "|" + back.hashCode() + "," + o.hashCode();
    } catch (Throwable e) { return ex(e); }
  }
  public static String a1Every() {
    try {
      String[] texts = {",", "\\", "\\\\", "\\,", ",\\", ":", "h", "r", "hopt:", "hopt:rx", "hopti:5", "null", "void", "", "\0", "a\u0000,b", "é😀,"};
      List<Object> l = new ArrayList<>();
      for (String t : texts) l.add(Optional.of(t));
      Set<Object> s = new HashSet<>(l);
      Map<Object, String> m = new HashMap<>();
      for (String t : texts) m.put(Optional.of(t), t);
      StringBuilder sb = new StringBuilder();
      int bad = 0;
      for (int i = 0; i < texts.length; i++) {
        Object got = ((Optional<?>) l.get(i)).get();
        if (!texts[i].equals(got)) { bad++; sb.append("[" + i + "]"); }
        if (!s.contains(Optional.of(texts[i]))) { bad++; sb.append("s" + i); }
        if (!texts[i].equals(m.get(Optional.of(texts[i])))) { bad++; sb.append("m" + i); }
      }
      return "bad=" + bad + sb + "|" + s.size() + "," + m.size() + "|" + l.get(5) + l.get(9) + l.get(13);
    } catch (Throwable e) { return ex(e); }
  }
  public static String a1Entry() {
    try {
      List<Object> l = new ArrayList<>();
      l.add(Map.entry(Optional.of("k,v"), Optional.of("w\\")));
      Map.Entry<?, ?> e = (Map.Entry<?, ?>) l.get(0);
      return ((Optional<?>) e.getKey()).get() + "|" + ((Optional<?>) e.getValue()).get() + "|" + l;
    } catch (Throwable e) { return ex(e); }
  }
  public static String a1Array() {
    try {
      Object[] a = {Optional.of("a,b"), Optional.of("c\\")};
      Object[] b = a.clone();
      return ((Optional<?>) a[0]).get() + "|" + ((Optional<?>) b[1]).get() + "|" + Arrays.toString(a) + Arrays.asList(a).indexOf(Optional.of("c\\"));
    } catch (Throwable e) { return ex(e); }
  }
  public static String a1Capture() {
    try {
      Optional<String> o = Optional.of("a,b\\");
      Supplier<String> s = () -> o.get() + "!";
      Function<String, String> f = x -> x + o.get();
      return s.get() + "|" + f.apply("z,") + "|" + Optional.of("q,r").map(x -> x + o.get()).get();
    } catch (Throwable e) { return ex(e); }
  }
  public static String a1Stream() {
    try {
      List<Optional<String>> l = List.of(Optional.of("a,b"), Optional.empty(), Optional.of("c\\"));
      return l.stream().filter(Optional::isPresent).map(Optional::get).toList() + "|" + l.stream().map(o -> o.orElse("-")).reduce("", (x, y) -> x + y)
          + "|" + l.stream().findFirst().get().get();
    } catch (Throwable e) { return ex(e); }
  }

  // ---------- A2 neighbours: primitive optionals edge values ----------
  static double div(double a, double b) { return a / b; }
  public static String a2Double() {
    try {
      OptionalDouble nz = OptionalDouble.of(-0.0), pz = OptionalDouble.of(0.0);
      return nz + "|" + pz + "|" + (1 / nz.getAsDouble()) + "|" + nz.equals(pz) + pz.equals(nz) + "|" + nz.hashCode() + "," + pz.hashCode()
          + "|" + OptionalDouble.of(Double.MIN_VALUE) + OptionalDouble.of(Double.MAX_VALUE) + OptionalDouble.of(Double.NEGATIVE_INFINITY)
          + OptionalDouble.of(1e-7) + OptionalDouble.of(1e21) + OptionalDouble.of(100.0)
          + "|" + OptionalDouble.of(Double.POSITIVE_INFINITY).hashCode() + "," + OptionalDouble.of(Double.MIN_VALUE).hashCode()
          + "|" + new HashSet<>(List.of(nz, pz, OptionalDouble.of(div(0, 0)), OptionalDouble.of(Double.NaN), OptionalDouble.empty())).size()
          + "|" + Optional.of(-0.0).equals(Optional.of(0.0)) + Optional.of(div(0, 0)).equals(Optional.of(Double.NaN))
          + Optional.of(-0.0).hashCode() + "|" + Optional.of(0.0f).equals(Optional.of(-0.0f)) + Optional.of(Float.NaN).equals(Optional.of(div(0, 0) > 0 ? 1f : Float.intBitsToFloat(0x7fc00001)));
    } catch (Throwable e) { return ex(e); }
  }
  public static String a2LongInt() {
    try {
      OptionalLong mn = OptionalLong.of(Long.MIN_VALUE), mx = OptionalLong.of(Long.MAX_VALUE), z = OptionalLong.of(0);
      OptionalInt imn = OptionalInt.of(Integer.MIN_VALUE), iz = OptionalInt.of(0), neg = OptionalInt.of(-1);
      return mn + "" + mx + z + imn + iz + neg + "|" + mn.hashCode() + "," + mx.hashCode() + "," + z.hashCode() + "," + OptionalLong.of(-1).hashCode()
          + "," + imn.hashCode() + "," + iz.hashCode() + "|" + z.equals(OptionalLong.empty()) + iz.equals(OptionalInt.empty())
          + iz.equals(OptionalLong.of(0)) + "|" + new HashSet<>(List.of(z, OptionalLong.empty(), iz, OptionalInt.empty())).size()
          + "|" + Optional.of(5).equals(Optional.of(5L)) + Optional.of((short) 5).equals(Optional.of(5)) + Optional.of('a').equals(Optional.of("a"))
          + Optional.of((byte) 1).equals(Optional.of((byte) 1)) + "|" + mn.getAsLong() + imn.getAsInt();
    } catch (Throwable e) { return ex(e); }
  }
  public static String a2Streams() {
    try {
      OptionalDouble avg = java.util.stream.IntStream.of(1, 2).average();
      OptionalInt mx = java.util.stream.IntStream.of(3, 9, 4).max();
      Optional<Integer> ff = java.util.stream.Stream.of(7, 8).findFirst();
      return avg + "" + avg.equals(OptionalDouble.of(1.5)) + "|" + mx + mx.equals(OptionalInt.of(9)) + "|" + ff + ff.equals(Optional.of(7))
          + ff.hashCode() + "|" + new HashSet<>(List.of(ff, Optional.of(7))).size();
    } catch (Throwable e) { return ex(e); }
  }

  // ---------- A3 neighbours: held jar objects and host collections ----------
  public static String a3Map() {
    try {
      Map<Object, Integer> m = new HashMap<>();
      m.put(Optional.of(new Node("a")), 1);
      m.put(Optional.of(new ArrayList<>(List.of(1))), 2);
      m.put(Optional.of(new Rec("r", 1)), 3);
      return m.get(Optional.of(new Node("a"))) + "," + m.get(Optional.of(new ArrayList<>(List.of(1)))) + "," + m.get(Optional.of(new Rec("r", 1)))
          + "," + m.size() + "|" + Optional.of(new Rec("r", 1)).hashCode() + "," + new Rec("r", 1).hashCode()
          + "|" + Optional.of(new Rec("r", 1)).equals(Optional.of(new Rec("r", 1))) + Optional.of(new Rec("r", 1));
    } catch (Throwable e) { return ex(e); }
  }
  public static String a3Text() {
    try {
      Optional<Node> a = Optional.of(new Node("a"));
      List<Object> host = List.of(a);
      List<Object> mut = new ArrayList<>(host);
      return String.valueOf(host) + "|" + mut + "|" + ("" + mut) + "|" + String.format("%s/%s", a, mut) + "|" + new StringBuilder().append(a).append(mut)
          + "|" + Objects.toString(a) + "|" + java.util.stream.Stream.of(a).map(Object::toString).toList() + "|" + Map.of("k", a);
    } catch (Throwable e) { return ex(e); }
  }
  public static String a3Collections() {
    try {
      Optional<List<Integer>> ol = Optional.of(List.of(1, 2));
      Optional<Map<String, Integer>> om = Optional.of(Map.of("a", 1));
      Optional<Set<String>> os = Optional.of(Set.of("x"));
      return ol.hashCode() + "," + om.hashCode() + "," + os.hashCode() + "|" + ol.equals(Optional.of(new ArrayList<>(List.of(1, 2))))
          + om.equals(Optional.of(new HashMap<>(Map.of("a", 1)))) + "|" + Objects.hash(ol, om) + "|" + ol + om + os
          + "|" + Objects.equals(ol, Optional.of(List.of(1, 2))) + Objects.hashCode(ol) + "|" + Optional.of(List.of("a,b")).get().get(0);
    } catch (Throwable e) { return ex(e); }
  }
  static final class NoHash { }
  public static String a3Identity() {
    try {
      NoHash n = new NoHash();
      Optional<NoHash> o = Optional.of(n);
      return (o.hashCode() == n.hashCode()) + "|" + o.equals(Optional.of(n)) + o.equals(Optional.of(new NoHash()));
    } catch (Throwable e) { return ex(e); }
  }

  // ---------- B5 neighbours: casts ----------
  static String c(Supplier<Object> s) { try { Object o = s.get(); return "ok:" + (o == null ? "null" : o.getClass().getName()); } catch (ClassCastException e) { return e.getMessage(); } catch (Throwable e) { return ex(e); } }
  public static String b5Arrays() {
    Object e = Optional.empty();
    return c(() -> (String[]) e) + "|" + c(() -> (Object[]) e) + "|" + c(() -> (int[]) e);
  }
  public static String b5Jar() {
    Object e = Optional.empty();
    Object node = new Node("n");
    return c(() -> (Node) e) + "|" + c(() -> (Optional<?>) node) + "|" + c(() -> (Rec) (Object) OptionalInt.of(1));
  }
  public static String b5Others() {
    Object arr = new String[0];
    Object iarr = new int[0];
    Object list = List.of(1);
    Object alist = new ArrayList<>();
    Object cls = String.class;
    Object entry = Map.entry("a", "b");
    return c(() -> (Optional<?>) arr) + "|" + c(() -> (Optional<?>) iarr) + "|" + c(() -> (Optional<?>) list) + "|" + c(() -> (Optional<?>) alist)
        + "|" + c(() -> (Optional<?>) cls) + "|" + c(() -> (OptionalInt) entry);
  }
  public static String b5Boxes() {
    Object i = 5, l = 5L, ch = 'c', d = 1.5, f = 1.5f, b = true;
    return c(() -> (Optional<?>) i) + "|" + c(() -> (OptionalInt) l) + "|" + c(() -> (OptionalLong) ch) + "|" + c(() -> (OptionalDouble) d)
        + "|" + c(() -> (Optional<?>) f) + "|" + c(() -> (Optional<?>) b);
  }
  public static String b5Ifaces() {
    Object e = Optional.empty(), i = OptionalInt.of(1);
    return c(() -> (Runnable) e) + "|" + c(() -> (Supplier<?>) e) + "|" + c(() -> (java.io.Serializable) e) + "|" + c(() -> (Iterable<?>) i)
        + "|" + c(() -> (CharSequence) e) + "|" + c(() -> (Comparable<?>) i) + "|" + c(() -> (Number) i);
  }
  public static String b5Null() {
    Object n = null;
    Object e = Optional.empty();
    return c(() -> (Optional<?>) n) + "|" + (n instanceof Optional) + "|" + (e instanceof java.io.Serializable) + (e instanceof Iterable)
        + "|" + Optional.class.isInstance(e) + Optional.class.isInstance("x") + OptionalInt.class.isInstance(e)
        + "|" + (e.getClass() == Optional.class) + e.getClass().getSimpleName() + "|" + c(() -> Optional.class.cast("optional"))
        + "|" + c(() -> String.class.cast(e));
  }
  public static String b5Spelled() {
    Object s1 = "hopt:", s2 = "hopti:5", s3 = "optional", s4 = "optional-int/5", s5 = "Optional.empty";
    return c(() -> (Optional<?>) s1) + "|" + c(() -> (OptionalInt) s2) + "|" + c(() -> (Optional<?>) s3) + "|" + c(() -> (OptionalInt) s4)
        + "|" + c(() -> (Optional<?>) s5);
  }

  // ---------- B6 / identity ----------
  public static String b6IdentityMapOps() {
    try {
      Map<Object, Integer> m = new IdentityHashMap<>();
      Optional<Integer> five = Optional.of(5);
      m.put(five, 1);
      return m.containsKey(Optional.of(5)) + "," + m.get(Optional.of(5)) + "," + m.remove(Optional.of(5)) + "," + m.size() + "," + m.containsKey(five);
    } catch (Throwable e) { return ex(e); }
  }
  public static String b6IdentityMapSame() {
    try {
      Map<Object, Integer> m = new IdentityHashMap<>();
      Optional<String> o = Optional.of("x");
      m.put(o, 1);
      m.put(o, 2);
      m.put(Optional.empty(), 3);
      m.put(Optional.empty(), 4);
      return m.size() + "," + m.get(o) + "," + m.get(Optional.empty());
    } catch (Throwable e) { return ex(e); }
  }
  public static String b6IdentitySet() {
    try {
      Set<Object> s = Collections.newSetFromMap(new IdentityHashMap<>());
      s.add(Optional.of(5));
      s.add(Optional.of(5));
      return s.size() + "," + s.contains(Optional.of(5));
    } catch (Throwable e) { return ex(e); }
  }
  public static String b6IdHash() {
    try {
      return "" + (System.identityHashCode(Optional.of(5)) == System.identityHashCode(Optional.of(5)));
    } catch (Throwable e) { return ex(e); }
  }
  public static String idDifferentKeys() {
    Optional<String> a = Optional.of("a"), b = Optional.of("b");
    Object x = Optional.empty(), y = OptionalInt.empty();
    return "" + (a == b) + (a != b) + (x == y) + (OptionalInt.of(1) == (Object) OptionalLong.of(1));
  }
  public static String idThroughList() {
    Optional<String> a = Optional.of("a");
    List<Object> l = new ArrayList<>();
    l.add(a);
    Object back = l.get(0);
    return "" + (back == a) + (Optional.of(5).filter(x -> true) == Optional.of(5));
  }
  public static String idOfIntBox() {
    // the same Optional object, read twice through a stream model
    Optional<Integer> ff = java.util.stream.Stream.of(7).findFirst();
    Object o1 = ff;
    List<Object> l = new ArrayList<>(List.of(o1));
    return "" + (l.get(0) == ff) + (l.indexOf(ff));
  }
}
