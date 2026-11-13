package demo;

import java.util.*;

/** IdentityHashMap with optionals as keys: each key-taking member, once with
 * the same handle ("same*", one object: Java's answer is exact) and once with
 * an equal handle made separately ("other*", a different object). */
public final class IdMap {
  static Map<Object, Integer> map(Object key) {
    Map<Object, Integer> m = new IdentityHashMap<>();
    m.put(key, 1);
    return m;
  }

  public static String sameMembers() {
    Optional<String> k = Optional.of("k");
    Map<Object, Integer> m = map(k);
    StringBuilder out = new StringBuilder();
    out.append(m.containsKey(k)).append(m.get(k)).append(m.getOrDefault(k, 9)).append(m.put(k, 2))
        .append(m.putIfAbsent(k, 3)).append(m.replace(k, 4)).append(m.replace(k, 4, 5)).append(m.merge(k, 1, Integer::sum))
        .append(m.compute(k, (a, v) -> v + 1)).append(m.computeIfPresent(k, (a, v) -> v + 1))
        .append(m.computeIfAbsent(k, a -> 0)).append(m.containsValue(9)).append(m.keySet().contains(k))
        .append(m.size()).append(m.remove(k, 1)).append(m.remove(k)).append(m.size());
    return out.toString();
  }

  public static String sameEmpty() {
    Map<Object, Integer> m = map(Optional.empty());
    m.put(Optional.empty(), 2);
    m.put(OptionalInt.empty(), 3);
    return m.size() + "," + m.get(Optional.empty()) + m.containsKey(OptionalInt.empty()) + m.containsKey(OptionalLong.empty());
  }

  public static String otherContainsKey() { return "" + map(Optional.of("k")).containsKey(Optional.of("k")); }
  public static String otherGet() { return "" + map(Optional.of("k")).get(Optional.of("k")); }
  public static String otherGetOrDefault() { return "" + map(Optional.of("k")).getOrDefault(Optional.of("k"), 9); }
  public static String otherPut() { Map<Object, Integer> m = map(Optional.of("k")); m.put(Optional.of("k"), 2); return "" + m.size(); }
  public static String otherPutIfAbsent() { Map<Object, Integer> m = map(Optional.of("k")); return m.putIfAbsent(Optional.of("k"), 2) + "," + m.size(); }
  public static String otherRemove() { Map<Object, Integer> m = map(Optional.of("k")); return m.remove(Optional.of("k")) + "," + m.size(); }
  public static String otherRemoveValue() { Map<Object, Integer> m = map(Optional.of("k")); return m.remove(Optional.of("k"), 1) + "," + m.size(); }
  public static String otherReplace() { Map<Object, Integer> m = map(Optional.of("k")); return m.replace(Optional.of("k"), 2) + "," + m.get(m.keySet().iterator().next()); }
  public static String otherMerge() { Map<Object, Integer> m = map(Optional.of("k")); m.merge(Optional.of("k"), 1, Integer::sum); return "" + m.size(); }
  public static String otherCompute() { Map<Object, Integer> m = map(Optional.of("k")); m.compute(Optional.of("k"), (a, v) -> 7); return "" + m.size(); }
  public static String otherComputeIfPresent() { Map<Object, Integer> m = map(Optional.of("k")); return m.computeIfPresent(Optional.of("k"), (a, v) -> 7) + "," + m.size(); }
  public static String otherComputeIfAbsent() { Map<Object, Integer> m = map(Optional.of("k")); return m.computeIfAbsent(Optional.of("k"), a -> 7) + "," + m.size(); }
  public static String otherContainsValue() { Map<Object, Object> m = new IdentityHashMap<>(); m.put("a", Optional.of("v")); return "" + m.containsValue(Optional.of("v")); }
  public static String otherKeySetContains() { return "" + map(Optional.of("k")).keySet().contains(Optional.of("k")); }
  public static String otherKeySetRemove() { Map<Object, Integer> m = map(Optional.of("k")); return m.keySet().remove(Optional.of("k")) + "," + m.size(); }
  public static String otherEntrySetContains() { return "" + map(Optional.of("k")).entrySet().contains(Map.entry(Optional.of("k"), 1)); }
  public static String otherCopy() { return "" + new IdentityHashMap<>(Map.of(OptionalInt.of(5), 1)).containsKey(OptionalInt.of(5)); }
  public static String otherPutAll() { Map<Object, Integer> m = map(Optional.of("k")); m.putAll(Map.of(Optional.of("k"), 2)); return "" + m.size(); }
}
