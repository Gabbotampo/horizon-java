import demo.HostEvents;
import java.util.function.Supplier;
import net.neoforged.bus.api.BusBuilder;
import net.neoforged.bus.api.Event;
import net.neoforged.bus.api.ICancellableEvent;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.DeferredWorkQueue;
import net.neoforged.fml.InterModComms;
import net.neoforged.fml.ModContainer;
import net.neoforged.neoforge.common.NeoForge;

/**
 * Runs every {@code HostEvents} probe on the real classes and prints {@code name=answer} lines: the
 * game bus is NeoForge 26.3.0.101's own, a mod bus is built as {@code FMLModContainer.<init>}
 * builds one, the futures come from loader-12.0.12's {@code DeferredWorkQueue#enqueueWork}, the
 * messages are loader-12.0.12's {@code IMCMessage} records.
 */
public final class HostEventsOracle {
  /** A jar's own cancellable event. */
  static final class Cancellable extends Event implements ICancellableEvent {}

  public static void main(String[] args) {
    IEventBus game = NeoForge.EVENT_BUS;
    IEventBus mod = BusBuilder.builder().allowPerPhasePost().build();
    Supplier<String> supplier = () -> "x";
    InterModComms.IMCMessage message = new InterModComms.IMCMessage("demo", "other", "m", supplier);
    InterModComms.IMCMessage other = new InterModComms.IMCMessage("demo", "other", "n", () -> "y");
    DeferredWorkQueue queue = new DeferredWorkQueue("p8");
    Object future = queue.enqueueWork((ModContainer) null, () -> {});
    Object another = queue.enqueueWork((ModContainer) null, () -> {});
    Cancellable cancelled = new Cancellable();
    cancelled.setCanceled(true);
    Cancellable open = new Cancellable();
    print("encoded", HostEvents.encoded());
    print("spelled", HostEvents.spelled());
    print("spelledTypes", HostEvents.spelledTypes());
    print("bus", HostEvents.bus(game, game, mod));
    print("fabric", HostEvents.fabric());
    print("imc", HostEvents.imc(message, message, other, supplier));
    print("imcEquals", HostEvents.imcEquals(message, other));
    print("future", HostEvents.future(future, future, another));
    print("posted", HostEvents.posted(cancelled, open));
    print("postedIdentity", HostEvents.postedIdentity(cancelled, cancelled));
  }

  static void print(String name, String answer) {
    System.out.println(name + "=" + answer);
  }
}
