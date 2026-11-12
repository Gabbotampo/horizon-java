package demo;

import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

/// Subscribers FML does not inject into mod `demo` on a dedicated server.
public class SkippedSubscribers {
    /// Client only.
    @EventBusSubscriber(value = Dist.CLIENT, modid = "demo")
    public static class ClientOnly {
        static {
            System.out.println("client-clinit");
        }

        @SubscribeEvent
        public static void tick(ServerTickEvent.Post event) {
            System.out.println("client-tick");
        }
    }

    /// Another mod's.
    @EventBusSubscriber(modid = "other")
    public static class OtherMod {
        static {
            System.out.println("other-clinit");
        }

        @SubscribeEvent
        public static void tick(ServerTickEvent.Post event) {
            System.out.println("other-tick");
        }
    }
}
