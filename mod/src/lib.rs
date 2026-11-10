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

use mod_api::{
    ClientHost, ClientSetupEvent, ConfigPath, ConfigFileError, Hooks, Host, ServerAboutToStartEvent,
    ServerHost,
};
use serde_json::json;

mod runtime;

/// This mod's report, in its own `config/horizon_java/` folder.
const REPORT: &str = "horizon_java/java-runtime.json";
/// The report's schema version. A reader that does not know it says so instead of guessing.
const SCHEMA: u64 = 1;
/// The line every log entry starts with, so one `grep` finds all of them.
const LOG: &str = "Horizon Java";

/// The mod's state. `server_about_to_start` runs once per server the window hosts, so the report
/// is written from scratch each time; nothing else is kept between hooks yet.
#[derive(Default)]
pub struct HorizonJava;

#[mod_api::horizon_mod]
impl Hooks for HorizonJava {
    /// The server is about to open its worlds (NeoForge's `ServerAboutToStartEvent`). Nothing of
    /// the world is loaded yet, and on a dedicated server the game has already read the mods'
    /// configs, so this is the earliest moment the mod's own `config/` files can be read or
    /// written. Milestone 1 runs jars here.
    fn server_about_to_start(
        &self,
        host: &mut dyn ServerHost,
        event: &mut ServerAboutToStartEvent,
    ) {
        let jars: Vec<serde_json::Value> = Vec::new();
        match write_report(host, &jars) {
            Ok(path) => log::info!(
                "{LOG} {}: server about to start (dedicated={}); {} jar(s) ran, report in {path}",
                env!("CARGO_PKG_VERSION"),
                event.dedicated,
                jars.len(),
            ),
            Err(error) => log::warn!(
                "{LOG}: server about to start (dedicated={}), but {REPORT} could not be written: {error}",
                event.dedicated,
            ),
        }
    }

    /// The client finished its first load (NeoForge's `FMLClientSetupEvent`), once per process.
    /// A dedicated server never fires it. Milestone 0 logs the moment and nothing else.
    fn client_setup(&self, _host: &mut dyn ClientHost, _event: &mut ClientSetupEvent) {
        log::info!(
            "{LOG} {}: client setup",
            env!("CARGO_PKG_VERSION"),
        );
    }
}

/// Writes the report, replacing the previous one, and answers the path it was written to (as a
/// reader finds it under the game's `config/`).
fn write_report(
    host: &dyn Host,
    jars: &[serde_json::Value],
) -> Result<String, ConfigFileError> {
    let path = ConfigPath::new(REPORT)?;
    let report = json!({ "schema": SCHEMA, "jars": jars });
    let mut bytes = serde_json::to_vec_pretty(&report).unwrap_or_default();
    bytes.push(b'\n');
    host.config_files().write(&path, &bytes)?;
    Ok(format!("config/{REPORT}"))
}
