package demo;

import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;
import net.fabricmc.fabric.impl.base.event.ArrayBackedEvent;
import org.spongepowered.asm.mixin.Mixin;

/**
 * A jar's accessor Mixins on Fabric's {@code ArrayBackedEvent}, as c2me ships one
 * ({@code @Mixin(ArrayBackedEvent.class) interface IArrayBackedEvent}, then {@code instanceof} and a
 * cast on an API event). Mixin merges the interface into the target's, so every Fabric event is one.
 * Not run on a plain JVM: no Mixin applies there.
 */
public final class HostEventsMixin {
  @Mixin(ArrayBackedEvent.class)
  interface HeldAcc {}

  @Mixin(targets = "net.fabricmc.fabric.impl.base.event.ArrayBackedEvent")
  interface NamedAcc {}

  /** The class-literal Mixin, which the runtime's Mixin scan reads. */
  public static String accessor() {
    Object e = ServerLifecycleEvents.SERVER_STARTING;
    return "" + (e instanceof HeldAcc) + (((HeldAcc) e) != null);
  }

  /** A Mixin named only by text, which the scan does not read. */
  public static String namedAccessor() {
    Object e = ServerLifecycleEvents.SERVER_STARTING;
    return "" + (e instanceof NamedAcc);
  }
}
