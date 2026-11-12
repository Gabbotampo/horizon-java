package imcrecv;

import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.InterModComms;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.event.lifecycle.InterModEnqueueEvent;
import net.neoforged.fml.event.lifecycle.InterModProcessEvent;
import net.neoforged.neoforge.network.event.RegisterPayloadHandlersEvent;

/// The receiving jar. Every line it prints starts with `R`.
@Mod("receiver")
public class ReceiveMod {
    public ReceiveMod(IEventBus modBus) {
        modBus.addListener((InterModEnqueueEvent e) ->
                System.out.println("R enqueue back " + InterModComms.sendTo("sender", "back", () -> "from-receiver")));
        modBus.addListener((InterModProcessEvent e) -> {
            System.out.println("R process self " + InterModComms.sendTo("receiver", "self", () -> "me"));
            InterModComms.getMessages("receiver", m -> m.equals("greet") || m.equals("named") || m.equals("count"))
                    .forEach(m -> {
                        System.out.println("R got " + m.senderModId() + " " + m.modId() + " " + m.method());
                        System.out.println("R value " + m.messageSupplier().get());
                        if (m.method().equals("count")) {
                            System.out.println("R again " + m.messageSupplier().get());
                        }
                    });
            InterModComms.getMessages("receiver", m -> !m.equals("unread") && !m.equals("object"))
                    .forEach(m -> System.out.println("R then " + m.senderModId() + " " + m.method() + " " + m.messageSupplier().get()));
            System.out.println("R nobody " + InterModComms.getMessages("nobody").count());
        });
        modBus.addListener((RegisterPayloadHandlersEvent e) -> InterModComms.getMessages("receiver").forEach(m -> {
            System.out.println("R left " + m.senderModId() + " " + m.method());
            if (m.method().equals("object")) {
                System.out.println("R object-is-string " + (m.messageSupplier().get() instanceof String));
            }
            if (m.method().equals("late")) {
                System.out.println("R late " + m.messageSupplier().get());
            }
        }));
    }
}
