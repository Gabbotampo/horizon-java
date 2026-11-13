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
  /** A jar's own cancellable event, and a plain one. */
  static final class Cancellable extends Event implements ICancellableEvent {}

  static final class Plain extends Event {}

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
    Plain open = new Plain();
    print("encoded", HostEvents.encoded());
    print("spelled", HostEvents.spelled());
    print("spelledTypes", HostEvents.spelledTypes());
    print("bus", HostEvents.bus(game, game, mod));
    print("busForge", HostEvents.busForge(game));
    print("fabric", HostEvents.fabric());
    print("fabricNotAClass", HostEvents.fabricNotAClass());
    print("fabricOtherInterface", HostEvents.fabricOtherInterface());
    print("fabricInvoker", HostEvents.fabricInvoker());
    print("fabricInvokerSame", HostEvents.fabricInvokerSame());
    print("imc", HostEvents.imc(message, message, other, supplier));
    print("imcEquals", HostEvents.imcEquals(message, other));
    print("future", HostEvents.future(future, future, another));
    print("supplierClass", HostEvents.supplierClass(supplier));
    print("supplierTypes", HostEvents.supplierTypes(supplier));
    print("supplierRunnable", HostEvents.supplierRunnable(supplier));
    print("supplierAssignable", HostEvents.supplierAssignable(supplier));
    print("supplierSwitch", HostEvents.supplierSwitch(supplier));
    print("supplierFinalViaMissing", HostEvents.supplierFinalViaMissing(supplier));
    print("supplierFinalList", HostEvents.supplierFinalList(supplier));
    print("jarViaMissing", HostEvents.jarViaMissing());
    print("jarViaJdk", HostEvents.jarViaJdk());
    print("posted", HostEvents.posted(cancelled));
    print("postedOpen", HostEvents.postedOpen(open));
    print("postedIdentity", HostEvents.postedIdentity(cancelled, cancelled));
  }

  static void print(String name, String answer) {
    System.out.println(name + "=" + answer);
  }
}
