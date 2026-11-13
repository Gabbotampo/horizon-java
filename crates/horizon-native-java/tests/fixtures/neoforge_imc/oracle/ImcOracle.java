import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.List;
import java.util.function.BiFunction;
import net.neoforged.bus.api.BusBuilder;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.DeferredWorkQueue;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.ModList;
import net.neoforged.fml.ModLoadingContext;
import net.neoforged.fml.event.IModBusEvent;
import net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent;
import net.neoforged.fml.event.lifecycle.FMLConstructModEvent;
import net.neoforged.fml.event.lifecycle.FMLDedicatedServerSetupEvent;
import net.neoforged.fml.event.lifecycle.FMLLoadCompleteEvent;
import net.neoforged.fml.event.lifecycle.InterModEnqueueEvent;
import net.neoforged.fml.event.lifecycle.InterModProcessEvent;
import net.neoforged.fml.event.lifecycle.ParallelDispatchEvent;
import net.neoforged.neoforge.network.event.RegisterPayloadHandlersEvent;
import net.neoforged.neoforgespi.language.IModInfo;

/// The fixture's sender and receiver jars through a dedicated server's loading steps on the real
/// loader-12.0.12 classes: the real `InterModComms`, `ModList` (`ModList.of`, then the package-private
/// `setLoadedMods` FML calls, with `minecraft`, `sender`, `receiver`), `ModLoadingContext`,
/// `DeferredWorkQueue` and lifecycle events. The steps are `CommonModLoader.begin`/`load`'s, written
/// from their bytecode as in `neoforge_lifecycle/oracle/LifecycleOracle.java`; around each
/// container's construction and post the active container is set and cleared as
/// `ModLoader.lambda$dispatchParallelTask$3` @33-55 does (`postEvent` sets none). Prints one JSON
/// line: every printed line, with `== <step>` before each step.
public class ImcOracle {
    static final List<String> lines = new ArrayList<>();

    static final class Container extends ModContainer {
        final IEventBus bus = BusBuilder.builder().markerType(IModBusEvent.class).allowPerPhasePost().build();

        Container(String id) {
            super(info(id));
        }

        @Override
        public IEventBus getEventBus() {
            return bus;
        }
    }

    static IModInfo info(String id) {
        return (IModInfo) Proxy.newProxyInstance(IModInfo.class.getClassLoader(), new Class<?>[] {IModInfo.class},
                (proxy, method, args) -> switch (method.getName()) {
                    case "getModId", "getNamespace" -> id;
                    case "toString" -> "IModInfo(" + id + ")";
                    case "hashCode" -> System.identityHashCode(proxy);
                    case "equals" -> proxy == args[0];
                    default -> null;
                });
    }

    static void capture(Runnable body) {
        PrintStream real = System.out;
        ByteArrayOutputStream buffer = new ByteArrayOutputStream();
        System.setOut(new PrintStream(buffer, true));
        try {
            body.run();
        } finally {
            System.setOut(real);
            for (String line : buffer.toString().split("\n")) {
                if (!line.isEmpty()) lines.add(line);
            }
        }
    }

    /// `dispatchParallelTask`'s handler: the container active while `body` runs.
    static void active(ModContainer container, Runnable body) {
        ModLoadingContext.get().setActiveContainer(container);
        try {
            body.run();
        } finally {
            ModLoadingContext.get().setActiveContainer(null);
        }
    }

    static void step(String name, List<Container> mods, BiFunction<ModContainer, DeferredWorkQueue, ParallelDispatchEvent> make) {
        lines.add("== " + name);
        DeferredWorkQueue queue = new DeferredWorkQueue(name);
        for (Container mod : mods) {
            capture(() -> active(mod, () -> mod.acceptEvent(make.apply(mod, queue))));
        }
        capture(queue::runTasks);
    }

    public static void main(String[] args) throws Exception {
        Container minecraft = new Container("minecraft");
        Container sender = new Container("sender");
        Container receiver = new Container("receiver");
        ModList modList = ModList.of(List.of(), List.of());
        Method setLoadedMods = ModList.class.getDeclaredMethod("setLoadedMods", List.class);
        setLoadedMods.setAccessible(true);
        setLoadedMods.invoke(modList, List.of(minecraft, sender, receiver));
        List<Container> mods = List.of(sender, receiver);
        lines.add("== Mod Construction");
        DeferredWorkQueue construction = new DeferredWorkQueue("Mod Construction");
        capture(() -> {
            active(sender, () -> {
                new imcsend.SendMod(sender.getEventBus());
                sender.acceptEvent(new FMLConstructModEvent(sender, construction));
            });
            active(receiver, () -> {
                new imcrecv.ReceiveMod(receiver.getEventBus());
                receiver.acceptEvent(new FMLConstructModEvent(receiver, construction));
            });
        });
        capture(construction::runTasks);
        step("Common setup", mods, FMLCommonSetupEvent::new);
        step("Sided setup", mods, FMLDedicatedServerSetupEvent::new);
        step("Enqueue IMC", mods, InterModEnqueueEvent::new);
        step("Process IMC", mods, InterModProcessEvent::new);
        step("Complete loading", mods, FMLLoadCompleteEvent::new);
        lines.add("== Network registry lock");
        RegisterPayloadHandlersEvent payloads = new RegisterPayloadHandlersEvent();
        capture(() -> {
            for (EventPriority phase : EventPriority.values()) {
                for (Container mod : mods) mod.acceptEvent(phase, payloads);
            }
        });
        List<String> quoted = new ArrayList<>();
        for (String line : lines) quoted.add("\"" + line + "\"");
        System.out.println("{\"case\":\"two-jars\",\"out\":[" + String.join(",", quoted) + "]}");
        System.out.println("{\"java\":\"" + System.getProperty("java.vm.name") + " " + System.getProperty("java.runtime.version") + "\"}");
    }
}
