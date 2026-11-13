package demo;

import com.google.common.collect.Lists;
import java.util.ArrayList;
import java.util.List;

public final class Probe {
    public static String run() {
        List<String> source = new ArrayList<>();
        source.add("a");
        source.add("b");
        source.add("c");
        // Each copy is a collection the host makes; make well over 256.
        int total = 0;
        for (int i = 0; i < 400; i++) {
            total += Lists.newArrayList(source).size();
        }
        List<String> three = Lists.newArrayList(source);
        List<String> two = Lists.newArrayList(source.subList(0, 2));
        List<String> head = three.subList(0, 2);
        return total + "|" + three.size() + three + "|" + two.size() + two + "|" + three.get(2)
            + "|" + head;
    }

}
