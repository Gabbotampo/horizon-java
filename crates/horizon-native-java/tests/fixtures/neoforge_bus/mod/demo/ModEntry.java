package demo;

import net.neoforged.bus.api.IEventBus;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.RegisterCommandsEvent;

/// The idiom `NeoForge.EVENT_BUS.register(this)` in a `@Mod` constructor.
@Mod("demo")
public class ModEntry {
    private final String name = "entry";

    public ModEntry(IEventBus modBus) {
        NeoForge.EVENT_BUS.register(this);
    }

    @SubscribeEvent
    public void commands(RegisterCommandsEvent event) {
        System.out.println(name + "-commands");
    }
}
