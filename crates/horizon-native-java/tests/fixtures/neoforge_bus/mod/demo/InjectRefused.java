package demo;

import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

/// One `@EventBusSubscriber` per refusal, each its own mod so a refusal fails only that container.
public class InjectRefused {
    @EventBusSubscriber(modid = "refuse0")
    public static class NotStatic {
        @SubscribeEvent
        public void on(ServerTickEvent.Post event) {}
    }

    @EventBusSubscriber(modid = "refuse1")
    public static class TwoArguments {
        @SubscribeEvent
        public static void on(ServerTickEvent.Post event, int extra) {}
    }

    @EventBusSubscriber(modid = "refuse2")
    public static class NotAnEvent {
        @SubscribeEvent
        public static void on(String text) {}
    }

    @EventBusSubscriber(modid = "refuse3")
    public static class AbstractEvent {
        @SubscribeEvent
        public static void on(ServerTickEvent event) {}
    }
}
