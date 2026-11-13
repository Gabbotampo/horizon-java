package org.spongepowered.asm.mixin;

import java.lang.annotation.*;

// Compile-only stub of Mixin's annotation (class retention, as Mixin's own).
@Retention(RetentionPolicy.CLASS)
@Target(ElementType.TYPE)
public @interface Mixin {
    Class<?>[] value() default {};

    String[] targets() default {};
}
