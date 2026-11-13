package net.neoforged.neoforge.attachment;
import com.mojang.serialization.MapCodec;
import java.util.function.Function;
import java.util.function.Predicate;
import java.util.function.Supplier;
public final class AttachmentType<T> {
  public static <T> Builder<T> builder(Supplier<T> defaultValue) { return null; }
  public static <T> Builder<T> builder(Function<IAttachmentHolder, T> defaultValue) { return null; }
  public static final class Builder<T> {
    public Builder<T> serialize(MapCodec<T> codec) { return this; }
    public Builder<T> serialize(MapCodec<T> codec, Predicate<? super T> shouldSerialize) { return this; }
    public Builder<T> copyOnDeath() { return this; }
    public Builder<T> copyHandler(IAttachmentCopyHandler<T> handler) { return this; }
    public Builder<T> sync(AttachmentSyncHandler<T> handler) { return this; }
    public AttachmentType<T> build() { return null; }
  }
}
