package demo;

import static java.nio.file.StandardWatchEventKinds.ENTRY_CREATE;
import static java.nio.file.StandardWatchEventKinds.ENTRY_DELETE;
import static java.nio.file.StandardWatchEventKinds.ENTRY_MODIFY;
import static java.nio.file.StandardWatchEventKinds.OVERFLOW;

import java.io.BufferedWriter;
import java.io.FileWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.WatchEvent;
import java.nio.file.WatchKey;
import java.nio.file.WatchService;
import java.util.concurrent.TimeUnit;

public final class Probe {
    public interface Step {
        Object run() throws Exception;
    }

    /// Lets a JVM's poller thread catch up before a drain. The oracle's driver
    /// sets it to a 200 ms sleep; the runtime's events are settled already.
    public static Step settle = () -> null;

    static String attempt(Step step) {
        try {
            return String.valueOf(step.run());
        } catch (Exception error) {
            return error.getClass().getSimpleName() + ":" + error.getMessage();
        }
    }

    static String drain(WatchService service) throws Exception {
        settle.run();
        StringBuilder out = new StringBuilder();
        WatchKey key;
        while ((key = service.poll(300, TimeUnit.MILLISECONDS)) != null) {
            for (WatchEvent<?> event : key.pollEvents()) {
                out.append(' ').append(event.kind()).append('/').append(event.context());
                if (event.kind() == ENTRY_CREATE || event.kind() == ENTRY_DELETE) {
                    out.append('x').append(event.count());
                }
            }
            out.append(" reset=").append(key.reset());
        }
        return out.toString();
    }

    static String step(WatchService service, String name, Step step) throws Exception {
        return name + ":" + attempt(step) + drain(service) + "\n";
    }

    public static String run() throws Exception {
        Path w = Files.createDirectories(Path.of("w"));
        Path other = Files.createDirectories(Path.of("other"));
        WatchService service = w.getFileSystem().newWatchService();
        WatchKey watched = w.register(service, ENTRY_CREATE, ENTRY_DELETE, ENTRY_MODIFY);
        Path a = w.resolve("a.txt");
        StringBuilder out = new StringBuilder();
        out.append("watchable:").append(watched.watchable() == w).append('\n');
        out.append(step(service, "A", () -> Files.writeString(a, "x")));
        out.append(step(service, "B", () -> Files.writeString(a, "yy")));
        out.append(step(service, "E", () -> Files.writeString(a, "")));
        out.append(step(service, "D", () -> Files.writeString(w.resolve("b.txt"), "")));
        out.append(step(service, "F", () -> Files.createFile(w.resolve("c.txt"))));
        out.append(step(service, "G", () -> Files.createDirectory(w.resolve("sub"))));
        out.append(step(service, "H", () -> Files.createDirectories(w.resolve("n1/n2/n3"))));
        out.append(step(service, "H2", () -> Files.createDirectories(w.resolve("n1"))));
        out.append(step(service, "I", () -> { Files.delete(w.resolve("c.txt")); return null; }));
        out.append(step(service, "J", () -> Files.deleteIfExists(w.resolve("zz"))));
        out.append(step(service, "K", () -> Files.move(w.resolve("b.txt"), w.resolve("b2.txt"))));
        out.append(step(service, "L", () -> {
            Files.writeString(w.resolve("t.new"), "q");
            return Files.move(w.resolve("t.new"), a, StandardCopyOption.ATOMIC_MOVE);
        }));
        out.append(step(service, "L2", () -> {
            Files.writeString(w.resolve("t.new"), "r");
            return Files.move(w.resolve("t.new"), a, StandardCopyOption.REPLACE_EXISTING);
        }));
        out.append(step(service, "M", () -> Files.move(w.resolve("b2.txt"), other.resolve("b2.txt"))));
        out.append(step(service, "N", () -> Files.move(other.resolve("b2.txt"), w.resolve("b3.txt"))));
        out.append(step(service, "O", () -> Files.copy(a, w.resolve("cp.txt"))));
        out.append(step(service, "P", () ->
            Files.copy(a, w.resolve("cp.txt"), StandardCopyOption.REPLACE_EXISTING)));
        out.append(step(service, "Q", () -> Files.copy(w.resolve("b3.txt"), w.resolve("cpe.txt"))));
        out.append(step(service, "R", () -> {
            try (BufferedWriter writer = Files.newBufferedWriter(w.resolve("bw.txt"))) {
                writer.write("abc");
            }
            return null;
        }));
        out.append(step(service, "R2", () -> {
            try (BufferedWriter writer = Files.newBufferedWriter(w.resolve("bw0.txt"))) {
                return null;
            }
        }));
        out.append(step(service, "S", () -> {
            try (BufferedWriter writer = Files.newBufferedWriter(w.resolve("bw.txt"))) {
                writer.write("abcd");
            }
            return null;
        }));
        out.append(step(service, "U", () -> {
            try (FileWriter writer = new FileWriter(w.resolve("fw.txt").toFile())) {
                writer.write("abc");
            }
            return null;
        }));
        out.append(step(service, "V", () -> w.resolve("fd").toFile().mkdir()));
        out.append(step(service, "V2", () -> w.resolve("cnf").toFile().createNewFile()));
        // A truncating open of a.txt, a new file, then the write: the second
        // ENTRY_MODIFY of a.txt joins the first.
        out.append(step(service, "W", () -> {
            BufferedWriter writer = Files.newBufferedWriter(a);
            Files.writeString(w.resolve("wb.txt"), "");
            writer.write("1");
            writer.close();
            return null;
        }));
        out.append(step(service, "X", () -> Files.writeString(w.resolve("sub/x.txt"), "x")));
        out.append(step(service, "Y", () -> {
            Files.delete(w.resolve("sub/x.txt"));
            Files.delete(w.resolve("sub"));
            return null;
        }));

        // A key's kinds filter its events; a key nobody reset is not queued again.
        WatchService second = other.getFileSystem().newWatchService();
        WatchKey modifyOnly = other.register(second, ENTRY_MODIFY);
        Files.writeString(other.resolve("m.txt"), "x");
        settle.run();
        WatchKey got = second.poll();
        out.append("Z:").append(got == modifyOnly);
        for (WatchEvent<?> event : got.pollEvents()) {
            out.append(' ').append(event.kind()).append('/').append(event.context());
        }
        Files.writeString(other.resolve("m.txt"), "y");
        settle.run();
        out.append(" unreset=").append(second.poll());
        out.append(" reset=").append(got.reset());
        out.append(" requeued=").append(second.poll() == modifyOnly);
        out.append(" again=").append(got.pollEvents().size()).append(',').append(got.reset())
            .append(',').append(second.poll()).append('\n');
        out.append("reregister:").append(other.register(second, ENTRY_CREATE) == modifyOnly);
        for (int i = 0; i < 600; i++) {
            Files.writeString(other.resolve("f" + i), "");
        }
        settle.run();
        WatchKey flood = second.poll();
        java.util.List<WatchEvent<?>> flooded = flood.pollEvents();
        out.append(" flood=").append(flooded.size()).append(',').append(flooded.get(0).kind())
            .append(',').append(flooded.get(0).context()).append(',').append(flood.reset())
            .append('\n');

        // A watched directory deleted: its key is signalled and invalid.
        Path gone = Files.createDirectory(w.resolve("gone"));
        WatchKey goneKey = gone.register(second, ENTRY_CREATE);
        Files.delete(gone);
        settle.run();
        WatchKey signalled = second.poll();
        out.append("gone:").append(signalled == goneKey).append(',').append(goneKey.isValid())
            .append(',').append(signalled.pollEvents().size()).append(',').append(goneKey.reset())
            .append('\n');

        modifyOnly.cancel();
        Files.writeString(other.resolve("after-cancel"), "x");
        settle.run();
        out.append("cancel:").append(second.poll()).append(',').append(modifyOnly.isValid())
            .append(',').append(modifyOnly.reset()).append('\n');
        second.close();
        second.close();
        out.append("closed:").append(attempt(second::poll)).append(',')
            .append(attempt(second::take)).append(',')
            .append(attempt(() -> other.register(second, ENTRY_CREATE))).append(',')
            .append(watched.isValid()).append('\n');
        out.append("register:")
            .append(attempt(() -> Path.of("nope").register(service, ENTRY_CREATE))).append(',')
            .append(attempt(() -> a.register(service, ENTRY_CREATE))).append(',')
            .append(attempt(() -> w.register(service))).append(',')
            .append(attempt(() -> w.register(service, OVERFLOW))).append(',')
            .append(attempt(() -> w.register(service, (WatchEvent.Kind<?>) null))).append('\n');
        out.append("delete:")
            .append(attempt(() -> { Files.delete(w.resolve("zz")); return null; })).append(',')
            .append(attempt(() -> { Files.delete(w.resolve("n1")); return null; })).append(',')
            .append(attempt(() -> Files.deleteIfExists(w.resolve("n1")))).append(',')
            .append(w.resolve("zz").toFile().delete()).append(',')
            .append(w.resolve("n1").toFile().delete()).append(',')
            .append(w.resolve("n1").toFile().isDirectory()).append(drain(service)).append('\n');
        Files.writeString(w.resolve("ev.txt"), "");
        settle.run();
        WatchKey last = service.poll();
        WatchEvent<?> event = last.pollEvents().get(0);
        out.append("event:").append(event.kind().name()).append(',')
            .append(((Path) event.context()).isAbsolute()).append(',')
            .append(last.pollEvents().isEmpty()).append(',').append(service.poll());
        service.close();
        out.append(',').append(watched.isValid());
        return out.toString();
    }

    /// What the model can't answer exactly: each call is counted.
    public static String counted() throws Exception {
        Path w = Files.createDirectories(Path.of("w"));
        WatchService service = w.getFileSystem().newWatchService();
        w.register(service, ENTRY_CREATE, ENTRY_MODIFY);
        // The JVM's ENTRY_MODIFY count depends on timing: counted.
        Files.writeString(w.resolve("a.txt"), "x");
        Files.writeString(w.resolve("a.txt"), "y");
        WatchKey key = service.poll();
        StringBuilder out = new StringBuilder();
        for (WatchEvent<?> event : key.pollEvents()) {
            out.append(event.kind()).append(event.kind() == ENTRY_MODIFY ? event.count() : 1);
        }
        key.reset();
        // Nothing pending: take() would block for ever.
        out.append(',').append(service.take());
        // A writer holding unflushed text: whether the JVM wrote it depends
        // on its buffer.
        BufferedWriter writer = Files.newBufferedWriter(w.resolve("b.txt"));
        writer.write("pending");
        out.append(',').append(service.poll() == key);
        writer.close();
        return out.toString();
    }
}
