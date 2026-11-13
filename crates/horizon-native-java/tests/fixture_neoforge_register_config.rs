//! NeoForge's `ModContainer.registerConfig` (both overloads) over a spec the
//! `ModConfigSpec.Builder` model built, run in the interpreter against values
//! recorded from the real classes on a JVM.
//!
//! `PROBE` was compiled twice: here against signature-only stubs (only `demo/`
//! ships, so every NeoForge call reaches the runtime's model), and for the
//! oracle against `neoforge-26.3.0.101-universal.jar` and FancyModLoader
//! 12.0.12's `loader` jar with the night-config, Guava, commons-lang3, log4j,
//! slf4j, mojang-logging, maven-artifact and bus jars they ship with, run with
//! `java` (OpenJDK 27). Both compilations name the same 153 NeoForge call
//! sites in `Probe` and 21 in `Counted`. The oracle's driver called
//! `FMLPaths.loadAbsolutePaths(<empty dir>)` (a fresh install) and passed each
//! method a `ModContainer` subclass whose `IModInfo` answers the id `demo`;
//! its output is pasted verbatim into `EXPECTED`. No expected value is derived
//! from the runtime.
//!
//! The oracle ran every method in one JVM, so one `ConfigTracker` saw them all;
//! each method uses its own file names, so a fresh tracker per method (one
//! interpreter run each, as here) registers the same configs.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{StdHost, Value};
const PROBE: &str = r##"package demo;

import java.util.List;
import java.util.function.Supplier;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.config.ModConfig;
import net.neoforged.neoforge.common.ModConfigSpec;

public final class Probe {
    static final StringBuilder LOG = new StringBuilder();

    static String failure(Throwable error) {
        return error.getClass().getSimpleName() + ":" + error.getMessage();
    }

    static boolean logged(String name, boolean answer) {
        LOG.append(name).append(';');
        return answer;
    }

    static ModConfigSpec one(String path, boolean value) {
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();
        builder.define(path, value);
        return builder.build();
    }

    /** carryon's shape: a named LOCAL and a default-named CLIENT config, read in the constructor. */
    public static String registered(ModContainer container) {
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();
        ModConfigSpec.BooleanValue flag = builder.push("general").define("flag", true);
        ModConfigSpec.IntValue count = builder.defineInRange("count", 3, 0, 9);
        builder.pop();
        ModConfigSpec local = builder.build();
        container.registerConfig(ModConfig.Type.LOCAL, local, "reg-named.toml");
        ModConfigSpec.Builder other = new ModConfigSpec.Builder();
        ModConfigSpec.DoubleValue ratio = other.defineInRange("ratio", 0.25, 0.0, 1.0);
        ModConfigSpec client = other.build();
        container.registerConfig(ModConfig.Type.CLIENT, client);
        StringBuilder out = new StringBuilder();
        try { out.append(flag.get()); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { out.append(count.getAsInt()); } catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { out.append(ratio.get()); } catch (RuntimeException e) { out.append(failure(e)); }
        return out + "|" + local.isLoaded() + "," + client.isLoaded() + "," + count.getDefault();
    }

    /** An empty spec is logged and dropped before the tracker sees it, whatever its type. */
    public static String empty(ModContainer container) {
        ModConfigSpec nothing = new ModConfigSpec.Builder().build();
        StringBuilder out = new StringBuilder();
        ModConfig.Type[] types = {
            ModConfig.Type.STARTUP, ModConfig.Type.LOCAL, ModConfig.Type.SYNCED, ModConfig.Type.CLIENT};
        for (int i = 0; i < types.length; i++) {
            try {
                container.registerConfig(types[i], nothing, "empty.toml");
                container.registerConfig(types[i], nothing);
                out.append(i).append(',');
            } catch (RuntimeException e) { out.append(failure(e)).append(','); }
        }
        // Nothing was tracked: the name is still free.
        container.registerConfig(ModConfig.Type.LOCAL, one("e", true), "empty.toml");
        return out + "" + nothing.isLoaded();
    }

    /** The tracker keys configs by file name, and the two-argument form names it `<modid>-<type>.toml`. */
    public static String conflicts(ModContainer container) {
        StringBuilder out = new StringBuilder();
        container.registerConfig(ModConfig.Type.LOCAL, one("a", true), "clash.toml");
        try { container.registerConfig(ModConfig.Type.SYNCED, one("b", true), "clash.toml"); out.append("second"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        container.registerConfig(ModConfig.Type.SYNCED, one("c", false));
        try { container.registerConfig(ModConfig.Type.LOCAL, one("d", false), "demo-synced.toml"); out.append("named"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { container.registerConfig(ModConfig.Type.SYNCED, one("e", false)); out.append("again"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        // A conflict throws after validation: this one fails validation first.
        ModConfigSpec.Builder bad = new ModConfigSpec.Builder();
        bad.define("bad", 1, o -> false);
        try { container.registerConfig(ModConfig.Type.LOCAL, bad.build(), "clash.toml"); out.append("bad"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        return out.toString();
    }

    /** `validateSpec`: each value's restart type, then its validator on its own default. */
    public static String validation(ModContainer container) {
        StringBuilder out = new StringBuilder();
        ModConfigSpec.Builder game = new ModConfigSpec.Builder();
        game.push("a").gameRestart().define("b", true);
        game.pop();
        ModConfigSpec gameSpec = game.build();
        try { container.registerConfig(ModConfig.Type.SYNCED, gameSpec, "v-game-synced.toml"); out.append("synced"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        try { container.registerConfig(ModConfig.Type.LOCAL, gameSpec, "v-game-local.toml"); out.append("local"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        ModConfigSpec.Builder world = new ModConfigSpec.Builder();
        world.worldRestart().define("w", 1);
        try { container.registerConfig(ModConfig.Type.SYNCED, world.build(), "v-world.toml"); out.append("world"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        ModConfigSpec.Builder rejects = new ModConfigSpec.Builder();
        rejects.define("x", 5, o -> logged("x", false));
        try { container.registerConfig(ModConfig.Type.LOCAL, rejects.build(), "v-rejects.toml"); out.append("rejects"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        ModConfigSpec.Builder range = new ModConfigSpec.Builder();
        range.defineInRange("high", 50, 0, 10);
        try { container.registerConfig(ModConfig.Type.LOCAL, range.build(), "v-range.toml"); out.append("range"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        ModConfigSpec.Builder nan = new ModConfigSpec.Builder();
        nan.defineInRange("nan", Double.NaN, 0.0, 1.0);
        try { container.registerConfig(ModConfig.Type.LOCAL, nan.build(), "v-nan.toml"); out.append("nan"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        // `Range.test` compares `doubleValue()`s, so a long one past the maximum can round onto it.
        ModConfigSpec.Builder rounded = new ModConfigSpec.Builder();
        rounded.defineInRange("big", Long.MAX_VALUE, 0L, Long.MAX_VALUE - 1);
        try { container.registerConfig(ModConfig.Type.LOCAL, rounded.build(), "v-rounded.toml"); out.append("rounded"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        ModConfigSpec.Builder nullFlag = new ModConfigSpec.Builder();
        nullFlag.define("maybe", (Supplier<Boolean>) () -> null);
        try { container.registerConfig(ModConfig.Type.LOCAL, nullFlag.build(), "v-null.toml"); out.append("null"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        ModConfigSpec.Builder textFlag = new ModConfigSpec.Builder();
        textFlag.define("text", (Supplier) () -> "TRUE");
        try { container.registerConfig(ModConfig.Type.LOCAL, textFlag.build(), "v-text.toml"); out.append("text"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        // `defineList`'s non-empty size range is not part of its validator.
        ModConfigSpec.Builder emptyList = new ModConfigSpec.Builder();
        emptyList.defineList("none", List.of(), o -> true);
        try { container.registerConfig(ModConfig.Type.LOCAL, emptyList.build(), "v-empty-list.toml"); out.append("emptyList"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        out.append('|');
        LOG.setLength(0);
        ModConfigSpec.Builder elements = new ModConfigSpec.Builder();
        elements.defineList("items", List.of("p", "q", "r"), o -> logged(String.valueOf(o), !"q".equals(o)));
        try { container.registerConfig(ModConfig.Type.LOCAL, elements.build(), "v-elements.toml"); out.append("elements"); }
        catch (RuntimeException e) { out.append(failure(e)); }
        return out + "|" + LOG;
    }

    /** Values validate in the spec's tree order, last definition of a path winning, built values only. */
    public static String order(ModContainer container) {
        LOG.setLength(0);
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();
        builder.push("a").define("x", 1, o -> logged("ax", true));
        builder.pop().push("b").define("y", 1, o -> logged("by", true));
        builder.pop().push("a").define("z", 1, o -> logged("az", true));
        builder.pop();
        builder.define("dup", 1, o -> logged("dup1", true));
        builder.define("c", () -> { LOG.append("supply;"); return 2; }, o -> logged("c", true));
        builder.define("dup", 2, o -> logged("dup2", true));
        ModConfigSpec spec = builder.build();
        builder.define("late", 1, o -> logged("late", true));
        container.registerConfig(ModConfig.Type.LOCAL, spec, "order.toml");
        String first = LOG.toString();
        // An empty spec whose builder gains a value later is no longer empty, but has nothing to validate.
        LOG.setLength(0);
        ModConfigSpec.Builder grows = new ModConfigSpec.Builder();
        ModConfigSpec grown = grows.build();
        grows.define("after", 1, o -> logged("after", true));
        container.registerConfig(ModConfig.Type.LOCAL, grown, "grown.toml");
        String second;
        try { container.registerConfig(ModConfig.Type.SYNCED, one("g", true), "grown.toml"); second = "untracked"; }
        catch (RuntimeException e) { second = failure(e); }
        return first + "|" + LOG + "|" + second;
    }
}
"##;
const COUNTED: &str = r##"package demo;

import java.util.List;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.config.ModConfig;
import net.neoforged.neoforge.common.ModConfigSpec;

/** Registrations whose outcome the runtime cannot see: each stays counted. */
public final class Counted {
    /** A STARTUP config loads inside `registerConfig`: the load and the spec's reads are counted. */
    public static String startup(ModContainer container) {
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();
        ModConfigSpec.BooleanValue flag = builder.define("flag", true);
        ModConfigSpec spec = builder.build();
        container.registerConfig(ModConfig.Type.STARTUP, spec);
        return flag.get() + "," + spec.isLoaded();
    }

    /** `defineInList`'s validator is a `Collection.contains` the model does not answer. */
    public static String inList(ModContainer container) {
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();
        builder.defineInList("pick", "a", List.of("a", "b"));
        container.registerConfig(ModConfig.Type.LOCAL, builder.build());
        return "done";
    }

    /** A builder call the model leaves counted hides a value the spec would validate. */
    public static String unknownDefine(ModContainer container) {
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();
        builder.define("known", true);
        builder.defineInRange("generic", 1.5f, 0.0f, 2.0f, Float.class);
        container.registerConfig(ModConfig.Type.LOCAL, builder.build(), "unknown.toml");
        return "done";
    }

    /** A default read from a counted call (a neutral `0`) cannot convict the mod's range. */
    public static String afterUnknown(ModContainer container) {
        ModConfigSpec.Builder builder = new ModConfigSpec.Builder();
        builder.defineInRange("limit", org.example.Unknown.number(), 1, 10);
        container.registerConfig(ModConfig.Type.LOCAL, builder.build(), "after.toml");
        return "registered";
    }
}
"##;

/// A `@Mod` class FML constructs with its own container.
const ENTRY: &str = r##"package demo;

import net.neoforged.fml.ModContainer;
import net.neoforged.fml.config.ModConfig;

@net.neoforged.fml.common.Mod("demo")
public final class Entry {
    public Entry(ModContainer container) {
        StringBuilder out = new StringBuilder(container.getModId());
        container.registerConfig(ModConfig.Type.SYNCED, Probe.one("s", true));
        try {
            container.registerConfig(ModConfig.Type.LOCAL, Probe.one("t", true), "demo-synced.toml");
            out.append("|named");
        } catch (RuntimeException e) { out.append('|').append(Probe.failure(e)); }
        System.out.println(out.toString());
    }
}
"##;

const MOD_STUB: &str = r##"package net.neoforged.fml.common;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
@Retention(RetentionPolicy.RUNTIME)
public @interface Mod { String value(); }
"##;

const UNKNOWN_STUB: &str = r##"package org.example; public final class Unknown { public static int number() { throw new UnsupportedOperationException(); } }
"##;
const MOD_CONTAINER_STUB: &str = r##"package net.neoforged.fml;

import net.neoforged.fml.config.IConfigSpec;
import net.neoforged.fml.config.ModConfig;

/** Compile-time signatures only; the runtime's model answers every call. */
public abstract class ModContainer {
    public final String getModId() { throw new UnsupportedOperationException(); }
    public void registerConfig(ModConfig.Type type, IConfigSpec spec) { throw new UnsupportedOperationException(); }
    public void registerConfig(ModConfig.Type type, IConfigSpec spec, String fileName) { throw new UnsupportedOperationException(); }
}
"##;
const MOD_CONFIG_STUB: &str = r##"package net.neoforged.fml.config;

/** Compile-time signatures only: FML 12's four config types. */
public final class ModConfig {
    public enum Type { LOCAL, CLIENT, SYNCED, STARTUP }
}
"##;
const CONFIG_SPEC_STUB: &str = r##"package net.neoforged.fml.config;

public interface IConfigSpec {}
"##;
const MOD_CONFIG_SPEC_STUB: &str = r##"package net.neoforged.neoforge.common;

import java.util.Collection;
import java.util.List;
import java.util.function.Predicate;
import java.util.function.Supplier;
import net.neoforged.fml.config.IConfigSpec;

/** Compile-time signatures only; the runtime's model answers every call. */
public class ModConfigSpec implements IConfigSpec {
    public boolean isLoaded() { throw new UnsupportedOperationException(); }

    public static class Builder {
        public Builder worldRestart() { throw new UnsupportedOperationException(); }
        public Builder gameRestart() { throw new UnsupportedOperationException(); }
        public Builder push(String path) { throw new UnsupportedOperationException(); }
        public Builder pop() { throw new UnsupportedOperationException(); }
        public BooleanValue define(String path, boolean value) { throw new UnsupportedOperationException(); }
        public BooleanValue define(String path, Supplier<Boolean> value) { throw new UnsupportedOperationException(); }
        public <T> ConfigValue<T> define(String path, T value) { throw new UnsupportedOperationException(); }
        public <T> ConfigValue<T> define(String path, T value, Predicate<Object> validator) { throw new UnsupportedOperationException(); }
        public <T> ConfigValue<T> define(String path, Supplier<T> value, Predicate<Object> validator) { throw new UnsupportedOperationException(); }
        public IntValue defineInRange(String path, int value, int min, int max) { throw new UnsupportedOperationException(); }
        public LongValue defineInRange(String path, long value, long min, long max) { throw new UnsupportedOperationException(); }
        public DoubleValue defineInRange(String path, double value, double min, double max) { throw new UnsupportedOperationException(); }
        public <V extends Comparable<? super V>> ConfigValue<V> defineInRange(String path, V value, V min, V max, Class<V> clazz) { throw new UnsupportedOperationException(); }
        public <T> ConfigValue<List<? extends T>> defineList(String path, List<? extends T> value, Predicate<Object> validator) { throw new UnsupportedOperationException(); }
        public <T> ConfigValue<T> defineInList(String path, T value, Collection<? extends T> acceptable) { throw new UnsupportedOperationException(); }
        public ModConfigSpec build() { throw new UnsupportedOperationException(); }
    }

    public static class ConfigValue<T> implements Supplier<T> {
        public T get() { throw new UnsupportedOperationException(); }
        public T getDefault() { throw new UnsupportedOperationException(); }
    }

    public static class BooleanValue extends ConfigValue<Boolean> {}

    public static class IntValue extends ConfigValue<Integer> {
        public int getAsInt() { throw new UnsupportedOperationException(); }
    }

    public static class LongValue extends ConfigValue<Long> {}

    public static class DoubleValue extends ConfigValue<Double> {}
}
"##;
/// `java -cp <classes>:<NeoForge 26.3.0.101 class path> Oracle <empty game dir>`,
/// one line per `Probe` method, verbatim.
const EXPECTED: &[(&str, &str)] = &[
    (
        "registered",
        r#"IllegalStateException:Cannot get config value before config is loaded.|IllegalStateException:Cannot get config value before config is loaded.|IllegalStateException:Cannot get config value before config is loaded.|false,false,3"#,
    ),
    ("empty", r#"0,1,2,3,false"#),
    (
        "conflicts",
        r#"RuntimeException:Detected config file conflict on clash.toml from demo (already registered by demo)|RuntimeException:Detected config file conflict on demo-synced.toml from demo (already registered by demo)|RuntimeException:Detected config file conflict on demo-synced.toml from demo (already registered by demo)|IllegalArgumentException:Configuration value bad defined in config clash.toml has a validator that does not accept its own default value of 1"#,
    ),
    (
        "validation",
        r#"IllegalArgumentException:Configuration value a.b defined in config v-game-synced.toml has restart of type GAME which cannot be used for configs of type SYNCED|local|world|IllegalArgumentException:Configuration value x defined in config v-rejects.toml has a validator that does not accept its own default value of 5|IllegalArgumentException:Configuration value high defined in config v-range.toml has a validator that does not accept its own default value of 50|IllegalArgumentException:Configuration value nan defined in config v-nan.toml has a validator that does not accept its own default value of NaN|rounded|IllegalArgumentException:Configuration value maybe defined in config v-null.toml has a validator that does not accept its own default value of null|text|emptyList|IllegalArgumentException:Configuration value items defined in config v-elements.toml has a validator that does not accept its own default value of [p, q, r]|p;q;"#,
    ),
    (
        "order",
        r#"ax;az;by;dup2;supply;c;||RuntimeException:Detected config file conflict on grown.toml from demo (already registered by demo)"#,
    ),
];

/// The mod container the probe's methods receive: the loader's marker for
/// the mod `demo`.
const CONTAINER: &str = "neoforge-mod-container/demo/1.0.0";

fn fixture(test: &str) -> javac_fixture::Fixture {
    javac_fixture::Fixture::build(
        test,
        &[
            ("demo/Probe.java", PROBE),
            ("demo/Counted.java", COUNTED),
            ("org/example/Unknown.java", UNKNOWN_STUB),
            ("demo/Entry.java", ENTRY),
            ("net/neoforged/fml/common/Mod.java", MOD_STUB),
            ("net/neoforged/fml/ModContainer.java", MOD_CONTAINER_STUB),
            ("net/neoforged/fml/config/ModConfig.java", MOD_CONFIG_STUB),
            (
                "net/neoforged/fml/config/IConfigSpec.java",
                CONFIG_SPEC_STUB,
            ),
            (
                "net/neoforged/neoforge/common/ModConfigSpec.java",
                MOD_CONFIG_SPEC_STUB,
            ),
        ],
    )
}

/// Run one static `String demo.<class>.<method>(ModContainer)` and return its
/// text and the host's unknown calls.
fn run(
    fixture: &javac_fixture::Fixture,
    class: &str,
    method: &str,
) -> (String, Vec<(String, usize)>) {
    let mut host = fixture.jar.seed_host(StdHost::new());
    let text = match fixture
        .jar
        .invoke_method_with_args(
            class,
            method,
            "(Lnet/neoforged/fml/ModContainer;)Ljava/lang/String;",
            None,
            &[Value::Ref(Some(CONTAINER.into()))],
            &mut host,
        )
        .unwrap_or_else(|error| panic!("{method} failed: {error}"))
    {
        Value::Ref(Some(text)) => text,
        other => panic!("{method} did not return a string: {other:?}"),
    };
    (text, host.unknown_host_calls())
}

/// Every oracle line, answered by the model with no unknown host call left:
/// `LOCAL`/`CLIENT`/`SYNCED` registrations that leave the spec unloaded, empty
/// specs, file-name conflicts and every `validateSpec` outcome.
#[test]
fn register_config_matches_neoforge_on_a_jvm() {
    let fixture = fixture("register-config");
    for (method, text) in EXPECTED {
        let (answer, unknown) = run(&fixture, "demo/Probe", method);
        assert_eq!(answer, *text, "Probe.{method}");
        assert!(unknown.is_empty(), "Probe.{method} counted {unknown:?}");
    }
}

/// A registration whose outcome the runtime cannot see stays counted, and so
/// do the reads of a spec that may have loaded there.
#[test]
fn registrations_the_model_cannot_see_are_counted() {
    let fixture = fixture("register-config-counted");
    let register =
        |descriptor: &str| format!("net/neoforged/fml/ModContainer#registerConfig{descriptor}");
    let two = register(
        "(Lnet/neoforged/fml/config/ModConfig$Type;Lnet/neoforged/fml/config/IConfigSpec;)V",
    );
    let three = register(
        "(Lnet/neoforged/fml/config/ModConfig$Type;Lnet/neoforged/fml/config/IConfigSpec;Ljava/lang/String;)V",
    );
    // A STARTUP config loads inside `registerConfig` on NeoForge (a TOML round
    // trip and a `ModConfigEvent.Loading` post): the call, `get()` and
    // `isLoaded()` are counted and answer the neutral value.
    let (text, unknown) = run(&fixture, "demo/Counted", "startup");
    assert_eq!(text, "null,false");
    let names: Vec<&str> = unknown.iter().map(|(name, _)| name.as_str()).collect();
    for counted in [
        two.as_str(),
        "net/neoforged/neoforge/common/ModConfigSpec$BooleanValue#get()Ljava/lang/Object;",
        "net/neoforged/neoforge/common/ModConfigSpec#isLoaded()Z",
    ] {
        assert!(names.contains(&counted), "{counted} not in {names:?}");
    }
    let (_, unknown) = run(&fixture, "demo/Counted", "inList");
    assert!(
        unknown.iter().any(|(name, _)| *name == two),
        "defineInList: {unknown:?}"
    );
    let (_, unknown) = run(&fixture, "demo/Counted", "unknownDefine");
    assert!(
        unknown.iter().any(|(name, _)| *name == three),
        "counted define: {unknown:?}"
    );
    // Not NeoForge's behaviour, a named departure: NeoForge throws whenever a
    // validator refuses its default. The runtime throws that
    // `IllegalArgumentException` only in a clean run (`Probe.validation`'s
    // `high`); after a neutral answer the default may be that answer, so the
    // registration is counted instead.
    let (text, unknown) = run(&fixture, "demo/Counted", "afterUnknown");
    assert_eq!(text, "registered");
    let names: Vec<&str> = unknown.iter().map(|(name, _)| name.as_str()).collect();
    for counted in ["org/example/Unknown#number()I", three.as_str()] {
        assert!(names.contains(&counted), "{counted} not in {names:?}");
    }
}

/// FML constructs a `@Mod("demo")` class with the container of mod `demo`:
/// `getModId()` answers it, and the two-argument `registerConfig` names its
/// file `demo-synced.toml` (the oracle's `conflicts` line shows that name and
/// the conflict message for the same calls).
#[test]
fn the_mod_constructor_receives_its_own_container() {
    let fixture = fixture("register-config-entry");
    let mut host = fixture.jar.seed_host(StdHost::new());
    fixture
        .jar
        .execute("demo.Entry", None, "neoforge", &mut host)
        .unwrap_or_else(|error| panic!("Entry failed: {error}"));
    assert_eq!(
        host.take_stdout(),
        "demo|RuntimeException:Detected config file conflict on demo-synced.toml from demo (already registered by demo)\n"
    );
    assert!(
        host.unknown_host_calls().is_empty(),
        "{:?}",
        host.unknown_host_calls()
    );
}
