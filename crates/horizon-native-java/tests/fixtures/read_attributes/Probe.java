package demo;

import java.io.File;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.BasicFileAttributes;

public final class Probe {
    interface Step {
        Object run() throws Exception;
    }

    static String attempt(Step step) {
        try {
            return String.valueOf(step.run());
        } catch (Exception error) {
            return error.getClass().getSimpleName() + ":" + error.getMessage();
        }
    }

    static String kind(Path path) throws Exception {
        BasicFileAttributes attributes = Files.readAttributes(path, BasicFileAttributes.class);
        return attributes.isDirectory() + "," + attributes.isRegularFile() + ","
            + attributes.isSymbolicLink() + "," + attributes.isOther();
    }

    static String is(Path path) {
        return Files.isDirectory(path) + "/" + Files.isRegularFile(path);
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    public static String run() throws Exception {
        Path a = Path.of("a.toml");
        Path empty = Path.of("empty.toml");
        Path nested = Path.of("d/e");
        Path missing = Path.of("missing.toml");
        StringBuilder out = new StringBuilder();
        Files.writeString(a, "x");
        Files.createFile(empty);
        Files.createDirectories(nested);
        out.append(kind(a)).append('|').append(kind(empty)).append('|')
            .append(kind(nested)).append('|').append(kind(Path.of("d"))).append('|');
        out.append(is(a)).append(',').append(is(empty)).append(',').append(is(nested)).append(',')
            .append(is(Path.of("d"))).append(',').append(is(missing)).append('|');
        new File("f").mkdirs();
        out.append(new File("f").isDirectory()).append(',').append(new File("f").isFile()).append(',')
            .append(new File("a.toml").isDirectory()).append(',').append(new File("a.toml").isFile())
            .append('|');
        out.append(attempt(() -> kind(missing))).append('|');
        out.append(attempt(() -> kind(Path.of("d/missing/x")))).append('|');
        out.append(attempt(() -> Files.readAttributes(a, (Class) null))).append('|');
        out.append(attempt(() -> Files.readAttributes(a, (Class) String.class))).append('|');
        out.append(attempt(() -> Files.readAttributes(null, BasicFileAttributes.class))).append('|');
        out.append(attempt(() -> Files.isDirectory(null))).append('|');
        out.append(Files.readAttributes(a, BasicFileAttributes.class)
            == Files.readAttributes(a, BasicFileAttributes.class)).append('|');
        Files.delete(Path.of("f"));
        out.append(is(Path.of("f"))).append(',').append(attempt(() -> kind(Path.of("f")))).append('|');
        Files.createDirectory(Path.of("g"));
        Files.move(Path.of("g"), Path.of("h"));
        out.append(is(Path.of("g"))).append(',').append(is(Path.of("h"))).append('|');
        out.append(kind(Path.of("."))).append(',').append(is(Path.of(""))).append('|');
        out.append(java.util.stream.Stream.of(a, nested, missing)
            .filter(Files::isDirectory).toList().size());
        return out.toString();
    }

    /// The attributes the model cannot know: counted.
    public static String counted() throws Exception {
        Path a = Path.of("a.toml");
        Files.writeString(a, "x");
        BasicFileAttributes attributes = Files.readAttributes(a, BasicFileAttributes.class);
        attributes.size();
        attributes.lastModifiedTime();
        attributes.fileKey();
        return String.valueOf(attributes.isRegularFile());
    }
}
