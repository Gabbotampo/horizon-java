package life;

import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.event.lifecycle.FMLClientSetupEvent;
import net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent;
import net.neoforged.fml.event.lifecycle.FMLConstructModEvent;
import net.neoforged.fml.event.lifecycle.FMLDedicatedServerSetupEvent;
import net.neoforged.fml.event.lifecycle.FMLLoadCompleteEvent;
import net.neoforged.fml.event.lifecycle.InterModEnqueueEvent;
import net.neoforged.fml.event.lifecycle.InterModProcessEvent;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.RegisterCommandsEvent;
import net.neoforged.neoforge.network.event.RegisterPayloadHandlersEvent;

/// A mod that listens to every loading stage; each listener and each piece of deferred work prints
/// its name when it runs. Run by the runtime (`neoforge_lifecycle_tests.rs`) and by the oracle on
/// the real loader classes (`oracle/LifecycleOracle.java`).
@Mod("life")
public class LifeMod {
    public LifeMod(IEventBus modBus, ModContainer container) {
        System.out.println("constructor " + container.getModId());
        modBus.addListener(this::construct);
        modBus.addListener(EventPriority.LOW, (FMLCommonSetupEvent e) -> System.out.println("common-low"));
        modBus.addListener(this::common);
        modBus.addListener(EventPriority.HIGHEST, (FMLCommonSetupEvent e) -> System.out.println("common-highest"));
        modBus.addListener((FMLDedicatedServerSetupEvent e) -> System.out.println("server-setup"));
        modBus.addListener((FMLClientSetupEvent e) -> System.out.println("client-setup"));
        modBus.addListener((InterModEnqueueEvent e) -> System.out.println("imc-enqueue"));
        modBus.addListener(this::process);
        modBus.addListener(this::complete);
        modBus.addListener((RegisterPayloadHandlersEvent e) -> {
            System.out.println("payloads");
            e.registrar("1");
        });
    }

    private void construct(FMLConstructModEvent event) {
        System.out.println("construct " + event.getContainer().getModId());
        event.enqueueWork(() -> System.out.println("construct-work"));
    }

    private void common(FMLCommonSetupEvent event) {
        System.out.println("common");
        event.enqueueWork(() -> System.out.println("common-work-run"));
        event.enqueueWork(() -> {
            System.out.println("common-work-supply");
            return "supplied";
        });
        // A game-bus listener added during setup is on the game bus from then on.
        NeoForge.EVENT_BUS.addListener((RegisterCommandsEvent e) -> System.out.println("commands"));
        System.out.println("common-after-enqueue");
    }

    private void process(InterModProcessEvent event) {
        System.out.println("imc-process");
        event.enqueueWork(() -> {
            throw new IllegalStateException("work fails");
        });
        event.enqueueWork(() -> System.out.println("work-after-failure"));
    }

    private void complete(FMLLoadCompleteEvent event) {
        System.out.println("load-complete");
        event.enqueueWork(() -> System.out.println("load-complete-work"));
    }
}
