package demo;

import java.lang.reflect.*;
import java.util.*;

/** Reflection objects as host handles (#156 P10): each method is one case. */
public class Refl {
  public static final int COUNT = 9;
  private transient volatile String[] names;
  protected static long[][] grid;
  int plain;
  enum Mode { ON }

  public Refl() {}
  protected Refl(int a, String... rest) throws java.io.IOException, IllegalStateException {}
  public static int target(int x) throws java.io.IOException { return x * 2; }
  static synchronized String join(int a, String[] b, List<String> c) { return a + b[0] + c; }
  private final void hidden() {}

  public interface Api {
    String name();
    default int size() { return 1; }
    static void util() {}
  }

  public static class Inner {
    public Inner(Refl[] all) {}
  }

  static Method target() throws Exception { return Refl.class.getMethod("target", int.class); }
  static Field count() throws Exception { return Refl.class.getDeclaredField("COUNT"); }
  static Constructor<?> ctor() throws Exception { return Refl.class.getDeclaredConstructor(int.class, String[].class); }

  static String castMethod(Object o) {
    try { return "ok" + ((Method) o).getName(); } catch (ClassCastException e) { return e.getMessage(); }
  }

  /** Strings spelled like the old markers (and the handles' encodings) stay Strings. */
  public static String spelled() throws Exception {
    Object[] texts = {
      "reflect-method/demo/Refl#target(I)I", "reflect-field/demo/Refl#COUNT:I:25", "reflect-field/demo/Refl#COUNT:I:25#3",
      "reflect-constructor/demo/Refl#()V", "hrmth:demo/Refl#target(I)I:9", "hrfld:demo/Refl#COUNT:I:25#3", "hrctr:demo/Refl#()V:1"
    };
    Method m = target();
    Field f = count();
    Constructor<?> k = Refl.class.getConstructor();
    StringBuilder out = new StringBuilder();
    Map<Object, String> map = new HashMap<>();
    for (Object t : texts) {
      out.append(t instanceof Method).append(t instanceof Field).append(t instanceof Constructor)
          .append(t instanceof Member).append(t.equals(m)).append(m.equals(t)).append(f.equals(t)).append(k.equals(t))
          .append(t.equals(t.toString())).append('|').append("" + t).append(',');
      // A `word/rest` string (the old markers) is still not a String to
      // is_plain_string until #156 P17, and any text with a `#` is an object
      // to getClass until P12; the handles' encodings are Strings here.
      if (t.toString().startsWith("h")) {
        out.append(t instanceof String).append(t.hashCode()).append(',');
        map.put(t, "s");
      }
    }
    map.put(m, "m");
    map.put(f, "f");
    return out.append(map.size()).append(map.get(texts[4])).append(map.get(m)).append(map.get(f))
        .append('|').append(castMethod(texts[4])).append('|').append(castMethod(m)).toString();
  }

  /** Java 25's toString of every modifier shape. */
  public static String text() throws Exception {
    Class<?> c = Refl.class;
    Object[] members = {
      target(), c.getDeclaredMethod("join", int.class, String[].class, List.class), c.getDeclaredMethod("hidden"),
      Api.class.getMethod("name"), Api.class.getMethod("size"), Api.class.getMethod("util"),
      count(), c.getDeclaredField("names"), c.getDeclaredField("grid"), c.getDeclaredField("plain"), Mode.class.getField("ON"),
      c.getConstructor(), ctor(), Inner.class.getConstructor(Refl[].class)
    };
    StringBuilder out = new StringBuilder();
    for (Object member : members) {
      out.append(member).append(" #").append(member.hashCode()).append('\n');
    }
    return out.append(String.valueOf(target())).append('|').append("" + count()).toString();
  }

  /** Copies are equal, share a hash, and are each their class. */
  public static String real() throws Exception {
    Method m = target();
    Field f = count();
    Constructor<?> k = ctor();
    Object o = m;
    return "" + m.equals(target()) + f.equals(count()) + k.equals(ctor()) + m.equals(f) + f.equals(m)
        + k.equals(Refl.class.getConstructor()) + m.equals(Refl.class.getDeclaredMethod("hidden"))
        + "|" + (m.hashCode() == target().hashCode()) + (f.hashCode() == count().hashCode())
        + "|" + m.getClass().getName() + "," + f.getClass().getName() + "," + k.getClass().getName()
        + "|" + (o instanceof Executable) + (o instanceof Member) + (o instanceof AccessibleObject)
        + (o instanceof GenericDeclaration) + (o instanceof AnnotatedElement) + (((Object) f) instanceof GenericDeclaration)
        + (((Object) f) instanceof Executable) + (((Object) k) instanceof Executable) + (o instanceof Comparable)
        + "|" + m.invoke(null, 21) + f.get(null) + m.getName() + f.getName() + k.getParameterCount()
        + Refl.class.getConstructor().newInstance().plain;
  }

  /** Collections find a copy by Field/Method equals. */
  public static String collections() throws Exception {
    Field f1 = count(), f2 = count();
    Method m1 = target(), m2 = target();
    List<Object> list = new ArrayList<>(List.of(f1, m1));
    Set<Object> set = new HashSet<>(List.of(f1, m1));
    set.add(f2);
    set.add(m2);
    Map<Object, Integer> map = new HashMap<>();
    map.put(f1, 1);
    map.put(m1, 2);
    return "" + list.contains(f2) + list.indexOf(m2) + set.size() + map.get(f2) + map.get(m2)
        + List.of(f1).contains(f2) + Optional.of(m1).equals(Optional.of(m2)) + list.equals(List.of(f2, m2))
        + "|" + (list.get(0) == f1) + (list.get(0) != f2);
  }

  /** The same Field, Method or Constructor flowing to both sides is one object. */
  public static String sameHandle() throws Exception {
    Method m = target();
    Method n = m;
    Field f = count();
    Object g = f;
    Constructor<?> k = ctor();
    Constructor<?> l = k;
    List<Field> fields = new ArrayList<>(List.of(f));
    fields.get(0).setAccessible(true);
    return "" + (m == n) + (f == g) + (k == l) + (fields.get(0) == f) + f.isAccessible() + count().isAccessible();
  }

  /** Two lookups of one field are two Field copies: never ==. */
  public static String fieldCopies() throws Exception {
    Field a = count(), b = count();
    Map<Object, Integer> identity = new IdentityHashMap<>();
    identity.put(a, 1);
    identity.put(b, 2);
    return "" + (a == b) + (a != b) + identity.size();
  }

  /** Two lookups of one method: equal, not == (counted). */
  public static String methodCopies() throws Exception {
    return "" + (target() == target());
  }

  /** Two lookups of one constructor in an IdentityHashMap: two keys (counted). */
  public static String constructorCopies() throws Exception {
    Map<Object, Integer> identity = new IdentityHashMap<>();
    identity.put(ctor(), 1);
    identity.put(ctor(), 2);
    return "" + identity.size();
  }

  public static void main(String[] args) throws Exception {
    for (String name : new String[] {"spelled", "text", "real", "collections", "sameHandle", "fieldCopies", "methodCopies", "constructorCopies"}) {
      System.out.println("== " + name);
      System.out.println(Refl.class.getMethod(name).invoke(null));
    }
  }
}
