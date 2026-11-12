import demo.BusDemo;
import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.util.ArrayList;
import java.util.List;
import net.neoforged.bus.api.BusBuilder;
import net.neoforged.bus.api.Event;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.event.IModBusEvent;
import net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.living.LivingDeathEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

/// Runs the fixture's registrations on the real bus-8.0.5 and prints what each post runs, one JSON
/// line per case. The game bus is NeoForge 26.3.0.101's own `NeoForge.EVENT_BUS`; a mod bus is built
/// the way `FMLModContainer.<init>` @31-62 builds it (its exception handler only logs).
/// See README.md for the classpath.
public class BusOracle {
    /// A fresh bus with `NeoForge.EVENT_BUS`'s own class checker (`NeoForge.lambda$static$0`, the
    /// real method, called reflectively): the real game bus is one static, and these cases need
    /// empty ones.
    static IEventBus gameBus() {
        try {
            java.lang.reflect.Method check = NeoForge.class.getDeclaredMethod("lambda$static$0", Class.class);
            check.setAccessible(true);
            return BusBuilder.builder().classChecker(type -> {
                try {
                    check.invoke(null, type);
                } catch (java.lang.reflect.InvocationTargetException e) {
                    throw (IllegalArgumentException) e.getCause();
                } catch (IllegalAccessException e) {
                    throw new AssertionError(e);
                }
            }).build();
        } catch (NoSuchMethodException e) {
            throw new AssertionError(e);
        }
    }

    static IEventBus modBus() {
        return BusBuilder.builder().markerType(IModBusEvent.class).allowPerPhasePost().build();
    }

    /// The lines the listeners print during one post, and whether the event ends cancelled.
    static String post(IEventBus bus, Event event) {
        PrintStream real = System.out;
        ByteArrayOutputStream buffer = new ByteArrayOutputStream();
        System.setOut(new PrintStream(buffer, true));
        try {
            bus.post(event);
        } finally {
            System.setOut(real);
        }
        List<String> lines = new ArrayList<>();
        for (String line : buffer.toString().split("\n")) {
            if (!line.isEmpty()) lines.add("\"" + line + "\"");
        }
        String canceled = event instanceof LivingDeathEvent death ? ",\"canceled\":" + death.isCanceled() : "";
        return "\"out\":[" + String.join(",", lines) + "]" + canceled;
    }

    static String quote(String text) {
        return "\"" + text.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n") + "\"";
    }

    /// FML's `AutomaticEventSubscriber.lambda$inject$4` (loader-12.0.12) for `container` on a
    /// dedicated server, written from its bytecode: the real `inject` needs `FMLLoader.getCurrent()`,
    /// which only a running loader has. The registrations themselves are the real bus's
    /// `register(Method)`.
    static void inject(String container, IEventBus modBus, IEventBus gameBus, Class<?>... classes) {
        for (Class<?> type : classes) {
            net.neoforged.fml.common.EventBusSubscriber annotation =
                    type.getAnnotation(net.neoforged.fml.common.EventBusSubscriber.class);
            net.neoforged.fml.common.Mod mod = type.getAnnotation(net.neoforged.fml.common.Mod.class);
            String modid = !annotation.modid().isEmpty() ? annotation.modid() : mod != null ? mod.value() : container;
            if (!modid.equals(container)) continue;
            if (!java.util.Arrays.asList(annotation.value()).contains(net.neoforged.api.distmarker.Dist.DEDICATED_SERVER)) continue;
            try {
                Class.forName(type.getName(), true, type.getClassLoader());
                for (java.lang.reflect.Method method : type.getDeclaredMethods()) {
                    if (!method.isAnnotationPresent(net.neoforged.bus.api.SubscribeEvent.class)) continue;
                    if (!java.lang.reflect.Modifier.isStatic(method.getModifiers())) {
                        throw new IllegalArgumentException("Method " + method + " annotated with @SubscribeEvent is not static");
                    }
                    if (method.getParameterCount() != 1 || !Event.class.isAssignableFrom(method.getParameterTypes()[0])) {
                        throw new IllegalArgumentException("Method " + method + " annotated with @SubscribeEvent must have only one parameter that is an Event subtype");
                    }
                    (IModBusEvent.class.isAssignableFrom(method.getParameterTypes()[0]) ? modBus : gameBus).register(method);
                }
            } catch (Exception e) {
                throw new RuntimeException(e);
            }
        }
    }

    /// The lines `run` prints.
    static String printed(Runnable run) {
        PrintStream real = System.out;
        ByteArrayOutputStream buffer = new ByteArrayOutputStream();
        System.setOut(new PrintStream(buffer, true));
        try {
            run.run();
        } finally {
            System.setOut(real);
        }
        List<String> lines = new ArrayList<>();
        for (String line : buffer.toString().split("\n")) {
            if (!line.isEmpty()) lines.add("\"" + line + "\"");
        }
        return "\"out\":[" + String.join(",", lines) + "]";
    }

    public static void main(String[] args) {
        IEventBus game = NeoForge.EVENT_BUS;
        game.start();
        BusDemo.order(game);
        System.out.println("{\"case\":\"order\"," + post(game, new ServerTickEvent.Post()) + "}");
        BusDemo.cancel(game);
        System.out.println("{\"case\":\"cancel\"," + post(game, new LivingDeathEvent()) + "}");
        // Two mods on one bus (what `BusDemo.cancel` registers, twice): one listener list,
        // priority first, then the mods' registration order.
        IEventBus shared = BusBuilder.builder().build();
        BusDemo.cancel(shared);
        BusDemo.cancel(shared);
        System.out.println("{\"case\":\"cancel_two_mods\"," + post(shared, new LivingDeathEvent()) + "}");
        BusDemo.unregister(game);
        System.out.println("{\"case\":\"unregister\"," + post(game, new ServerTickEvent.Pre()) + "}");
        IEventBus mod = modBus();
        BusDemo.notFired(mod, game);
        System.out.println("{\"case\":\"not_fired\"," + post(mod, new FMLCommonSetupEvent()) + "}");
        IEventBus subscribed = gameBus();
        BusDemo.subscribe(subscribed);
        System.out.println("{\"case\":\"subscribe_tick\"," + post(subscribed, new ServerTickEvent.Post()) + "}");
        System.out.println("{\"case\":\"subscribe_death\"," + post(subscribed, new LivingDeathEvent()) + "}");
        System.out.println("{\"case\":\"subscribe_pre\"," + post(subscribed, new ServerTickEvent.Pre()) + "}");
        IEventBus unsubscribed = BusBuilder.builder().build();
        BusDemo.unsubscribe(unsubscribed);
        System.out.println("{\"case\":\"unsubscribe\"," + post(unsubscribed, new ServerTickEvent.Post()) + "}");
        for (int which = 0; which < 8; which++) {
            System.out.println("{\"case\":\"refused_register_" + which + "\",\"bus\":\"game\",\"result\":"
                + quote(BusDemo.refusedRegister(gameBus(), which)) + "}");
        }
        new demo.ModEntry(modBus());
        System.out.println("{\"case\":\"mod_entry\"," + post(game, new net.neoforged.neoforge.event.RegisterCommandsEvent()) + "}");
        String[] buses = {"game", "mod", "game", "game"};
        for (int which = 0; which < 4; which++) {
            IEventBus bus = buses[which].equals("game") ? game : modBus();
            System.out.println("{\"case\":\"refused_" + which + "\",\"bus\":\"" + buses[which] + "\",\"result\":"
                + quote(BusDemo.refused(bus, which)) + "}");
        }
        IEventBus injectGame = gameBus();
        IEventBus injectMod = modBus();
        BusDemo.beforeInject(injectGame);
        System.out.println("{\"case\":\"inject_init\"," + printed(() -> inject("demo", injectMod, injectGame,
                demo.AutoSubscribers.class, demo.SkippedSubscribers.ClientOnly.class, demo.SkippedSubscribers.OtherMod.class)) + "}");
        System.out.println("{\"case\":\"inject_tick\"," + post(injectGame, new ServerTickEvent.Post()) + "}");
        System.out.println("{\"case\":\"inject_setup\"," + post(injectMod, new FMLCommonSetupEvent()) + "}");
        Class<?>[] refusedInject = {demo.InjectRefused.NotStatic.class, demo.InjectRefused.TwoArguments.class,
                demo.InjectRefused.NotAnEvent.class, demo.InjectRefused.AbstractEvent.class};
        for (int which = 0; which < refusedInject.length; which++) {
            String result;
            try {
                inject("refuse" + which, modBus(), gameBus(), refusedInject[which]);
                result = "registered";
            } catch (RuntimeException e) {
                result = e.getClass().getName() + ": " + e.getMessage();
            }
            System.out.println("{\"case\":\"inject_refused_" + which + "\",\"result\":" + quote(result) + "}");
        }
        System.out.println("{\"java\":" + quote(System.getProperty("java.vm.name") + " " + System.getProperty("java.runtime.version")) + "}");
    }
}
