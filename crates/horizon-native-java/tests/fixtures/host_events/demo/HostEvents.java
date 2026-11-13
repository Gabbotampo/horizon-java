package demo;

import java.util.*;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionStage;
import java.util.concurrent.Future;
import java.util.function.Supplier;
import net.fabricmc.fabric.api.event.Event;
import net.fabricmc.fabric.api.event.EventFactory;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;
import net.neoforged.bus.api.ICancellableEvent;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.InterModComms.IMCMessage;
import net.neoforged.neoforge.common.NeoForge;

/**
 * The P8 host objects (#156): event buses, Fabric events and their invokers, IMC messages and
 * suppliers, the future enqueueWork answers, and a posted cancellable event. Each probe takes the
 * objects as {@code Object}: the oracle passes the real ones, the runtime its handles.
 */
public final class HostEvents {
  /** The families' old marker spellings. */
  static final String[] OLD = {
    "neoforge-bus/game", "neoforge-bus/mod/demo", "forge-bus/demo/Tick", "fabric-event/0",
    "fabric-event/0/invoker", "neoforge-imc-message/0", "neoforge-imc-supplier/0",
    "neoforge-deferred-work/1", "event/cancelled/demo", "cancelled/demo"
  };

  /** The handles' own text forms. */
  static final String[] ENCODED = {
    "hnfbus:game", "hfgbus:demo/Tick", "hfevt:0", "hfinv:0", "himcm:0", "himcs:0", "hnfdw:1",
    "hpev:cancelled/demo"
  };

  static int identities(Object... objects) {
    Map<Object, Boolean> seen = new IdentityHashMap<>();
    for (Object o : objects) seen.put(o, true);
    return seen.size();
  }

  /** A string spelled like a handle's text form is a String, whole. */
  public static String encoded() {
    Object bus = NeoForge.EVENT_BUS;
    StringBuilder out = new StringBuilder();
    Map<Object, String> m = new HashMap<>();
    for (Object t : ENCODED) {
      out.append(t instanceof String).append(t.getClass().getName()).append(t.hashCode())
          .append(t.equals(bus)).append(bus.equals(t)).append(t instanceof IEventBus)
          .append(t instanceof Event).append(t instanceof IMCMessage).append(t instanceof CompletableFuture)
          .append(t instanceof ICancellableEvent).append(t).append(',');
      m.put(t, "s" + t.toString().length());
    }
    return out.append(m.get("hnfbus:game")).append(m.size()).toString();
  }

  /** A string spelled like an old marker equals only itself and prints as itself. */
  public static String spelled() {
    Object bus = NeoForge.EVENT_BUS;
    StringBuilder out = new StringBuilder();
    List<Object> list = new ArrayList<>();
    for (Object t : OLD) {
      out.append(t.equals(bus)).append(bus.equals(t)).append(t.equals(t.toString()))
          .append("" + t).append(((String) t).length()).append(',');
      list.add(t);
    }
    return out.append(list.indexOf("fabric-event/0")).append(list.contains("neoforge-bus/game"))
        .append(list.get(8)).toString();
  }

  /** ... and is none of the host objects' types. */
  public static String spelledTypes() {
    StringBuilder out = new StringBuilder();
    for (Object t : OLD) {
      out.append(t instanceof IEventBus).append(t instanceof Event).append(t instanceof IMCMessage)
          .append(t instanceof CompletableFuture).append(t instanceof ICancellableEvent).append(',');
    }
    return out.toString();
  }

  /** NeoForge's game bus (read twice) and a mod bus. */
  public static String bus(Object game, Object again, Object mod) {
    return "" + (game instanceof IEventBus) + (mod instanceof IEventBus) + (game instanceof Supplier)
        + game.getClass().getName() + "|" + (game == again) + (game == mod) + (NeoForge.EVENT_BUS == game)
        + (game != mod) + "|" + game.equals(again) + game.equals(mod) + game.equals("neoforge-bus/game")
        + "|" + identities(game, again, mod, NeoForge.EVENT_BUS);
  }

  /** NeoForge's {@code EventBus} is not Forge's bus interface. */
  public static String busForge(Object game) {
    return "" + (game instanceof net.minecraftforge.eventbus.api.IEventBus);
  }

  /** {@code EventBus} prints and hashes by identity. */
  public static String busText(Object game) {
    return game + "," + game.hashCode();
  }

  /** Fabric's array-backed events, their invokers, and an API field. */
  public static String fabric() {
    Event<Runnable> a = EventFactory.createArrayBacked(Runnable.class, all -> () -> {
      for (Runnable r : all) r.run();
    });
    Event<Runnable> b = EventFactory.createArrayBacked(Runnable.class, all -> () -> {
      for (Runnable r : all) r.run();
    });
    Object invoker = a.invoker();
    Object field = ServerLifecycleEvents.SERVER_STARTING;
    return "" + (a instanceof Event) + ((Object) a).getClass().getName() + (field instanceof Event)
        + "|" + (a == b) + a.equals(b) + a.equals(a) + a.equals("fabric-event/0") + "|"
        + (a.invoker() == b.invoker()) + invoker.equals(a) + "|"
        + (field == ServerLifecycleEvents.SERVER_STARTING) + "|" + identities(a, a, b);
  }

  /** {@code register} builds a new invoker: two reads may be two objects. */
  public static String fabricInvoker() {
    Event<Runnable> e = EventFactory.createArrayBacked(Runnable.class, all -> () -> {
      for (Runnable r : all) r.run();
    });
    Object a = e.invoker(), b = e.invoker();
    e.register((Runnable) () -> {});
    Object c = e.invoker();
    return "" + (a == b) + "|" + (a == c) + a.equals(c);
  }

  /** One invoker read, held twice: one object. */
  public static String fabricInvokerSame() {
    Event<Runnable> e = EventFactory.createArrayBacked(Runnable.class, all -> () -> {
      for (Runnable r : all) r.run();
    });
    Object a = e.invoker();
    Object b = a;
    return "" + (a == b) + a.equals(b) + identities(a, b);
  }

  /** IMC messages: one record read twice, another, and a supplier. */
  public static String imc(Object message, Object same, Object other, Object supplier) {
    return "" + (message instanceof IMCMessage) + (message instanceof Record) + (message instanceof Supplier)
        + message.getClass().getName() + (supplier instanceof Supplier) + "|" + (message == same)
        + (message == other) + message.equals(same) + message.equals("neoforge-imc-message/0")
        + "|" + identities(message, same, other, supplier);
  }

  /** A record's equals compares its components. */
  public static String imcEquals(Object message, Object other) {
    return "" + message.equals(other);
  }

  /** The future {@code enqueueWork} answers: one future twice, and another. */
  public static String future(Object future, Object same, Object other) {
    return "" + (future instanceof CompletableFuture) + (future instanceof Future)
        + (future instanceof CompletionStage) + (future instanceof Supplier) + future.getClass().getName()
        + "|" + (future == same) + (future == other) + future.equals(same) + future.equals(other)
        + "|" + identities(future, same, other);
  }

  /** A supplier another jar sent: its class is the sender's lambda, which the runtime does not know. */
  public static String supplierClass(Object supplier) {
    return "" + (supplier.getClass() != null);
  }

  /** ... but it is surely a {@code Supplier}, and never a {@code String} or {@code Integer} (final). */
  public static String supplierTypes(Object supplier) {
    return "" + (supplier instanceof Supplier) + (supplier instanceof String) + (supplier instanceof Integer);
  }

  /** Whether it is also a {@code Runnable} depends on its class. */
  public static String supplierRunnable(Object supplier) {
    return "" + (supplier instanceof Runnable);
  }

  /** A final class that is a {@code Supplier} through an interface of a jar that is not loaded. */
  static final class FinalViaMissing implements rvx.MissingIface {
    public Object get() {
      return null;
    }
  }

  /** A final jar class whose superclass is the JDK's. */
  static final class FinalList extends AbstractList<Object> {
    public Object get(int index) {
      return null;
    }

    public int size() {
      return 0;
    }
  }

  /** Neither final class can be ruled out from what the runtime holds (a supplier may be one). */
  public static String supplierFinalViaMissing(Object supplier) {
    return "" + (supplier instanceof FinalViaMissing);
  }

  public static String supplierFinalList(Object supplier) {
    return "" + (supplier instanceof FinalList);
  }

  /**
   * A jar object over an absent jar's interface may still be a platform type (that interface's
   * parents are unknown); one over a JDK superclass is not (no JDK class implements one).
   */
  public static String jarViaMissing() {
    Object o = new FinalViaMissing();
    return "" + (o instanceof net.minecraftforge.eventbus.api.IEventBus);
  }

  public static String jarViaJdk() {
    Object o = new FinalList();
    return "" + (o instanceof net.minecraftforge.eventbus.api.IEventBus);
  }

  /** A cancellable event, cancelled. */
  public static String posted(Object cancelled) {
    return "" + (cancelled instanceof net.neoforged.bus.api.Event) + (cancelled instanceof ICancellableEvent)
        + ((ICancellableEvent) cancelled).isCanceled();
  }

  /** An event that is not cancellable. */
  public static String postedOpen(Object open) {
    String which = switch (open) {
      case ICancellableEvent c -> "C";
      case net.neoforged.bus.api.Event e -> "E";
      default -> "d";
    };
    return "" + (open instanceof net.neoforged.bus.api.Event) + which;
  }

  /** {@code ==} on two events. */
  public static String postedIdentity(Object a, Object b) {
    return "" + (a == b);
  }
}
