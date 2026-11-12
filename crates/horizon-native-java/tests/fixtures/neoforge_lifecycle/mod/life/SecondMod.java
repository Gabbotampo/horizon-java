package life;

import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent;
import net.neoforged.fml.event.lifecycle.InterModEnqueueEvent;

/// A second mod in the same jar: its setup work runs with the first mod's, after both events; a
/// listener of its that throws ends its own post.
@Mod("second")
public class SecondMod {
    public SecondMod(IEventBus modBus) {
        System.out.println("second-constructor");
        modBus.addListener((FMLCommonSetupEvent e) -> {
            System.out.println("second-common");
            e.enqueueWork(() -> System.out.println("second-work"));
        });
        modBus.addListener(EventPriority.HIGH, (InterModEnqueueEvent e) -> {
            System.out.println("second-enqueue-throws");
            throw new IllegalStateException("listener fails");
        });
        modBus.addListener((InterModEnqueueEvent e) -> System.out.println("second-enqueue-never"));
    }
}
