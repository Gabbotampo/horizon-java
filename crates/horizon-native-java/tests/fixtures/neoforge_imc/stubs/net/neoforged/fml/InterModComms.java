package net.neoforged.fml;

import java.util.function.Predicate;
import java.util.function.Supplier;
import java.util.stream.Stream;

// Compile-only stub of loader-12.0.12's class: the descriptors are the real ones.
public class InterModComms {
    public record IMCMessage(String senderModId, String modId, String method, Supplier<?> messageSupplier) {}

    public static boolean sendTo(String modId, String method, Supplier<?> thing) {
        return false;
    }

    public static boolean sendTo(String senderModId, String modId, String method, Supplier<?> thing) {
        return false;
    }

    public static Stream<IMCMessage> getMessages(String modId, Predicate<String> methodMatcher) {
        return null;
    }

    public static Stream<IMCMessage> getMessages(String modId) {
        return null;
    }
}
