package demo;

import java.io.InputStream;
import java.nio.file.CopyOption;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;

public final class Probe {
    static String copy(InputStream in, Path target, CopyOption... options) {
        try {
            return String.valueOf(Files.copy(in, target, options));
        } catch (Exception error) {
            return error.getClass().getSimpleName() + ":" + error.getMessage();
        }
    }

    public static String run() throws Exception {
        Path target = Path.of("cfg.toml");
        StringBuilder out = new StringBuilder();
        out.append(copy(Probe.class.getResourceAsStream("/defaults.toml"), target)).append('|');
        out.append(Files.readString(target).trim()).append('|');
        out.append(copy(Probe.class.getResourceAsStream("/defaults.toml"), target)).append('|');
        InputStream again = Probe.class.getResourceAsStream("/defaults.toml");
        out.append(copy(again, target, StandardCopyOption.REPLACE_EXISTING)).append('|');
        out.append(copy(again, target, StandardCopyOption.REPLACE_EXISTING)).append('|');
        out.append(Files.readString(target).length()).append('|');
        out.append(copy(Probe.class.getResourceAsStream("/defaults.toml"), target,
            StandardCopyOption.COPY_ATTRIBUTES)).append('|');
        out.append(copy(Probe.class.getResourceAsStream("/defaults.toml"), target,
            (CopyOption) null)).append('|');
        out.append(copy(null, target));
        return out.toString();
    }

}
