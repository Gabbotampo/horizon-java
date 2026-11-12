package demo;

import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.neoforge.event.entity.living.LivingDeathEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

/// Instance `@SubscribeEvent` methods, for `register(object)`. No two share an event and priority,
/// so the order `Class#getDeclaredMethods` lists them in never shows.
public class Subscribers {
    private final String name;

    public Subscribers(String name) {
        this.name = name;
    }

    @SubscribeEvent(priority = EventPriority.HIGH)
    public void high(ServerTickEvent.Post event) {
        System.out.println(name + "-high");
    }

    @SubscribeEvent
    public void normal(ServerTickEvent.Post event) {
        System.out.println(name + "-normal");
    }

    @SubscribeEvent(priority = EventPriority.LOWEST, receiveCanceled = true)
    public void death(LivingDeathEvent event) {
        System.out.println(name + "-death");
    }

    @SubscribeEvent
    public void demo(DemoEvent event) {
        System.out.println(name + "-demo");
    }

    /// Not annotated: never registered.
    public void ignored(ServerTickEvent.Post event) {
        System.out.println(name + "-ignored");
    }
}
