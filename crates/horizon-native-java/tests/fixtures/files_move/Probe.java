package demo;

import java.nio.file.CopyOption;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;

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

    public static String run() throws Exception {
        Path a = Path.of("a.toml");
        Path b = Path.of("b.toml");
        Path c = Path.of("c.toml");
        Path missing = Path.of("missing.toml");
        StringBuilder out = new StringBuilder();
        Files.writeString(a, "one");
        out.append(Files.move(a, b)).append(',').append(Files.exists(a)).append(',')
            .append(Files.readString(b)).append('|');
        Files.writeString(a, "two");
        out.append(attempt(() -> Files.move(a, b))).append('|');
        Files.move(a, b, StandardCopyOption.REPLACE_EXISTING);
        out.append(Files.readString(b)).append('|');
        Files.writeString(c, "three");
        Files.move(c, b, StandardCopyOption.ATOMIC_MOVE);
        out.append(Files.readString(b)).append(',').append(Files.exists(c)).append('|');
        out.append(Files.copy(b, a)).append(',').append(Files.readString(a)).append(',')
            .append(Files.exists(b)).append('|');
        out.append(attempt(() -> Files.copy(b, a))).append('|');
        out.append(attempt(() -> Files.move(missing, b))).append('|');
        out.append(attempt(() -> Files.move(missing, b, StandardCopyOption.ATOMIC_MOVE))).append('|');
        out.append(attempt(() -> Files.move(b, a, StandardCopyOption.COPY_ATTRIBUTES))).append('|');
        out.append(attempt(() -> Files.copy(b, c, StandardCopyOption.ATOMIC_MOVE))).append('|');
        out.append(attempt(() -> Files.move(b, b))).append(',').append(Files.readString(b)).append('|');
        out.append(attempt(() -> Files.move(b, a, (CopyOption) null)));
        return out.toString();
    }

}
