//! Three answers the runtime used to give silently, now counted or refused
//! ("refuse or count, never guess"):
//!
//! - `Object#getClass` on a marker no model names answered `null` uncounted;
//! - `instanceof` answered `false` uncounted for a host marker against a type
//!   no model knows (Bookshelf's `instanceof FMLModContainer` on NeoForge's
//!   mod container read that as a definite "no" and threw);
//! - `System.load`/`loadLibrary` answered `void` (and `Runtime#load` failed as
//!   an unnamed missing method).
//!
//! Where the runtime matches the JVM the expected text is OpenJDK 27's output
//! for the same `Probe` (`java -cp` on the compiled fixture with the stubs on
//! the class path), pasted verbatim. The counted and refused cases are the
//! runtime's contract, not JVM behaviour: a JVM knows the class, and loads the
//! library.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::{EntrypointSkip, NativeError, StdHost, Value};
use javac_fixture::Fixture;

/// Stand-ins for `javac` only: none of them ship, so every use reaches the
/// host as it does in a real mod.
const REGISTRIES_STUB: &str = r#"package net.minecraft.core.registries;
public final class BuiltInRegistries {
    public static final Object ITEM = new Object();
}
"#;
const BLOCK_STUB: &str = r#"package net.minecraft.world.level.block;
public class Block { }
"#;
const ENTITY_BLOCK_STUB: &str = r#"package net.minecraft.world.level.block;
public interface EntityBlock { }
"#;
const FML_STUB: &str = r#"package net.neoforged.fml.javafmlmod;
public class FMLModContainer { }
"#;

const PROBE: &str = r#"package demo;

import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.EntityBlock;
import net.neoforged.fml.javafmlmod.FMLModContainer;

public final class Probe {
    static final class Plain { }
    static final class MyBlock extends Block { }

    /// Known: a plain text's class.
    public static String textClass() {
        Object text = "abc";
        return text.getClass().getName();
    }

    /// Counted: a host object's class, which no model names.
    public static String hostClass() {
        Class<?> c = BuiltInRegistries.ITEM.getClass();
        return c == null ? "null" : "class";
    }

    /// Known "no"s: a string, a box, and a jar object whose chain ends at
    /// `Object` are never a loader or game type.
    public static String knownNo() {
        Object text = "abc";
        Object box = Integer.valueOf(3);
        Object plain = new Plain();
        return (text instanceof FMLModContainer) + "|" + (box instanceof FMLModContainer)
            + "|" + (plain instanceof EntityBlock) + "|" + (plain instanceof Block);
    }

    /// Unknown: a host object against a loader class no model knows, and a jar
    /// block (whose game superclass the runtime does not hold) against a game
    /// interface.
    public static String unknown() {
        Object container = BuiltInRegistries.ITEM;
        Object block = new MyBlock();
        return (container instanceof FMLModContainer) + "|" + (block instanceof EntityBlock);
    }

    public static String loadLibrary() {
        try {
            System.loadLibrary("velocity-compress");
            return "loaded";
        } catch (Throwable error) {
            return "caught " + error.getClass().getName();
        }
    }

    public static String runtimeLoad() {
        try {
            Runtime.getRuntime().load("/tmp/natives/velocity-cipher.so");
            return "loaded";
        } catch (Throwable error) {
            return "caught " + error.getClass().getName();
        }
    }
}
"#;

fn fixture(test: &str) -> Fixture {
    Fixture::build(
        test,
        &[
            ("demo/Probe.java", PROBE),
            (
                "net/minecraft/core/registries/BuiltInRegistries.java",
                REGISTRIES_STUB,
            ),
            ("net/minecraft/world/level/block/Block.java", BLOCK_STUB),
            (
                "net/minecraft/world/level/block/EntityBlock.java",
                ENTITY_BLOCK_STUB,
            ),
            (
                "net/neoforged/fml/javafmlmod/FMLModContainer.java",
                FML_STUB,
            ),
        ],
    )
}

fn counted(host: &StdHost, key: &str) -> usize {
    host.unknown_host_calls()
        .into_iter()
        .find(|(method, _)| method == key)
        .map_or(0, |(_, count)| count)
}

const GET_CLASS: &str = "java/lang/Object#getClass()Ljava/lang/Class;";

#[test]
fn get_class_names_text_and_counts_a_marker_it_cannot_name() {
    let fixture = fixture("get-class");
    let mut host = fixture.jar.seed_host(StdHost::new());
    // OpenJDK 27: `java.lang.String`.
    assert_eq!(fixture.text_on(&mut host, "textClass"), "java.lang.String");
    assert_eq!(counted(&host, GET_CLASS), 0);

    // The JVM prints `class`; the runtime has no class for the marker, so it
    // keeps the neutral null and counts the call instead of hiding it.
    assert_eq!(fixture.text_on(&mut host, "hostClass"), "null");
    assert_eq!(counted(&host, GET_CLASS), 1);
}

#[test]
fn instanceof_counts_only_the_answers_it_cannot_know() {
    let fixture = fixture("instanceof");
    let mut host = fixture.jar.seed_host(StdHost::new());
    // OpenJDK 27: `false|false|false|false`, and none of it is a guess.
    assert_eq!(
        fixture.text_on(&mut host, "knownNo"),
        "false|false|false|false"
    );
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{:?}",
        host.unknown_host_calls()
    );

    // The JVM prints `false|false` here too, but only because the stubs are
    // empty: a real `MyBlock` may be an `EntityBlock` through `Block`, and a
    // host object may be the loader's container. Both are counted.
    assert_eq!(fixture.text_on(&mut host, "unknown"), "false|false");
    assert_eq!(
        counted(
            &host,
            "net/neoforged/fml/javafmlmod/FMLModContainer#<instanceof>(Ljava/lang/Object;)Z"
        ),
        1
    );
    assert_eq!(
        counted(
            &host,
            "net/minecraft/world/level/block/EntityBlock#<instanceof>(Ljava/lang/Object;)Z"
        ),
        1
    );
}

fn refusal(fixture: &Fixture, method: &str) -> NativeError {
    let mut host = fixture.jar.seed_host(StdHost::new());
    fixture
        .jar
        .invoke_method_with_args(
            "demo/Probe",
            method,
            "()Ljava/lang/String;",
            None,
            &[],
            &mut host,
        )
        .map(|value: Value| panic!("{method} answered {value:?} instead of refusing"))
        .unwrap_err()
}

/// Not JVM behaviour: a JVM loads the library (or throws a catchable
/// `UnsatisfiedLinkError` the jar falls back from). The runtime refuses past
/// the jar's own `catch (Throwable)`, with the library named.
#[test]
fn loading_a_native_library_is_a_named_refusal_the_jar_cannot_catch() {
    let fixture = fixture("native-library");
    let error = refusal(&fixture, "loadLibrary");
    assert_eq!(
        error.to_string(),
        "loads a native library (java/lang/System#loadLibrary \"velocity-compress\"); \
         the native path cannot run native code"
    );
    assert_eq!(error.entrypoint_skip(), Some(EntrypointSkip::NativeLibrary));
    assert_eq!(EntrypointSkip::NativeLibrary.label(), "native-gap");

    let error = refusal(&fixture, "runtimeLoad");
    assert!(
        matches!(
            &error,
            NativeError::NativeLibrary { call, library }
                if call == "java/lang/Runtime#load" && library == "/tmp/natives/velocity-cipher.so"
        ),
        "{error:?}"
    );
}
