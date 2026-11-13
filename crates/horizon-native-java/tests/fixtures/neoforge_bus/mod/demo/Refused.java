package demo;

import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

/// One class per registration NeoForge refuses; each has at most one `@SubscribeEvent` method.
public final class Refused {
    private Refused() {}

    public static class OneInstance {
        @SubscribeEvent
        public void on(ServerTickEvent.Post event) {}
    }

    public static class OneStatic {
        @SubscribeEvent
        public static void on(ServerTickEvent.Post event) {}
    }

    public static class Nothing {
        public void on(ServerTickEvent.Post event) {}
    }

    public static class Parent {
        @SubscribeEvent
        public void inherited(ServerTickEvent.Post event) {}
    }

    public static class Child extends Parent {
        @SubscribeEvent
        public void own(ServerTickEvent.Pre event) {}
    }

    public static class TwoArguments {
        @SubscribeEvent
        public static void on(ServerTickEvent.Post event, int extra) {}
    }

    public static class NotAnEvent {
        @SubscribeEvent
        public static void on(String text) {}
    }

    public static class ModBusOnly {
        @SubscribeEvent
        protected static synchronized void on(FMLCommonSetupEvent event) {}
    }

    public static class Abstract {
        @SubscribeEvent
        public static void on(ServerTickEvent event) {}
    }
}
