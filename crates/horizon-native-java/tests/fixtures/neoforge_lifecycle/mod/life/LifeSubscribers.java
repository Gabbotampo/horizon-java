package life;

import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent;
import net.neoforged.fml.event.lifecycle.FMLLoadCompleteEvent;

/// `life`'s `@EventBusSubscriber`: injected after the constructor, on the mod bus.
@EventBusSubscriber(modid = "life")
public class LifeSubscribers {
    @SubscribeEvent(priority = EventPriority.HIGH)
    public static void common(FMLCommonSetupEvent event) {
        System.out.println("subscriber-common-high");
    }

    @SubscribeEvent(priority = EventPriority.LOWEST)
    public static void complete(FMLLoadCompleteEvent event) {
        System.out.println("subscriber-complete-lowest");
    }
}
