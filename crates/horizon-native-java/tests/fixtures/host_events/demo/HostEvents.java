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
        + (invoker == a.invoker()) + (a.invoker() == b.invoker()) + invoker.equals(a) + "|"
        + (field == ServerLifecycleEvents.SERVER_STARTING) + "|" + identities(a, a, b, invoker, a.invoker());
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

  /** A cancellable event, cancelled, and one that is not. */
  public static String posted(Object cancelled, Object open) {
    return "" + (cancelled instanceof net.neoforged.bus.api.Event) + (cancelled instanceof ICancellableEvent)
        + ((ICancellableEvent) cancelled).isCanceled() + ((ICancellableEvent) open).isCanceled();
  }

  /** {@code ==} on two events. */
  public static String postedIdentity(Object a, Object b) {
    return "" + (a == b);
  }
}
