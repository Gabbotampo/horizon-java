package demo;

import java.util.*;

public final class Rv4 {
  static String ex(Throwable e) { return "EXC " + e.getClass().getName(); }
  static final class Node {
    final String name;
    Node(String name) { this.name = name; }
    @Override public int hashCode() { return 42; }
    @Override public boolean equals(Object o) { return o instanceof Node && ((Node) o).name.equals(name); }
    @Override public String toString() { return "Node(" + name + ")"; }
  }
  static Optional<String> nul() { return null; }
  static OptionalInt nuli() { return null; }
  public static String n1() { try { return "" + nul().hashCode(); } catch (Throwable e) { return ex(e); } }
  public static String n2() { try { return "" + nul().toString(); } catch (Throwable e) { return ex(e); } }
  public static String n3() { try { return "" + nul().equals(Optional.empty()); } catch (Throwable e) { return ex(e); } }
  public static String n4() { try { return "" + nuli().hashCode(); } catch (Throwable e) { return ex(e); } }
  public static String n5() { try { return "" + nul().isPresent(); } catch (Throwable e) { return ex(e); } }
  public static String o1() { try { Optional<Optional<Node>> o = Optional.of(Optional.of(new Node("a"))); return o.hashCode() + "," + o.equals(Optional.of(Optional.of(new Node("a")))) + "," + new HashSet<>(List.of(o)).contains(Optional.of(Optional.of(new Node("a")))); } catch (Throwable e) { return ex(e); } }
}
