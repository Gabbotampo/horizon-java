package demo;

import java.nio.charset.Charset;
import java.nio.charset.StandardCharsets;

public final class Probe {
    interface Make { String make() throws Exception; }

    static String t(Make make) {
        try {
            String s = make.make();
            StringBuilder out = new StringBuilder();
            for (int i = 0; i < s.length(); i++) {
                char c = s.charAt(i);
                if (c < 0x20 || c > 0x7e) out.append("+").append((int) c); else out.append(c);
            }
            return out.toString();
        } catch (Exception e) {
            return e.getClass().getSimpleName() + ":" + e.getMessage();
        }
    }

    public static String run() {
        byte[] utf8 = {'c', 'a', 'f', (byte) 0xC3, (byte) 0xA9};
        byte[] latin = {'c', (byte) 0xE9};
        byte[] none = null;
        String noText = null;
        Charset noCharset = null;
        String noName = null;
        return t(() -> new String(utf8)) + "|" + t(() -> new String(utf8, 1, 4)) + "|"
            + t(() -> new String(latin, StandardCharsets.ISO_8859_1)) + "|"
            + t(() -> new String(latin, StandardCharsets.US_ASCII)) + "|"
            + t(() -> new String(utf8, 0, 3, "US-ASCII")) + "|" + t(() -> new String(latin, "latin1")) + "|"
            + t(() -> new String(utf8, 3, 5)) + "|" + t(() -> new String(none)) + "|"
            + t(() -> new String(none, 0, 1, StandardCharsets.UTF_8)) + "|"
            + t(() -> new String(utf8, noCharset)) + "|" + t(() -> new String(none, noName)) + "|"
            + t(() -> new String(utf8, 1, 9, noCharset)) + "|" + t(() -> new String(noText)) + "|"
            + t(() -> new String()) + "|" + t(() -> new String("copy"));
    }

    public static void main(String[] args) { System.out.println(run()); }
}
