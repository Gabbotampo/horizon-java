//! NeoForge's `ModConfigSpec.Builder`, its values and the spec it builds, run in
//! the interpreter against values recorded from the real classes on a JVM.
//!
//! `PROBE` was compiled twice: here against signature-only stubs (only `demo/`
//! ships, so every NeoForge and commons-lang call reaches the runtime's model),
//! and for the oracle against `neoforge-26.3.0.101-universal.jar` with the
//! night-config, Guava, commons-lang3, log4j-api and FML jars it ships with,
//! run with `java` (OpenJDK 27). Both compilations name the same 98 NeoForge
//! and commons-lang call sites. The oracle's output is pasted verbatim into
//! `EXPECTED`; no expected value is derived from the runtime.
//!
//! The probe reads the enum default as `getDefault().name()`: `"" + value` of a
//! jar enum typed `Object` prints the interpreter's marker today, a gap in
//! `String.valueOf` that is not this model's.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};

const PROBE: &str = r##"package demo;

import java.util.List;
import java.util.function.Supplier;
import net.neoforged.neoforge.common.ModConfigSpec;
import org.apache.commons.lang3.tuple.Pair;

public final class Probe {
    public enum Mode { A, B }

    static int supplied = 0;

    static String failure(Throwable error) {
        return error.getClass().getSimpleName() + ":" + error.getMessage();
    }

    /** Declared defaults of every value kind, read without a loaded config. */
    public static String defaults() {
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();
        builder.comment("General").push("general");
        ModConfigSpec.BooleanValue flag = builder.comment("a flag").define("flag", true);
        ModConfigSpec.IntValue count = builder.defineInRange("count", 5, 0, 10);
        ModConfigSpec.DoubleValue ratio = builder.defineInRange("ratio", 0.5, 0.0, 1.0);
        ModConfigSpec.LongValue big = builder.defineInRange("big", 7L, 0L, 100L);
        ModConfigSpec.ConfigValue<String> name = builder.translation("demo.name").define("name", "steve");
        ModConfigSpec.EnumValue<Mode> mode = builder.defineEnum("mode", Mode.B);
        ModConfigSpec.ConfigValue<List<? extends String>> list =
            builder.defineListAllowEmpty("list", List.of("x", "y"), () -> "", o -> true);
        ModConfigSpec.ConfigValue<String> picked = builder.defineInList("picked", "b", List.of("a", "b"));
        ModConfigSpec.Builder back = builder.worldRestart().define("restart", false).next();
        builder.pop();
        ModConfigSpec spec = builder.build();
        return flag.getDefault() + "," + count.getDefault() + "," + ratio.getDefault() + ","
            + big.getDefault() + "," + name.getDefault() + "," + mode.getDefault().name() + ","
            + list.getDefault().size() + "," + picked.getDefault() + "," + spec.isLoaded() + ","
            + (back == builder);
    }

    /** `get()` before `build()`, after `build()`, and through the typed getters. */
    public static String reads() {
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();
        ModConfigSpec.BooleanValue flag = builder.define("flag", true);
        ModConfigSpec.IntValue count = builder.defineInRange("count", 2, 0, 4);
        StringBuilder out = new StringBuilder();
        try { out.append(flag.get()); } catch (RuntimeException e) { out.append(failure(e)); }
        builder.build();
        out.append('|');
        try { out.append(flag.get()); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { out.append(flag.getAsBoolean()); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { out.append(count.getAsInt()); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        ModConfigSpec.BooleanValue late = builder.define("late", false);
        try { out.append(late.get()); } catch (RuntimeException e) { out.append(failure(e)); }
        return out.toString();
    }

    /** Every throw the builder makes while a mod declares its spec. */
    public static String errors() {
        StringBuilder out = new StringBuilder();
        try { new ModConfigSpec.Builder().pop(); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { new ModConfigSpec.Builder().push("a.b").pop(3); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { new ModConfigSpec.Builder().defineInRange("r", 5, 10, 1); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { new ModConfigSpec.Builder().defineInRange("d", 0.0, 1.0, -1.0); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { new ModConfigSpec.Builder().define("n", (Object) null); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { new ModConfigSpec.Builder().comment((String) null); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { new ModConfigSpec.Builder().comment("a", null); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { new ModConfigSpec.Builder().comment("dangling").build(); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { new ModConfigSpec.Builder().translation("k").build(); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { new ModConfigSpec.Builder().worldRestart().push("x"); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { new ModConfigSpec.Builder().gameRestart().build(); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { new ModConfigSpec.Builder().define("s", (Supplier<Object>) null, o -> true); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { new ModConfigSpec.Builder().define("p", "v", null); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { new ModConfigSpec.Builder().defineList("l", List.of(), null); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        // A failed define leaves its comment pending: the next build sees it.
        ModConfigSpec.Builder kept = new ModConfigSpec.Builder();
        try { kept.comment("kept").define("s", (Supplier<Object>) null, o -> true); } catch (RuntimeException e) { out.append("first;"); }
        try { kept.build(); out.append("built"); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        // A push consumes the comment and translation; a define consumes them too.
        ModConfigSpec.Builder clean = new ModConfigSpec.Builder();
        clean.comment("level").translation("lvl").push("a").pop();
        clean.translation("t").comment("c").define("t", true);
        clean.build();
        out.append("clean");
        return out.toString();
    }

    /** A supplier default runs once at `defineInRange` (its range comment) and at each `getDefault`. */
    public static String supplierCalls() {
        supplied = 0;
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();
        ModConfigSpec.IntValue ranged = builder.defineInRange("s", () -> { supplied++; return 3; }, 0, 5);
        int afterRange = supplied;
        ModConfigSpec.BooleanValue flag = builder.define("b", () -> { supplied += 10; return true; });
        int afterFlag = supplied;
        Object value = ranged.getDefault();
        int afterDefault = supplied;
        Object flagValue = flag.getDefault();
        return afterRange + "," + afterFlag + "," + value + "," + afterDefault + "," + flagValue + "," + supplied;
    }

    static final class Holder {
        final ModConfigSpec.BooleanValue enabled;
        Holder(ModConfigSpec.Builder builder) { enabled = builder.define("enabled", false); }
    }

    /** `configure` runs the function on the builder, then builds. */
    public static String configure() {
        Pair<Holder, ModConfigSpec> pair = new ModConfigSpec.Builder().configure(Holder::new);
        String read;
        try { read = String.valueOf(pair.getLeft().enabled.get()); } catch (RuntimeException e) { read = failure(e); }
        return pair.getLeft().enabled.getDefault() + "," + (pair.getRight() != null) + ","
            + pair.getRight().isLoaded() + "," + read;
    }
}
"##;

const COUNTED: &str = r##"package demo;

import net.neoforged.neoforge.common.ModConfigSpec;

/** A path where the answer depends on state the runtime cannot see. */
public final class Counted {
    /** A spec handed to a loader call: `get()` and `isLoaded()` are counted. */
    public static String registered() {
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();
        ModConfigSpec.BooleanValue flag = builder.define("flag", true);
        ModConfigSpec spec = builder.build();
        org.example.Loader.register(spec);
        return flag.get() + "," + spec.isLoaded();
    }
}
"##;

const MOD_CONFIG_SPEC_STUB: &str = r##"package net.neoforged.neoforge.common;

import java.util.Collection;
import java.util.List;
import java.util.function.BooleanSupplier;
import java.util.function.Function;
import java.util.function.IntSupplier;
import java.util.function.Predicate;
import java.util.function.Supplier;
import org.apache.commons.lang3.tuple.Pair;

/** Compile-time signatures only; the runtime's model answers every call. */
public class ModConfigSpec {
    public boolean isLoaded() { throw new UnsupportedOperationException(); }

    public static class Builder {
        public Builder comment(String line) { throw new UnsupportedOperationException(); }
        public Builder comment(String... lines) { throw new UnsupportedOperationException(); }
        public Builder translation(String key) { throw new UnsupportedOperationException(); }
        public Builder worldRestart() { throw new UnsupportedOperationException(); }
        public Builder gameRestart() { throw new UnsupportedOperationException(); }
        public Builder push(String path) { throw new UnsupportedOperationException(); }
        public Builder pop() { throw new UnsupportedOperationException(); }
        public Builder pop(int count) { throw new UnsupportedOperationException(); }
        public BooleanValue define(String path, boolean value) { throw new UnsupportedOperationException(); }
        public BooleanValue define(String path, Supplier<Boolean> value) { throw new UnsupportedOperationException(); }
        public <T> ConfigValue<T> define(String path, T value) { throw new UnsupportedOperationException(); }
        public <T> ConfigValue<T> define(String path, T value, Predicate<Object> validator) { throw new UnsupportedOperationException(); }
        public <T> ConfigValue<T> define(String path, Supplier<T> value, Predicate<Object> validator) { throw new UnsupportedOperationException(); }
        public IntValue defineInRange(String path, int value, int min, int max) { throw new UnsupportedOperationException(); }
        public IntValue defineInRange(String path, Supplier<Integer> value, int min, int max) { throw new UnsupportedOperationException(); }
        public LongValue defineInRange(String path, long value, long min, long max) { throw new UnsupportedOperationException(); }
        public DoubleValue defineInRange(String path, double value, double min, double max) { throw new UnsupportedOperationException(); }
        public <V extends Enum<V>> EnumValue<V> defineEnum(String path, V value) { throw new UnsupportedOperationException(); }
        public <T> ConfigValue<List<? extends T>> defineList(String path, List<? extends T> value, Predicate<Object> validator) { throw new UnsupportedOperationException(); }
        public <T> ConfigValue<List<? extends T>> defineListAllowEmpty(String path, List<? extends T> value, Supplier<T> element, Predicate<Object> validator) { throw new UnsupportedOperationException(); }
        public <T> ConfigValue<T> defineInList(String path, T value, Collection<? extends T> acceptable) { throw new UnsupportedOperationException(); }
        public <T> Pair<T, ModConfigSpec> configure(Function<Builder, T> consumer) { throw new UnsupportedOperationException(); }
        public ModConfigSpec build() { throw new UnsupportedOperationException(); }
    }

    public static class ConfigValue<T> implements Supplier<T> {
        public T get() { throw new UnsupportedOperationException(); }
        public T getDefault() { throw new UnsupportedOperationException(); }
        public Builder next() { throw new UnsupportedOperationException(); }
    }

    public static class BooleanValue extends ConfigValue<Boolean> implements BooleanSupplier {
        public boolean getAsBoolean() { throw new UnsupportedOperationException(); }
    }

    public static class IntValue extends ConfigValue<Integer> implements IntSupplier {
        public int getAsInt() { throw new UnsupportedOperationException(); }
    }

    public static class LongValue extends ConfigValue<Long> {}

    public static class DoubleValue extends ConfigValue<Double> {}

    public static class EnumValue<T extends Enum<T>> extends ConfigValue<T> {}
}
"##;

const PAIR_STUB: &str = r##"package org.apache.commons.lang3.tuple;

public abstract class Pair<L, R> {
    public abstract L getLeft();
    public abstract R getRight();
}
"##;

const LOADER_STUB: &str = r##"package org.example; public final class Loader { public static void register(Object spec) { throw new UnsupportedOperationException(); } }
"##;

/// `java -cp <classes>:<NeoForge 26.3.0.101 class path> Oracle <method>...`,
/// one line per `Probe` method, verbatim.
const EXPECTED: &[(&str, &str)] = &[
    ("defaults", r#"true,5,0.5,7,steve,B,2,b,false,true"#),
    (
        "reads",
        r#"NullPointerException:Cannot get config value before spec is built|IllegalStateException:Cannot get config value before config is loaded.|IllegalStateException:Cannot get config value before config is loaded.|IllegalStateException:Cannot get config value before config is loaded.|NullPointerException:Cannot get config value before spec is built"#,
    ),
    (
        "errors",
        r#"IllegalArgumentException:Attempted to pop 1 elements when we only had: []|IllegalArgumentException:Attempted to pop 3 elements when we only had: [a, b]|IllegalArgumentException:Range min must be less then max.|IllegalArgumentException:Range min must be less then max.|NullPointerException:Default value can not be null|NullPointerException:Passed in null value for comment|NullPointerException:Comment string at 1 is null.|IllegalStateException:Non-empty comment when empty expected|IllegalStateException:Non-null translation key when null expected|IllegalStateException:Dangling restart value set to WORLD|IllegalStateException:Dangling restart value set to GAME|NullPointerException:Default supplier can not be null|NullPointerException:Validator can not be null|NullPointerException:ElementValidator can not be null|first;IllegalStateException:Non-empty comment when empty expected|clean"#,
    ),
    ("supplierCalls", r#"1,1,3,2,true,12"#),
    (
        "configure",
        r#"false,true,false,IllegalStateException:Cannot get config value before config is loaded."#,
    ),
];

fn fixture(test: &str) -> javac_fixture::Fixture {
    javac_fixture::Fixture::build(
        test,
        &[
            ("demo/Probe.java", PROBE),
            ("demo/Counted.java", COUNTED),
            (
                "net/neoforged/neoforge/common/ModConfigSpec.java",
                MOD_CONFIG_SPEC_STUB,
            ),
            ("org/apache/commons/lang3/tuple/Pair.java", PAIR_STUB),
            ("org/example/Loader.java", LOADER_STUB),
        ],
    )
}

/// Run one static `String demo.<class>.<method>()` and return its text and
/// the host's unknown calls.
fn run(
    fixture: &javac_fixture::Fixture,
    class: &str,
    method: &str,
) -> (String, Vec<(String, usize)>) {
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = match fixture
        .jar
        .invoke_method_with_args(class, method, "()Ljava/lang/String;", None, &[], &mut host)
        .unwrap_or_else(|error| panic!("{method} failed: {error}"))
    {
        Value::Ref(Some(text)) => text,
        other => panic!("{method} did not return a string: {other:?}"),
    };
    (text, host.unknown_host_calls())
}

/// Every oracle line, answered by the model with no unknown host call left.
#[test]
fn config_spec_matches_neoforge_on_a_jvm() {
    let fixture = fixture("config-spec");
    for (method, text) in EXPECTED {
        let (answer, unknown) = run(&fixture, "demo/Probe", method);
        assert_eq!(answer, *text, "Probe.{method}");
        assert!(unknown.is_empty(), "Probe.{method} counted {unknown:?}");
    }
}

/// A spec handed to a loader call may have been registered and loaded there,
/// which the runtime cannot see: `get()` and `isLoaded()` stay counted (and
/// answer the neutral value) rather than claim NeoForge's not-loaded throw.
#[test]
fn a_spec_given_to_the_loader_is_counted_not_answered() {
    let fixture = fixture("config-spec-registered");
    let (text, unknown) = run(&fixture, "demo/Counted", "registered");
    assert_eq!(text, "null,false");
    let names: Vec<&str> = unknown.iter().map(|(name, _)| name.as_str()).collect();
    for counted in [
        "org/example/Loader#register(Ljava/lang/Object;)V",
        "net/neoforged/neoforge/common/ModConfigSpec$BooleanValue#get()Ljava/lang/Object;",
        "net/neoforged/neoforge/common/ModConfigSpec#isLoaded()Z",
    ] {
        assert!(names.contains(&counted), "{counted} not in {names:?}");
    }
}
