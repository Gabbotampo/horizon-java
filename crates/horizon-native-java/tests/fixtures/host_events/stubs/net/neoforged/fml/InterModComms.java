package net.neoforged.fml;

import java.util.function.Supplier;

// Compile-only stub of loader-12.0.12's class.
public class InterModComms {
    public record IMCMessage(String senderModId, String modId, String method, Supplier<?> messageSupplier) {}
}
