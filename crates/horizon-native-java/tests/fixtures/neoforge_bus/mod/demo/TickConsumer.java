package demo;

import java.util.function.Consumer;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

/// A listener object: typetools reads its event from this class's `Consumer<...>` signature.
public class TickConsumer implements Consumer<ServerTickEvent.Post> {
    private final String name;

    TickConsumer(String name) {
        this.name = name;
    }

    @Override
    public void accept(ServerTickEvent.Post event) {
        System.out.println(name);
    }
}
