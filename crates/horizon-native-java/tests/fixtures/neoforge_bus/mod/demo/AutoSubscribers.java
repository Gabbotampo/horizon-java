package demo;

import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

/// An `@EventBusSubscriber` of mod `demo`: FML initialises it and registers its static
/// `@SubscribeEvent` methods, a game event on the game bus and a lifecycle event on the mod bus.
@EventBusSubscriber(modid = "demo")
public class AutoSubscribers {
    static {
        System.out.println("auto-clinit");
    }

    @SubscribeEvent(priority = EventPriority.HIGH)
    public static void tickHigh(ServerTickEvent.Post event) {
        System.out.println("auto-high");
    }

    @SubscribeEvent
    public static void setup(FMLCommonSetupEvent event) {
        System.out.println("auto-setup");
    }

    // Without `@SubscribeEvent`: never registered.
    public static void ignored(ServerTickEvent.Post event) {
        System.out.println("ignored");
    }
}
