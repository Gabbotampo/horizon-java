package demo;

import java.util.*;

public final class Rv3 {
  static String ex(Throwable e) { return "EXC " + e.getClass().getName() + ": " + e.getMessage(); }
  static final class Node {
    final String name;
    Node(String name) { this.name = name; }
    @Override public int hashCode() { return 42; }
    @Override public boolean equals(Object o) { return o instanceof Node && ((Node) o).name.equals(name); }
  }
  public static String m1() { try { return "" + Map.of(Optional.of("x"), 7).get(Optional.of("x")); } catch (Throwable e) { return ex(e); } }
  public static String m2() { try { return "" + Map.of(Optional.of(1), 7).get(Optional.of(1)); } catch (Throwable e) { return ex(e); } }
  public static String m3() { try { return "" + Map.of(Optional.of("a,b"), 7).get(Optional.of("a,b")) + Map.of(Optional.of("a,b"), 7).containsKey(Optional.of("a,b")); } catch (Throwable e) { return ex(e); } }
  public static String m4() { try { return "" + Set.of(Optional.of("a,b")).contains(Optional.of("a,b")) + Set.of(Optional.of(List.of(1))).contains(Optional.of(List.of(1))); } catch (Throwable e) { return ex(e); } }
  public static String m5() { try { return "" + List.of(Optional.of(List.of(1))).contains(Optional.of(List.of(1))) + List.of(Optional.of(List.of(1))).indexOf(Optional.of(List.of(1))); } catch (Throwable e) { return ex(e); } }
  public static String m6() { try { return "" + Map.of(Optional.of(List.of(1)), 7).keySet().iterator().next().get(); } catch (Throwable e) { return ex(e); } }
  public static String m7() { try { return "" + Map.of(Optional.of(List.of(1)), 7); } catch (Throwable e) { return ex(e); } }
  public static String m8() { try { return "" + Map.of(List.of(1), 7).get(List.of(1)); } catch (Throwable e) { return ex(e); } }
  public static String m9() { try { return "" + Set.of(Optional.of(new Node("a"))).contains(Optional.of(new Node("a"))) + Set.of(new Node("a")).contains(new Node("a")); } catch (Throwable e) { return ex(e); } }
  public static String e1() { try { return "" + Objects.equals(Map.of("a", 1), new HashMap<>(Map.of("a", 1))) + Objects.equals(List.of(1), new ArrayList<>(List.of(1))); } catch (Throwable e) { return ex(e); } }
  public static String e2() { try { return "" + Optional.of(List.of(1)).equals(Optional.of(new ArrayList<>(List.of(1)))) + Optional.of(Set.of(1)).equals(Optional.of(new HashSet<>(Set.of(1)))) + Optional.of(new HashMap<>(Map.of("a", 1))).equals(Optional.of(Map.of("a", 1))); } catch (Throwable e) { return ex(e); } }
  public static String e3() { try { Map<String, Integer> hm = new HashMap<>(); hm.put("a", 1); return "" + Optional.of(Map.of("a", 1)).equals(Optional.of(hm)) + Map.of("a", 1).equals(hm) + Objects.equals(Map.of("a", 1), hm); } catch (Throwable e) { return ex(e); } }
}
