package net.minecraft.nbt;
import com.mojang.serialization.Codec;
public class CompoundTag implements Tag {
  public static final Codec<CompoundTag> CODEC = null;
  public CompoundTag() {}
  public CompoundTag copy() { return this; }
  public boolean contains(String key) { return false; }
  public Tag get(String key) { return null; }
  public Tag put(String key, Tag tag) { return null; }
  public void putString(String key, String value) {}
  public void putBoolean(String key, boolean value) {}
  public void putInt(String key, int value) {}
  public void putLong(String key, long value) {}
  public String getStringOr(String key, String fallback) { return fallback; }
  public boolean getBooleanOr(String key, boolean fallback) { return fallback; }
  public int getIntOr(String key, int fallback) { return fallback; }
  public CompoundTag getCompoundOrEmpty(String key) { return this; }
}
