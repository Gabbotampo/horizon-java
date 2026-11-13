//! JDK 21's `SequencedCollection` members on lists (`sequenced.rs`), and a
//! `javafml` mod's `FMLModContainer` (`neoforge_loader.rs`).
//!
//! The list expectations are OpenJDK 27's output for the same `Probe`
//! (`java -cp`), pasted verbatim. The container expectations come from
//! FancyModLoader 12.0.12's bytecode (NeoForge 26.3.0.101): a mod whose
//! `neoforge.mods.toml` says `modLoader="javafml"` is built by
//! `FMLJavaModLanguageProvider` as `new FMLModContainer(...)`, whose
//! `getEventBus()` returns its bus field, and `minecraft` is a
//! `MinecraftModContainer`.

#[path = "support/javac_fixture.rs"]
mod javac_fixture;

use horizon_native_java::StdHost;
use javac_fixture::Fixture;

const PROBE: &str = r#"package demo;

import java.util.*;
import java.util.function.Supplier;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.ModList;
import net.neoforged.fml.javafmlmod.FMLModContainer;

public final class Probe {
    static String t(Supplier<Object> s) {
        try {
            return String.valueOf(s.get());
        } catch (RuntimeException e) {
            return e.getClass().getSimpleName() + (e.getMessage() == null ? "" : ":" + e.getMessage());
        }
    }

    static String ends(List<String> empty, List<String> ab) {
        return t(empty::getFirst) + " " + t(empty::getLast) + " " + t(ab::getFirst) + " " + t(ab::getLast)
            + " " + t(empty::removeFirst) + " " + t(ab::removeLast) + " " + t(() -> ab)
            + " " + t(() -> { ab.addFirst("z"); return ab; }) + " " + t(() -> { ab.addLast("y"); return ab; })
            + " " + t(ab::removeFirst) + " " + t(() -> ab);
    }

    public static String arrayList() {
        return ends(new ArrayList<>(), new ArrayList<>(List.of("a", "b")));
    }

    public static String linkedList() {
        return ends(new LinkedList<>(), new LinkedList<>(List.of("a", "b")));
    }

    public static String immutable() {
        return ends(List.of(), List.of("a", "b"));
    }

    public static String view() {
        return ends(Collections.unmodifiableList(new ArrayList<>()),
            Collections.unmodifiableList(new ArrayList<>(List.of("a", "b"))));
    }

    /// `reversed()`: a live view that writes through, and whose own
    /// `reversed()` is the list.
    public static String reversed() {
        ArrayList<String> base = new ArrayList<>(List.of("a", "b", "c"));
        List<String> r = base.reversed();
        String before = r + " " + r.getClass().getName() + " " + (r.reversed() == base) + " " + r.get(0)
            + " " + r.size() + " " + (r instanceof RandomAccess) + " " + r.getFirst() + " " + r.indexOf("a");
        r.add("z");
        String written = base + " " + r;
        base.add("d");
        List<String> ir = List.of("a", "b").reversed();
        LinkedList<String> ll = new LinkedList<>(List.of("a", "b"));
        return before + " | " + written + " " + r + " | " + ir + " " + t(() -> { ir.add("q"); return ir; })
            + " " + t(ir::removeFirst) + " | " + ll.reversed() + " " + ll.reversed().getClass().getName();
    }

    /// The queue members already modelled: `NoSuchElementException` has no
    /// message on any of them.
    public static String emptyQueues() {
        ArrayDeque<String> d = new ArrayDeque<>();
        return t(d::getFirst) + " " + t(d::removeFirst) + " " + t(d::pop) + " " + t(d::element) + " "
            + t(d::getLast) + " " + t(d::removeLast);
    }

    public static String containers() {
        ModContainer own = ModList.get().getModContainerById("probe-fixture").orElseThrow();
        ModContainer game = ModList.get().getModContainerById("minecraft").orElseThrow();
        String bus = own instanceof FMLModContainer fml ? (fml.getEventBus() == null ? "null" : "bus") : "not-fml";
        return (own instanceof FMLModContainer) + "|" + (own instanceof ModContainer) + "|"
            + (game instanceof FMLModContainer) + "|" + bus;
    }
}
"#;

/// Stand-ins for `javac` only; the fixture ships `demo/` alone, so the host
/// answers every loader call as it does for a real mod.
const LOADER_STUBS: &[(&str, &str)] = &[
    (
        "net/neoforged/fml/ModList.java",
        "package net.neoforged.fml; import java.util.Optional; public class ModList { public static ModList get() { return null; } public Optional<ModContainer> getModContainerById(String id) { return Optional.empty(); } }",
    ),
    (
        "net/neoforged/fml/ModContainer.java",
        "package net.neoforged.fml; import net.neoforged.bus.api.IEventBus; public abstract class ModContainer { public IEventBus getEventBus() { return null; } }",
    ),
    (
        "net/neoforged/fml/javafmlmod/FMLModContainer.java",
        "package net.neoforged.fml.javafmlmod; import net.neoforged.bus.api.IEventBus; public class FMLModContainer extends net.neoforged.fml.ModContainer { public IEventBus getEventBus() { return null; } }",
    ),
    (
        "net/neoforged/bus/api/IEventBus.java",
        "package net.neoforged.bus.api; public interface IEventBus {}",
    ),
];

const MODS_TOML: &[u8] = b"modLoader=\"javafml\" # the Java language loader\nloaderVersion=\"[4,)\"\nlicense=\"MIT\"\n[[mods]]\nmodId=\"probe-fixture\"\nversion=\"1.0\"\n";

fn fixture() -> Fixture {
    let mut sources = vec![("demo/Probe.java", PROBE)];
    sources.extend_from_slice(LOADER_STUBS);
    Fixture::build_with_lib(
        "sequenced-lists",
        &sources,
        &[("META-INF/neoforge.mods.toml", MODS_TOML)],
    )
}

fn text(fixture: &Fixture, method: &str) -> String {
    let mut host = fixture
        .jar
        .seed_host(StdHost::new().with_game_version("26.3"));
    let text = fixture.text_on(&mut host, method);
    assert_eq!(
        host.unknown_host_call_count(),
        0,
        "{method}: {:?}",
        host.unknown_host_calls()
    );
    text
}

#[test]
fn sequenced_members_answer_as_jdk_27_on_every_list_kind() {
    let fixture = fixture();
    // OpenJDK 27, line for line.
    assert_eq!(
        text(&fixture, "arrayList"),
        "NoSuchElementException NoSuchElementException a b NoSuchElementException b [a] [z, a] [z, a, y] z [a, y]"
    );
    assert_eq!(
        text(&fixture, "linkedList"),
        "NoSuchElementException NoSuchElementException a b NoSuchElementException b [a] [z, a] [z, a, y] z [a, y]"
    );
    assert_eq!(
        text(&fixture, "immutable"),
        "NoSuchElementException NoSuchElementException a b UnsupportedOperationException \
         UnsupportedOperationException [a, b] UnsupportedOperationException UnsupportedOperationException \
         UnsupportedOperationException [a, b]"
    );
    assert_eq!(
        text(&fixture, "view"),
        "NoSuchElementException NoSuchElementException a b NoSuchElementException \
         UnsupportedOperationException [a, b] UnsupportedOperationException UnsupportedOperationException \
         UnsupportedOperationException [a, b]"
    );
    assert_eq!(
        text(&fixture, "emptyQueues"),
        "NoSuchElementException NoSuchElementException NoSuchElementException NoSuchElementException \
         NoSuchElementException NoSuchElementException"
    );
}

#[test]
fn reversed_is_a_live_view_that_writes_through() {
    let fixture = fixture();
    assert_eq!(
        text(&fixture, "reversed"),
        "[c, b, a] java.util.ReverseOrderListView$Rand true c 3 true c 2 | [z, a, b, c] [c, b, a, z] \
         [d, c, b, a, z] | [b, a] UnsupportedOperationException UnsupportedOperationException | [b, a] \
         java.util.LinkedList$ReverseOrderLinkedListView"
    );
}

#[test]
fn a_javafml_mod_container_is_an_fml_container_with_its_bus() {
    let fixture = fixture();
    assert_eq!(text(&fixture, "containers"), "true|true|false|bus");
}
