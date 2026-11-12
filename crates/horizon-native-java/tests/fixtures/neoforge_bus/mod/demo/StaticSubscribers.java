package demo;

import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.neoforge.event.entity.living.LivingDeathEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

/// Static `@SubscribeEvent` methods, for `register(StaticSubscribers.class)`.
public class StaticSubscribers {
    @SubscribeEvent(priority = EventPriority.LOW)
    public static void tick(ServerTickEvent.Post event) {
        System.out.println("static-low");
    }

    @SubscribeEvent(priority = EventPriority.LOW)
    static void cancel(LivingDeathEvent event) {
        System.out.println("static-cancels");
        event.setCanceled(true);
    }

    @SubscribeEvent(priority = EventPriority.LOWEST)
    private static void skipped(ServerTickEvent.Pre event) {
        System.out.println("static-pre");
    }
}
