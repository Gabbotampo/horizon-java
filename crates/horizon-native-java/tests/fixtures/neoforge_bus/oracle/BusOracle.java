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
        String[] buses = {"game", "mod", "game", "game"};
        for (int which = 0; which < 4; which++) {
            IEventBus bus = buses[which].equals("game") ? game : modBus();
            System.out.println("{\"case\":\"refused_" + which + "\",\"bus\":\"" + buses[which] + "\",\"result\":"
                + quote(BusDemo.refused(bus, which)) + "}");
        }
        System.out.println("{\"java\":" + quote(System.getProperty("java.vm.name") + " " + System.getProperty("java.runtime.version")) + "}");
    }
}
