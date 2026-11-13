package demo;

public final class Probe {
    interface Step {
        void run();
    }

    static String attempt(Step step) {
        try {
            step.run();
            return "ok";
        } catch (RuntimeException error) {
            return error.getClass().getSimpleName() + ":" + error.getMessage();
        }
    }

    public static String run() {
        StringBuilder out = new StringBuilder();
        char[] src = {'a', 'b', 'c', 'd'};
        char[] dst = new char[6];
        System.arraycopy(src, 0, dst, 1, 3);
        out.append(dst[1]).append(dst[2]).append(dst[3]).append((int) dst[0]).append((int) dst[4]).append('|');
        char[] same = {'1', '2', '3', '4', '5'};
        System.arraycopy(same, 0, same, 1, 4);
        for (char c : same) out.append(c);
        out.append(',');
        char[] back = {'1', '2', '3', '4', '5'};
        System.arraycopy(back, 1, back, 0, 4);
        for (char c : back) out.append(c);
        out.append('|');
        int[] ints = {1, 2, 3};
        int[] intsDst = new int[3];
        System.arraycopy(ints, 1, intsDst, 0, 2);
        out.append(intsDst[0]).append(intsDst[1]).append(intsDst[2]).append('|');
        String[] names = {"x", "y"};
        Object[] objects = new Object[3];
        System.arraycopy(names, 0, objects, 1, 2);
        out.append(objects[0]).append(objects[1]).append(objects[2]).append('|');
        out.append(attempt(() -> System.arraycopy(src, 0, dst, 0, -1))).append('|');
        out.append(attempt(() -> System.arraycopy(src, -1, dst, 0, 1))).append('|');
        out.append(attempt(() -> System.arraycopy(src, 0, dst, -1, 1))).append('|');
        out.append(attempt(() -> System.arraycopy(src, 2, dst, 0, 3))).append('|');
        out.append(attempt(() -> System.arraycopy(src, 0, dst, 4, 3))).append('|');
        out.append(attempt(() -> System.arraycopy(names, 0, objects, 2, 2))).append('|');
        out.append(attempt(() -> System.arraycopy(null, 0, dst, 0, 1))).append('|');
        out.append(attempt(() -> System.arraycopy(src, 0, null, 0, 1))).append('|');
        out.append(attempt(() -> System.arraycopy(src, 0, ints, 0, 1))).append('|');
        out.append(attempt(() -> System.arraycopy("str", 0, dst, 0, 1))).append('|');
        out.append(attempt(() -> System.arraycopy(src, 0, objects, 0, 1))).append('|');
        out.append(attempt(() -> System.arraycopy(src, 4, dst, 6, 0))).append('|');
        char[] grown = java.util.Arrays.copyOf(src, 6);
        System.arraycopy(src, 0, grown, 2, 4);
        for (char c : grown) out.append(c);
        out.append(grown.length);
        return out.toString();
    }

}
