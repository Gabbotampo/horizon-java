package demo;

import com.mojang.serialization.Codec;
import com.mojang.serialization.DataResult;
import net.minecraft.nbt.CompoundTag;

/** carryon's `CarryOnData` shape: the whole state is one compound, encoded as a copy of it. */
public final class Data {
  public static final Codec<Data> CODEC = CompoundTag.CODEC.flatXmap(Data::decode, Data::encode);

  public final CompoundTag nbt;

  public Data(CompoundTag nbt) {
    this.nbt = nbt;
  }

  private static DataResult<Data> decode(CompoundTag tag) {
    try {
      return DataResult.success(new Data(tag));
    } catch (Exception e) {
      return DataResult.error(() -> "decode: " + e);
    }
  }

  private static DataResult<CompoundTag> encode(Data data) {
    try {
      return DataResult.success(data.nbt.copy());
    } catch (Exception e) {
      return DataResult.error(() -> "encode: " + e);
    }
  }
}
