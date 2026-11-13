import java.lang.module.ModuleFinder;
import java.lang.reflect.Modifier;
import java.util.Set;
import java.util.TreeSet;

/**
 * Prints `src/jdk_supertypes_25.txt`: for each class the runtime's JDK hierarchy answers for, its
 * flags and every supertype (getSuperclass/getInterfaces, transitively, the class itself left out),
 * then every package of every system module (a JDK class's supertypes can only come from these).
 * Run on OpenJDK 25: `java JdkSupertypes.java > ../../../src/jdk_supertypes_25.txt`.
 */
public final class JdkSupertypes {
  static final String[] CLASSES = {
    // jdk_model::platform_supertypes
    "java.lang.Integer", "java.lang.Long", "java.lang.Short", "java.lang.Byte", "java.lang.Float",
    "java.lang.Double", "java.lang.Boolean", "java.lang.Character", "java.util.ArrayList",
    "java.util.Vector", "java.util.Stack", "java.util.concurrent.CopyOnWriteArrayList",
    "java.util.LinkedList", "java.util.HashSet", "java.util.concurrent.CopyOnWriteArraySet",
    "java.util.LinkedHashSet", "java.util.TreeSet", "java.util.concurrent.ConcurrentSkipListSet",
    "java.util.HashMap", "java.util.IdentityHashMap", "java.util.WeakHashMap", "java.util.EnumMap",
    "java.util.Hashtable", "java.util.LinkedHashMap", "java.util.TreeMap",
    "java.util.concurrent.ConcurrentSkipListMap", "java.util.concurrent.ConcurrentHashMap",
    "java.util.ArrayDeque", "java.util.concurrent.ConcurrentLinkedDeque", "java.util.PriorityQueue",
    "java.util.concurrent.ConcurrentLinkedQueue", "java.io.File",
    // java_immutable::supertypes
    "java.util.ImmutableCollections$ListN", "java.util.Collections$UnmodifiableRandomAccessList",
    "java.util.ImmutableCollections$SetN", "java.util.Collections$UnmodifiableSet",
    "java.util.ImmutableCollections$MapN", "java.util.Collections$UnmodifiableMap",
    // host_ref's closed classes
    "java.lang.Object", "java.lang.String", "java.lang.Class", "java.lang.Record", "java.lang.Enum",
    "java.lang.Number", "java.util.Optional", "java.util.OptionalInt", "java.util.OptionalLong",
    "java.util.OptionalDouble",
  };

  static void walk(Class<?> c, Set<String> out) {
    if (c == null || !out.add(c.getName().replace('.', '/'))) return;
    walk(c.getSuperclass(), out);
    for (Class<?> i : c.getInterfaces()) walk(i, out);
  }

  public static void main(String[] args) throws Exception {
    System.out.println("# Java " + System.getProperty("java.version")
        + " supertypes and system-module packages: tests/fixtures/jdk_supertypes/JdkSupertypes.java");
    Set<String> sorted = new TreeSet<>();
    for (String n : CLASSES) sorted.add(n);
    for (String n : sorted) {
      Class<?> c = Class.forName(n);
      Set<String> supers = new TreeSet<>();
      walk(c, supers);
      supers.remove(c.getName().replace('.', '/'));
      String flags = c.isInterface() ? "interface" : Modifier.isFinal(c.getModifiers()) ? "final" : "class";
      System.out.println("class\t" + c.getName().replace('.', '/') + "\t" + flags + "\t" + String.join(" ", supers));
    }
    Set<String> packages = new TreeSet<>();
    ModuleFinder.ofSystem().findAll().forEach(m -> m.descriptor().packages()
        .forEach(p -> packages.add(p.replace('.', '/'))));
    for (String p : packages) System.out.println("package\t" + p);
  }
}
