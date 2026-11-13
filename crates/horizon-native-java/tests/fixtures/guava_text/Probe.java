package demo;

import com.google.common.base.Joiner;
import com.google.common.base.Splitter;
import com.google.common.collect.Lists;
import java.util.ArrayList;
import java.util.List;

public final class Probe {
    static String attempt(java.util.function.Supplier<Object> body) {
        try {
            return String.valueOf(body.get());
        } catch (RuntimeException error) {
            return error.getClass().getSimpleName() + ":" + error.getMessage();
        }
    }

    public static String split() {
        Splitter dot = Splitter.on(".");
        Iterable<String> parts = dot.split("a.b..c.");
        List<String> list = Lists.newArrayList(parts);
        StringBuilder out = new StringBuilder();
        out.append(parts).append('|').append(list.size()).append('|');
        out.append(Splitter.on('/').split("x/y")).append('|');
        out.append(Splitter.on("::").split("p::q:r::")).append('|');
        out.append(Lists.newArrayList(dot.split(""))).append('|');
        out.append(attempt(() -> Splitter.on(""))).append('|');
        out.append(attempt(() -> dot.split(null)));
        return out.toString();
    }

    public static String join() {
        List<String> path = new ArrayList<>();
        path.add("general");
        path.add("value");
        Joiner dot = Joiner.on(".");
        StringBuilder out = new StringBuilder();
        out.append(dot.join(path)).append('|');
        out.append(Joiner.on("\n").join(new ArrayList<String>())).append('|');
        out.append(Joiner.on(',').join(Lists.newArrayList(Splitter.on('/').split("a/b/")))).append('|');
        List<String> holes = new ArrayList<>();
        holes.add("a");
        holes.add(null);
        out.append(attempt(() -> dot.join(holes))).append('|');
        out.append(dot.join(Lists.newArrayList(Splitter.on(".").split("k.l.m")))).append('|');
        out.append(attempt(() -> Lists.newArrayList((Iterable<String>) null)));
        return out.toString();
    }

}
