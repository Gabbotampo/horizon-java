package net.minecraft.world.entity.player;
import java.util.Optional;
import java.util.function.Supplier;
import net.neoforged.neoforge.attachment.AttachmentType;
public abstract class Player {
  public <T> T getData(AttachmentType<T> type) { return null; }
  public <T> T getData(Supplier<AttachmentType<T>> type) { return null; }
  public <T> T setData(AttachmentType<T> type, T data) { return null; }
  public <T> T setData(Supplier<AttachmentType<T>> type, T data) { return null; }
  public boolean hasData(AttachmentType<?> type) { return false; }
  public <T> boolean hasData(Supplier<AttachmentType<T>> type) { return false; }
  public <T> Optional<T> getExistingData(Supplier<AttachmentType<T>> type) { return Optional.empty(); }
  public <T> T getExistingDataOrNull(Supplier<AttachmentType<T>> type) { return null; }
  public <T> T removeData(Supplier<AttachmentType<T>> type) { return null; }
  public void syncData(Supplier<? extends AttachmentType<?>> type) {}
}
