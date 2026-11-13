package net.neoforged.bus.api;

// Compile-only stub of bus-8.0.5's interface: the fixture jar never holds it.
public interface ICancellableEvent {
    default void setCanceled(boolean canceled) {}

    default boolean isCanceled() {
        return false;
    }
}
