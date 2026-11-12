import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.List;
import java.util.function.BiFunction;
import net.neoforged.bus.api.BusBuilder;
import net.neoforged.bus.api.Event;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.DeferredWorkQueue;
import net.neoforged.fml.ModContainer;
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

/// The fixture's two mods through a dedicated server's loading steps, on the real loader-12.0.12
/// and NeoForge 26.3.0.101 classes: real `ModContainer#acceptEvent`, real lifecycle events, real
/// `DeferredWorkQueue`, a mod bus built as `FMLModContainer.<init>` @31-62 builds one. The sequence
/// of steps is `CommonModLoader.begin`/`load`'s, written from their bytecode (README.md): the steps
/// themselves need a running `FMLLoader`. Prints one JSON line: everything the mods printed, with
/// `!post-failed <mod>` / `!work-failed` where a post or a step's deferred work threw.
public class LifecycleOracle {
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

    /// Runs `body` and keeps what it printed.
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

    /// FML's `AutomaticEventSubscriber.lambda$inject$4` for a class of `container` on a dedicated
    /// server (see `neoforge_bus/oracle/BusOracle.java`); `register(Method)` is the real bus's.
    static void inject(Container container, Class<?> type) throws Exception {
        Class.forName(type.getName(), true, type.getClassLoader());
        for (Method method : type.getDeclaredMethods()) {
            if (!method.isAnnotationPresent(SubscribeEvent.class)) continue;
            if (!Modifier.isStatic(method.getModifiers())) throw new IllegalArgumentException("not static");
            Class<?> event = method.getParameterTypes()[0];
            if (!IModBusEvent.class.isAssignableFrom(event)) throw new AssertionError("only mod-bus subscribers here");
            container.getEventBus().register(method);
        }
    }

    /// `ModLoader.dispatchParallelEvent`: each container's event, then the step's deferred work.
    static void step(String name, List<Container> mods, BiFunction<ModContainer, DeferredWorkQueue, ParallelDispatchEvent> make) {
        DeferredWorkQueue queue = new DeferredWorkQueue(name);
        for (Container mod : mods) {
            capture(() -> {
                try {
                    mod.acceptEvent(make.apply(mod, queue));
                } catch (Throwable failed) {
                    System.out.println("!post-failed " + mod.getModId());
                }
            });
        }
        capture(() -> {
            try {
                queue.runTasks();
            } catch (Throwable failed) {
                System.out.println("!work-failed");
            }
        });
    }

    public static void main(String[] args) throws Exception {
        Container life = new Container("life");
        Container second = new Container("second");
        List<Container> mods = List.of(life, second);
        // Mod Construction: per container, the `@Mod` constructor, `inject`, `FMLConstructModEvent`
        // (`FMLModContainer#constructMod`, `ModLoader.lambda$constructMods$0`); then the queue.
        // FML constructs the containers in parallel, so the two containers' steps may interleave
        // any way; this is the interleaving the runtime takes (every constructor, then each
        // container's injection and event).
        DeferredWorkQueue construction = new DeferredWorkQueue("Mod Construction");
        capture(() -> {
            new life.LifeMod(life.getEventBus(), life);
            new life.SecondMod(second.getEventBus());
            try {
                inject(life, life.LifeSubscribers.class);
            } catch (Exception e) {
                throw new RuntimeException(e);
            }
            life.acceptEvent(new FMLConstructModEvent(life, construction));
            second.acceptEvent(new FMLConstructModEvent(second, construction));
        });
        capture(construction::runTasks);
        step("Common setup", mods, FMLCommonSetupEvent::new);
        step("Sided setup", mods, FMLDedicatedServerSetupEvent::new);
        step("Enqueue IMC", mods, InterModEnqueueEvent::new);
        step("Process IMC", mods, InterModProcessEvent::new);
        step("Complete loading", mods, FMLLoadCompleteEvent::new);
        // Network registry lock: `ModLoader.postEvent`, phase by phase through the mods.
        RegisterPayloadHandlersEvent payloads = new RegisterPayloadHandlersEvent();
        capture(() -> {
            for (EventPriority phase : EventPriority.values()) {
                for (Container mod : mods) mod.acceptEvent(phase, payloads);
            }
        });
        List<String> quoted = new ArrayList<>();
        for (String line : lines) quoted.add("\"" + line + "\"");
        System.out.println("{\"case\":\"server\",\"out\":[" + String.join(",", quoted) + "]}");
        System.out.println("{\"java\":\"" + System.getProperty("java.vm.name") + " " + System.getProperty("java.runtime.version") + "\"}");
    }
}
