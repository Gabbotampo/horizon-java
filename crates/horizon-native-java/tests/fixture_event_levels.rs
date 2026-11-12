//! `MinecraftServer.getAllLevels()` answers the levels the firings announced.
//!
//! The game's hook payloads carry no server level map, so a jar that only
//! listens for `server_started` and walks `getAllLevels()` (Chunky's
//! `Server#getWorlds`) would see an empty iterable and throw on its next
//! `get(0)`. The runtime records the level every firing names — the
//! level-load/unload events say when one appears and goes away — and
//! `getAllLevels` answers that set. The expected value here is what a vanilla
//! server would hand a listener at `server_started`: the levels it loaded, so
//! one overworld is `1`, and after `LevelUnload` it is `0`.

use horizon_native_java::{
    ExecutionBudget, NativeJar, StdHost,
    events::{EventContext, EventFacade, EventFiring, GameEvent},
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};
use zip::write::SimpleFileOptions;

#[path = "support/unique_dir.rs"]
mod unique_dir;
use unique_dir::unique_suffix;

const ENTRY: &str = r##"package demo;

import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;

public class Entry implements net.fabricmc.api.ModInitializer {
    @Override
    public void onInitialize() {
        ServerLifecycleEvents.SERVER_STARTED.register(server -> {
            int levels = 0;
            for (Object ignored : server.getAllLevels()) {
                levels++;
            }
            System.out.println("levels=" + levels);
        });
    }
}
"##;

/// The Fabric API and game classes the fixture compiles against, left out of
/// the jar so the host answers their statics.
const STUBS: &[(&str, &str)] = &[
    (
        "net/fabricmc/api/ModInitializer.java",
        "package net.fabricmc.api;\npublic interface ModInitializer { void onInitialize(); }\n",
    ),
    (
        "net/fabricmc/fabric/api/event/Event.java",
        r#"package net.fabricmc.fabric.api.event;

public interface Event<T> {
    void register(T listener);
}
"#,
    ),
    (
        "net/fabricmc/fabric/api/event/lifecycle/v1/ServerLifecycleEvents.java",
        r#"package net.fabricmc.fabric.api.event.lifecycle.v1;

import net.fabricmc.fabric.api.event.Event;

public final class ServerLifecycleEvents {
    private ServerLifecycleEvents() {}

    public static final Event<ServerStarted> SERVER_STARTED = null;

    public interface ServerStarted {
        void onServerStarted(net.minecraft.server.MinecraftServer server);
    }
}
"#,
    ),
    (
        "net/minecraft/server/MinecraftServer.java",
        r#"package net.minecraft.server;

public abstract class MinecraftServer {
    public abstract Iterable<Object> getAllLevels();
}
"#,
    ),
];

fn javac() -> PathBuf {
    std::env::var_os("JAVA_HOME")
        .map(|home| PathBuf::from(home).join("bin/javac"))
        .unwrap_or_else(|| "javac".into())
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

struct Fixture {
    root: PathBuf,
    jar: NativeJar,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Fixture {
    fn build(test: &str) -> Self {
        let unique = unique_suffix();
        let root = std::env::temp_dir().join(format!(
            "horizon-native-java-levels-{test}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let mut fixture = Self {
            root,
            jar: NativeJar::default(),
        };
        let classes = fixture.root.join("classes");
        let mut javac = Command::new(javac());
        javac.args(["--release", "21", "-d"]).arg(&classes);
        let mut sources = vec![("demo/Entry.java", ENTRY)];
        sources.extend(STUBS.iter().copied());
        for (path, source) in sources {
            let file = fixture.root.join("src").join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, source).unwrap();
            javac.arg(file);
        }
        let status = javac.status().unwrap();
        assert!(status.success(), "javac failed for fixture {test}");

        let jar_path = fixture.root.join("levels.jar");
        let mut zip = zip::ZipWriter::new(fs::File::create(&jar_path).unwrap());
        zip.start_file("fabric.mod.json", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(
            br#"{"id":"levels-fixture","version":"1.0","entrypoints":{"main":"demo.Entry"}}"#,
        )
        .unwrap();
        for class in files_under(&classes.join("demo")) {
            let entry = class
                .strip_prefix(&classes)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(entry, SimpleFileOptions::default()).unwrap();
            zip.write_all(&fs::read(class).unwrap()).unwrap();
        }
        zip.finish().unwrap();

        fixture.jar = NativeJar::from_path(&jar_path).unwrap();
        fixture
    }

    /// Run the entrypoint, fire `event` (after any earlier firings the test
    /// gives), and return what the listener printed.
    fn fire(&self, before: &[(GameEvent, Option<&str>)], event: GameEvent) -> String {
        let jar = &self.jar;
        let mut host = jar.seed_host(StdHost::new().with_environment_type("SERVER"));
        let entries = jar.discover_entrypoints_on("main", "SERVER");
        for entry in &entries {
            jar.execute_with_budget(
                &entry.class,
                entry.method.as_deref(),
                "main",
                &mut host,
                &ExecutionBudget::within(std::time::Duration::from_secs(30)),
            )
            .unwrap_or_else(|error| panic!("entrypoint failed: {error}"));
        }
        let listeners = host.game_listeners(jar, true);
        let facade = std::sync::Arc::new(EventFacade {
            // The game's hook payload carries no level list; only the event
            // itself names its level.
            dedicated: true,
            ..EventFacade::default()
        });
        for (earlier, level) in before {
            let mut firing = EventFiring::new(
                *earlier,
                facade.clone(),
                EventContext {
                    level: level.map(str::to_owned),
                    ..EventContext::default()
                },
            );
            // What the game bridge does for every jar on every firing, with or
            // without a listener for the event.
            host.note_firing_levels(&firing);
            for listener in listeners
                .listeners
                .iter()
                .filter(|listener| listener.event == *earlier)
            {
                jar.run_game_listener(
                    listener,
                    &mut firing,
                    &mut host,
                    &ExecutionBudget::within(std::time::Duration::from_secs(30)),
                )
                .unwrap();
            }
        }
        let mut firing = EventFiring::new(event, facade, EventContext::default());
        for listener in listeners
            .listeners
            .iter()
            .filter(|listener| listener.event == event)
        {
            jar.run_game_listener(
                listener,
                &mut firing,
                &mut host,
                &ExecutionBudget::within(std::time::Duration::from_secs(30)),
            )
            .unwrap_or_else(|error| panic!("listener failed: {error}"));
        }
        host.take_stdout()
    }
}

#[test]
fn a_level_announced_before_server_started_is_listed() {
    let fixture = Fixture::build("loaded");
    let out = fixture.fire(
        &[(GameEvent::LevelLoad, Some("minecraft:overworld"))],
        GameEvent::ServerStarted,
    );
    assert_eq!(out.trim(), "levels=1");
}

#[test]
fn an_unloaded_level_is_no_longer_listed() {
    let fixture = Fixture::build("unloaded");
    let out = fixture.fire(
        &[
            (GameEvent::LevelLoad, Some("minecraft:overworld")),
            (GameEvent::LevelUnload, Some("minecraft:overworld")),
        ],
        GameEvent::ServerStarted,
    );
    assert_eq!(out.trim(), "levels=0");
}
