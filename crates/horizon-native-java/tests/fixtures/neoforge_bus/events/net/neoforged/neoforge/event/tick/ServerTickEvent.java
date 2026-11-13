package net.neoforged.neoforge.event.tick;

import net.neoforged.bus.api.Event;

// Stand-in with NeoForge 26.3.0.101's shape (abstract, two concrete nested events; the real ones
// carry the server). Used to compile the fixture and, in the oracle, as the class the real bus sees.
public abstract class ServerTickEvent extends Event {
    public static class Pre extends ServerTickEvent {}

    public static class Post extends ServerTickEvent {}
}
