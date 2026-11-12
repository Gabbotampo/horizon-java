package com.mojang.serialization;
import java.util.function.Supplier;
public interface DataResult<R> {
  static <R> DataResult<R> success(R result) { return null; }
  static <R> DataResult<R> error(Supplier<String> message) { return null; }
}
