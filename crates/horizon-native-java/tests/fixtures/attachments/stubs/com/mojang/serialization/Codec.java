package com.mojang.serialization;
import java.util.function.Function;
public interface Codec<A> {
  default <S> Codec<S> flatXmap(Function<? super A, ? extends DataResult<? extends S>> to, Function<? super S, ? extends DataResult<? extends A>> from) { return null; }
  default MapCodec<A> fieldOf(String name) { return null; }
}
