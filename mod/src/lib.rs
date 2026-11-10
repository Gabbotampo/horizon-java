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
//! Jars are run one at a time, so a jar cannot link against its neighbours yet: cross-jar classes
//! and game bindings are the next milestones ([docs/mod-plan.md](../../docs/mod-plan.md)).

use std::time::Duration;

use mod_api::{
    ClientHost, ClientSetupEvent, ConfigEntryKind, ConfigFileError, ConfigPath, Hooks, Host,
    ServerAboutToStartEvent, ServerHost,
};
use runtime::StdHost;
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

/// The mod's state. `server_about_to_start` runs once per server the window hosts, so the report
/// is written from scratch each time; nothing else is kept between hooks yet.
#[derive(Default)]
pub struct HorizonJava;

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
        let rows = jars
            .iter()
            .map(|(file, bytes)| {
                let row = run_jar(file, bytes);
                log::info!("{LOG}: {row}");
                row
            })
            .collect::<Vec<_>>();
        let rows = rows.iter().map(JarRow::json).collect::<Vec<_>>();
        let count = |verdict: &str| {
            rows.iter()
                .filter(|row| row["verdict"] == verdict)
                .count()
        };
        match write_report(host, &rows) {
            Ok(path) => log::info!(
                "{LOG} {}: server about to start (dedicated={}); {} jar(s) in config/{JARS}: {} ran-clean, {} ran-hollow, {} refused; report in {path}",
                env!("CARGO_PKG_VERSION"),
                event.dedicated,
                rows.len(),
                count("ran-clean"),
                count("ran-hollow"),
                count("refused"),
            ),
            Err(error) => log::warn!(
                "{LOG}: server about to start (dedicated={}), {} jar(s) ran, but {REPORT} could not be written: {error}",
                event.dedicated,
                rows.len(),
            ),
        }
    }

    /// The client finished its first load (NeoForge's `FMLClientSetupEvent`), once per process.
    /// A dedicated server never fires it. Milestone 1 runs no client jars, so the hook logs the
    /// moment and nothing else.
    fn client_setup(&self, _host: &mut dyn ClientHost, _event: &mut ClientSetupEvent) {
        log::info!("{LOG} {}: client setup", env!("CARGO_PKG_VERSION"));
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
    /// Why a refused jar was refused; `None` for one that ran.
    reason: Option<String>,
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
                    log::warn!("{LOG}: config/{JARS}/{} could not be read: {error}", entry.name);
                    None
                }
            }
        })
        .collect::<Vec<_>>();
    jars.sort_by(|a, b| a.0.cmp(&b.0));
    jars
}

/// Runs one jar the way a server would: its server-side entrypoints, in its own host, with no
/// game behind it. Anything the runtime does not model is counted by the host, which is what
/// `ran-hollow` means.
fn run_jar(file: &str, bytes: &[u8]) -> JarRow {
    let stem = file.strip_suffix(".jar").unwrap_or(file).to_owned();
    let jar = match runtime::NativeJar::from_bytes(bytes) {
        Ok(jar) => jar,
        Err(error) => return JarRow::refused(file, stem, format!("not a readable jar: {error}")),
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
        return JarRow::refused(file, id, reason.to_owned());
    };
    let entrypoints = jar.discover_entrypoints(kind);
    if let Some(entry) = entrypoints.iter().find(|entry| entry.adapter.is_some()) {
        return JarRow::refused(
            file,
            id,
            format!(
                "{}: entrypoint adapter {} is not supported by the native path",
                entry.class,
                entry.adapter.as_deref().unwrap_or_default()
            ),
        );
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
                return JarRow {
                    file: file.to_owned(),
                    id,
                    verdict: "refused",
                    host_calls: calls,
                    unknown_host_calls: unknown,
                    reason: Some(format!("{}: {error}", entry.class)),
                };
            }
        }
    }
    let unknown = host.unknown_host_call_count();
    JarRow {
        file: file.to_owned(),
        id,
        verdict: if unknown == 0 {
            "ran-clean"
        } else {
            "ran-hollow"
        },
        host_calls: calls,
        unknown_host_calls: unknown,
        reason: None,
    }
}

/// Writes the report, replacing the previous one, and answers the path it was written to (as a
/// reader finds it under the game's `config/`).
fn write_report(host: &dyn Host, jars: &[serde_json::Value]) -> Result<String, ConfigFileError> {
    let path = ConfigPath::new(REPORT)?;
    let report = json!({ "schema": SCHEMA, "jars": jars });
    let mut bytes = serde_json::to_vec_pretty(&report).unwrap_or_default();
    bytes.push(b'\n');
    host.config_files().write(&path, &bytes)?;
    Ok(format!("config/{REPORT}"))
}
