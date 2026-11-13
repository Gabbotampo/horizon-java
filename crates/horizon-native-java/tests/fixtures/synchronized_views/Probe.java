package demo;

import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

public final class Probe {
    public static String run() {
        StringBuilder out = new StringBuilder();
        Map<String, Integer> backing = new LinkedHashMap<>();
        Map<String, Integer> map = Collections.synchronizedMap(backing);
        map.put("a", 1);
        backing.put("b", 2);
        out.append(map.get("b")).append('|').append(backing.get("a")).append('|')
            .append(map.size()).append('|').append(map.containsKey("a")).append('|');
        out.append(map).append('|').append(map == backing).append('|')
            .append(map.equals(backing)).append('|').append(backing.equals(map)).append('|');
        map.remove("a");
        out.append(backing).append('|').append(map.isEmpty()).append('|');
        List<String> list = Collections.synchronizedList(new ArrayList<>());
        list.add("x");
        list.add("y");
        for (String item : list) {
            out.append(item);
        }
        out.append('|').append(list.indexOf("y")).append('|').append(list).append('|');
        Set<String> set = Collections.synchronizedSet(new LinkedHashSet<>());
        set.add("p");
        set.add("p");
        set.add("q");
        out.append(set).append('|').append(set.contains("q")).append('|');
        try {
            Collections.synchronizedMap(null);
            out.append("none");
        } catch (NullPointerException error) {
            out.append("npe:").append(error.getMessage());
        }
        return out.toString();
    }

}
