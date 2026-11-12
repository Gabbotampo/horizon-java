package imcsend;

import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.InterModComms;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.event.lifecycle.InterModEnqueueEvent;
import net.neoforged.fml.event.lifecycle.InterModProcessEvent;
import net.neoforged.neoforge.network.event.RegisterPayloadHandlersEvent;

/// The sending jar. Every line it prints starts with `S`. Its suppliers print when they run, so
/// the receiver's reads show when (and how often) they are called.
@Mod("sender")
public class SendMod {
    static int counted;

    public SendMod(IEventBus modBus) {
        System.out.println("S constructor early " + InterModComms.sendTo("receiver", "early", () -> "from-constructor"));
        modBus.addListener((InterModEnqueueEvent e) -> {
            System.out.println("S enqueue greet " + InterModComms.sendTo("receiver", "greet", () -> {
                System.out.println("S supplier greet runs");
                return "hello";
            }));
            System.out.println("S enqueue count " + InterModComms.sendTo("receiver", "count", () -> {
                counted++;
                System.out.println("S supplier count runs " + counted);
                return counted * 10;
            }));
            System.out.println("S enqueue unread " + InterModComms.sendTo("receiver", "unread", () -> {
                System.out.println("S supplier unread runs");
                return "never-read";
            }));
            System.out.println("S enqueue object " + InterModComms.sendTo("receiver", "object", () -> new StringBuilder("sb")));
            System.out.println("S enqueue missing " + InterModComms.sendTo("missing", "x", () -> "never"));
            System.out.println("S enqueue null-target " + InterModComms.sendTo(null, "x", () -> "never"));
            System.out.println("S enqueue named " + InterModComms.sendTo("custom-sender", "receiver", "named", () -> "named-value"));
            e.enqueueWork(() -> System.out.println("S work deferred " + InterModComms.sendTo("receiver", "deferred", () -> "from-work")));
        });
        modBus.addListener((InterModProcessEvent e) -> InterModComms.getMessages("sender").forEach(m ->
                System.out.println("S got " + m.senderModId() + " " + m.modId() + " " + m.method() + " " + m.messageSupplier().get())));
        modBus.addListener((RegisterPayloadHandlersEvent e) ->
                System.out.println("S payloads late " + InterModComms.sendTo("receiver", "late", () -> "after-loading")));
    }
}
