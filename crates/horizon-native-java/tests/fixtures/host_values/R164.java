package demo;

import java.io.File;
import java.nio.file.*;
import java.util.*;
import net.neoforged.fml.loading.FMLPaths;

/** Review of horizon-java #164: one static String method per case. */
public final class R164 {
  static String ex(Throwable e) { return "EXC " + e.getClass().getName() + ":" + e.getMessage(); }

  // ---------- 1. forgery ----------
  static String forged(String t) {
    Object o = t;
    Map<Object, String> m = new HashMap<>();
    m.put(t, "s");
    return o.getClass().getName() + "," + (o instanceof String) + "," + (o instanceof Path) + "," + (o instanceof Comparable)
        + "," + o.hashCode() + "," + ("" + o) + "," + String.valueOf(o) + "," + o.toString().length() + "," + m.get(t)
        + "," + List.of(o) + "," + Objects.toString(o) + "," + new StringBuilder().append(o) + "," + String.format("%s", o);
  }
  public static String fPath() { return forged("path:/horizon/game"); }
  public static String fPathEmpty() { return forged("path:"); }
  public static String fPathRel() { return forged("path:config"); }
  public static String fHPath() { return forged("hpath:/x"); }
  public static String fFml() { return forged("fml-path/GAMEDIR"); }
  public static String fFmlLong() { return forged("fml-path/GAMEDIR/path:/horizon/game"); }
  public static String fHFml() { return forged("hfmlp:GAMEDIR"); }
  public static String fWatchKind() { return forged("watch-kind/ENTRY_CREATE"); }
  public static String fWatchSvc() { return forged("watch-service/1"); }
  public static String fWatchKey() { return forged("watch-key/1"); }
  public static String fWatchEvt() { return forged("watch-event/1"); }
  public static String fHWkind() { return forged("hwkind:ENTRY_CREATE"); }
  public static String fHWkey() { return forged("hwkey:1"); }
  public static String fFile() { return forged("file:/a/b"); }

  /** The forged text next to the real object. */
  public static String fVsReal() {
    Object s = "path:/x";
    Object p = Path.of("/x");
    Object w = "watch-kind/ENTRY_CREATE";
    Object k = StandardWatchEventKinds.ENTRY_CREATE;
    Object g = "fml-path/GAMEDIR";
    Object f = FMLPaths.GAMEDIR;
    Map<Object, Integer> m = new HashMap<>();
    m.put(s, 1); m.put(p, 2); m.put(w, 3); m.put(k, 4); m.put(g, 5); m.put(f, 6);
    return "" + s.equals(p) + p.equals(s) + w.equals(k) + k.equals(w) + g.equals(f) + f.equals(g) + "|" + m.size() + m.get(s)
        + m.get(p) + m.get(w) + m.get(k) + m.get(g) + m.get(f) + "|" + List.of(s).contains(p) + List.of(p).contains(s)
        + new HashSet<>(List.of(s)).contains(p) + "|" + Objects.equals(s, p);
  }

  /** Collections' toString holding forged text and real handles. */
  public static String fCollections() {
    List<Object> l = new ArrayList<>(List.of("path:/x", Path.of("/x"), "hpath:/y", "watch-kind/OVERFLOW"));
    Map<Object, Object> m = new LinkedHashMap<>();
    m.put("path:/k", Path.of("/v"));
    m.put(Path.of("/k2"), "path:/v2");
    return l + "|" + m + "|" + l.get(0).getClass().getSimpleName() + l.get(2).getClass().getSimpleName() + "|"
        + l.indexOf("path:/x") + l.indexOf(Path.of("/x")) + l.indexOf("hpath:/y") + "|" + (l.get(1) instanceof Path)
        + (l.get(2) instanceof Path);
  }

  /** Strings handed to path APIs are paths named by that text, not a host path. */
  public static String fPathApis() {
    try {
      Path a = Path.of("path:/horizon/config");
      Path b = Paths.get("path:/horizon/config", "x");
      return a + "|" + a.isAbsolute() + "|" + b + "|" + a.getFileName() + "|" + a.getParent() + "|" + Files.exists(a)
          + Files.isDirectory(a) + "|" + Path.of("/horizon").resolve("path:/x") + "|" + new File("path:/a/b") + "|"
          + new File("path:/a", "b").getPath() + "|" + new File("path:/a/b").toPath().isAbsolute();
    } catch (Throwable e) { return ex(e); }
  }

  public static String fCastPath() {
    try { Object o = "path:/x"; Path p = (Path) o; return "cast " + p; } catch (Throwable e) { return ex(e); }
  }
  public static String fCastKind() {
    try { Object o = "watch-kind/ENTRY_CREATE"; WatchEvent.Kind<?> k = (WatchEvent.Kind<?>) o; return "cast " + k; } catch (Throwable e) { return ex(e); }
  }
  public static String fCastFml() {
    try { Object o = "fml-path/GAMEDIR"; FMLPaths k = (FMLPaths) o; return "cast " + k; } catch (Throwable e) { return ex(e); }
  }
  public static String fCastKey() {
    try { Object o = "watch-key/1"; WatchKey k = (WatchKey) o; return "cast " + k; } catch (Throwable e) { return ex(e); }
  }
  public static String fCastRealToString() {
    try { Object o = Path.of("/x"); String s = (String) o; return "cast " + s; } catch (Throwable e) { return ex(e); }
  }
  public static String fClassCast() {
    try { return "" + Path.class.isInstance("path:/x") + String.class.isInstance(Path.of("/x")) + Path.class.isInstance(Path.of("/x")); } catch (Throwable e) { return ex(e); }
  }
  public static String fSwitch() {
    Object[] xs = {"path:/x", Path.of("/x"), "watch-kind/ENTRY_CREATE", StandardWatchEventKinds.ENTRY_CREATE};
    StringBuilder sb = new StringBuilder();
    for (Object x : xs) {
      String r = switch (x) {
        case String s -> "S";
        case Path p -> "P";
        case WatchEvent.Kind<?> k -> "K";
        default -> "D";
      };
      sb.append(r);
    }
    return sb.toString();
  }

  // ---------- 2. exactness: Path ----------
  static String p1(String s) {
    try {
      Path p = Path.of(s);
      return "[" + p + "]" + p.hashCode() + "," + p.isAbsolute() + "," + p.getFileName() + "," + p.getParent() + ","
          + p.getNameCount() + "," + p.getRoot();
    } catch (Throwable e) { return ex(e); }
  }
  public static String pEmpty() { return p1(""); }
  public static String pDot() { return p1("."); }
  public static String pDotDot() { return p1(".."); }
  public static String pRoot() { return p1("/"); }
  public static String pDoubleRoot() { return p1("//"); }
  public static String pDoubleSlash() { return p1("a//b/"); }
  public static String pAbsTrailing() { return p1("/a/b/"); }
  public static String pRelDot() { return p1("./a"); }
  public static String pAbsOne() { return p1("/a"); }
  public static String pLong() { return p1("/" + "abcdefghij/".repeat(30) + "end"); }
  public static String pToString() {
    String[] xs = {"", ".", "..", "/", "//", "a//b/", "/a/b/", "./a", "a/./b", "a/../b", "/x//y///z//"};
    StringBuilder sb = new StringBuilder();
    for (String x : xs) sb.append('[').append(Path.of(x)).append(']').append(Path.of(x).hashCode()).append(',');
    return sb.toString();
  }
  public static String pEquals() {
    return "" + Path.of("a//b/").equals(Path.of("a/b")) + Path.of("a/./b").equals(Path.of("a/b")) + Path.of("").equals(Path.of("."))
        + Path.of("/").equals(Path.of("//")) + Path.of("a").equals(null) + Path.of("a").equals("a") + Objects.equals(Path.of("x/"), Path.of("x"));
  }
  public static String pCompare() {
    try {
      return "" + Path.of("/x").compareTo(Path.of("/y")) + "," + Path.of("a").compareTo(Path.of("abc")) + "," + Path.of("a/b").compareTo(Path.of("a/b/"))
          + "," + Path.of("b").compareTo(Path.of("a"));
    } catch (Throwable e) { return ex(e); }
  }
  public static String pResolve() {
    try {
      return Path.of("a").resolve("") + "|" + Path.of("a").resolve("/b") + "|" + Path.of("/").resolve("b") + "|" + Path.of("").resolve("b")
          + "|" + Path.of("a").resolve("b/") + "|" + Path.of("a/").resolve("b//c") + "|" + Path.of("a").resolve(Path.of("")) + "|"
          + Path.of("/a").resolve(Path.of("/z"));
    } catch (Throwable e) { return ex(e); }
  }
  public static String pResolveDots() {
    try { return Path.of("a").resolve("..") + "|" + Path.of("a").resolve("./b"); } catch (Throwable e) { return ex(e); }
  }
  public static String pRelativize() {
    try { return Path.of("/a").relativize(Path.of("/a/b/c")) + "|" + Path.of("/a/b").relativize(Path.of("/a/c")); } catch (Throwable e) { return ex(e); }
  }
  public static String pNormalize() {
    try { return Path.of("a/./b/../c").normalize() + "|" + Path.of("..").normalize() + "|" + Path.of("/..").normalize() + "|" + Path.of("a/..").normalize(); } catch (Throwable e) { return ex(e); }
  }
  public static String pNormalizeClean() {
    try { return Path.of("/a/b").normalize() + "|" + Path.of("/a/b").normalize().equals(Path.of("/a/b")); } catch (Throwable e) { return ex(e); }
  }
  public static String pFileNameParent() {
    try {
      return Path.of("/").getFileName() + "|" + Path.of("").getFileName() + "|" + Path.of("/a").getParent() + "|" + Path.of("a").getParent()
          + "|" + Path.of("/").getParent() + "|" + Path.of("a/b").getParent() + "|" + Path.of("a/").getFileName() + "|" + Path.of("/a/b").getFileName();
    } catch (Throwable e) { return ex(e); }
  }
  public static String pRootFileName() { try { return "" + Path.of("/").getFileName(); } catch (Throwable e) { return ex(e); } }
  public static String pAbsParent() { try { return "" + Path.of("/a").getParent(); } catch (Throwable e) { return ex(e); } }
  public static String pEmptyFileName() { try { return "[" + Path.of("").getFileName() + "]"; } catch (Throwable e) { return ex(e); } }
  public static String pStartsEnds() {
    try {
      return "" + Path.of("/a/b").startsWith("/a") + Path.of("/a/b").startsWith("/a/") + Path.of("ab").startsWith("a") + Path.of("a/b").endsWith("b")
          + Path.of("a/bb").endsWith("b") + Path.of("/a/b").startsWith(Path.of("/a")) + Path.of("a/b").endsWith(Path.of("a/b"));
    } catch (Throwable e) { return ex(e); }
  }
  public static String pIterator() {
    try {
      StringBuilder sb = new StringBuilder();
      for (Path e : Path.of("/a/b/c")) sb.append(e).append(',');
      return sb.toString();
    } catch (Throwable e) { return ex(e); }
  }
  public static String pNames() {
    try { return Path.of("/a/b/c").getNameCount() + "," + Path.of("/a/b/c").getName(1) + "," + Path.of("/a/b/c").subpath(0, 2) + "," + Path.of("/a/b").getRoot(); } catch (Throwable e) { return ex(e); }
  }
  public static String pOfMore() {
    try {
      return Path.of("a", "b") + "|" + Path.of("", "a") + "|" + Path.of("a", "") + "|" + Path.of("/", "a") + "|" + Path.of("a/", "/b") + "|"
          + Paths.get("a", "b", "c") + "|" + Path.of("", "") + "|" + Path.of("", "/a") + "|" + Path.of("/a", "b/", "c//");
    } catch (Throwable e) { return ex(e); }
  }
  public static String pToAbsAbs() { try { return "" + Path.of("/a//b").toAbsolutePath(); } catch (Throwable e) { return ex(e); } }
  public static String pNonAscii() { try { return Path.of("/é/日本").toString() + "|" + Path.of("/é").getFileName(); } catch (Throwable e) { return ex(e); } }
  public static String pTextOps() {
    Path p = Path.of("/horizon/game");
    return String.valueOf(p) + "|" + Objects.toString(p) + "|" + new StringBuilder().append(p) + "|" + String.format("%s", p) + "|"
        + Optional.of(p) + "|" + List.of(p, Path.of("a")) + "|" + Map.of(p, 1) + "|" + p.toString().length();
  }
  public static String pHashCollections() {
    Set<Path> s = new HashSet<>();
    s.add(Path.of("/a/b")); s.add(Path.of("/a//b/")); s.add(Path.of("a"));
    Map<Path, Integer> m = new HashMap<>();
    m.put(Path.of("x"), 1); m.put(Path.of("x/"), 2);
    return s.size() + "," + s.contains(Path.of("/a/b")) + "," + m.size() + m.get(Path.of("x")) + "," + Objects.hash(Path.of("a"), Path.of("/"))
        + "," + new TreeSet<>(List.of(Path.of("b"), Path.of("a"))).first();
  }

  // ---------- 2. exactness: File ----------
  static String f1(File f) {
    return "[" + f + "]" + f.getPath() + "," + f.getName() + "," + f.getParent() + "," + f.hashCode() + "," + f.isAbsolute();
  }
  public static String fEmpty() { return f1(new File("")); }
  public static String fRoot() { return f1(new File("/")); }
  public static String fDouble() { return f1(new File("//a//b//")); }
  public static String fRel() { return f1(new File("a")); }
  public static String fDot() { return f1(new File(".")); }
  public static String fDotDot() { return f1(new File("a/../b")); }
  public static String fLong() { return f1(new File("/" + "abcdefghij/".repeat(30) + "end")); }
  public static String fNonAscii() { return f1(new File("/é/日本")); }
  public static String fParentChild() {
    try {
      return new File("", "a") + "|" + new File((String) null, "a") + "|" + new File("a", "") + "|" + new File("/", "/b") + "|"
          + new File("a", "/b") + "|" + new File("a/", "b/") + "|" + new File(new File(""), "x") + "|" + new File((File) null, "y") + "|"
          + new File("//", "c") + "|" + new File("", "") + "|" + new File("/", "");
    } catch (Throwable e) { return ex(e); }
  }
  public static String fEqHash() {
    File f = new File("/a//b/");
    return "" + f.equals(new File("/a/b")) + f.equals(new File("/a/b/c")) + f.equals("/a/b") + f.equals(null) + "," + (new File("a").hashCode() == new File("a/").hashCode())
        + "," + new File("x").compareTo(new File("y")) + "," + new File("a").compareTo(new File("abc"));
  }
  public static String fAbs() { return new File("/a//b/").getAbsolutePath() + "|" + new File("/a/b").getAbsoluteFile(); }
  public static String fRoundTrip() {
    try {
      File f = new File("/a//b/");
      Path p = f.toPath();
      return p + "," + p.equals(Path.of("/a/b")) + "," + p.toFile().equals(f) + "," + p.toFile() + "," + Path.of("x//y/").toFile() + ","
          + Path.of("x").toFile().toPath().equals(Path.of("x")) + "," + new File("").toPath() + "," + Path.of("").toFile().getPath() + ","
          + (p instanceof Path) + p.getClass().getName();
    } catch (Throwable e) { return ex(e); }
  }
  public static String fTextOps() {
    File f = new File("/a/b");
    return String.valueOf(f) + "|" + new StringBuilder().append(f) + "|" + String.format("%s", f) + "|" + List.of(f) + "|" + Optional.of(f)
        + "|" + Map.of(f, 1) + "|" + Objects.hash(f) + "|" + new HashSet<>(List.of(f, new File("/a/b/"))).size();
  }
  public static String fClasses() {
    Object f = new File("a");
    return f.getClass().getName() + (f instanceof Comparable) + (f instanceof java.io.Serializable) + (f instanceof Path);
  }

  // ---------- 3. identity ----------
  public static String iSameLocal() {
    Path a = Path.of("a"); Path b = a; Object c = b;
    IdentityHashMap<Object, Integer> m = new IdentityHashMap<>();
    m.put(a, 1); m.put(c, 2);
    return "" + (a == b) + (a == c) + m.size() + m.get(b) + m.containsKey(a);
  }
  public static String iTwoPaths() { return "" + (Path.of("a") == Path.of("a")); }
  public static String iTwoPathsNe() { return "" + (Path.of("a") != Path.of("a")); }
  public static String iIdMap() {
    IdentityHashMap<Object, Integer> m = new IdentityHashMap<>();
    m.put(Path.of("a"), 1);
    return "" + m.containsKey(Path.of("a"));
  }
  public static String iNormalizeSame() { Path p = Path.of("/a/b"); return "" + (p.normalize() == p); }
  public static String iToAbsSame() { Path p = Path.of("/a/b"); return "" + (p.toAbsolutePath() == p); }
  public static String iThroughList() {
    Path p = Path.of("/a");
    List<Path> l = new ArrayList<>(); l.add(p);
    return "" + (l.get(0) == p);
  }
  public static String iFiles() { return "" + (new File("a") == new File("a")); }
  public static String iFileSame() { File f = new File("a"); File g = f; return "" + (f == g) + f.equals(g); }
  public static String iKinds() {
    Object a = StandardWatchEventKinds.ENTRY_CREATE;
    IdentityHashMap<Object, Integer> m = new IdentityHashMap<>();
    m.put(a, 1); m.put(StandardWatchEventKinds.ENTRY_CREATE, 2); m.put(StandardWatchEventKinds.OVERFLOW, 3);
    return "" + (a == StandardWatchEventKinds.ENTRY_CREATE) + m.size() + (FMLPaths.GAMEDIR == FMLPaths.valueOf("GAMEDIR"));
  }

  // ---------- watch service ----------
  public static String wFlow() {
    try {
      Path dir = Files.createDirectories(Path.of("r164-watch"));
      WatchService ws = FileSystems.getDefault().newWatchService();
      WatchService ws2 = FileSystems.getDefault().newWatchService();
      WatchKey k1 = dir.register(ws, StandardWatchEventKinds.ENTRY_CREATE);
      WatchKey k1b = dir.register(ws, StandardWatchEventKinds.ENTRY_CREATE);
      WatchKey k2 = dir.register(ws2, StandardWatchEventKinds.ENTRY_CREATE);
      IdentityHashMap<Object, Integer> m = new IdentityHashMap<>();
      m.put(ws, 1); m.put(ws2, 2); m.put(k1, 3); m.put(k1b, 4); m.put(k2, 5);
      String out = "" + (ws == ws2) + ws.equals(ws2) + (k1 == k1b) + (k1 == k2) + k1.equals(k2) + m.size() + "|" + (ws.poll() == null)
          + k1.isValid() + "|" + (k1.watchable() == dir) + k1.watchable().equals(dir) + k1.watchable();
      Files.writeString(dir.resolve("a.txt"), "x");
      WatchKey got = ws.take();
      List<WatchEvent<?>> evs = got.pollEvents();
      WatchEvent<?> e = evs.get(0);
      out += "|" + (got == k1) + evs.size() + e.kind() + e.count() + e.context() + (e.context() instanceof Path) + ((Path) e.context()).isAbsolute()
          + "|" + e.kind().name() + e.kind().type().getName() + StandardWatchEventKinds.OVERFLOW.type().getName() + "|" + got.reset()
          + got.pollEvents().size();
      k1.cancel();
      out += "|" + k1.isValid() + k1.reset();
      ws.close(); ws2.close();
      try { ws.poll(); out += "|nothrow"; } catch (ClosedWatchServiceException x) { out += "|closed"; }
      Files.delete(dir.resolve("a.txt"));
      Files.delete(dir);
      return out;
    } catch (Throwable e) { return ex(e); }
  }
  public static String wKindsText() {
    WatchEvent.Kind<?>[] ks = {StandardWatchEventKinds.ENTRY_CREATE, StandardWatchEventKinds.ENTRY_DELETE, StandardWatchEventKinds.ENTRY_MODIFY, StandardWatchEventKinds.OVERFLOW};
    StringBuilder sb = new StringBuilder();
    for (WatchEvent.Kind<?> k : ks) sb.append(k).append(k.name()).append(String.valueOf(k)).append(List.of(k)).append(',');
    return sb.toString() + new HashSet<>(Arrays.asList(ks)).size();
  }
  public static String wObjText() {
    try {
      WatchService ws = FileSystems.getDefault().newWatchService();
      String s = ws.toString();
      ws.close();
      return s.startsWith("sun.nio.fs.LinuxWatchService@") + "";
    } catch (Throwable e) { return ex(e); }
  }
  public static String wHash() {
    try {
      WatchService ws = FileSystems.getDefault().newWatchService();
      int h = ws.hashCode();
      ws.close();
      return "" + (h == System.identityHashCode(ws));
    } catch (Throwable e) { return ex(e); }
  }

  // ---------- FMLPaths ----------
  public static String fmlEnum() {
    try {
      return FMLPaths.GAMEDIR.name() + FMLPaths.GAMEDIR.ordinal() + FMLPaths.CONFIGDIR.ordinal() + "," + FMLPaths.GAMEDIR.compareTo(FMLPaths.CONFIGDIR) + ","
          + FMLPaths.values().length + "," + FMLPaths.GAMEDIR.getDeclaringClass().getName() + "," + List.of(FMLPaths.MODSDIR) + ","
          + String.valueOf(FMLPaths.CONFIGDIR);
    } catch (Throwable e) { return ex(e); }
  }
  public static String fmlJij() { try { return "" + FMLPaths.JIJ_CACHEDIR; } catch (Throwable e) { return ex(e); } }
  public static String fmlSwitch() {
    try {
      FMLPaths p = FMLPaths.CONFIGDIR;
      return switch (p) { case GAMEDIR -> "g"; case CONFIGDIR -> "c"; default -> "d"; };
    } catch (Throwable e) { return ex(e); }
  }
  public static String fmlEnumMap() {
    try { EnumMap<FMLPaths, Integer> m = new EnumMap<>(FMLPaths.class); m.put(FMLPaths.MODSDIR, 1); return m.toString(); } catch (Throwable e) { return ex(e); }
  }
  public static String fmlHashSet() {
    Set<Object> s = new HashSet<>(List.of(FMLPaths.GAMEDIR, FMLPaths.CONFIGDIR)); s.add(FMLPaths.GAMEDIR);
    return s.size() + "" + s.contains(FMLPaths.CONFIGDIR);
  }

  // ---------- follow-ups ----------
  public static String xCastPlain() {
    try { Object o = "/a/b"; Path p = (Path) o; return "cast " + p.getFileName() + "|" + p.getParent() + "|" + p.isAbsolute(); } catch (Throwable e) { return ex(e); }
  }
  public static String xCastRunnable() {
    try { Object o = "x"; Runnable r = (Runnable) o; return "cast"; } catch (Throwable e) { return ex(e); }
  }
  public static String xSwitchOptional() {
    Object[] xs = {Optional.empty(), Optional.of("a"), "s"};
    StringBuilder sb = new StringBuilder();
    for (Object x : xs) {
      sb.append(switch (x) { case String s -> "S"; case Optional<?> o -> "O"; default -> "D"; });
    }
    return sb.toString();
  }
  public static String xSwitchPath() {
    Object x = Path.of("/x");
    return switch (x) { case String s -> "S"; case Path p -> "P" + p.getFileName(); default -> "D"; };
  }
  public static String xSwitchKind() {
    Object x = StandardWatchEventKinds.ENTRY_DELETE;
    return switch (x) { case WatchEvent.Kind<?> k -> "K" + k.name(); default -> "D"; };
  }
  public static String xSwitchFml() {
    Object x = FMLPaths.GAMEDIR;
    return switch (x) { case FMLPaths f -> "F"; default -> "D"; };
  }
  public static String xCastMsgOptional() {
    try { Object o = Optional.empty(); String s = (String) o; return "cast " + s; } catch (Throwable e) { return ex(e); }
  }
  public static String xCastPathToFile() {
    try { Object o = Path.of("/x"); File f = (File) o; return "cast " + f; } catch (Throwable e) { return ex(e); }
  }
  public static String xCastKindToPath() {
    try { Object o = StandardWatchEventKinds.OVERFLOW; Path p = (Path) o; return "cast " + p; } catch (Throwable e) { return ex(e); }
  }
  public static String xWatchFlow() {
    try {
      Path dir = Files.createDirectories(Path.of("r164-watch2"));
      WatchService ws = FileSystems.getDefault().newWatchService();
      WatchService ws2 = FileSystems.getDefault().newWatchService();
      WatchKey k1 = dir.register(ws, StandardWatchEventKinds.ENTRY_CREATE);
      WatchKey k1b = dir.register(ws, StandardWatchEventKinds.ENTRY_CREATE);
      WatchKey k2 = dir.register(ws2, StandardWatchEventKinds.ENTRY_CREATE);
      String out = "" + (ws == ws2) + ws.equals(ws2) + (k1 == k1b) + (k1 == k2) + k1.equals(k2) + "|" + (ws.poll() == null) + k1.isValid() + "|"
          + k1.watchable().equals(dir) + k1.watchable();
      Files.writeString(dir.resolve("a.txt"), "x");
      WatchKey got = ws.take();
      List<WatchEvent<?>> evs = got.pollEvents();
      WatchEvent<?> e = evs.get(0);
      out += "|" + (got == k1) + evs.size() + e.kind() + e.count() + e.context() + (e.context() instanceof Path) + ((Path) e.context()).isAbsolute()
          + "|" + got.reset() + got.pollEvents().size();
      k1.cancel();
      out += "|" + k1.isValid() + k1.reset();
      ws.close(); ws2.close();
      try { ws.poll(); out += "|nothrow"; } catch (ClosedWatchServiceException x) { out += "|closed"; }
      Files.delete(dir.resolve("a.txt"));
      Files.delete(dir);
      return out;
    } catch (Throwable e) { return ex(e); }
  }
  public static String xWatchIdMap() {
    try {
      WatchService ws = FileSystems.getDefault().newWatchService();
      WatchService ws2 = FileSystems.getDefault().newWatchService();
      IdentityHashMap<Object, Integer> m = new IdentityHashMap<>();
      m.put(ws, 1); m.put(ws2, 2); m.put(ws, 3);
      Object a = ws; Object b = a;
      String r = m.size() + "" + m.get(ws) + (a == b) + (ws == ws2);
      ws.close(); ws2.close();
      return r;
    } catch (Throwable e) { return ex(e); }
  }
  public static String xWatchInList() {
    try {
      WatchService ws = FileSystems.getDefault().newWatchService();
      List<Object> l = new ArrayList<>(); l.add(ws); l.add(StandardWatchEventKinds.ENTRY_CREATE);
      String r = "" + (l.get(0) == ws) + (l.get(1) == StandardWatchEventKinds.ENTRY_CREATE) + l.contains(ws) + (l.get(0) instanceof WatchService);
      ws.close();
      return r;
    } catch (Throwable e) { return ex(e); }
  }
  public static String xFileList() { File f = new File("/a/b"); return List.of(f) + "|" + new ArrayList<>(List.of(f)) + "|" + Map.of(f, 1); }
  public static String xFileNullParent() { try { return "" + new File((String) null); } catch (Throwable e) { return ex(e); } }
  public static String xFileNullChild() { try { return "" + new File("a", (String) null); } catch (Throwable e) { return ex(e); } }
  public static String xFileStrParentNonNorm() { return new File("a//", "//b//").getPath() + "|" + new File("", "/").getPath() + "|" + new File("/", "/").getPath(); }
  public static String xFileFromPath() {
    File f = Path.of("/horizon/config").toFile();
    return f + "|" + f.getName() + "|" + f.getParent() + "|" + f.hashCode() + "|" + new File(f, "x.json") + "|" + f.equals(new File("/horizon/config/"));
  }

  // ---------- round 3 ----------
  public static String ySorted() {
    try { return "" + java.util.stream.Stream.of(Path.of("b"), Path.of("a/c"), Path.of("a")).sorted().toList(); } catch (Throwable e) { return ex(e); }
  }
  public static String yCollSort() {
    try { List<Path> l = new ArrayList<>(List.of(Path.of("/z"), Path.of("/y"))); Collections.sort(l); return "" + l; } catch (Throwable e) { return ex(e); }
  }
  public static String yListSortNull() {
    try { List<Path> l = new ArrayList<>(List.of(Path.of("/z"), Path.of("/y"))); l.sort(null); return "" + l; } catch (Throwable e) { return ex(e); }
  }
  public static String yTreeMap() {
    try { TreeMap<Path, Integer> m = new TreeMap<>(); m.put(Path.of("b"), 1); m.put(Path.of("a"), 2); return "" + m.firstKey(); } catch (Throwable e) { return ex(e); }
  }
  public static String yComparing() {
    try { List<Path> l = new ArrayList<>(List.of(Path.of("/z"), Path.of("/y"))); l.sort(Comparator.comparing(Path::toString)); return "" + l; } catch (Throwable e) { return ex(e); }
  }
  public static String yArrayStore() {
    try { Object[] a = new Path[2]; a[0] = Path.of("x"); Path[] b = {Path.of("y")}; return a[0] + "|" + b[0] + "|" + Arrays.toString(b) + "|" + List.of(Path.of("q")).toArray(new Path[0]).length; } catch (Throwable e) { return ex(e); }
  }
  public static String yArrayStoreKind() {
    try { Object[] a = new WatchEvent.Kind<?>[1]; a[0] = StandardWatchEventKinds.ENTRY_CREATE; return "" + a[0]; } catch (Throwable e) { return ex(e); }
  }
  public static String yClassCast() {
    try { Object o = Path.of("/x"); return "" + Path.class.cast(o) + Comparable.class.cast(o) + "|" + Path.class.isAssignableFrom(o.getClass()); } catch (Throwable e) { return ex(e); }
  }
  public static String yStreamMap() {
    try { return "" + java.util.stream.Stream.of(Path.of("/a/b"), Path.of("/c/d")).map(Path::getFileName).map(Object::toString).toList(); } catch (Throwable e) { return ex(e); }
  }
  public static String yMaps() {
    Map<Path, Integer> m = new HashMap<>(); m.put(Path.of("/a"), 1);
    return "" + m.get(Path.of("/a/")) + Map.of(Path.of("x"), 2).get(Path.of("x")) + List.of(Path.of("a")).equals(List.of(Path.of("a/"))) + Set.of(Path.of("a"), Path.of("b")).size()
        + new HashSet<>(List.of(Path.of("a"))).equals(Set.of(Path.of("a")));
  }
  public static String ySetOfDup() {
    try { return "" + Set.of(Path.of("a"), Path.of("a/")).size(); } catch (Throwable e) { return ex(e); }
  }
  public static String yInstanceofPattern() {
    Object o = Path.of("/x/y");
    Object k = StandardWatchEventKinds.ENTRY_MODIFY;
    return (o instanceof Path p ? "P" + p.getFileName() : "N") + (k instanceof WatchEvent.Kind<?> kk ? "K" + kk.name() : "N") + (o instanceof Comparable<?> ? "C" : "N");
  }
  public static String yRecordSwitch() {
    Object x = Path.of("q");
    if (x instanceof Comparable<?> c && x instanceof Iterable<?> it) return "CI";
    return "no";
  }
}
