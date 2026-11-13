package net.neoforged.neoforge.registries;
import java.util.function.Supplier;
import net.minecraft.core.Registry;
import net.neoforged.bus.api.IEventBus;
public class DeferredRegister<T> {
  public static <T> DeferredRegister<T> create(Registry<T> registry, String namespace) { return null; }
  public <I extends T> DeferredHolder<T, I> register(String name, Supplier<? extends I> supplier) { return null; }
  public void register(IEventBus bus) {}
}
