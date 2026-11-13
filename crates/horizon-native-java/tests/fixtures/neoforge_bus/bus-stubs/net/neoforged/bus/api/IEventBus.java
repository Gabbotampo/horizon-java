package net.neoforged.bus.api;

import java.util.function.Consumer;

// Compile-only stub with bus-8.0.5's signatures (`javap -p net.neoforged.bus.api.IEventBus`); the
// oracle compiles the same fixture against the real jar.
public interface IEventBus {
    void register(Object target);

    <T extends Event> void addListener(Consumer<T> consumer);

    <T extends Event> void addListener(Class<T> eventType, Consumer<T> consumer);

    <T extends Event> void addListener(EventPriority priority, Consumer<T> consumer);

    <T extends Event> void addListener(EventPriority priority, Class<T> eventType, Consumer<T> consumer);

    <T extends Event> void addListener(EventPriority priority, boolean receiveCanceled, Consumer<T> consumer);

    <T extends Event> void addListener(EventPriority priority, boolean receiveCanceled, Class<T> eventType, Consumer<T> consumer);

    <T extends Event> void addListener(boolean receiveCanceled, Consumer<T> consumer);

    <T extends Event> void addListener(boolean receiveCanceled, Class<T> eventType, Consumer<T> consumer);

    void unregister(Object object);

    <T extends Event> T post(T event);

    <T extends Event> T post(EventPriority phase, T event);

    void start();
}
