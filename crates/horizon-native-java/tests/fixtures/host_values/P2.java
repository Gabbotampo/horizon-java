package demo;

import java.io.File;
import java.nio.file.*;
import java.util.*;
import net.neoforged.fml.loading.FMLPaths;

/**
 * #156 P2 (first half): paths, files, FMLPaths and the watch service as host
 * handles. Each method is one case; the expected strings are what OpenJDK
 * 25.0.5 printed for it.
 */
public final class P2 {
  static String ex(Throwable e) { return "EXC " + e.getClass().getName(); }

  /** Strings spelled like an old marker with no `word/` head stay Strings. */
  static String plain(Object t, Object real) {
    Map<Object, String> m = new HashMap<>();
    m.put(t, "s");
    m.put(real, "r");
    return t.getClass().getName() + (t instanceof String) + t.equals(real) + real.equals(t) + t.hashCode()
        + "|" + ("" + t) + "|" + t.toString() + "|" + m.size() + m.get(t) + m.get(real);
  }

  /** The same, for a spelling with a `word/` head (getClass/instanceof/hashCode wait for #156 P17). */
  static String worded(Object t, Object real) {
    Map<Object, String> m = new HashMap<>();
    m.put(t, "s");
    m.put(real, "r");
    return t.equals(real) + "" + real.equals(t) + "|" + ("" + t) + "|" + t.toString() + "|" + m.size() + m.get(t) + m.get(real);
  }

  static String wordedClass(Object t) {
    return t.getClass().getName() + (t instanceof String) + t.hashCode();
  }

  public static String pathSpelled() {
    try {
      return plain("path:/horizon/game", Path.of("/horizon/game")) + "|" + Path.of("path:/x") + Path.of("path:/x").getFileName()
          + Path.of("path:/x").getParent() + "|" + ("path:/x" instanceof Comparable);
    } catch (Throwable e) { return ex(e); }
  }

  public static String pathReal() {
    try {
      Path p = Path.of("/horizon//game/");
      Path q = Path.of("/horizon/game");
      Object o = p;
      Set<Path> set = new HashSet<>(List.of(p));
      set.add(q);
      return p + "|" + p.toString() + "|" + o.getClass().getName() + (o instanceof Path) + (o instanceof Comparable)
          + (o instanceof Iterable) + (o instanceof Watchable) + (o instanceof String) + "|" + p.equals(q) + q.equals(p)
          + p.equals("/horizon/game") + p.equals(Path.of("/horizon/gam")) + "|" + p.hashCode() + "," + Path.of("a").hashCode()
          + "," + Path.of("").hashCode() + "," + Path.of("/").hashCode() + "|[" + Path.of("") + "]" + Path.of("rel/x/")
          + Path.of("a", "b", "", "c/") + Paths.get("//d//") + "|" + p.resolve("x") + p.getFileName() + p.getParent()
          + p.isAbsolute() + Path.of("a").isAbsolute() + "|" + set.size() + set.contains(Path.of("/horizon/game/"))
          + List.of(p).indexOf(q) + new ArrayList<>(List.of(q)).contains(p) + "|" + Objects.hash(p, q);
    } catch (Throwable e) { return ex(e); }
  }

  public static String pathSameHandle() {
    Path a = Path.of("x");
    Path b = a;
    Map<Object, Integer> m = new IdentityHashMap<>();
    m.put(a, 1);
    m.put(b, 2);
    return "" + (a == b) + (a != b) + m.size() + m.get(a);
  }

  /** Two Path.of("x") are equal values and two objects: identity is counted. */
  public static String pathTwo() { return "" + (Path.of("x") == Path.of("x")); }

  public static String pathIdMap() {
    Map<Object, Integer> m = new IdentityHashMap<>();
    m.put(Path.of("x"), 1);
    m.put(Path.of("x"), 2);
    return "" + m.size();
  }

  public static String fileSpelled() {
    try { return plain("file:/a/b", new File("/a/b")); } catch (Throwable e) { return ex(e); }
  }

  public static String fileReal() {
    try {
      File f = new File("/a//b/");
      File g = new File("/a/b");
      Object o = f;
      Set<File> set = new HashSet<>(List.of(f));
      set.add(g);
      return f + "|" + f.getPath() + "|" + f.equals(g) + g.equals(f) + f.equals("/a/b") + f.equals(new File("/a/c")) + "|"
          + f.hashCode() + "," + new File("a").hashCode() + "," + new File("").hashCode() + "|" + new File("a", "b/")
          + new File(new File("a/"), "b") + "|" + set.size() + set.contains(new File("/a/b")) + "|" + f.getName() + f.getParent()
          + "|" + f.toPath().equals(Path.of("/a/b")) + Path.of("/a/b").toFile().equals(f) + "|" + (o instanceof File)
          + (o instanceof Comparable) + o.getClass().getName() + "|" + (new File("x") == new File("x"));
    } catch (Throwable e) { return ex(e); }
  }

  public static String fmlSpelled() {
    try { return worded("fml-path/GAMEDIR/path:/horizon/game", FMLPaths.GAMEDIR); } catch (Throwable e) { return ex(e); }
  }

  public static String fmlReal() {
    try {
      Object g = FMLPaths.GAMEDIR;
      Map<Object, Integer> m = new IdentityHashMap<>();
      m.put(FMLPaths.GAMEDIR, 1);
      m.put(FMLPaths.GAMEDIR, 2);
      return g + "|" + ("" + FMLPaths.CONFIGDIR) + "|" + (g == FMLPaths.GAMEDIR) + (g == FMLPaths.MODSDIR) + g.equals(FMLPaths.GAMEDIR)
          + g.equals(FMLPaths.CONFIGDIR) + g.equals("GAMEDIR") + "|" + (g instanceof Enum) + (g instanceof Comparable)
          + (g instanceof String) + g.getClass().getName() + "|" + m.size();
    } catch (Throwable e) { return ex(e); }
  }

  public static String watchSpelled() {
    try {
      return worded("watch-kind/ENTRY_CREATE", StandardWatchEventKinds.ENTRY_CREATE) + "|"
          + "watch-kind/ENTRY_CREATE".equals(StandardWatchEventKinds.ENTRY_CREATE.toString());
    } catch (Throwable e) { return ex(e); }
  }

  public static String watchKinds() {
    try {
      Object c = StandardWatchEventKinds.ENTRY_CREATE;
      Map<Object, Integer> m = new IdentityHashMap<>();
      m.put(StandardWatchEventKinds.ENTRY_MODIFY, 1);
      m.put(StandardWatchEventKinds.ENTRY_MODIFY, 2);
      return c + "|" + ("" + StandardWatchEventKinds.ENTRY_MODIFY) + StandardWatchEventKinds.OVERFLOW.name() + "|"
          + (c == StandardWatchEventKinds.ENTRY_CREATE) + (c == StandardWatchEventKinds.ENTRY_DELETE)
          + c.equals(StandardWatchEventKinds.ENTRY_CREATE) + c.equals("ENTRY_CREATE") + "|" + (c instanceof WatchEvent.Kind)
          + (c instanceof String) + c.getClass().getName() + "|" + m.size();
    } catch (Throwable e) { return ex(e); }
  }

  public static String watchService() {
    try {
      Path dir = Files.createDirectories(Path.of("p2-watch"));
      WatchService ws = FileSystems.getDefault().newWatchService();
      WatchKey k1 = dir.register(ws, StandardWatchEventKinds.ENTRY_CREATE);
      WatchKey k2 = dir.register(ws, StandardWatchEventKinds.ENTRY_CREATE, StandardWatchEventKinds.ENTRY_MODIFY);
      Object s = ws;
      Object k = k1;
      String out = "" + (k1 == k2) + k1.equals(k2) + (s instanceof WatchService) + (s instanceof AutoCloseable)
          + (k instanceof WatchKey) + (k instanceof String) + k1.watchable().equals(dir) + k1.isValid();
      Files.writeString(dir.resolve("a.txt"), "x");
      WatchKey got = ws.take();
      WatchEvent<?> event = got.pollEvents().get(0);
      Object e = event;
      out += "|" + (got == k1) + got.equals(k2) + (event.kind() == StandardWatchEventKinds.ENTRY_CREATE) + event.context()
          + (event.context() instanceof Path) + (e instanceof WatchEvent) + (e == event) + e.equals(event)
          + "|" + s.getClass().getName() + "," + k.getClass().getName() + "," + e.getClass().getName();
      ws.close();
      out += "|" + k1.isValid();
      Files.delete(dir.resolve("a.txt"));
      Files.delete(dir);
      return out;
    } catch (Throwable e) { return ex(e); }
  }

  /** A handle's class reaches its kind's row from every class check (#164 B1). */
  public static String classSurface() {
    Object p = Path.of("/x");
    Object k = StandardWatchEventKinds.ENTRY_CREATE;
    Class<?> c = p.getClass();
    String sw = switch (p) { case Comparable<?> x when x instanceof Watchable -> "CW"; default -> "D"; };
    return "" + Path.class.isInstance(p) + Comparable.class.isInstance(p) + Iterable.class.isInstance(p) + String.class.isInstance(p)
        + "|" + Path.class.isAssignableFrom(c) + Watchable.class.isAssignableFrom(c) + Iterable.class.isAssignableFrom(c)
        + Comparable.class.isAssignableFrom(c) + String.class.isAssignableFrom(c) + Object.class.isAssignableFrom(c)
        + "|" + WatchEvent.Kind.class.isAssignableFrom(k.getClass()) + Path.class.isAssignableFrom(k.getClass()) + "|" + sw;
  }

  /** A kind's class knows its supertypes, not which are direct: counted. */
  public static String classDirect() {
    Class<?> c = Path.of("/x").getClass();
    return c.getSuperclass().getName() + "|" + c.getInterfaces().length;
  }

  /** UnixPath.compareTo (byte order) in every caller of natural order (#164 B2). */
  public static String pathOrder() {
    List<Path> l = new ArrayList<>(List.of(Path.of("b"), Path.of("a/c"), Path.of("a"), Path.of("/z")));
    Collections.sort(l);
    TreeSet<Path> t = new TreeSet<>(List.of(Path.of("y"), Path.of("x")));
    Comparable<Path> c = Path.of("ab");
    TreeMap<Path, Integer> m = new TreeMap<>();
    m.put(Path.of("k2"), 1);
    m.put(Path.of("k1"), 2);
    return l + "|" + t + "|" + Path.of("a").compareTo(Path.of("abc")) + "," + c.compareTo(Path.of("aa")) + "|"
        + java.util.stream.Stream.of(Path.of("q"), Path.of("p")).sorted().toList() + "|" + m.firstKey() + m.lastKey();
  }

  /** A non-ASCII path's bytes are the machine's `sun.jnu.encoding`. */
  public static String pathNonAscii() { return "" + Path.of("\u00e9").hashCode(); }

  /** Spellings with a `word/` head: is_plain_string still reads them as markers (#156 P17). */
  public static String wordedClasses() {
    return wordedClass("watch-kind/ENTRY_CREATE") + wordedClass("fml-path/GAMEDIR/path:/horizon/game");
  }
}
