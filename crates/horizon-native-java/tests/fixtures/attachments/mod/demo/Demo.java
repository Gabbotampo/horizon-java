package demo;

import com.mojang.serialization.Codec;
import com.mojang.serialization.DataResult;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import java.util.function.Supplier;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.world.entity.player.Player;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.attachment.AttachmentType;
import net.neoforged.neoforge.registries.DeferredRegister;
import net.neoforged.neoforge.registries.NeoForgeRegistries;

/** One attachment type per case the tests need, registered the way carryon registers its own. */
public final class Demo {
  public static final DeferredRegister<AttachmentType<?>> TYPES =
      DeferredRegister.create(NeoForgeRegistries.ATTACHMENT_TYPES, "demo");
  public static int supplierRuns;

  static final Codec<Data> FAILING = CompoundTag.CODEC.flatXmap(
      tag -> DataResult.success(new Data(tag)), data -> DataResult.error(() -> "boom"));
  static final Codec<Data> TOUCHING = CompoundTag.CODEC.flatXmap(
      tag -> DataResult.success(new Data(tag)),
      data -> {
        NbtUtils.prettyPrint(data.nbt);
        return DataResult.success(data.nbt.copy());
      });

  /** 0: carryon's type: synced, serialized, no copyOnDeath. */
  public static final Supplier<AttachmentType<Data>> DATA = TYPES.register("data", () ->
      AttachmentType.builder(() -> {
            supplierRuns++;
            return new Data(new CompoundTag());
          })
          .sync(new Sync())
          .serialize(Data.CODEC.fieldOf("CarryOnData"))
          .build());
  /** 1: kept on death. */
  public static final Supplier<AttachmentType<Data>> KEPT = TYPES.register("kept", () ->
      AttachmentType.builder(() -> new Data(new CompoundTag()))
          .serialize(Data.CODEC.fieldOf("Kept"))
          .copyOnDeath()
          .build());
  /** 2: no serializer. */
  public static final Supplier<AttachmentType<Data>> TRANSIENT = TYPES.register("transient", () ->
      AttachmentType.builder(() -> new Data(new CompoundTag())).build());
  /** 3: a serializer outside the modelled codec nodes. */
  public static final Supplier<AttachmentType<Data>> OPAQUE = TYPES.register("opaque", () ->
      AttachmentType.builder(() -> new Data(new CompoundTag()))
          .serialize(RecordCodecBuilder.<Data>mapCodec(instance -> instance))
          .build());
  /** 4: an encoder that answers DataResult.error. */
  public static final Supplier<AttachmentType<Data>> FAILS = TYPES.register("fails", () ->
      AttachmentType.builder(() -> new Data(new CompoundTag()))
          .serialize(FAILING.fieldOf("F"))
          .build());
  /** 5: an encoder that makes an unmodelled call. */
  public static final Supplier<AttachmentType<Data>> TOUCHES = TYPES.register("touches", () ->
      AttachmentType.builder(() -> new Data(new CompoundTag()))
          .serialize(TOUCHING.fieldOf("T"))
          .build());
  /** 6: a default supplier that makes an unmodelled call. */
  public static final Supplier<AttachmentType<Data>> UNKNOWN_DEFAULT = TYPES.register("unknown_default", () ->
      AttachmentType.builder(() -> {
            NbtUtils.prettyPrint(null);
            return new Data(new CompoundTag());
          })
          .serialize(Data.CODEC.fieldOf("U"))
          .build());
  /** 7: serialized only while its compound holds "keep". */
  public static final Supplier<AttachmentType<Data>> SKIPPED = TYPES.register("skipped", () ->
      AttachmentType.builder(() -> new Data(new CompoundTag()))
          .serialize(Data.CODEC.fieldOf("S"), data -> data.nbt.contains("keep"))
          .build());
  /** 8: a non-terminating encoder (bounded by the jar's execution budget). */
  public static final Supplier<AttachmentType<Data>> SPINS = TYPES.register("spins", () ->
      AttachmentType.builder(() -> new Data(new CompoundTag()))
          .serialize(CompoundTag.CODEC.<Data>flatXmap(
                  tag -> DataResult.success(new Data(tag)),
                  data -> {
                    while (data != null) {
                      supplierRuns += 0;
                    }
                    return DataResult.success(new CompoundTag());
                  })
              .fieldOf("Spin"))
          .build());

  private Demo() {}

  public static void init(IEventBus bus) {
    TYPES.register(bus);
  }

  @SuppressWarnings({"unchecked", "rawtypes"})
  static Supplier<AttachmentType<Object>> pick(int which) {
    Supplier[] all = {DATA, KEPT, TRANSIENT, OPAQUE, FAILS, TOUCHES, UNKNOWN_DEFAULT, SKIPPED, SPINS};
    return (Supplier<AttachmentType<Object>>) all[which];
  }

  public static Object get(Player player, int which) {
    return player.getData(pick(which));
  }

  public static Object getByType(Player player) {
    return player.getData(DATA.get());
  }

  public static Object set(Player player, int which, Object value) {
    return player.setData(pick(which), value);
  }

  public static boolean has(Player player, int which) {
    return player.hasData(pick(which));
  }

  public static Object existing(Player player, int which) {
    return player.getExistingData(pick(which)).orElse(null);
  }

  public static Object existingOrNull(Player player, int which) {
    return player.getExistingDataOrNull(pick(which));
  }

  public static Object remove(Player player, int which) {
    return player.removeData(pick(which));
  }

  public static void sync(Player player, int which) {
    player.syncData(pick(which));
  }

  public static Object unbound() {
    return DATA.get();
  }

  public static Object fresh(int selected) {
    CompoundTag tag = new CompoundTag();
    tag.putInt("selected", selected);
    return new Data(tag);
  }

  public static void select(Object data, int selected) {
    ((Data) data).nbt.putInt("selected", selected);
  }

  public static void keep(Object data) {
    ((Data) data).nbt.putBoolean("keep", true);
  }

  public static int selected(Object data) {
    return ((Data) data).nbt.getIntOr("selected", -1);
  }

  public static Object wrap(CompoundTag tag) {
    return new Data(tag);
  }

  public static CompoundTag nbtOf(Object data) {
    return ((Data) data).nbt;
  }

  public static void touch() {
    NbtUtils.prettyPrint(null);
  }

  public static String builderCheck(int which) {
    try {
      if (which == 0) {
        AttachmentType.builder(() -> new Data(new CompoundTag())).copyOnDeath();
      } else if (which == 1) {
        AttachmentType.builder(() -> new Data(new CompoundTag()))
            .serialize(Data.CODEC.fieldOf("A"))
            .serialize(Data.CODEC.fieldOf("B"));
      } else if (which == 2) {
        AttachmentType.builder(() -> new Data(new CompoundTag())).copyHandler(value -> value);
      } else {
        AttachmentType.builder(() -> new Data(new CompoundTag())).serialize(Data.CODEC.fieldOf("A")).copyOnDeath().build();
      }
      return null;
    } catch (IllegalStateException e) {
      return e.getMessage();
    }
  }

  public static String nullSet(Player player) {
    try {
      player.setData(pick(0), null);
      return "returned";
    } catch (NullPointerException e) {
      return "npe";
    }
  }

  // The CompoundTag reads, on a compound the test builds.
  public static boolean booleanOr(CompoundTag tag, String key) {
    return tag.getBooleanOr(key, true);
  }

  public static int intOr(CompoundTag tag, String key) {
    return tag.getIntOr(key, -9);
  }

  public static String stringOr(CompoundTag tag, String key) {
    return tag.getStringOr(key, "dflt");
  }

  public static boolean contains(CompoundTag tag, String key) {
    return tag.contains(key);
  }

  /** The oracle's `compound/copy` steps. Answers {copy, missing}; `tag` is left as vanilla leaves it. */
  public static Object[] copyCase(CompoundTag tag) {
    CompoundTag copy = tag.copy();
    copy.getCompoundOrEmpty("inner").putInt("added", 1);
    copy.putInt("i", 5);
    CompoundTag missing = tag.getCompoundOrEmpty("i");
    missing.putInt("x", 1);
    return new Object[] {copy, missing, tag.getCompoundOrEmpty("inner") == tag.getCompoundOrEmpty("inner")};
  }

  /** The oracle's `compound/put` steps. */
  public static Object[] putCase() {
    CompoundTag tag = new CompoundTag();
    CompoundTag one = new CompoundTag();
    Object first = tag.put("k", one);
    Object second = tag.put("k", new CompoundTag());
    String nullPut;
    try {
      tag.put("n", null);
      nullPut = null;
    } catch (IllegalArgumentException e) {
      nullPut = e.getMessage();
    }
    String nullString;
    try {
      tag.putString("ns", null);
      nullString = null;
    } catch (NullPointerException e) {
      nullString = "npe";
    }
    return new Object[] {first, second == one, nullPut, nullString, tag};
  }
}
