package demo;

import java.io.BufferedWriter;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.function.Function;
import java.util.function.UnaryOperator;

public final class Probe {
    public static String run() throws Exception {
        StringBuilder out = new StringBuilder();
        "a😀b".codePoints().forEach(point -> out.append(point).append(','));
        out.append('|');
        Path first = Path.of("first.toml");
        try (Writer writer = Files.newBufferedWriter(first)) {
            char[] chars = "key = \"value\"\n".toCharArray();
            writer.write(chars, 0, 6);
            writer.write(chars, 6, chars.length - 6);
            writer.write(new char[] {'#', '\n'});
        }
        out.append(Files.readString(first)).append('|');
        Path second = Path.of("second.toml");
        try (Writer writer = new BufferedWriter(new OutputStreamWriter(
                Files.newOutputStream(second), StandardCharsets.UTF_8))) {
            char[] smile = "x😀y".toCharArray();
            writer.write(smile, 1, 3);
        }
        out.append(Files.readString(second).length()).append('|');
        UnaryOperator<String> unary = UnaryOperator.identity();
        Function<String, String> function = Function.identity();
        out.append(unary.apply("u")).append(function.apply("f"))
            .append(unary == UnaryOperator.<String>identity()).append(function == Function.<String>identity())
            .append((Object) unary == (Object) function).append('|');
        out.append(Character.toChars(65).length).append(Character.toChars(0x1F600).length)
            .append((int) Character.toChars(0x1F600)[0]).append(',').append((int) Character.toChars(0x1F600)[1]).append('|');
        try {
            Character.toChars(-1);
        } catch (IllegalArgumentException error) {
            out.append(error.getMessage());
        }
        return out.toString();
    }

}
