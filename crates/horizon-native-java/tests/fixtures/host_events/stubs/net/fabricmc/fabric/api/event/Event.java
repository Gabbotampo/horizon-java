package net.fabricmc.fabric.api.event;

// Compile-only stub of fabric-api-base 2.0.6's class.
public abstract class Event<T> {
    protected volatile T invoker;

    public final T invoker() {
        return invoker;
    }

    public abstract void register(T listener);
}
