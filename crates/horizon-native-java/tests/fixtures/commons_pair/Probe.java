package demo;

import org.apache.commons.lang3.tuple.ImmutablePair;
import org.apache.commons.lang3.tuple.Pair;

public final class Probe {
    public static String run() {
        StringBuilder out = new StringBuilder();
        Pair<String, Integer> pair = Pair.of("a", 1);
        out.append(pair.getLeft()).append('|').append(pair.getRight()).append('|')
            .append(pair.getKey()).append('|').append(pair.getValue()).append('|')
            .append(pair).append('|');
        ImmutablePair<String, String> made = new ImmutablePair<>("x", null);
        out.append(made.left).append('|').append(made.right).append('|').append(made).append('|');
        out.append(Pair.of(null, null) == Pair.of(null, null)).append('|');
        out.append(ImmutablePair.left("L").getRight()).append('|');
        out.append(ImmutablePair.right("R").getLeft()).append('|');
        try {
            made.setValue("y");
            out.append("set");
        } catch (UnsupportedOperationException error) {
            out.append("uoe:").append(error.getMessage());
        }
        return out.toString();
    }

}
