package demo;

import java.util.function.Consumer;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent;
import net.neoforged.neoforge.event.entity.living.LivingDeathEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

/// Registrations on NeoForge's buses, run by the runtime (`neoforge_bus_tests.rs`) and by the oracle
/// on the real bus-8.0.5 (`oracle/BusOracle.java`). Every listener prints its name when it runs.
public class BusDemo {
    private final String name;

    BusDemo(String name) {
        this.name = name;
    }

    public void onTick(ServerTickEvent.Post event) {
        System.out.println(name);
    }

    static void staticTick(ServerTickEvent.Post event) {
        System.out.println("static");
    }

    /// Every `addListener` overload, each priority, on one event.
    public static void order(IEventBus bus) {
        BusDemo bound = new BusDemo("bound");
        String captured = "captured";
        bus.addListener(BusDemo::staticTick);
        bus.addListener(EventPriority.LOW, bound::onTick);
        bus.addListener(EventPriority.HIGH, (ServerTickEvent.Post e) -> System.out.println(captured));
        bus.addListener(ServerTickEvent.Post.class, e -> System.out.println("typed"));
        bus.addListener(new TickConsumer("object"));
        bus.addListener(EventPriority.HIGHEST, false, ServerTickEvent.Post.class, e -> System.out.println("highest"));
        bus.addListener(true, (ServerTickEvent.Post e) -> System.out.println("receives"));
        bus.addListener(EventPriority.LOWEST, true, (ServerTickEvent.Post e) -> System.out.println("lowest"));
        bus.addListener(EventPriority.HIGH, ServerTickEvent.Post.class, e -> System.out.println("high-typed"));
        bus.addListener(false, ServerTickEvent.Post.class, e -> System.out.println("plain-typed"));
    }

    /// Cancellation on a cancellable event, with and without `receiveCanceled`.
    public static void cancel(IEventBus bus) {
        bus.addListener(EventPriority.HIGH, (LivingDeathEvent e) -> {
            System.out.println("cancels");
            e.setCanceled(true);
        });
        bus.addListener((LivingDeathEvent e) -> System.out.println("skipped"));
        bus.addListener(EventPriority.NORMAL, true, (LivingDeathEvent e) -> {
            System.out.println("uncancels");
            e.setCanceled(false);
        });
        bus.addListener(EventPriority.LOW, (LivingDeathEvent e) -> System.out.println("runs-again"));
        bus.addListener(EventPriority.LOWEST, (LivingDeathEvent e) -> {
            System.out.println("cancels-last");
            e.setCanceled(true);
        });
        bus.addListener(EventPriority.LOWEST, (LivingDeathEvent e) -> System.out.println("after-last"));
    }

    /// One listener `unregister` removes, one it keeps.
    public static void unregister(IEventBus bus) {
        Consumer<ServerTickEvent.Pre> gone = e -> System.out.println("gone");
        bus.addListener(gone);
        bus.addListener((ServerTickEvent.Pre e) -> System.out.println("kept"));
        bus.unregister(gone);
    }

    /// Accepted by NeoForge; nothing in the runtime fires these.
    public static void notFired(IEventBus modBus, IEventBus gameBus) {
        modBus.addListener((FMLCommonSetupEvent e) -> System.out.println("setup"));
        gameBus.addListener((DemoEvent e) -> System.out.println("demo"));
    }

    /// `register` between two `addListener`s: an object's instance methods, a class's static ones,
    /// and a second `register` of the same object, which NeoForge ignores.
    public static void subscribe(IEventBus bus) {
        bus.addListener((ServerTickEvent.Post e) -> System.out.println("lambda-first"));
        Subscribers object = new Subscribers("object");
        bus.register(object);
        bus.register(StaticSubscribers.class);
        bus.register(object);
        bus.addListener((ServerTickEvent.Post e) -> System.out.println("lambda-last"));
        bus.addListener(EventPriority.LOWEST, (LivingDeathEvent e) -> System.out.println("lambda-death"));
    }

    /// `unregister` of a registered object takes all of its methods.
    public static void unsubscribe(IEventBus bus) {
        Subscribers gone = new Subscribers("gone");
        bus.register(gone);
        bus.register(new Subscribers("kept"));
        bus.unregister(gone);
    }

    /// The `register` calls NeoForge refuses, and what it says.
    public static String refusedRegister(IEventBus bus, int which) {
        try {
            switch (which) {
                case 0 -> bus.register(Refused.OneInstance.class);
                case 1 -> bus.register(new Refused.OneStatic());
                case 2 -> bus.register(new Refused.Nothing());
                case 3 -> bus.register(new Refused.Child());
                case 4 -> bus.register(Refused.TwoArguments.class);
                case 5 -> bus.register(Refused.NotAnEvent.class);
                case 6 -> bus.register(Refused.ModBusOnly.class);
                case 7 -> bus.register(Refused.Abstract.class);
                default -> {}
            }
            return "accepted";
        } catch (IllegalArgumentException e) {
            return e.getClass().getName() + ": " + e.getMessage();
        }
    }

    /// The registrations NeoForge refuses, and what it says.
    public static String refused(IEventBus bus, int which) {
        try {
            switch (which) {
                case 0 -> bus.addListener((FMLCommonSetupEvent e) -> {});
                case 1 -> bus.addListener((ServerTickEvent.Post e) -> {});
                case 2 -> bus.addListener(ServerTickEvent.class, e -> {});
                case 3 -> bus.addListener((BadChild e) -> {});
                default -> {}
            }
            return "accepted";
        } catch (IllegalArgumentException e) {
            return e.getClass().getName() + ": " + e.getMessage();
        }
    }
}
