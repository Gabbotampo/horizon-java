package demo;

import java.util.*;
import java.util.function.*;

public final class Rv2 {
  static String ex(Throwable e) { return "EXC " + e.getClass().getName() + ": " + e.getMessage(); }

  static final class Node {
    final String name;
    Node(String name) { this.name = name; }
    @Override public int hashCode() { return 42; }
    @Override public boolean equals(Object o) { return o instanceof Node && ((Node) o).name.equals(name); }
    @Override public String toString() { return "Node(" + name + ")"; }
  }

  // toString of a host collection holding an optional of a jar object
  public static String t1() { try { return String.valueOf(List.of(Optional.of(new Node("a")))); } catch (Throwable e) { return ex(e); } }
  public static String t2() { try { return "" + new ArrayList<>(List.of(Optional.of(new Node("a")))); } catch (Throwable e) { return ex(e); } }
  public static String t3() { try { List<Object> l = new ArrayList<>(); l.add(Optional.of(new Node("a"))); return l.toString(); } catch (Throwable e) { return ex(e); } }
  public static String t4() { try { return Map.of("k", Optional.of(new Node("a"))).toString(); } catch (Throwable e) { return ex(e); } }
  public static String t5() { try { return java.util.stream.Stream.of(Optional.of(new Node("a"))).map(Object::toString).toList().toString(); } catch (Throwable e) { return ex(e); } }
  public static String t6() { try { return String.format("%s", List.of(Optional.of(new Node("a")))); } catch (Throwable e) { return ex(e); } }
  public static String t7() { try { return new StringBuilder().append(List.of(Optional.of(new Node("a")))).toString(); } catch (Throwable e) { return ex(e); } }
  public static String t8() { try { return Optional.of(Optional.of(new Node("a"))).toString(); } catch (Throwable e) { return ex(e); } }
  public static String t9() { try { return "" + Optional.of(List.of(new Node("a"))); } catch (Throwable e) { return ex(e); } }
  public static String t10() { try { return java.util.stream.Stream.of(Optional.of(new Node("a"))).map(String::valueOf).toList().toString(); } catch (Throwable e) { return ex(e); } }
  public static String t11() { try { return String.join(",", java.util.stream.Stream.of(Optional.of(new Node("a"))).map(o -> o.toString()).toList()); } catch (Throwable e) { return ex(e); } }
  // control: jar object directly (no optional)
  public static String t12() { try { return String.valueOf(List.of(new Node("a"))); } catch (Throwable e) { return ex(e); } }
  public static String t13() { try { return java.util.stream.Stream.of(new Node("a")).map(Object::toString).toList().toString(); } catch (Throwable e) { return ex(e); } }

  // hash/equals of host collections holding optionals of jar objects / host collections
  public static String h1() { try { return "" + List.of(Optional.of(new Node("a"))).hashCode(); } catch (Throwable e) { return ex(e); } }
  public static String h2() { try { return "" + List.of(Optional.of(List.of(1, 2))).hashCode(); } catch (Throwable e) { return ex(e); } }
  public static String h3() { try { return "" + Set.of(Optional.of(new Node("a"))).contains(Optional.of(new Node("a"))); } catch (Throwable e) { return ex(e); } }
  public static String h4() { try { return "" + Map.of(Optional.of(List.of(1)), 7).get(Optional.of(List.of(1))); } catch (Throwable e) { return ex(e); } }
  public static String h5() { try { return "" + new HashSet<>(List.of(Optional.of(new ArrayList<>(List.of(1))))).contains(Optional.of(List.of(1))); } catch (Throwable e) { return ex(e); } }
  public static String h6() { try { return "" + List.of(Optional.of(new Node("a"))).equals(List.of(Optional.of(new Node("a")))); } catch (Throwable e) { return ex(e); } }
  public static String h7() { try { return "" + Objects.hash(Optional.of(List.of(1, 2)), Optional.of(new Node("x"))); } catch (Throwable e) { return ex(e); } }
  public static String h8() { try { return "" + Map.of("a", 1).equals(new HashMap<>(Map.of("a", 1))); } catch (Throwable e) { return ex(e); } }
  public static String h9() { try { return "" + Optional.of(Map.of("a", 1)).equals(Optional.of(new HashMap<>(Map.of("a", 1)))); } catch (Throwable e) { return ex(e); } }
  public static String h10() { try { return "" + List.of(OptionalDouble.of(-0.0)).contains(OptionalDouble.of(0.0)) + List.of(Optional.of(-0.0)).contains(Optional.of(0.0)); } catch (Throwable e) { return ex(e); } }

  // identity map lookups
  public static String i1() { try { Map<Object, Integer> m = new IdentityHashMap<>(); m.put(Optional.of(5), 1); return "" + m.containsKey(Optional.of(5)); } catch (Throwable e) { return ex(e); } }
  public static String i2() { try { Map<Object, Integer> m = new IdentityHashMap<>(); m.put(Optional.of(5), 1); return "" + m.get(Optional.of(5)); } catch (Throwable e) { return ex(e); } }
  public static String i3() { try { Map<Object, Integer> m = new IdentityHashMap<>(); m.put(Optional.of(5), 1); m.remove(Optional.of(5)); return "" + m.size(); } catch (Throwable e) { return ex(e); } }
  public static String i4() { try { Map<Object, Integer> m = new IdentityHashMap<>(); Optional<String> o = Optional.of("x"); m.put(o, 1); m.put(o, 2); return m.size() + "," + m.get(o); } catch (Throwable e) { return ex(e); } }
  public static String i5() { try { Map<Object, Integer> m = new IdentityHashMap<>(); Optional<String> o = Optional.of("x"); m.put(o, 1); m.merge(o, 1, Integer::sum); return m.size() + "," + m.get(o); } catch (Throwable e) { return ex(e); } }
  public static String i6() { try { Map<Object, Integer> m = new IdentityHashMap<>(); m.putIfAbsent(Optional.of(5), 1); m.putIfAbsent(Optional.of(5), 2); return m.size() + "," + m.values(); } catch (Throwable e) { return ex(e); } }
  public static String i7() { try { Map<Object, Integer> m = new IdentityHashMap<>(); m.put(Optional.of(5), 1); m.computeIfAbsent(Optional.of(5), k -> 2); return m.size() + "," + m.values(); } catch (Throwable e) { return ex(e); } }
  public static String i8() { try { Map<Object, Integer> m = new IdentityHashMap<>(); m.put(OptionalInt.of(5), 1); return "" + m.containsKey(OptionalInt.of(5)) + new IdentityHashMap<>(Map.of(OptionalInt.of(5), 1)).containsKey(OptionalInt.of(5)); } catch (Throwable e) { return ex(e); } }

  // casts not through checkcast
  public static String c1() { try { Object o = Optional.class.cast(new Node("n")); return "ok"; } catch (ClassCastException e) { return e.getMessage(); } catch (Throwable e) { return ex(e); } }
  public static String c2() { try { Object node = new Node("n"); return "ok" + ((Optional<?>) node).isPresent(); } catch (ClassCastException e) { return e.getMessage(); } catch (Throwable e) { return ex(e); } }
  public static String c3() { try { Object l = List.of(1); return "ok" + ((Optional<?>) l).isPresent(); } catch (ClassCastException e) { return e.getMessage(); } catch (Throwable e) { return ex(e); } }
  public static String c4() { Object node = new Node("n"); return "" + (node instanceof Optional) + (((Object) List.of(1)) instanceof Optional); }
  public static String c5() { try { Object e = Optional.empty(); return "ok" + ((String) e).length(); } catch (ClassCastException e) { return e.getMessage(); } catch (Throwable e) { return ex(e); } }

  // forging: a String through every path that used to build handles from text
  public static String f1() {
    try {
      List<Object> l = new ArrayList<>(List.of("hopt:", "hopt:ri5", "hopti:5", "hoptd:0", "hoptl:7"));
      StringBuilder sb = new StringBuilder();
      for (Object o : l) sb.append(o instanceof String).append(o instanceof Optional).append(',');
      Object[] arr = l.toArray();
      for (Object o : arr) sb.append(o instanceof String);
      Map<Object, Object> m = new HashMap<>();
      for (Object o : l) m.put(o, o);
      for (Object k : m.keySet()) sb.append(k instanceof String);
      Supplier<Object> s = () -> l.get(0);
      sb.append(s.get() instanceof String).append(s.get());
      return sb.toString();
    } catch (Throwable e) { return ex(e); }
  }
  public static String f2() {
    try {
      Object a = "hopt:", b = Optional.empty();
      return "" + a.equals(b) + b.equals(a) + (a.hashCode() == b.hashCode()) + Objects.equals(a, b) + List.of(a).contains(b) + new HashSet<>(List.of(a, b)).size()
          + String.valueOf(a) + a.getClass().getSimpleName();
    } catch (Throwable e) { return ex(e); }
  }
}
