package demo;

public final class Probe {
    interface Step {
        Object run();
    }

    static String attempt(Step step) {
        try {
            return String.valueOf(step.run());
        } catch (RuntimeException error) {
            return error.getClass().getSimpleName() + ":" + error.getMessage();
        }
    }

    public static String run() {
        StringBuilder out = new StringBuilder();
        char[] chars = {'k', 'e', 'y', '\uD83D', '\uDE00', '!'};
        String whole = new String(chars);
        chars[0] = 'X';
        out.append(whole.substring(0, 3)).append((int) whole.charAt(3)).append('+').append((int) whole.charAt(4)).append(whole.charAt(5))
            .append(',').append(whole.length()).append(',').append(whole.equals("key😀!"))
            .append('|');
        out.append(new String(chars, 1, 2)).append(',').append(new String(chars, 3, 2).length())
            .append(',').append(new String(chars, 6, 0).isEmpty()).append('|');
        out.append(attempt(() -> new String(chars, 4, 5))).append('|');
        out.append(attempt(() -> new String(chars, -1, 2))).append('|');
        out.append(attempt(() -> new String(chars, 2, -1))).append('|');
        out.append(attempt(() -> new String((char[]) null))).append('|');
        out.append(attempt(() -> new String((char[]) null, 0, 1))).append('|');
        int[] points = {0x41, 0x1F600, 0x62};
        String mixed = new String(points, 0, 3);
        out.append(mixed.charAt(0)).append((int) mixed.charAt(1)).append('+').append((int) mixed.charAt(2)).append(mixed.charAt(3))
            .append(',').append(mixed.length())
            .append(',').append(new String(points, 1, 1).length()).append('|');
        out.append(attempt(() -> new String(points, 2, 2))).append('|');
        out.append(attempt(() -> new String(new int[] {0x41, -1}, 0, 2))).append('|');
        out.append(attempt(() -> new String(new int[] {0x110000}, 0, 1))).append('|');
        out.append(attempt(() -> new String((int[]) null, 0, 1)));
        return out.toString();
    }

}
