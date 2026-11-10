//! Horizon Java: Horizon runs Java-mod jars on a native, JVM-free bytecode runtime.
//!
//! The runtime is [`crate::runtime`], a vendored copy of `crates/horizon-native-java` from the
//! horizon-java repository (`tools/vendor.py`; `mod/vendor.lock` pins the commit). This crate is
//! the mod: it implements the game's typed [`Hooks`] and calls the runtime from them.
//!
//! # Milestone 0: the shell
//!
//! `server_about_to_start` and `client_setup` each log one line, and the server start writes
//! this mod's report, `config/horizon_java/java-runtime.json`, through the mod API's own config
//! service (`Host::config_files`), so the game files it beside every other mod's config under
//! `config/`. With no jars to run the report is the empty form:
//!
//! ```json
//! {"schema":1,"jars":[]}
//! ```
//!
//! # Milestone 1: run jars inside the game
//!
//! Every `.jar` in `config/horizon_java/java-mods/` is read as bytes through the same service,
//! loaded by the runtime from memory, and run with its own [`StdHost`]: no game bindings yet, so
//! a call the runtime does not model is counted, not answered by the world. Each jar logs one
//! line and gets one row in the report:
//!
//! * `ran-clean`: the jar's server entrypoints ran and no host call was unmodelled;
//! * `ran-hollow`: they ran, but at least one host call was answered neutrally (`unknown_host_call_count`);
//! * `refused`: the jar could not be read, has no server-side entrypoint, or an entrypoint failed,
//!   and `reason` says which.
//!
//! # Milestone 2a: listeners on real server lifecycle events
//!
//! The jar's runtime state (heap, statics and the listeners its entrypoints registered) is kept
//! for the life of the server instead of being thrown away after the entrypoints. This mod's
//! server lifecycle hooks then fire the runtime's recorded listeners:
//!
//! * `server_about_to_start` fires the Fabric `SERVER_STARTING` and NeoForge
//!   `ServerAboutToStartEvent` listeners (the runtime's `server_starting` event);
//! * `server_starting` fires the NeoForge `ServerStartingEvent` listeners, and `server_started`
//!   the Fabric `SERVER_STARTED` and NeoForge `ServerStartedEvent` ones (the runtime records both
//!   of the latter on its `server_started` event, so its source names the split);
//! * `server_stopping` and `server_stopped` fire the matching listeners.
//!
//! Each hook logs how many callbacks ran, how many failed, and how many leaned on a host call the
//! runtime does not model; a jar's `System.out` lines land in the game log with the jar's id.
//!
//! Jars are run one at a time, so a jar cannot link against its neighbours yet: cross-jar classes
//! are a later milestone ([docs/mod-plan.md](../../docs/mod-plan.md)).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mod_api::{
    ClientHost, ClientSetupEvent, ConfigEntryKind, ConfigFileError, ConfigPath, Hooks, Host,
    ServerAboutToStartEvent, ServerHost, ServerStartedEvent, ServerStartingEvent,
    ServerStoppedEvent, ServerStoppingEvent,
};
use runtime::events::{EventContext, EventFacade, EventFiring, GameEvent, GameListener};
use runtime::{ExecutionBudget, NativeJar, StdHost};
use serde_json::json;

mod runtime;

/// This mod's report, in its own `config/horizon_java/` folder.
const REPORT: &str = "horizon_java/java-runtime.json";
/// The folder every jar to run is dropped in, beside the report.
const JARS: &str = "horizon_java/java-mods";
/// The report's schema version. A reader that does not know it says so instead of guessing.
const SCHEMA: u64 = 1;
/// The wall-clock bound one jar's native execution may take. It is cooperative: the runtime checks
/// it between frames, so a jar that never returns is refused instead of hanging the server.
const JAR_BUDGET: Duration = Duration::from_secs(30);
/// The line every log entry starts with, so one `grep` finds all of them.
const LOG: &str = "Horizon Java";
/// The entrypoint kinds a server runs, in the order the probe picks them: Fabric's common
/// `main`, NeoForge's `server`, then the class a NeoForge jar discovers by annotation.
const SERVER_KINDS: [&str; 3] = ["main", "server", "neoforge"];

/// The mod's state. `server_about_to_start` runs once per server the window hosts, so every jar's
/// runtime is rebuilt there; the hooks after it fire the listeners those entrypoints registered.
#[derive(Default)]
pub struct HorizonJava {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// Jars that ran, in file-name order, with the state their listeners need.
    jars: Vec<LoadedJar>,
    /// Jars that could not run, kept for the report.
    refused: Vec<JarRow>,
}

/// One jar whose entrypoints ran: its runtime, the host that holds its heap and statics, and every
/// listener the host recorded. The host must outlive the entrypoints or a listener would read an
/// empty jar, which is what milestone 1 did.
struct LoadedJar {
    row: JarRow,
    jar: NativeJar,
    host: StdHost,
    listeners: Vec<GameListener>,
}

#[mod_api::horizon_mod]
impl Hooks for HorizonJava {
    /// The server is about to open its worlds (NeoForge's `ServerAboutToStartEvent`). Nothing of
    /// the world is loaded yet, and both server paths have read the mods' configs by now, so this
    /// is the earliest moment the mod's own `config/` files can be read or written.
    fn server_about_to_start(
        &self,
        host: &mut dyn ServerHost,
        event: &mut ServerAboutToStartEvent,
    ) {
        let jars = read_jars(host);
        let mut state = self.lock();
        *state = State::default();
        for (file, bytes) in jars {
            match load_jar(file, &bytes) {
                Ok(loaded) => {
                    log::info!("{LOG}: {}", loaded.row);
                    state.jars.push(loaded);
                }
                Err(row) => {
                    log::info!("{LOG}: {row}");
                    state.refused.push(row);
                }
            }
        }
        let count = |verdict: &str| {
            state
                .jars
                .iter()
                .map(|jar| jar.row.verdict)
                .chain(state.refused.iter().map(|row| row.verdict))
                .filter(|known| *known == verdict)
                .count()
        };
        let total = state.jars.len() + state.refused.len();
        match write_report(host, &state) {
            Ok(path) => log::info!(
                "{LOG} {}: server about to start (dedicated={}); {total} jar(s) in config/{JARS}: {} ran-clean, {} ran-hollow, {} refused; report in {path}",
                env!("CARGO_PKG_VERSION"),
                event.dedicated,
                count("ran-clean"),
                count("ran-hollow"),
                count("refused"),
            ),
            Err(error) => log::warn!(
                "{LOG}: server about to start (dedicated={}), {total} jar(s) ran, but {REPORT} could not be written: {error}",
                event.dedicated,
            ),
        }
        drop(state);
        // Fabric's `SERVER_STARTING` and NeoForge's `ServerAboutToStartEvent` are the runtime's
        // one `server_starting` event, and both belong to this moment: before the worlds load.
        self.fire(GameEvent::ServerStarting, event.dedicated, |_| true);
    }

    /// Immediately after `server_about_to_start`, before the server accepts play (NeoForge's
    /// `ServerStartingEvent`). The runtime records that event on `server_started` with the
    /// Fabric field; its source keeps the two apart.
    fn server_starting(&self, _host: &mut dyn ServerHost, event: &mut ServerStartingEvent) {
        self.fire(GameEvent::ServerStarted, event.dedicated, |listener| {
            listener.source.ends_with("ServerStartingEvent")
        });
    }

    /// The worlds are open and the tick loop is about to start (Fabric's `SERVER_STARTED`,
    /// NeoForge's `ServerStartedEvent`).
    fn server_started(&self, _host: &mut dyn ServerHost, event: &mut ServerStartedEvent) {
        self.fire(GameEvent::ServerStarted, event.dedicated, |listener| {
            !listener.source.ends_with("ServerStartingEvent")
        });
    }

    /// Teardown begins, worlds still loaded and players connected (Fabric's `SERVER_STOPPING`,
    /// NeoForge's `ServerStoppingEvent`). The report is rewritten here with the callback counts,
    /// while the config service is still answering.
    fn server_stopping(&self, host: &mut dyn ServerHost, event: &mut ServerStoppingEvent) {
        self.fire(GameEvent::ServerStopping, event.dedicated, |_| true);
        let state = self.lock();
        if let Err(error) = write_report(host, &state) {
            log::warn!("{LOG}: {REPORT} could not be updated at server stopping: {error}");
        }
    }

    /// Teardown is over (Fabric's `SERVER_STOPPED`, NeoForge's `ServerStoppedEvent`). The jar
    /// state is dropped once its last listeners have run.
    fn server_stopped(&self, _host: &mut dyn ServerHost, event: &mut ServerStoppedEvent) {
        self.fire(GameEvent::ServerStopped, event.dedicated, |_| true);
        *self.lock() = State::default();
    }

    /// The client finished its first load (NeoForge's `FMLClientSetupEvent`), once per process.
    /// A dedicated server never fires it. No client jar runs yet, so the hook logs the moment and
    /// nothing else.
    fn client_setup(&self, _host: &mut dyn ClientHost, _event: &mut ClientSetupEvent) {
        log::info!("{LOG} {}: client setup", env!("CARGO_PKG_VERSION"));
    }
}

impl HorizonJava {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Fires one runtime event to every jar's recorded listeners, and says what happened.
    fn fire(&self, event: GameEvent, dedicated: bool, wanted: impl Fn(&GameListener) -> bool) {
        let mut state = self.lock();
        let (mut ran, mut failed, mut unknown) = (0, 0, 0);
        for loaded in &mut state.jars {
            let id = loaded.row.id.clone();
            let (jar_ran, jar_failed, jar_unknown) = fire_listeners_with_output(
                &loaded.jar,
                &mut loaded.host,
                &loaded.listeners,
                event,
                dedicated,
                &wanted,
                &id,
            );
            loaded.row.callbacks_ran += jar_ran;
            loaded.row.callbacks_failed += jar_failed;
            loaded.row.callback_unknown_host_calls += jar_unknown;
            ran += jar_ran;
            failed += jar_failed;
            unknown += jar_unknown;
        }
        log::info!(
            "{LOG}: {} (dedicated={dedicated}): {ran} callback(s) ran, {failed} failed, {unknown} unknown host call(s)",
            event.name()
        );
    }
}

/// Fires the listeners of one event on one jar, drains what they printed, and returns how many
/// ran, how many failed and how many unmodelled host calls they made.
fn fire_listeners_with_output(
    jar: &NativeJar,
    host: &mut StdHost,
    listeners: &[GameListener],
    event: GameEvent,
    dedicated: bool,
    wanted: &dyn Fn(&GameListener) -> bool,
    id: &str,
) -> (usize, usize, usize) {
    let before = host.unknown_host_call_count();
    let (ran, failed) = fire_listeners(jar, host, listeners, event, dedicated, wanted);
    let unknown = host.unknown_host_call_count().saturating_sub(before);
    log_stdout(id, host);
    (ran, failed, unknown)
}

/// Runs the listeners of `event` that `wanted` keeps, in registration order, sharing one firing so
/// a cancellation carries between them.
fn fire_listeners(
    jar: &NativeJar,
    host: &mut StdHost,
    listeners: &[GameListener],
    event: GameEvent,
    dedicated: bool,
    wanted: impl Fn(&GameListener) -> bool,
) -> (usize, usize) {
    let mut firing = EventFiring::new(
        event,
        Arc::new(EventFacade {
            dedicated,
            ..EventFacade::default()
        }),
        EventContext::default(),
    );
    let (mut ran, mut failed) = (0, 0);
    for listener in listeners
        .iter()
        .filter(|listener| listener.event == event && wanted(listener))
    {
        match jar.run_game_listener(
            listener,
            &mut firing,
            host,
            &ExecutionBudget::within(JAR_BUDGET),
        ) {
            Ok(run) => {
                ran += usize::from(run.ran);
                if run.stop {
                    break;
                }
            }
            Err(error) => {
                failed += 1;
                log::warn!(
                    "{LOG}: {} listener from {} failed: {error}",
                    event.name(),
                    listener.source
                );
            }
        }
    }
    (ran, failed)
}

/// Sends the jar's `System.out` to the game log, one line per entry, with the jar's id.
fn log_stdout(id: &str, host: &mut StdHost) {
    for line in host.take_stdout().lines() {
        if !line.trim().is_empty() {
            log::info!("{LOG}: {id}: {}", line.trim_end());
        }
    }
}

/// One jar's outcome, the row the report holds and the line the log gets.
struct JarRow {
    file: String,
    id: String,
    /// `ran-clean`, `ran-hollow` or `refused`.
    verdict: &'static str,
    host_calls: usize,
    unknown_host_calls: usize,
    /// Reason a refused jar was refused; `None` for one that ran.
    reason: Option<String>,
    /// Server lifecycle callbacks that ran and failed, and the unmodelled host calls they made.
    callbacks_ran: usize,
    callbacks_failed: usize,
    callback_unknown_host_calls: usize,
}

impl JarRow {
    fn refused(file: &str, id: String, reason: String) -> Self {
        Self {
            file: file.to_owned(),
            id,
            verdict: "refused",
            host_calls: 0,
            unknown_host_calls: 0,
            reason: Some(reason),
            callbacks_ran: 0,
            callbacks_failed: 0,
            callback_unknown_host_calls: 0,
        }
    }

    fn json(&self) -> serde_json::Value {
        json!({
            "file": self.file,
            "id": self.id,
            "verdict": self.verdict,
            "host_calls": self.host_calls,
            "unknown_host_call_count": self.unknown_host_calls,
            "reason": self.reason,
            "callbacks": {
                "ran": self.callbacks_ran,
                "failed": self.callbacks_failed,
                "unknown_host_call_count": self.callback_unknown_host_calls,
            },
        })
    }
}

impl std::fmt::Display for JarRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}) {} ({} host calls, {} unknown host calls",
            self.file, self.id, self.verdict, self.host_calls, self.unknown_host_calls
        )?;
        if let Some(reason) = &self.reason {
            write!(f, ", reason: {reason}")?;
        }
        f.write_str(")")
    }
}

/// The jars to run: every file named `*.jar` in `config/horizon_java/java-mods/`, sorted by name
/// so one run's report reads the same as the next. An absent folder is no jars, which is the
/// state a fresh install is in; an unreadable one is a warning, not a failed server start.
fn read_jars(host: &dyn Host) -> Vec<(String, Vec<u8>)> {
    let files = host.config_files();
    let Ok(folder) = ConfigPath::new(JARS) else {
        return Vec::new();
    };
    let entries = match files.list(&folder) {
        Ok(entries) => entries,
        Err(ConfigFileError::NotFound) => return Vec::new(),
        Err(error) => {
            log::warn!("{LOG}: config/{JARS} could not be listed: {error}");
            return Vec::new();
        }
    };
    let mut jars = entries
        .into_iter()
        .filter(|entry| entry.stat.kind == ConfigEntryKind::File && entry.name.ends_with(".jar"))
        .filter_map(|entry| {
            let path = ConfigPath::new(&format!("{JARS}/{}", entry.name)).ok()?;
            match files.read(&path) {
                Ok(bytes) => Some((entry.name, bytes)),
                Err(error) => {
                    log::warn!(
                        "{LOG}: config/{JARS}/{} could not be read: {error}",
                        entry.name
                    );
                    None
                }
            }
        })
        .collect::<Vec<_>>();
    jars.sort_by(|a, b| a.0.cmp(&b.0));
    jars
}

/// Loads one jar the way a server would: its server-side entrypoints, in its own host, with no
/// game behind it. Anything the runtime does not model is counted by the host, which is what
/// `ran-hollow` means. The host is returned because the listeners it recorded need it for the
/// rest of the server's life.
fn load_jar(file: String, bytes: &[u8]) -> Result<LoadedJar, JarRow> {
    let stem = file.strip_suffix(".jar").unwrap_or(&file).to_owned();
    let jar = match runtime::NativeJar::from_bytes(bytes) {
        Ok(jar) => jar,
        Err(error) => {
            return Err(JarRow::refused(
                &file,
                stem,
                format!("not a readable jar: {error}"),
            ));
        }
    };
    let id = jar
        .fabric_id
        .clone()
        .or_else(|| jar.neoforge_id.clone())
        .unwrap_or_else(|| stem.clone());
    let Some(kind) = SERVER_KINDS
        .into_iter()
        .find(|kind| !jar.discover_entrypoints(kind).is_empty())
    else {
        let reason = if jar.discover_entrypoints("client").is_empty() {
            "no main, server or neoforge entrypoint"
        } else {
            "client-only entrypoint: a server never runs it"
        };
        return Err(JarRow::refused(&file, id, reason.to_owned()));
    };
    let entrypoints = jar.discover_entrypoints(kind);
    if let Some(entry) = entrypoints.iter().find(|entry| entry.adapter.is_some()) {
        return Err(JarRow::refused(
            &file,
            id,
            format!(
                "{}: entrypoint adapter {} is not supported by the native path",
                entry.class,
                entry.adapter.as_deref().unwrap_or_default()
            ),
        ));
    }
    let mut host = jar.seed_host(StdHost::new().with_environment_type("SERVER"));
    let mut calls = 0;
    for entry in &entrypoints {
        let outcome = jar.execute_with_budget(
            &entry.class,
            entry.method.as_deref(),
            kind,
            &mut host,
            &runtime::ExecutionBudget::within(JAR_BUDGET),
        );
        match outcome {
            Ok(report) => calls += report.calls,
            Err(error) => {
                let unknown = host.unknown_host_call_count();
                return Err(JarRow {
                    file,
                    id,
                    verdict: "refused",
                    host_calls: calls,
                    unknown_host_calls: unknown,
                    reason: Some(format!("{}: {error}", entry.class)),
                    callbacks_ran: 0,
                    callbacks_failed: 0,
                    callback_unknown_host_calls: 0,
                });
            }
        }
    }
    let unknown = host.unknown_host_call_count();
    let listeners = host.game_listeners(&jar, true).listeners;
    Ok(LoadedJar {
        row: JarRow {
            file,
            id,
            verdict: if unknown == 0 {
                "ran-clean"
            } else {
                "ran-hollow"
            },
            host_calls: calls,
            unknown_host_calls: unknown,
            reason: None,
            callbacks_ran: 0,
            callbacks_failed: 0,
            callback_unknown_host_calls: 0,
        },
        jar,
        host,
        listeners,
    })
}

/// Writes the report, replacing the previous one, and answers the path it was written to (as a
/// reader finds it under the game's `config/`).
fn write_report(host: &dyn Host, state: &State) -> Result<String, ConfigFileError> {
    let path = ConfigPath::new(REPORT)?;
    let rows: Vec<_> = state
        .jars
        .iter()
        .map(|jar| jar.row.json())
        .chain(state.refused.iter().map(JarRow::json))
        .collect();
    let report = json!({ "schema": SCHEMA, "jars": rows });
    let mut bytes = serde_json::to_vec_pretty(&report).unwrap_or_default();
    bytes.push(b'\n');
    host.config_files().write(&path, &bytes)?;
    Ok(format!("config/{REPORT}"))
}
