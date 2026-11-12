package net.neoforged.fml.event.lifecycle;

import java.util.concurrent.CompletableFuture;
import java.util.function.Supplier;

// Compile-only stub of loader-12.0.12's class.
public abstract class ParallelDispatchEvent extends ModLifecycleEvent {
    public CompletableFuture<Void> enqueueWork(Runnable work) {
        return null;
    }

    public <T> CompletableFuture<T> enqueueWork(Supplier<T> work) {
        return null;
    }
}
