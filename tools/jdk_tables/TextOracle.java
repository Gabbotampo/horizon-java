// Records what the running JDK answers for generated StringBuilder/StringBuffer
// calls, Double/Float.toString and Character queries, as JSON the Rust tests
// replay (`crates/horizon-native-java/src/jdk_text_oracle.rs`):
//
//     $JAVA25/bin/java -XX:-OmitStackTraceInFastThrow \
//         tools/jdk_tables/TextOracle.java 300 27 \
//         > crates/horizon-native-java/src/jdk_text_oracle.json
//
// `$JAVA25` is a Java 25 JDK: Minecraft 26.3's launcher manifest pins
// `"javaVersion": {"component": "java-runtime-epsilon", "majorVersion": 25}`,
// so Java 25 is what a mod's JDK calls answer with. The committed sample was
// written by Eclipse Adoptium 25.0.5+8-LTS; the file records the build.
//
// `-XX:-OmitStackTraceInFastThrow`: after enough identical implicit
// `NullPointerException`s HotSpot throws a preallocated one with no message,
// which is a JIT detail, not what a call answers.
//
// Arguments: how many builder cases (the toString and Character samples scale
// with it) and the random seed. A larger run is the full differential:
// `HORIZON_JDK_TEXT_ORACLE=<file> cargo test -p horizon-native-java
// jdk_text_oracle -- --ignored` replays any file this program writes.
//
// Texts are JSON strings, or arrays of UTF-16 code units when they hold a
// lone surrogate, so one in an argument or a result survives the trip. A call
// whose result holds one is recorded as `unmodelled`: the runtime keeps
// strings as Rust text and leaves such a call counted.
import java.lang.reflect.Constructor;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;

public class TextOracle {
    static Random random;

    static final String[] TEXTS = {
        "", "a", "ab", "hello", "héllo wörld", "😀", "a😀b😀",
        "ab\u0000c", "日本語", "The quick brown fox", "x𐐀y", "İstanbul",
    };
    static final String[] ARG_TEXTS = {
        null, "", "z", "ab", "😀", "ün", "bro", "o", "fox", "\ud83d",
    };
    static final int[] CHARS = {'x', 'A', 0, 0xe9, 0xd83d, 0xde00, 0x20ac, 0xffff, '9'};
    static final char[][] ARRAYS = {
        null, {}, {'x', 'y'}, {0xd83d, 0xde00}, {'a', 'b', 'c', 'd'}, {'q', 0xd83d},
    };

    static int smallInt(int length) {
        int[] pool = {-1, 0, 1, 2, 3, length - 1, length, length + 1, Integer.MIN_VALUE,
            Integer.MAX_VALUE, -5, 40};
        return random.nextInt(4) == 0 ? random.nextInt(length + 3) - 1 : pool[random.nextInt(pool.length)];
    }

    // A text as a JSON string, or as an array of code units when it holds a
    // lone surrogate (JSON parsers refuse an unpaired `\\uD83D` escape).
    static String units(CharSequence text) {
        if (text == null) return "null";
        if (!lone(text)) {
            StringBuilder out = new StringBuilder("\"");
            for (int i = 0; i < text.length(); i++) {
                char c = text.charAt(i);
                if (c == '"' || c == '\\') out.append('\\').append(c);
                else if (c >= 0x20 && c < 0x7f) out.append(c);
                else out.append(String.format("\\u%04x", (int) c));
            }
            return out.append('"').toString();
        }
        StringBuilder out = new StringBuilder("[");
        for (int i = 0; i < text.length(); i++) {
            if (i > 0) out.append(',');
            out.append((int) text.charAt(i));
        }
        return out.append(']').toString();
    }

    static String units(char[] chars) {
        return chars == null ? "null" : units(new String(chars));
    }

    static boolean lone(CharSequence text) {
        for (int i = 0; i < text.length(); i++) {
            char c = text.charAt(i);
            if (Character.isHighSurrogate(c)) {
                if (i + 1 < text.length() && Character.isLowSurrogate(text.charAt(i + 1))) { i++; continue; }
                return true;
            }
            if (Character.isLowSurrogate(c)) return true;
        }
        return false;
    }

    static Object argument(Class<?> type, int length, List<String> json) {
        if (type == int.class) { int v = smallInt(length); json.add("{\"int\":" + v + "}"); return v; }
        if (type == char.class) { int v = CHARS[random.nextInt(CHARS.length)]; json.add("{\"int\":" + v + "}"); return (char) v; }
        if (type == boolean.class) { boolean v = random.nextBoolean(); json.add("{\"int\":" + (v ? 1 : 0) + "}"); return v; }
        if (type == long.class) {
            long[] pool = {0, -1, Long.MIN_VALUE, Long.MAX_VALUE, 1234567890123L, random.nextLong()};
            long v = pool[random.nextInt(pool.length)]; json.add("{\"long\":" + v + "}"); return v;
        }
        if (type == float.class) {
            float v = random.nextInt(3) == 0 ? Float.intBitsToFloat(random.nextInt()) : (float) (random.nextGaussian() * Math.pow(10, random.nextInt(20) - 10));
            json.add("{\"float\":" + Integer.toUnsignedString(Float.floatToRawIntBits(v)) + "}"); return v;
        }
        if (type == double.class) {
            double v = random.nextInt(3) == 0 ? Double.longBitsToDouble(random.nextLong()) : random.nextGaussian() * Math.pow(10, random.nextInt(40) - 20);
            json.add("{\"double\":\"" + Long.toUnsignedString(Double.doubleToRawLongBits(v)) + "\"}"); return v;
        }
        if (type == char[].class) {
            char[] v = ARRAYS[random.nextInt(ARRAYS.length)];
            json.add("{\"chars\":" + units(v) + "}"); return v == null ? null : v.clone();
        }
        // String, CharSequence, Object, StringBuffer: text.
        String v = ARG_TEXTS[random.nextInt(ARG_TEXTS.length)];
        if (v != null && lone(v) && type != CharSequence.class && type != Object.class && type != String.class) v = "ok";
        json.add("{\"text\":" + units(v) + "}");
        if (type == StringBuffer.class) return v == null ? null : new StringBuffer(v);
        return v;
    }

    static final String[][] SURFACE = {
        // name, parameter types (Java names), for both builder classes
        {"<init>"}, {"<init>", "int"}, {"<init>", "String"}, {"<init>", "CharSequence"},
        {"length"}, {"isEmpty"}, {"toString"}, {"trimToSize"}, {"ensureCapacity", "int"},
        {"charAt", "int"}, {"codePointAt", "int"}, {"codePointBefore", "int"}, {"codePointCount", "int", "int"},
        {"setCharAt", "int", "char"}, {"setLength", "int"}, {"deleteCharAt", "int"}, {"delete", "int", "int"},
        {"replace", "int", "int", "String"}, {"reverse"}, {"substring", "int"}, {"substring", "int", "int"},
        {"subSequence", "int", "int"}, {"indexOf", "String"}, {"indexOf", "String", "int"},
        {"lastIndexOf", "String"}, {"lastIndexOf", "String", "int"}, {"getChars", "int", "int", "char[]", "int"},
        {"append", "String"}, {"append", "Object"}, {"append", "CharSequence"}, {"append", "StringBuffer"},
        {"append", "CharSequence", "int", "int"}, {"append", "char[]"}, {"append", "char[]", "int", "int"},
        {"append", "boolean"}, {"append", "char"}, {"append", "int"}, {"append", "long"}, {"append", "float"},
        {"append", "double"}, {"appendCodePoint", "int"},
        {"insert", "int", "String"}, {"insert", "int", "Object"}, {"insert", "int", "CharSequence"},
        {"insert", "int", "CharSequence", "int", "int"}, {"insert", "int", "char[]"},
        {"insert", "int", "char[]", "int", "int"}, {"insert", "int", "boolean"}, {"insert", "int", "char"},
        {"insert", "int", "int"}, {"insert", "int", "long"}, {"insert", "int", "float"}, {"insert", "int", "double"},
        {"repeat", "int", "int"}, {"repeat", "CharSequence", "int"}, {"compareTo", "SELF"},
    };

    static Class<?> type(String name, Class<?> self) {
        return switch (name) {
            case "int" -> int.class; case "char" -> char.class; case "boolean" -> boolean.class;
            case "long" -> long.class; case "float" -> float.class; case "double" -> double.class;
            case "char[]" -> char[].class; case "String" -> String.class; case "Object" -> Object.class;
            case "CharSequence" -> CharSequence.class; case "StringBuffer" -> StringBuffer.class;
            case "SELF" -> self;
            default -> throw new IllegalArgumentException(name);
        };
    }

    static String descriptor(Class<?> c) {
        if (c == int.class) return "I"; if (c == char.class) return "C"; if (c == boolean.class) return "Z";
        if (c == long.class) return "J"; if (c == float.class) return "F"; if (c == double.class) return "D";
        if (c == byte.class) return "B"; if (c == short.class) return "S";
        if (c == void.class) return "V"; if (c.isArray()) return "[" + descriptor(c.getComponentType());
        return "L" + c.getName().replace('.', '/') + ";";
    }

    static String builderCase(Class<?> owner) throws Exception {
        String[] shape = SURFACE[random.nextInt(SURFACE.length)];
        String initial = TEXTS[random.nextInt(TEXTS.length)];
        Class<?>[] types = new Class<?>[shape.length - 1];
        for (int i = 1; i < shape.length; i++) types[i - 1] = type(shape[i], owner);
        List<String> json = new ArrayList<>();
        Object[] values = new Object[types.length];
        for (int i = 0; i < types.length; i++) {
            if (shape[i + 1].equals("SELF")) {
                String v = ARG_TEXTS[random.nextInt(ARG_TEXTS.length)];
                if (v != null && lone(v)) v = "abc";
                json.add("{\"text\":" + units(v) + "}");
                values[i] = v == null ? null : (owner == StringBuilder.class ? new StringBuilder(v) : new StringBuffer(v));
            } else {
                values[i] = argument(types[i], initial.length(), json);
            }
        }
        String name = shape[0];
        StringBuilder d = new StringBuilder("(");
        for (Class<?> t : types) d.append(descriptor(t));
        d.append(')');
        Object receiver;
        Object result;
        Class<?> returns;
        String expect;
        try {
            if (name.equals("<init>")) {
                Constructor<?> constructor = owner.getConstructor(types);
                returns = void.class;
                receiver = constructor.newInstance(values);
                result = null;
            } else {
                receiver = owner.getConstructor(String.class).newInstance(initial);
                Method method = owner.getMethod(name, types);
                returns = method.getReturnType();
                result = method.invoke(receiver, values);
            }
            d.append(descriptor(returns));
            String after = receiver.toString();
            String answer;
            boolean unmodelled = lone(after);
            if (returns == void.class) {
                if (name.equals("getChars")) {
                    char[] dst = (char[]) values[2];
                    int at = (Integer) values[3];
                    int n = (Integer) values[1] - (Integer) values[0];
                    answer = "{\"write\":{\"at\":" + at + ",\"units\":" + units(new String(dst, at, n)) + "}}";
                } else {
                    answer = "\"void\"";
                }
            } else if (returns == owner || returns == Appendable.class) {
                if (result != receiver) throw new IllegalStateException(name + " did not return this");
                answer = "\"this\"";
            } else if (returns == int.class) {
                answer = "{\"int\":" + result + "}";
            } else if (returns == char.class) {
                answer = "{\"int\":" + (int) (Character) result + "}";
            } else if (returns == boolean.class) {
                answer = "{\"int\":" + ((Boolean) result ? 1 : 0) + "}";
            } else {
                CharSequence text = (CharSequence) result;
                unmodelled |= lone(text);
                answer = "{\"text\":" + units(text) + "}";
            }
            expect = unmodelled ? "\"unmodelled\"" : "{\"after\":" + units(after) + ",\"answer\":" + answer + "}";
        } catch (InvocationTargetException wrapped) {
            Throwable thrown = wrapped.getCause();
            if (thrown instanceof OutOfMemoryError) return null;
            if (d.charAt(d.length() - 1) == ')') {
                Class<?> r = name.equals("<init>") ? void.class : owner.getMethod(name, types).getReturnType();
                d.append(descriptor(r));
            }
            String message = thrown.getMessage();
            expect = "{\"throws\":{\"class\":\"" + thrown.getClass().getName().replace('.', '/')
                + "\",\"message\":" + (message == null ? "null" : units(message)) + "}}";
        }
        return "{\"owner\":\"" + owner.getName().replace('.', '/') + "\",\"name\":\"" + name
            + "\",\"descriptor\":\"" + d + "\",\"initial\":" + units(initial)
            + ",\"args\":[" + String.join(",", json) + "],\"expect\":" + expect + "}";
    }

    static double interestingDouble() {
        switch (random.nextInt(8)) {
            case 0: return Double.longBitsToDouble(random.nextLong());
            case 1: return Double.longBitsToDouble(random.nextLong() & 0x000fffffffffffffL); // subnormal
            case 2: return Math.pow(10, random.nextInt(640) - 330) * (random.nextBoolean() ? 1 : -1);
            case 3: return Math.scalb(1.0, random.nextInt(2100) - 1075);
            case 4: return Math.nextUp(Math.pow(10, random.nextInt(40) - 20));
            case 5: return (double) random.nextInt(100000) / Math.pow(10, random.nextInt(8));
            case 6: return Double.parseDouble((random.nextInt(9) + 1) + "e" + (random.nextInt(600) - 300));
            default: return random.nextGaussian() * Math.pow(10, random.nextInt(30) - 15);
        }
    }

    static float interestingFloat() {
        switch (random.nextInt(7)) {
            case 0: return Float.intBitsToFloat(random.nextInt());
            case 1: return Float.intBitsToFloat(random.nextInt() & 0x007fffff);
            case 2: return (float) Math.pow(10, random.nextInt(90) - 45);
            case 3: return Math.scalb(1.0f, random.nextInt(300) - 150);
            case 4: return random.nextInt(1 << 24) / 8.0f * (random.nextBoolean() ? 1 : -1); // ties
            case 5: return Float.parseFloat((random.nextInt(9) + 1) + "e" + (random.nextInt(90) - 45));
            default: return (float) (random.nextGaussian() * Math.pow(10, random.nextInt(20) - 10));
        }
    }

    static final double[] FIXED_DOUBLES = {
        0.0, -0.0, 1.0, -1.0, 0.1, 0.001, 0.0001, 1e7, 9999999.0, 1e-3, 1e23, 2e23, 4.9e-324, Double.MIN_NORMAL,
        Double.MAX_VALUE, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY, 100.0, 1.0E20,
        123456789.0, 0.3, 2.0 / 3, 5e-324, 1.7976931348623157e308, 9.995, 1e22, 5.0E-5, 2.2250738585072014E-308,
    };
    static final float[] FIXED_FLOATS = {
        0f, -0f, 1f, 0.1f, 1.4e-45f, Float.MIN_NORMAL, Float.MAX_VALUE, Float.NaN, Float.POSITIVE_INFINITY,
        -219738.625f, 1e7f, 9999999f, 0.001f, 1.0E-4f, 3.4028235e38f, 16777216f, 33554432f, 8.589973e9f,
    };

    static final String[] CHARACTER_INT = {
        "isLetter", "isDigit", "isLetterOrDigit", "isTitleCase", "isSpaceChar", "isDefined", "isLowerCase",
        "isUpperCase", "isWhitespace", "isJavaIdentifierStart", "isJavaIdentifierPart", "isUnicodeIdentifierStart",
        "isUnicodeIdentifierPart", "isIdentifierIgnorable", "isMirrored", "isISOControl", "isAlphabetic",
        "isIdeographic", "isEmoji", "isEmojiPresentation", "isEmojiModifier", "isEmojiModifierBase",
        "isEmojiComponent", "isExtendedPictographic", "isBmpCodePoint", "isValidCodePoint",
        "isSupplementaryCodePoint", "getType", "getDirectionality", "getNumericValue", "toUpperCase",
        "toLowerCase", "toTitleCase", "charCount",
    };
    static final String[] CHARACTER_CHAR = {
        "isLetter", "isDigit", "isLetterOrDigit", "isTitleCase", "isSpaceChar", "isDefined", "isLowerCase",
        "isUpperCase", "isWhitespace", "isJavaIdentifierStart", "isJavaIdentifierPart", "isUnicodeIdentifierStart",
        "isUnicodeIdentifierPart", "isIdentifierIgnorable", "isMirrored", "isISOControl", "isJavaLetter",
        "isJavaLetterOrDigit", "isSpace", "isHighSurrogate", "isLowSurrogate", "isSurrogate", "getType",
        "getDirectionality", "getNumericValue", "toUpperCase", "toLowerCase", "toTitleCase", "hashCode",
        "reverseBytes",
    };

    static String answerOf(Method m, Object... args) throws Exception {
        try {
            Object r = m.invoke(null, args);
            if (r instanceof Boolean b) return b ? "1" : "0";
            if (r instanceof Character c) return Integer.toString(c);
            if (r instanceof Number n) return Long.toString(n.longValue());
            if (r == null) return "null";
            if (r instanceof String s) return lone(s) ? "\"unmodelled\"" : "{\"text\":" + units(s) + "}";
            return "{\"block\":\"" + r + "\"}";
        } catch (InvocationTargetException e) {
            String message = e.getCause().getMessage();
            return "{\"throws\":{\"class\":\"" + e.getCause().getClass().getName().replace('.', '/')
                + "\",\"message\":" + (message == null ? "null" : units(message)) + "}}";
        }
    }

    static String characterCase(int cp) throws Exception {
        List<String> answers = new ArrayList<>();
        for (String name : CHARACTER_INT) {
            Method m = Character.class.getMethod(name, int.class);
            answers.add("\"" + name + descriptor(int.class, m) + "\":" + answerOf(m, cp));
        }
        Method block = Character.UnicodeBlock.class.getMethod("of", int.class);
        answers.add("\"UnicodeBlock.of(I)\":" + answerOf(block, cp));
        Method toString = Character.class.getMethod("toString", int.class);
        answers.add("\"toString(I)Ljava/lang/String;\":" + answerOf(toString, cp));
        for (int radix : new int[] {-1, 2, 10, 16, 36, 37}) {
            answers.add("\"digit(II)I/" + radix + "\":" + Character.digit(cp, radix));
            answers.add("\"forDigit(II)C/" + radix + "\":" + (int) Character.forDigit(cp, radix));
        }
        answers.add("\"highSurrogate(I)C\":" + (int) Character.highSurrogate(cp));
        answers.add("\"lowSurrogate(I)C\":" + (int) Character.lowSurrogate(cp));
        if (cp >= 0 && cp <= 0xFFFF) {
            char c = (char) cp;
            for (String name : CHARACTER_CHAR) {
                Method m = Character.class.getMethod(name, char.class);
                answers.add("\"" + name + descriptor(char.class, m) + "\":" + answerOf(m, c));
            }
            answers.add("\"UnicodeBlock.of(C)\":" + answerOf(Character.UnicodeBlock.class.getMethod("of", char.class), c));
            answers.add("\"toString(C)Ljava/lang/String;\":" + answerOf(Character.class.getMethod("toString", char.class), c));
            for (int radix : new int[] {-1, 2, 10, 16, 36, 37}) {
                answers.add("\"digit(CI)I/" + radix + "\":" + Character.digit(c, radix));
            }
            int other = random.nextInt(0x10000);
            answers.add("\"compare(CC)I/" + other + "\":" + Character.compare(c, (char) other));
            answers.add("\"isSurrogatePair(CC)Z/" + other + "\":" + (Character.isSurrogatePair(c, (char) other) ? 1 : 0));
            answers.add("\"toCodePoint(CC)I/" + other + "\":" + Character.toCodePoint(c, (char) other));
        }
        return "{\"cp\":" + cp + ",\"answers\":{" + String.join(",", answers) + "}}";
    }

    static String descriptor(Class<?> param, Method m) {
        return "(" + descriptor(param) + ")" + descriptor(m.getReturnType());
    }

    public static void main(String[] args) throws Exception {
        int cases = Integer.parseInt(args[0]);
        random = new Random(Long.parseLong(args[1]));
        List<String> builders = new ArrayList<>();
        while (builders.size() < cases) {
            String one = builderCase(random.nextInt(4) == 0 ? StringBuffer.class : StringBuilder.class);
            if (one != null) builders.add(one);
        }
        List<String> doubles = new ArrayList<>();
        for (double v : FIXED_DOUBLES) doubles.add("[\"" + Long.toUnsignedString(Double.doubleToRawLongBits(v)) + "\",\"" + v + "\"]");
        for (int i = 0; i < cases / 2; i++) {
            double v = interestingDouble();
            doubles.add("[\"" + Long.toUnsignedString(Double.doubleToRawLongBits(v)) + "\",\"" + v + "\"]");
        }
        List<String> floats = new ArrayList<>();
        for (float v : FIXED_FLOATS) floats.add("[" + Integer.toUnsignedString(Float.floatToRawIntBits(v)) + ",\"" + v + "\"]");
        for (int i = 0; i < cases / 2; i++) {
            float v = interestingFloat();
            floats.add("[" + Integer.toUnsignedString(Float.floatToRawIntBits(v)) + ",\"" + v + "\"]");
        }
        List<String> characters = new ArrayList<>();
        int[] fixed = {-1, Integer.MIN_VALUE, 0x110000, Integer.MAX_VALUE, 0, 'a', 'Z', '5', 0x130, 0x1c5, 0xdf,
            0x10ec5, 0x1d7ce, 0xd800, 0xdfff, 0xff21, 0x2160, 0x10ffff, 0x3000, 0x2028, 0xfeff, 0x1f600};
        for (int cp : fixed) characters.add(characterCase(cp));
        for (int i = 0; i < cases / 12; i++) {
            int cp = random.nextInt(3) == 0 ? random.nextInt(0x300) : random.nextInt(0x110000);
            characters.add(characterCase(cp));
        }
        System.out.println("{\"jdk\":\"" + System.getProperty("java.version") + "\",\"jdk_build\":\""
            + System.getProperty("java.vendor") + " " + System.getProperty("java.runtime.version")
            + "\",\"seed\":" + args[1] + ",");
        System.out.println("\"builders\":[\n" + String.join(",\n", builders) + "\n],");
        System.out.println("\"doubles\":[\n" + String.join(",\n", doubles) + "\n],");
        System.out.println("\"floats\":[\n" + String.join(",\n", floats) + "\n],");
        System.out.println("\"characters\":[\n" + String.join(",\n", characters) + "\n]}");
    }
}
