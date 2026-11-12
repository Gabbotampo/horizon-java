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
//! Every `.jar` in `config/horizon_java/java-mods/` is read as bytes, loaded by the runtime from
//! memory, and run with its own [`StdHost`]: no game bindings yet, so a call the runtime does not
//! model is counted, not answered by the world. (Milestone 3 moved the read before the content
//! phase, where `config_files` does not exist yet; [`content`] explains why it is `std::fs`.)
//! Each jar logs one line and gets one row in the report:
//!
//! * `ran-clean`: every server entrypoint stage ran and no host call was unmodelled;
//! * `ran-hollow`: every stage ran, but at least one host call was answered neutrally
//!   (`unknown_host_call_count`);
//! * `ran-partial`: at least one stage ran and at least one failed (an error or a caught
//!   interpreter panic); the listeners the running stages registered stay active, and `reason`
//!   names every failed stage and its error. A real loader would have thrown, so this is never
//!   counted as clean;
//! * `refused`: the jar could not be read, has no server-side entrypoint, or every stage failed,
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
//! # Milestone 3: a jar's items and blocks
//!
//! `blocks` and `items` are asked once, at the game's Registration phase and before any resource
//! loads, so that is where the jars now run ([`content`]). Each typed `RegistryDeclaration` the
//! runtime recorded becomes a native content declaration: an item keeps its inferred template and
//! the properties the content vocabulary carries, and a block brings an item only when the jar
//! registered one (a block does not invent one). The game's
//! content root joins them under the jar's namespace, so `/give` and the inventory see
//! `<jar id>:<path>`. Jar assets, models, lang and loot stay outside this pass, as
//! `docs/m3-content-design.md` maps.
//!
//! # Milestone 2b: Java commands
//!
//! At `register_commands` the recorded Fabric `CommandRegistrationCallback` (and NeoForge
//! `RegisterCommandsEvent`) listeners build their Brigadier tree on the runtime's façade, and this
//! mod translates the literals and the string/integer argument nodes into [`Command`] usages.
//! `run_mod_command` maps the game's typed invocation back onto the runtime's parsed command, runs
//! the Java `Command.run` callback with a `CommandSourceStack` façade, and sends what it replied
//! through `Commands::feedback` (or to the player it named). A node whose argument type, redirect or
//! requirement this bridge cannot carry is refused by name and left out, never guessed.
//!
//! Jars are run one at a time, so a jar cannot link against its neighbours yet: cross-jar classes
//! are a later milestone ([docs/mod-plan.md](../../docs/mod-plan.md)).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use attachments::Lifecycle;
use mod_api::{
    AttackEntityEvent, Authority, BlocksEvent, BreakBlockEvent, ClientHost, ClientSetupEvent,
    Command, CommandArgumentValue, CommandSource, ConfigFileError, ConfigPath, DimensionKey,
    EntityInteractEvent, EntityJoinLevelEvent, EntityLeaveLevelEvent, Flow, Hand, Hooks, Host,
    HostSide, InteractionResult, ItemStack, ItemsEvent, LeftClickBlockEvent, LevelDimension,
    LevelLoadEvent, LevelRandomHost, LevelRef, LevelTickPostEvent, LevelTickPreEvent,
    LevelUnloadEvent, LivingDamagePostEvent, LivingDeathEvent, LivingDeathPostEvent,
    LivingIncomingDamageEvent, PlayerChangedDimensionEvent, PlayerCloneEvent, PlayerLoggedInEvent,
    PlayerLoggedOutEvent, PlayerRef, PlayerRespawnedEvent, RegisterCommandsEvent,
    RegistrationContext, RightClickBlockEvent, RightClickItemEvent, RunModCommandEvent,
    ServerAboutToStartEvent, ServerHost, ServerStartedEvent, ServerStartingEvent,
    ServerStoppedEvent, ServerStoppingEvent, ServerTickEvent, ServerTickPhase, Text,
};
use runtime::brigadier::{
    self, ArgumentKind, CommandInvocation as RuntimeInvocation, CommandMessage, CommandOutcome,
    CommandSource as RuntimeSource, CommandText, MessageKind, NodeKind, Requirement,
};
use runtime::events::{
    EventContext, EventFacade, EventFiring, FacadeEntity, FacadeLevel, FacadePlayer, GameEvent,
    GameListener, PhaseOrdering, ServerFacts, dispatch_order,
};
use runtime::{ExecutionBudget, NativeJar, StdHost};
use serde_json::json;

mod attachments;
mod content;
mod runtime;
mod server_facts;

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
/// The mod's state. The content hooks run the jars once per process, before any server exists;
/// every later hook reuses what they recorded, and a second server this process hosts only resets
/// the per-server state.
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
    /// Whether the jars have been read and run this process. The content hooks run once, before
    /// the server exists; the server hooks reuse what they recorded.
    loaded: bool,
    /// The online players as the host names them at the last join, leave or dimension change:
    /// UUID, name, level and entity id. A level the host could not name stays `None`, and the
    /// façade refuses that read instead of guessing.
    players: Vec<FacadePlayer>,
    /// The server levels the host reports open, with their clocks, refreshed on load, unload and
    /// each level tick. A level whose clocks the host cannot read is left out rather than carried
    /// with a made-up time.
    levels: Vec<FacadeLevel>,
    /// `MinecraftServer.getTickCount()` as the last server tick reported it.
    tick_count: i32,
    /// Whether the open server is a dedicated one, from `server_about_to_start`.
    dedicated: bool,
    /// The world folder and registries the host gives (`server_facts.rs`), read at
    /// `server_about_to_start` and again as each level loads.
    server: Arc<ServerFacts>,
}

/// What one game event's listeners decided, for the hook that fired it.
#[derive(Default)]
struct Fired {
    /// A listener cancelled the vanilla work.
    cancelled: bool,
    /// The first non-`PASS` `InteractionResult` a Fabric listener returned.
    interaction: Option<String>,
}

/// One jar whose entrypoints ran: its runtime, the host that holds its heap and statics, and every
/// listener the host recorded. The host must outlive the entrypoints or a listener would read an
/// empty jar, which is what milestone 1 did.
struct LoadedJar {
    row: JarRow,
    jar: Arc<NativeJar>,
    host: StdHost,
    listeners: Vec<GameListener>,
    /// The jar's Fabric `addPhaseOrdering` calls, which order every jar's listeners of that field.
    orderings: Vec<PhaseOrdering>,
    /// Commands the jar registered through the Brigadier façade, translated once.
    commands: Vec<JavaCommand>,
    native_commands: Vec<Command>,
    commands_registered: bool,
}

/// One Java command this bridge can run: its usage, the literal path that selects it, the
/// arguments it takes, and the `Command.run` callback marker.
#[derive(Clone)]
struct JavaCommand {
    usage: String,
    literals: Vec<String>,
    arguments: Vec<(String, JavaArgument)>,
    run: runtime::Value,
}

/// The argument kinds this bridge carries, and the usage spelling each one gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JavaArgument {
    Word,
    Text,
    Integer,
    /// Brigadier's `EntityArgument.player()` or `.players()`, carried as the mod API's `player`
    /// kind: the game resolves the selector to exactly one online player before the command
    /// runs. `player()` is exact. `players()` is the one-player subset of what vanilla accepts:
    /// a selector that picks two or more players is refused by the game's parser with feedback
    /// instead of running once per player (named in `docs/mod-plan.md`, "Departures").
    Player,
}

impl JavaArgument {
    fn usage_kind(self) -> &'static str {
        match self {
            Self::Word => "word",
            Self::Text => "text",
            Self::Integer => "integer",
            Self::Player => "player",
        }
    }

    /// The kinds the runtime's façade records that this bridge carries. Everything else is
    /// refused by name rather than approximated.
    ///
    /// `EntityArgument.entity()` and `.entities()` stay refused: the mod API's only selector
    /// kind resolves players, and narrowing "any entity" to "one player" would run the command on
    /// a different set than the jar asked for (the fork seam is in `docs/mod-plan.md`).
    fn from_kind(kind: &ArgumentKind) -> Option<Self> {
        match kind {
            ArgumentKind::Word | ArgumentKind::Quoted => Some(Self::Word),
            ArgumentKind::Greedy => Some(Self::Text),
            ArgumentKind::Integer { .. } => Some(Self::Integer),
            ArgumentKind::Entity { players: true, .. } => Some(Self::Player),
            _ => None,
        }
    }

    fn describe(kind: &ArgumentKind) -> String {
        match kind {
            ArgumentKind::Word => "word".to_owned(),
            ArgumentKind::Quoted => "quoted string".to_owned(),
            ArgumentKind::Greedy => "greedy string".to_owned(),
            ArgumentKind::Integer { .. } => "integer".to_owned(),
            ArgumentKind::Entity {
                single: true,
                players: false,
            } => "single-entity selector (EntityArgument.entity)".to_owned(),
            ArgumentKind::Entity {
                single: false,
                players: false,
            } => "entity selector (EntityArgument.entities)".to_owned(),
            other => format!("{other:?}"),
        }
    }

    /// The runtime's parsed argument for one value the game resolved, and the word
    /// `CommandContext.getInput()` shows for it. `None` when the value is not this kind.
    ///
    /// A player is the runtime's player list of one, so `EntityArgument.getPlayer`/`getPlayers`
    /// answer the same `game/player/<uuid>` marker the event façade already answers name, UUID
    /// and level for. The game hands over the resolved player, not the selector text, so the
    /// input line names the player (or, without a name, their UUID, which vanilla also accepts
    /// as a selector) rather than repeating `@s` or `@p`.
    fn runtime_value(
        self,
        value: &CommandArgumentValue,
        player_name: impl Fn(PlayerRef) -> Option<String>,
    ) -> Option<(brigadier::CommandArgument, String)> {
        Some(match (self, value) {
            (Self::Word | Self::Text, CommandArgumentValue::String(text)) => {
                (brigadier::CommandArgument::Str(text.clone()), text.clone())
            }
            (Self::Integer, CommandArgumentValue::Integer(value)) => {
                (brigadier::CommandArgument::Int(*value), value.to_string())
            }
            (Self::Player, CommandArgumentValue::Player(player)) => {
                selected_player(player.uuid(), player_name(*player))
            }
            _ => return None,
        })
    }
}

/// One selected player as the runtime's argument and its input word: the player's name, or
/// their UUID when the game does not name them.
fn selected_player(uuid: u128, name: Option<String>) -> (brigadier::CommandArgument, String) {
    (
        brigadier::CommandArgument::Players(vec![uuid]),
        name.unwrap_or_else(|| hyphenated_uuid(uuid)),
    )
}

/// A UUID in Java's `UUID.toString()` form (`8-4-4-4-12` lower-case hex).
fn hyphenated_uuid(uuid: u128) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        uuid >> 96,
        (uuid >> 80) & 0xffff,
        (uuid >> 64) & 0xffff,
        (uuid >> 48) & 0xffff,
        uuid & 0xffff_ffff_ffff
    )
}

#[mod_api::horizon_mod]
impl Hooks for HorizonJava {
    /// The content root still accepts native block declarations here, at the Registration phase
    /// and before any resource loads (NeoForge's `RegisterEvent` drained from
    /// `DeferredRegister.Blocks`). This is the one moment a jar's registrations can become content
    /// in the same start, so the first content question is also where the jars run.
    fn blocks(&self, context: &RegistrationContext, event: &mut BlocksEvent<'_>) {
        let mut state = self.lock();
        content::declare_blocks(&mut state, context, event);
    }

    /// The item counterpart of [`Self::blocks`](Hooks::blocks) (`DeferredRegister.Items`).
    fn items(&self, context: &RegistrationContext, event: &mut ItemsEvent<'_>) {
        let mut state = self.lock();
        content::declare_items(&mut state, context, event);
    }

    /// The server is about to open its worlds (NeoForge's `ServerAboutToStartEvent`). Nothing of
    /// the world is loaded yet, and both server paths have read the mods' configs by now, so this
    /// is the earliest moment the mod's own `config/` files can be read or written.
    fn server_about_to_start(
        &self,
        host: &mut dyn ServerHost,
        event: &mut ServerAboutToStartEvent,
    ) {
        // The content hooks normally ran before any server; a game that never asked them still
        // gets milestone 1's rows, just without content.
        self.ensure_jars();
        let mut state = self.lock();
        state.dedicated = event.dedicated;
        state.players.clear();
        state.levels.clear();
        state.tick_count = 0;
        state.server = Arc::new(server_facts::server_facts(host));
        log::info!(
            "{LOG}: server facts at server start: {}",
            server_facts::describe(&state.server)
        );
        state.reset_server();
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
                "{LOG} {}: server about to start (dedicated={}); {total} jar(s) in config/{JARS}: {} ran-clean, {} ran-hollow, {} ran-partial, {} refused; report in {path}",
                env!("CARGO_PKG_VERSION"),
                event.dedicated,
                count("ran-clean"),
                count("ran-hollow"),
                count("ran-partial"),
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
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::ServerStarting,
            event.dedicated,
            EventContext::default(),
            |_| true,
        );
    }

    /// Immediately after `server_about_to_start`, before the server accepts play (NeoForge's
    /// `ServerStartingEvent`). The runtime records that event on `server_started` with the
    /// Fabric field; its source keeps the two apart.
    fn server_starting(&self, host: &mut dyn ServerHost, event: &mut ServerStartingEvent) {
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::ServerStarted,
            event.dedicated,
            EventContext::default(),
            |listener| listener.source.ends_with("ServerStartingEvent"),
        );
    }

    /// The worlds are open and the tick loop is about to start (Fabric's `SERVER_STARTED`,
    /// NeoForge's `ServerStartedEvent`).
    fn server_started(&self, host: &mut dyn ServerHost, event: &mut ServerStartedEvent) {
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::ServerStarted,
            event.dedicated,
            EventContext::default(),
            |listener| !listener.source.ends_with("ServerStartingEvent"),
        );
    }

    /// Teardown begins, worlds still loaded and players connected (Fabric's `SERVER_STOPPING`,
    /// NeoForge's `ServerStoppingEvent`). The report is rewritten here with the callback counts,
    /// while the config service is still answering.
    fn server_stopping(&self, host: &mut dyn ServerHost, event: &mut ServerStoppingEvent) {
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::ServerStopping,
            event.dedicated,
            EventContext::default(),
            |_| true,
        );
        let state = self.lock();
        // One line per jar with the counts the report now holds: the report is the authority, but
        // the log must never make a jar with failing listeners look like a clean one.
        for row in state.rows() {
            log::info!("{LOG}: {row}");
        }
        if let Err(error) = write_report(host, &state) {
            log::warn!("{LOG}: {REPORT} could not be updated at server stopping: {error}");
        }
    }

    /// Teardown is over (Fabric's `SERVER_STOPPED`, NeoForge's `ServerStoppedEvent`). The jar
    /// state stays: the entrypoints ran at the content phase, and the next world this process
    /// hosts reuses it.
    fn server_stopped(&self, host: &mut dyn ServerHost, event: &mut ServerStoppedEvent) {
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::ServerStopped,
            event.dedicated,
            EventContext::default(),
            |_| true,
        );
    }

    /// One jar changed its commands (`Commands::refresh_commands`): the Java registrations cannot
    /// change at runtime, so the same set is offered again.
    fn register_commands(&self, _host: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        let mut state = self.lock();
        let facade = state.facade();
        for loaded in &mut state.jars {
            if !loaded.commands_registered {
                loaded.commands_registered = true;
                let mut firing = EventFiring::new(
                    GameEvent::RegisterCommands,
                    Arc::new(facade.clone()),
                    EventContext::default(),
                );
                let before = loaded.host.unknown_host_call_count();
                let order = listener_order(std::slice::from_ref(&*loaded), &firing, &|_| true);
                let (ran, failed) =
                    run_listeners(std::slice::from_mut(loaded), &order, &mut firing)[0];
                let listener_unknown = loaded.host.unknown_host_call_count().saturating_sub(before);
                let before = loaded.host.unknown_host_call_count();
                let (native, commands, refused) =
                    translate_commands(&loaded.jar, &mut loaded.host, &loaded.row.id);
                let translation_unknown =
                    loaded.host.unknown_host_call_count().saturating_sub(before);
                // Registration is a callback like any other: its listeners' work and failures
                // belong in the jar's row, not only in a log line.
                count_registration(
                    &mut loaded.row,
                    (ran, failed),
                    listener_unknown,
                    native.len(),
                    refused,
                    translation_unknown,
                );
                log::info!(
                    "{LOG}: {}: command registration: {ran} callback(s) ran, {failed} failed, \
                     {listener_unknown} unknown host call(s); {} command usage(s) translated, \
                     {refused} refused, {translation_unknown} unknown host call(s)",
                    loaded.row.id,
                    native.len(),
                );
                log_stdout(&loaded.row.id, &mut loaded.host);
                loaded.native_commands = native;
                loaded.commands = commands;
            }
            event
                .commands
                .extend(loaded.native_commands.iter().cloned());
        }
    }

    /// A player (or the console) ran a command this mod registered. The typed invocation is
    /// mapped back onto the runtime's parsed command, the Java callback runs, and what it replied
    /// goes back through the command source's own feedback.
    fn run_mod_command(
        &self,
        host: &mut dyn ServerHost,
        event: &mut RunModCommandEvent<'_>,
    ) -> Flow {
        let mut state = self.lock();
        let facade = state.facade();
        for loaded in &mut state.jars {
            // One Java literal path can have several executable usages: the bare command and
            // each deeper argument node. The invocation carries the arguments it parsed, so the
            // usage that declares exactly those (the deepest one) is the one the Java callback
            // belongs to.
            let Some(command) = loaded
                .commands
                .iter()
                .filter(|command| command.literals == event.command.literals)
                .filter(|command| {
                    command
                        .arguments
                        .iter()
                        .all(|(name, _)| event.command.argument(name).is_some())
                })
                .max_by_key(|command| command.arguments.len())
                .cloned()
            else {
                continue;
            };
            let mut arguments = Vec::new();
            let mut words = command.literals.clone();
            for (name, kind) in &command.arguments {
                match event.command.argument(name) {
                    Some(value) => {
                        let Some((argument, word)) =
                            kind.runtime_value(value, |player| player.name(&*host))
                        else {
                            return refuse(
                                host,
                                &command.usage,
                                format!("argument '{name}' is not the kind this bridge carries"),
                            );
                        };
                        arguments.push((name.clone(), argument));
                        words.push(word);
                    }
                    None => {
                        return refuse(
                            host,
                            &command.usage,
                            format!("argument '{name}' is missing"),
                        );
                    }
                }
            }
            let source = runtime_source(event.source, host);
            let invocation = RuntimeInvocation {
                input: words.join(" "),
                source,
                arguments,
            };
            let opened = attachments::open(
                host,
                &loaded.row.id,
                &loaded.jar,
                &mut loaded.host,
                Lifecycle::None,
            );
            let before = loaded.host.unknown_host_call_count();
            let outcome = caught(
                &format!("{} command /{}", loaded.row.id, command.usage),
                || {
                    loaded.jar.run_command(
                        &command.run,
                        &invocation,
                        Arc::new(facade.clone()),
                        &mut loaded.host,
                        &ExecutionBudget::within(JAR_BUDGET),
                    )
                },
            );
            let unknown = loaded.host.unknown_host_call_count().saturating_sub(before);
            loaded.row.command_unknown_host_calls += unknown;
            attachments::close(
                host,
                &loaded.row.id,
                &loaded.jar,
                &mut loaded.host,
                opened,
                Lifecycle::None,
            );
            let run = match outcome {
                Ok(run) => run,
                Err(panic) => {
                    // The command ran as far as the panic: a failed command, not a crashed game.
                    loaded.row.commands_refused += 1;
                    let usage = command.usage.clone();
                    log_stdout(&loaded.row.id, &mut loaded.host);
                    return refuse(host, &usage, format!("the Java command panicked: {panic}"));
                }
            };
            match &run.outcome {
                CommandOutcome::Refused(_) => loaded.row.commands_refused += 1,
                _ => loaded.row.commands_ran += 1,
            }
            log_stdout(&loaded.row.id, &mut loaded.host);
            deliver(
                host,
                &loaded.row.id,
                &command.usage,
                run.outcome.clone(),
                &run.messages,
            );
            return Flow::Cancel;
        }
        log::warn!(
            "{LOG}: /{} was not registered by a Java jar",
            event.command.literals.join(" ")
        );
        Flow::Continue
    }

    /// The authoritative server tick's start and end (Fabric's `ServerTickEvents.START_SERVER_TICK`
    /// and `END_SERVER_TICK`, NeoForge's `ServerTickEvent.Pre` and `.Post`).
    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        let game = match event.phase {
            ServerTickPhase::ServerTickStart { .. } => GameEvent::ServerTickStart,
            ServerTickPhase::ServerTickEnd { .. } => GameEvent::ServerTickEnd,
            // A phase this build does not know: nothing to fire rather than a guess.
            _ => return,
        };
        let tick_count = event.context.tick.get().min(i32::MAX as u64) as i32;
        self.lock().tick_count = tick_count;
        let dedicated = event.context.authority == Authority::DedicatedServer;
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            game,
            dedicated,
            EventContext::default(),
            |_| true,
        );
    }

    /// A server level's tick starts (Fabric's `START_LEVEL_TICK`, NeoForge's
    /// `LevelTickEvent.Pre`).
    fn level_tick_pre(&self, host: HostSide<'_>, event: &mut LevelTickPreEvent) {
        if let HostSide::Server(host) = host {
            // The level's clocks are what `level.getGameTime()`/`getDayTime()` answer, so they
            // are refreshed from the level the hook names, before the listeners run.
            self.refresh_level(host, event.level);
        }
        let context = EventContext {
            level: Some(event.level.dimension_id().to_owned()),
            ..EventContext::default()
        };
        self.fire_event(GameEvent::LevelTickStart, self.dedicated(), context, |_| {
            true
        });
    }

    /// A server level's tick ends (Fabric's `END_LEVEL_TICK`, NeoForge's
    /// `LevelTickEvent.Post`).
    fn level_tick_post(&self, _host: HostSide<'_>, event: &mut LevelTickPostEvent) {
        let context = EventContext {
            level: Some(event.level.dimension_id().to_owned()),
            ..EventContext::default()
        };
        self.fire_event(GameEvent::LevelTickEnd, self.dedicated(), context, |_| true);
    }

    /// A server level opened (Fabric's `ServerLevelEvents.LOAD`, NeoForge's `LevelEvent.Load`).
    fn level_load(&self, host: &mut dyn ServerHost, event: &mut LevelLoadEvent<'_>) {
        self.refresh_levels(host);
        // The world folder and the data-pack registries may only be complete once a level
        // is open; a fact that did not change keeps its `Arc`.
        let facts = server_facts::server_facts(host);
        let mut state = self.lock();
        if *state.server != facts {
            log::info!(
                "{LOG}: server facts at level load: {}",
                server_facts::describe(&facts)
            );
            state.server = Arc::new(facts);
        }
        drop(state);
        let context = EventContext {
            level: Some(event.dimension.to_owned()),
            ..EventContext::default()
        };
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::LevelLoad,
            self.dedicated(),
            context,
            |_| true,
        );
    }

    /// A server level closed (Fabric's `ServerLevelEvents.UNLOAD`, NeoForge's
    /// `LevelEvent.Unload`).
    fn level_unload(&self, host: &mut dyn ServerHost, event: &mut LevelUnloadEvent<'_>) {
        self.lock()
            .levels
            .retain(|level| level.id != event.dimension);
        let context = EventContext {
            level: Some(event.dimension.to_owned()),
            ..EventContext::default()
        };
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::LevelUnload,
            self.dedicated(),
            context,
            |_| true,
        );
    }

    /// A player joined (Fabric's `ServerPlayConnectionEvents.JOIN` and
    /// `ServerPlayerEvents.JOIN`, NeoForge's `PlayerLoggedInEvent`).
    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        // The hook names the joining player; the host knows every player's level and entity id.
        // A level it cannot name stays `None`, never a guessed overworld.
        self.refresh_players(host);
        let context = EventContext {
            player: Some(event.player),
            ..EventContext::default()
        };
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::Joined(event.player),
            GameEvent::PlayerJoin,
            self.dedicated(),
            context,
            |_| true,
        );
    }

    /// A player left (Fabric's `ServerPlayConnectionEvents.DISCONNECT` and
    /// `ServerPlayerEvents.LEAVE`, NeoForge's `PlayerLoggedOutEvent`). The player is already off
    /// the list when a listener hears it, as the hook says.
    fn player_logged_out(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedOutEvent<'_>) {
        self.refresh_players(host);
        let context = EventContext {
            player: Some(event.player),
            ..EventContext::default()
        };
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::Left(event.player),
            GameEvent::PlayerLeave,
            self.dedicated(),
            context,
            |_| true,
        );
    }

    /// A player respawned (Fabric's `ServerPlayerEvents.AFTER_RESPAWN`, NeoForge's
    /// `PlayerRespawnEvent`).
    fn player_respawned(&self, host: &mut dyn ServerHost, event: &mut PlayerRespawnedEvent<'_>) {
        let context = EventContext {
            player: Some(event.player),
            ..EventContext::default()
        };
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::Resynced(event.player),
            GameEvent::PlayerRespawn,
            self.dedicated(),
            context,
            |_| true,
        );
    }

    /// A player's data was copied to their new player (Fabric's `COPY_FROM`, NeoForge's
    /// `PlayerEvent.Clone`); `was_death` says a death caused it.
    fn player_clone(&self, host: &mut dyn ServerHost, event: &mut PlayerCloneEvent<'_>) {
        let context = player_clone_context(event);
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::Cloned(event.player, event.was_death),
            GameEvent::PlayerCopy,
            self.dedicated(),
            context,
            |_| true,
        );
    }

    /// A player moved to another dimension (Fabric's `AFTER_PLAYER_CHANGE_LEVEL`, NeoForge's
    /// `PlayerChangedDimensionEvent`).
    fn player_changed_dimension(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PlayerChangedDimensionEvent<'_>,
    ) {
        self.refresh_players(host);
        let context = EventContext {
            player: Some(event.player),
            origin: Some(event.from.to_owned()),
            level: Some(event.to.to_owned()),
            ..EventContext::default()
        };
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::Resynced(event.player),
            GameEvent::PlayerChangeLevel,
            self.dedicated(),
            context,
            |_| true,
        );
    }

    /// A living entity took damage (Fabric's `ServerLivingEntityEvents.AFTER_DAMAGE`, NeoForge's
    /// `LivingDamageEvent.Post`).
    ///
    /// Fabric fires `AFTER_DAMAGE` only while the entity is not dying (`LivingEntityMixin`
    /// checks `isDeadOrDying()` before the call), but this hook does not carry the victim's
    /// remaining health, so a killing hit cannot be told from a surviving one here. Such a hit
    /// is fired with its real amounts rather than dropped on a guess: the departure is named in
    /// the mod's report documentation.
    fn living_damage_post(
        &self,
        _level: &mut dyn LevelRandomHost,
        event: &mut LivingDamagePostEvent<'_>,
    ) {
        let context = living_damage_context(event);
        self.fire_event(
            GameEvent::LivingAfterDamage,
            self.dedicated(),
            context,
            |_| true,
        );
    }

    /// An entity left a server level (Fabric's `ServerEntityEvents.ENTITY_UNLOAD`, NeoForge's
    /// `EntityLeaveLevelEvent`).
    fn entity_leave_level(&self, host: &mut dyn ServerHost, event: &mut EntityLeaveLevelEvent<'_>) {
        let context = EventContext {
            level: Some(event.dimension.to_owned()),
            entity: Some(FacadeEntity {
                id: event.entity as i32,
                kind: event.kind.to_owned(),
                level: Some(event.dimension.to_owned()),
            }),
            ..EventContext::default()
        };
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::EntityUnload,
            self.dedicated(),
            context,
            |_| true,
        );
    }

    /// A player broke the block at `pos` (Fabric's `PlayerBlockBreakEvents.BEFORE`, NeoForge's
    /// `BreakBlockEvent`). A cancel keeps the block, and the Fabric `CANCELED` listeners hear it.
    fn break_block(&self, host: &mut dyn ServerHost, event: &mut BreakBlockEvent<'_>) -> Flow {
        let context = self.block_context(host, event.pos, event.state, None, None);
        let fired = self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::BlockBreakBefore,
            self.dedicated(),
            context.clone(),
            |_| true,
        );
        if fired.cancelled {
            self.fire_bridged(
                Some(&mut *host),
                Lifecycle::None,
                GameEvent::BlockBreakCanceled,
                self.dedicated(),
                context,
                |_| true,
            );
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// A player left-clicked a block (Fabric's `AttackBlockCallback`, NeoForge's
    /// `PlayerInteractEvent.LeftClickBlock`).
    fn left_click_block(&self, host: &mut dyn ServerHost, event: &mut LeftClickBlockEvent) -> Flow {
        let context = self.block_context(host, event.pos, event.state, Some(event.face), None);
        let fired = self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::AttackBlock,
            self.dedicated(),
            context,
            |_| true,
        );
        if fired.cancelled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// A player right-clicked a block (Fabric's `UseBlockCallback`, NeoForge's
    /// `PlayerInteractEvent.RightClickBlock`). The Java callback's `InteractionResult` becomes the
    /// cancellation result, and a NeoForge `setCanceled(true)` keeps the default `PASS`.
    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        if !matches!(host, HostSide::Server(_)) {
            return Flow::Continue;
        }
        let click = event.click;
        let context = EventContext {
            level: Some(dimension_id(click.dimension).to_owned()),
            player: Some(click.player),
            pos: Some(click.pos),
            face: Some(click.hit.face),
            off_hand: Some(click.hand == Hand::Off),
            ..EventContext::default()
        };
        let fired = self.fire_event(GameEvent::UseBlock, self.dedicated(), context, |_| true);
        if fired.cancelled {
            *event.cancellation_result = interaction_result(fired.interaction.as_deref());
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// A player used the item in their hand (Fabric's `UseItemCallback`, NeoForge's
    /// `PlayerInteractEvent.RightClickItem`).
    fn right_click_item(&self, host: HostSide<'_>, event: &mut RightClickItemEvent<'_>) -> Flow {
        let HostSide::Server(host) = host else {
            return Flow::Continue;
        };
        let click = event.click;
        // `player.getItemInHand(hand)` is the hook's own stack; the other hand and the game
        // mode are read from the host for this firing only.
        let held = host
            .player(click.player)
            .and_then(|player| server_facts::held_items(host, player, click.hand, click.stack));
        let context = EventContext {
            level: Some(dimension_id(click.dimension).to_owned()),
            player: Some(click.player),
            pos: Some(click.pos),
            off_hand: Some(click.hand == Hand::Off),
            held,
            ..EventContext::default()
        };
        let fired = self.fire_event(GameEvent::UseItem, self.dedicated(), context, |_| true);
        if fired.cancelled {
            *event.cancellation_result = interaction_result(fired.interaction.as_deref());
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// A player attacked an entity (Fabric's `AttackEntityCallback`, NeoForge's
    /// `AttackEntityEvent`).
    fn attack_entity(&self, host: &mut dyn ServerHost, event: &mut AttackEntityEvent<'_>) -> Flow {
        let context = EventContext {
            player: self.acting_player(host),
            entity: Some(FacadeEntity {
                id: event.entity,
                kind: event.kind.to_owned(),
                level: None,
            }),
            ..EventContext::default()
        };
        let fired = self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::AttackEntity,
            self.dedicated(),
            context,
            |_| true,
        );
        if fired.cancelled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// A player right-clicked an entity (Fabric's `UseEntityCallback`, NeoForge's
    /// `PlayerInteractEvent.EntityInteract`).
    fn entity_interact(
        &self,
        host: &mut dyn ServerHost,
        event: &mut EntityInteractEvent<'_>,
    ) -> Flow {
        let context = EventContext {
            player: self.acting_player(host),
            // The hook fires once per hand with that hand's stack (Fabric's `UseEntityCallback`
            // carries the `InteractionHand`); the pin's event names the stack, not the hand, so
            // which of the acting player's hands holds it is what tells them apart.
            off_hand: self
                .acting_hand(host, event.stack)
                .map(|hand| hand == Hand::Off),
            entity: Some(FacadeEntity {
                id: event.entity,
                kind: event.kind.to_owned(),
                level: None,
            }),
            ..EventContext::default()
        };
        let fired = self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::UseEntity,
            self.dedicated(),
            context,
            |_| true,
        );
        if fired.cancelled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// A living entity's death has finished (Fabric's `ServerLivingEntityEvents.AFTER_DEATH`): the
    /// loot, experience and death message went out. The runtime's `living_after_death` event; a
    /// death `living_death` cancelled never reaches it.
    fn living_death_post(&self, host: &mut dyn ServerHost, event: &mut LivingDeathPostEvent<'_>) {
        let context = EventContext {
            entity: Some(FacadeEntity {
                id: event.entity as i32,
                // The hook names the dead entity and the killing blow, not its type.
                kind: String::new(),
                level: None,
            }),
            damage: Some(event.damage_type.to_owned()),
            ..EventContext::default()
        };
        self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::LivingAfterDeath,
            self.dedicated(),
            context,
            |_| true,
        );
    }

    /// A living entity is about to die (Fabric's `ServerLivingEntityEvents.ALLOW_DEATH`, NeoForge's
    /// `LivingDeathEvent`).
    fn living_death(&self, host: &mut dyn ServerHost, event: &mut LivingDeathEvent<'_>) -> Flow {
        let context = EventContext {
            entity: Some(FacadeEntity {
                id: event.entity as i32,
                kind: event.kind.to_owned(),
                level: None,
            }),
            damage: Some(event.damage_type.to_owned()),
            ..EventContext::default()
        };
        let fired = self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::LivingAllowDeath,
            self.dedicated(),
            context,
            |_| true,
        );
        if fired.cancelled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// A living entity is about to be hurt (Fabric's `ServerLivingEntityEvents.ALLOW_DAMAGE`,
    /// NeoForge's `LivingIncomingDamageEvent`).
    fn living_incoming_damage(
        &self,
        _level: &mut dyn LevelRandomHost,
        event: &mut LivingIncomingDamageEvent<'_>,
    ) -> Flow {
        let context = EventContext {
            entity: Some(FacadeEntity {
                id: event.entity,
                kind: event.kind.to_owned(),
                level: None,
            }),
            damage: Some(event.source.damage_type.to_owned()),
            amount: event.container.new_damage(),
            flag: event.container.blocked() > 0.0,
            ..EventContext::default()
        };
        let fired = self.fire_event(
            GameEvent::LivingAllowDamage,
            self.dedicated(),
            context,
            |_| true,
        );
        if fired.cancelled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// An entity joined a server level (Fabric's `ServerEntityEvents.ENTITY_LOAD`, NeoForge's
    /// `EntityJoinLevelEvent`). A cancel keeps it out.
    fn entity_join_level(
        &self,
        host: &mut dyn ServerHost,
        event: &mut EntityJoinLevelEvent<'_>,
    ) -> Flow {
        let context = EventContext {
            level: Some(event.dimension.to_owned()),
            entity: Some(FacadeEntity {
                id: event.entity as i32,
                kind: event.kind.to_owned(),
                level: Some(event.dimension.to_owned()),
            }),
            flag: event.loaded_from_disk,
            ..EventContext::default()
        };
        let fired = self.fire_bridged(
            Some(&mut *host),
            Lifecycle::None,
            GameEvent::EntityLoad,
            self.dedicated(),
            context,
            |_| true,
        );
        if fired.cancelled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// The client finished its first load (NeoForge's `FMLClientSetupEvent`), once per process.
    /// A dedicated server never fires it. No client jar runs yet, so the hook logs the moment and
    /// nothing else.
    fn client_setup(&self, _host: &mut dyn ClientHost, _event: &mut ClientSetupEvent) {
        log::info!("{LOG} {}: client setup", env!("CARGO_PKG_VERSION"));
    }
}

impl HorizonJava {
    /// Which hand the acting player used: the event's `stack` is the held stack of that hand, so
    /// it matches exactly one of the player's two hands. `None` when it matches both or neither —
    /// two identical stacks, the common both-empty case included — because the hook does not carry
    /// the hand (`arismoko/horizon#2797`) and a guess here would be a confident wrong answer.
    fn acting_hand(&self, host: &dyn ServerHost, stack: Option<&ItemStack>) -> Option<Hand> {
        let player = host.players().acting_player()?;
        let living = host.living(player.entity())?;
        let main = living.hand_item(host, Hand::Main);
        let off = living.hand_item(host, Hand::Off);
        one_hand(stack, main.as_ref(), off.as_ref())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Runs the jars if the content phase has not done so yet.
    fn ensure_jars(&self) {
        use mod_api::__private::game::RegistrationContextNew;
        self.lock()
            .ensure_loaded(&mod_api::RegistrationContext::new());
    }

    /// Fires one runtime event with its context to every jar's listeners and returns what they
    /// decided. One firing is shared by every jar, so a cancellation or an `InteractionResult`
    /// carries from one jar's listener to the next, as it does between Fabric listeners.
    ///
    /// The tick events fire twenty times a second, so their line is written only when a listener
    /// failed or leaned on an unmodelled host call; the report counts every callback either way.
    fn fire_event(
        &self,
        event: GameEvent,
        dedicated: bool,
        context: EventContext,
        wanted: impl Fn(&GameListener) -> bool,
    ) -> Fired {
        self.fire_bridged(None, Lifecycle::None, event, dedicated, context, wanted)
    }

    /// [`Self::fire_event`] from a hook that has a `ServerHost`: each jar with attachment types
    /// the bridge runs gets a batch around its listeners (`attachments.rs`), and `lifecycle` is
    /// the attachment step NeoForge takes at this moment, before the listeners run.
    fn fire_bridged(
        &self,
        mut server: Option<&mut dyn ServerHost>,
        lifecycle: Lifecycle,
        event: GameEvent,
        dedicated: bool,
        context: EventContext,
        wanted: impl Fn(&GameListener) -> bool,
    ) -> Fired {
        let mut state = self.lock();
        let mut facade = state.facade();
        facade.dedicated = dedicated;
        let mut firing = EventFiring::new(event, Arc::new(facade), context);
        // Every jar's batch opens first: the listeners then run as one event, in the loaders'
        // order across every jar (a NeoForge priority or a Fabric phase is global, as one bus or
        // one `Event` field is in a real game), and the batches close after.
        let mut opened = Vec::with_capacity(state.jars.len());
        let mut before = Vec::with_capacity(state.jars.len());
        for loaded in &mut state.jars {
            // Every jar's host hears about the level this firing names, whether or not it has a
            // listener for the event: `MinecraftServer.getAllLevels()` answers from what the
            // game announced, and a jar that only listens for `server_started` must still see
            // the levels loaded before it.
            loaded.host.note_firing_levels(&firing);
            opened.push(server.as_deref_mut().and_then(|server| {
                attachments::open(
                    server,
                    &loaded.row.id,
                    &loaded.jar,
                    &mut loaded.host,
                    lifecycle,
                )
            }));
            before.push(loaded.host.unknown_host_call_count());
        }
        let order = listener_order(&state.jars, &firing, &wanted);
        let counts = run_listeners(&mut state.jars, &order, &mut firing);
        let (mut ran, mut failed, mut unknown) = (0, 0, 0);
        for (((loaded, opened), before), (jar_ran, jar_failed)) in
            state.jars.iter_mut().zip(opened).zip(before).zip(counts)
        {
            let id = loaded.row.id.clone();
            match server.as_deref_mut() {
                Some(server) => attachments::close(
                    server,
                    &id,
                    &loaded.jar,
                    &mut loaded.host,
                    opened,
                    lifecycle,
                ),
                None => {
                    if let Lifecycle::Left(player) = lifecycle {
                        loaded.host.attachment_player_left(player);
                    }
                }
            }
            let jar_unknown = loaded.host.unknown_host_call_count().saturating_sub(before);
            log_stdout(&id, &mut loaded.host);
            loaded.row.callbacks_ran += jar_ran;
            loaded.row.callbacks_failed += jar_failed;
            loaded.row.callback_unknown_host_calls += jar_unknown;
            ran += jar_ran;
            failed += jar_failed;
            unknown += jar_unknown;
        }
        let quiet = matches!(
            event,
            GameEvent::ServerTickStart
                | GameEvent::ServerTickEnd
                | GameEvent::LevelTickStart
                | GameEvent::LevelTickEnd
        );
        if !quiet || failed > 0 || unknown > 0 {
            log::info!(
                "{LOG}: {} (dedicated={dedicated}): {ran} callback(s) ran, {failed} failed, {unknown} unknown host call(s)",
                event.name()
            );
        }
        Fired {
            cancelled: firing.cancelled,
            interaction: firing.interaction.clone(),
        }
    }

    /// Whether this server is a dedicated one.
    fn dedicated(&self) -> bool {
        self.lock().dedicated
    }

    /// Rebuilds the online player list from the host: every player the server has, with the
    /// name, level and entity id the host knows. A level the host cannot name is `None`, so the
    /// façade's `level()` refuses it (and counts the read) instead of answering a guess.
    fn refresh_players(&self, host: &dyn ServerHost) {
        let players: Vec<FacadePlayer> = host
            .players()
            .server_players()
            .into_iter()
            .map(|player| FacadePlayer {
                uuid: player.uuid(),
                name: player.name(host).unwrap_or_default(),
                level: player
                    .entity()
                    .level(host)
                    .map(|level| level.dimension_id().to_owned()),
                entity_id: Some(player.entity().id() as i32),
            })
            .collect();
        self.lock().players = players;
    }

    /// Rebuilds the open level list from the host, with each level's clocks. A level whose
    /// clocks the host does not give is left out: a level in `getAllLevels()` with a made-up
    /// time would answer `getGameTime()` with a value nobody read.
    fn refresh_levels(&self, host: &dyn ServerHost) {
        let levels: Vec<FacadeLevel> = host
            .levels()
            .list_levels()
            .into_iter()
            .filter_map(|level| level_entry(host, level))
            .collect();
        self.lock().levels = levels;
    }

    /// Refreshes one open level's clocks, after its tick or after it opened.
    fn refresh_level(&self, host: &dyn ServerHost, level: LevelRef) {
        let Some(entry) = level_entry(host, level) else {
            self.lock()
                .levels
                .retain(|known| known.id != level.dimension_id());
            return;
        };
        let mut state = self.lock();
        match state.levels.iter_mut().find(|known| known.id == entry.id) {
            Some(known) => *known = entry,
            None => state.levels.push(entry),
        }
    }

    /// The player whose action a hook hears, as the mod API's own helpers name them.
    fn acting_player(&self, host: &dyn ServerHost) -> Option<u128> {
        host.players().acting_player().map(|player| player.uuid())
    }

    /// The firing context for a block a player acted on: the acting player, the position, the
    /// block's id (the runtime's block-state façade names it), and the face when the hook gives
    /// one.
    fn block_context(
        &self,
        host: &dyn ServerHost,
        pos: [i32; 3],
        state: mod_api::BlockState,
        face: Option<[i32; 3]>,
        off_hand: Option<bool>,
    ) -> EventContext {
        let block = host
            .block_state_string(state)
            .map(|state| state.split('[').next().unwrap_or_default().to_owned());
        EventContext {
            player: self.acting_player(host),
            pos: Some(pos),
            block,
            face,
            off_hand,
            ..EventContext::default()
        }
    }
}

/// The hand `held` names among the two, or `None` when it names both or neither: the hook's
/// stack is the acting hand's, so exactly one hand holding it is the only conclusive answer.
fn one_hand<T: PartialEq>(held: Option<&T>, main: Option<&T>, off: Option<&T>) -> Option<Hand> {
    let holds = |hand: Option<&T>| match (held, hand) {
        (Some(held), Some(hand)) => held == hand,
        (Some(_), None) => false,
        (None, hand) => hand.is_none(),
    };
    match (holds(main), holds(off)) {
        (true, false) => Some(Hand::Main),
        (false, true) => Some(Hand::Off),
        _ => None,
    }
}

/// The dimension id of a hook's `LevelDimension`.
fn dimension_id(dimension: LevelDimension) -> &'static str {
    dimension.key().id()
}

/// Fabric's `COPY_FROM` `alive` argument, from the hook's `was_death`.
///
/// The bytecode is the specification. `ServerPlayerMixin.onCopyFrom` passes its `alive`
/// parameter straight through (`iload_2`) to `ServerPlayerEvents.CopyFrom.copyFromPlayer`, and
/// that parameter is vanilla's `ServerPlayer.restoreFrom(ServerPlayer, boolean
/// keepAllPlayerData)` (`@Inject(method = "restoreFrom", at = @At("TAIL"))`). The one caller,
/// `PlayerList.respawn`, passes `false` for the death respawn (`ServerGamePacketListenerImpl`
/// `PERFORM_RESPAWN` with health <= 0) and `true` when the player leaves the End after the
/// credits. So `alive` is true after an End return and false after a death: the opposite of
/// the hook's `was_death`.
fn copy_from_alive(was_death: bool) -> bool {
    !was_death
}

/// The `COPY_FROM`/`AFTER_RESPAWN` firing context for one player clone.
fn player_clone_context(event: &PlayerCloneEvent<'_>) -> EventContext {
    EventContext {
        player: Some(event.player),
        flag: copy_from_alive(event.was_death),
        ..EventContext::default()
    }
}

/// The `AFTER_DAMAGE` amounts and block flag, from the hit the game reports.
///
/// Returns `(baseDamageTaken, damageTaken, blocked)`. Fabric's
/// `(LivingEntity, DamageSource, float baseDamageTaken, float damageTaken, boolean blocked)`
/// is filled by `LivingEntityMixin.afterDamage` from `hurtServer`'s locals: `baseDamageTaken`
/// is the `originalDamage` local (`damage` as it came in, before blocking and the reductions)
/// and `damageTaken` is `hurtServer`'s `damage` at its `TAIL`, the amount `actuallyHurt` was
/// given (after blocking and the armor/enchantment/Resistance steps, before absorption). The
/// game's damage sequence names those two `original()` and `inflicted()`; `health_damage()` is
/// what the health actually lost after absorption, which is a third, later value.
fn after_damage_arguments(original: f32, inflicted: f32, blocked: f32) -> (f32, f32, bool) {
    (original, inflicted, blocked > 0.0)
}

/// The `AFTER_DAMAGE` firing context for one hit.
fn living_damage_context(event: &LivingDamagePostEvent<'_>) -> EventContext {
    let container = event.container;
    let (base, taken, blocked) = after_damage_arguments(
        container.original(),
        container.inflicted(),
        container.blocked(),
    );
    EventContext {
        entity: Some(FacadeEntity {
            id: event.entity,
            kind: event.kind.to_owned(),
            level: None,
        }),
        damage: Some(event.source.damage_type.to_owned()),
        amount: taken,
        base_amount: Some(base),
        flag: blocked,
        ..EventContext::default()
    }
}

/// The `InteractionResult` a Fabric listener returned, as the mod API's own result. A NeoForge
/// listener that cancelled without a result keeps `PASS`, the default the hook documents.
fn interaction_result(result: Option<&str>) -> InteractionResult {
    match result {
        Some("SUCCESS") => InteractionResult::Success,
        Some("SUCCESS_SERVER") => InteractionResult::SuccessServer,
        Some("CONSUME") => InteractionResult::Consume,
        Some("FAIL") => InteractionResult::Fail,
        _ => InteractionResult::Pass,
    }
}

/// The `(jar, slot)` listeners of `firing`'s event that `wanted` keeps, in the order the loaders
/// run them ([`dispatch_order`]): Fabric phases (with every jar's `addPhaseOrdering`), then
/// NeoForge/Forge `EventPriority`, then the jars' load order and each jar's registration order.
fn listener_order(
    jars: &[LoadedJar],
    firing: &EventFiring,
    wanted: &dyn Fn(&GameListener) -> bool,
) -> Vec<(usize, usize)> {
    let entries: Vec<(usize, usize, &GameListener)> = jars
        .iter()
        .enumerate()
        .flat_map(|(at, loaded)| {
            loaded
                .listeners
                .iter()
                .enumerate()
                .map(move |(slot, listener)| (at, slot, listener))
        })
        .filter(|(_, _, listener)| listener.event == firing.event && wanted(listener))
        .collect();
    let orderings: Vec<&PhaseOrdering> = jars.iter().flat_map(|jar| &jar.orderings).collect();
    dispatch_order(&entries, &orderings)
}

/// Runs the `(jar, slot)` listeners in `order` against one shared firing, each on its own jar's
/// host, and answers each jar's `(ran, failed)`. A listener that decides the event (a Fabric
/// `boolean` or `InteractionResult` invoker's answer) ends the firing for every jar.
fn run_listeners(
    jars: &mut [LoadedJar],
    order: &[(usize, usize)],
    firing: &mut EventFiring,
) -> Vec<(usize, usize)> {
    let mut counts = vec![(0, 0); jars.len()];
    let event = firing.event;
    for &(at, slot) in order {
        let LoadedJar {
            row,
            jar,
            host,
            listeners,
            ..
        } = &mut jars[at];
        let (id, listener) = (&row.id, &listeners[slot]);
        let outcome = caught(&format!("{} listener from {}", id, listener.source), || {
            jar.run_game_listener(listener, firing, host, &ExecutionBudget::within(JAR_BUDGET))
        });
        match outcome {
            Ok(Ok(run)) => {
                counts[at].0 += usize::from(run.ran);
                if run.stop {
                    break;
                }
            }
            Ok(Err(error)) => {
                counts[at].1 += 1;
                log::warn!(
                    "{LOG}: {id}: {} listener from {} failed: {error}",
                    event.name(),
                    listener.source
                );
            }
            Err(panic) => {
                counts[at].1 += 1;
                log::warn!(
                    "{LOG}: {id}: {} listener from {} failed: panicked: {panic}",
                    event.name(),
                    listener.source
                );
            }
        }
    }
    counts
}

/// Runs one Java callback, turning a panic into a named failure.
///
/// The interpreter can panic on a value a jar hands it (`Integer.parseInt(s, 40)` on an
/// unchecked radix, a slice index from malformed bytecode). A Java callback must fail, be counted
/// in the report and leave the game running: never take the process down. `what` names the
/// callback (entrypoint, listener or command) in the log line.
fn caught<T>(what: &str, run: impl FnOnce() -> T) -> Result<T, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
        Ok(value) => Ok(value),
        Err(payload) => {
            let reason = panic_message(payload.as_ref());
            log::warn!("{LOG}: {what} panicked: {reason}");
            Err(reason)
        }
    }
}

/// A panic payload as text: the message `panic!` carried, or a note that it carried none.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "panic with a non-string payload".to_owned()
    }
}

/// Adds one command-registration pass to the jar's row: listeners that ran or failed, usages
/// translated or left out, and the unmodelled host calls each phase made.
fn count_registration(
    row: &mut JarRow,
    listeners: (usize, usize),
    listener_unknown: usize,
    translated: usize,
    refused: usize,
    translation_unknown: usize,
) {
    row.callbacks_ran += listeners.0;
    row.callbacks_failed += listeners.1;
    row.callback_unknown_host_calls += listener_unknown;
    row.commands_translated += translated;
    row.commands_refused_at_registration += refused;
    row.command_unknown_host_calls += translation_unknown;
}

/// Sends the jar's `System.out` to the game log, one line per entry, with the jar's id.
fn log_stdout(id: &str, host: &mut StdHost) {
    for line in host.take_stdout().lines() {
        if !line.trim().is_empty() {
            log::info!("{LOG}: {id}: {}", line.trim_end());
        }
    }
}

/// Answers a command the bridge could not map with the jar's own failure text.
fn refuse(host: &mut dyn ServerHost, usage: &str, reason: String) -> Flow {
    log::warn!("{LOG}: /{usage} not run: {reason}");
    let _ = host.commands().feedback(Text::literal(format!(
        "java command /{usage} could not run: {reason}"
    )));
    Flow::Cancel
}

/// Puts a finished Java command's replies where a mod command's feedback goes: the originating
/// source for `sendSuccess`/`sendFailure`/`sendSystemMessage`, a named player for a direct
/// `ServerPlayer.sendSystemMessage`.
fn deliver(
    host: &mut dyn ServerHost,
    id: &str,
    usage: &str,
    outcome: CommandOutcome,
    messages: &[CommandMessage],
) {
    for message in messages {
        let text = mod_text(&message.text);
        if let MessageKind::Player(uuid) = message.kind
            && let Some(player) = host.player(uuid)
        {
            let _ = player.tell(host, text);
            continue;
        }
        let _ = host.commands().feedback(text);
    }
    match outcome {
        CommandOutcome::Returned(result) => log::info!(
            "{LOG}: {id}: /{usage} ran (returned {result}, {} message(s))",
            messages.len()
        ),
        CommandOutcome::Syntax(text) => {
            let _ = host.commands().feedback(mod_text(&text));
            log::warn!("{LOG}: {id}: /{usage} had a syntax error");
        }
        CommandOutcome::Refused(reason) => {
            log::warn!("{LOG}: {id}: /{usage} refused: {reason}");
            let _ = host
                .commands()
                .feedback(Text::translate("command.failed", Vec::new()));
        }
    }
}

/// A command text as the mod API's component. A translation key stays a key; styled component
/// JSON is flattened to its plain text, which the runtime already carries beside it.
fn mod_text(text: &CommandText) -> Text {
    if text.translate {
        Text::translate(&text.text, Vec::new())
    } else {
        Text::literal(text.text.clone())
    }
}

/// One open level as the façade carries it, or `None` when the host does not give both clocks.
fn level_entry(host: &dyn Host, level: LevelRef) -> Option<FacadeLevel> {
    Some(FacadeLevel {
        id: level.dimension_id().to_owned(),
        game_time: level.game_time(host)?,
        day_time: level.day_time(host)?,
        // `getWorldBorder()`'s bounds, refreshed with the clocks on every level tick, so a
        // border the server moves is read as it is now.
        world_border: level.world_border(host),
    })
}

/// The runtime's view of the command source.
///
/// Every source's level is the fact the mod API names, and unknown when it names none. A
/// command block carries its own dimension (`CommandSource::CommandBlock.level`), so a block in
/// the Nether answers `getLevel()` with the Nether. The console's is the overworld, which is
/// vanilla's console source level; a minecart's and a function's level is not in the hook and
/// stays unknown (a `getLevel()` read is counted, never answered with the overworld). A
/// player's source is `player.createCommandSourceStack()`, whose level is `player.level()`:
/// the level the host names for the player's entity, or unknown when it names none.
fn runtime_source(source: &CommandSource, host: &dyn ServerHost) -> RuntimeSource {
    let level = source.permission().level();
    let levels = host.levels().list_levels();
    let dimension_of = |key: DimensionKey| {
        levels
            .iter()
            .find(|level| level.dimension() == key)
            .map(|level| level.dimension_id().to_owned())
    };
    match source {
        CommandSource::Player { player, .. } => RuntimeSource {
            name: player.name(host).unwrap_or_else(|| "Player".to_owned()),
            level,
            player: Some(player.uuid()),
            dimension: player
                .entity()
                .level(host)
                .map(|level| level.dimension_id().to_owned()),
        },
        _ => RuntimeSource {
            name: "Server".to_owned(),
            level,
            player: None,
            dimension: source_dimension(source, dimension_of),
        },
    }
}

/// The dimension id a source's `getLevel()` answers, or `None` when the hook does not name it.
fn source_dimension(
    source: &CommandSource,
    dimension: impl Fn(DimensionKey) -> Option<String>,
) -> Option<String> {
    match source {
        // Vanilla's console source is the overworld's (MinecraftServer.overworld()).
        CommandSource::Console { .. } => Some("minecraft:overworld".to_owned()),
        CommandSource::CommandBlock { level, .. } => dimension(*level),
        // Neither carries a level in the hook; a player's is read from the host by
        // `runtime_source`, not from the source.
        CommandSource::Player { .. }
        | CommandSource::CommandBlockMinecart { .. }
        | CommandSource::Function { .. } => None,
        _ => None,
    }
}

/// Walks the Brigadier tree the jar's registration callback built and turns every executable
/// literal path into a [`Command`] and a [`JavaCommand`]. A node this bridge cannot carry is
/// left out with a log line naming it.
struct Translator<'a> {
    jar: &'a NativeJar,
    host: &'a mut StdHost,
    tree: brigadier::CommandTree,
    budget: ExecutionBudget,
    id: &'a str,
    native: Vec<Command>,
    commands: Vec<JavaCommand>,
    refused: usize,
}

fn translate_commands(
    jar: &NativeJar,
    host: &mut StdHost,
    id: &str,
) -> (Vec<Command>, Vec<JavaCommand>, usize) {
    let tree = host.command_tree();
    let mut translator = Translator {
        jar,
        host,
        tree,
        budget: ExecutionBudget::within(JAR_BUDGET),
        id,
        native: Vec::new(),
        commands: Vec::new(),
        refused: 0,
    };
    let roots: Vec<usize> = translator.tree.roots().map(|(id, _)| id).collect();
    for root in roots {
        translator.walk(root, &mut Vec::new(), &mut Vec::new(), 0);
    }
    let Translator {
        native,
        commands,
        refused,
        ..
    } = translator;
    (native, commands, refused)
}

impl Translator<'_> {
    fn walk(
        &mut self,
        node_id: usize,
        path: &mut Vec<String>,
        arguments: &mut Vec<(String, JavaArgument)>,
        level: u8,
    ) {
        let Some(node) = self.tree.nodes.get(node_id).cloned() else {
            return;
        };
        let mut level = level;
        if let Some(requirement) = &node.requirement {
            match requirement {
                Requirement::Level(node_level) => level = level.max((*node_level).min(4)),
                Requirement::Predicate(_) => {
                    match caught(
                        &format!(
                            "{} permission predicate for /{}",
                            self.id,
                            usage(path, arguments)
                        ),
                        || {
                            self.jar
                                .requirement_level(requirement, true, self.host, &self.budget)
                        },
                    )
                    .and_then(|answer| answer.map_err(|error| error.to_string()))
                    {
                        Ok(node_level) => level = level.max(node_level.min(4)),
                        Err(error) => {
                            log::warn!(
                                "{LOG}: {}: /{} needs a permission predicate this bridge cannot evaluate ({error}); the usage is left out",
                                self.id,
                                usage(path, arguments),
                            );
                            self.refused += 1;
                            return;
                        }
                    }
                }
            }
        }
        if node.redirect.is_some() || node.modifier {
            log::warn!(
                "{LOG}: {}: /{} redirects or forks its source; the usage is left out",
                self.id,
                usage(path, arguments),
            );
            self.refused += 1;
            return;
        }
        match &node.kind {
            NodeKind::Root => {
                let children = node.children.clone();
                for child in children {
                    self.walk(child, path, arguments, level);
                }
            }
            NodeKind::Literal(name) => {
                path.push(name.clone());
                if let Some(run) = node.command.clone() {
                    self.finish(run, path, arguments, level);
                }
                let children = node.children.clone();
                for child in children {
                    self.walk(child, path, arguments, level);
                }
                path.pop();
            }
            NodeKind::Argument { name, kind } => {
                let Some(kind) = JavaArgument::from_kind(kind) else {
                    log::warn!(
                        "{LOG}: {}: /{} takes a {} argument '{}', which this bridge does not carry; the usage is left out",
                        self.id,
                        usage(path, arguments),
                        JavaArgument::describe(kind),
                        name,
                    );
                    self.refused += 1;
                    return;
                };
                arguments.push((name.clone(), kind));
                if let Some(run) = node.command.clone() {
                    self.finish(run, path, arguments, level);
                }
                let children = node.children.clone();
                for child in children {
                    self.walk(child, path, arguments, level);
                }
                arguments.pop();
            }
        }
    }

    fn finish(
        &mut self,
        run: runtime::Value,
        path: &[String],
        arguments: &[(String, JavaArgument)],
        level: u8,
    ) {
        if path.is_empty() {
            return;
        }
        let usage = usage(path, arguments);
        let command = if level == 0 {
            Command::new(&usage)
        } else {
            Command::op(&usage, level)
        };
        self.native.push(command);
        self.commands.push(JavaCommand {
            usage,
            literals: path.to_vec(),
            arguments: arguments.to_vec(),
            run,
        });
    }
}

/// The usage spelling of one literal path and its arguments (`hello <name:word>`).
fn usage(path: &[String], arguments: &[(String, JavaArgument)]) -> String {
    let mut usage = path.join(" ");
    for (name, kind) in arguments {
        usage.push_str(&format!(" <{name}:{}>", kind.usage_kind()));
    }
    usage
}

impl State {
    /// The server state every firing this mod starts exposes: the dedicated flag, the tick, the
    /// online players and the open levels.
    ///
    /// `max_players` is deliberately not set: the pinned mod API has no `max-players` read, and a
    /// made-up 0 is worse than a refusal. #39 makes that member unknown (the runtime refuses it
    /// instead of answering the default); this façade carries no value either way.
    fn facade(&self) -> EventFacade {
        EventFacade {
            dedicated: self.dedicated,
            tick_count: self.tick_count,
            players: self.players.clone(),
            levels: self.levels.clone(),
            server: Arc::clone(&self.server),
            ..EventFacade::default()
        }
    }

    /// Every row the report carries, run jars first, in file-name order.
    fn rows(&self) -> impl Iterator<Item = &JarRow> {
        self.jars
            .iter()
            .map(|jar| &jar.row)
            .chain(self.refused.iter())
    }

    /// Counters are per server: the jars ran once at the content phase, and a second world this
    /// process hosts starts its own report with zero callbacks and commands.
    fn reset_server(&mut self) {
        for jar in &mut self.jars {
            jar.row.callbacks_ran = 0;
            jar.row.callbacks_failed = 0;
            jar.row.callback_unknown_host_calls = 0;
            jar.row.commands_ran = 0;
            jar.row.commands_refused = 0;
            jar.row.command_unknown_host_calls = 0;
            jar.row.commands_translated = 0;
            jar.row.commands_refused_at_registration = 0;
            // No player value carries into another server's world.
            jar.host.reset_attachment_values();
        }
    }
}

/// One jar's outcome, the row the report holds and the line the log gets.
struct JarRow {
    file: String,
    id: String,
    /// `ran-clean`, `ran-hollow`, `ran-partial` or `refused`.
    verdict: &'static str,
    host_calls: usize,
    unknown_host_calls: usize,
    /// Why a jar was refused or ran partially; `None` for a full run.
    reason: Option<String>,
    /// Server lifecycle callbacks that ran and failed, and the unmodelled host calls they made.
    callbacks_ran: usize,
    callbacks_failed: usize,
    callback_unknown_host_calls: usize,
    /// Commands that ran or were refused, and the unmodelled host calls their bodies made (a
    /// player source's `getLevel()`, for one).
    commands_ran: usize,
    commands_refused: usize,
    command_unknown_host_calls: usize,
    /// Usages the registration callback built and this bridge translated, and usages it left out
    /// (an argument kind or predicate it cannot carry). Registration failures are callbacks:
    /// they are counted in `callbacks_failed` too.
    commands_translated: usize,
    commands_refused_at_registration: usize,
    /// Content declarations this jar contributed to the game (milestone 3). Zero for a refused
    /// jar, and for a jar whose registrations the content vocabulary cannot carry.
    content_blocks: usize,
    content_items: usize,
    /// Typed declarations in a namespace the mod does not own, by namespace: (blocks, items).
    /// The game's declaration fence would refuse them, so they are skipped; a baked content
    /// package of that namespace declares them instead.
    content_skipped: BTreeMap<String, (usize, usize)>,
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
            commands_ran: 0,
            commands_refused: 0,
            command_unknown_host_calls: 0,
            commands_translated: 0,
            commands_refused_at_registration: 0,
            content_blocks: 0,
            content_items: 0,
            content_skipped: BTreeMap::new(),
        }
    }

    /// The verdict plus the failures that happened after the entrypoints. The verdict itself is
    /// the entrypoint's (Fabric's and NeoForge's own loaders do not fail a load for a listener
    /// that throws); the summary is what the log line and the report show, so a jar with failing
    /// listeners never reads as an unqualified clean run.
    fn summary(&self) -> String {
        if self.callbacks_failed == 0 {
            return self.verdict.to_owned();
        }
        let failures = self.callbacks_failed;
        let noun = if failures == 1 {
            "listener failure"
        } else {
            "listener failures"
        };
        format!("{}, {failures} {noun}", self.verdict)
    }

    fn json(&self) -> serde_json::Value {
        json!({
            "file": self.file,
            "id": self.id,
            "verdict": self.verdict,
            // `verdict` is the entrypoint's; `summary` adds what happened while the jar ran.
            "summary": self.summary(),
            "host_calls": self.host_calls,
            "unknown_host_call_count": self.unknown_host_calls,
            "reason": self.reason,
            "callbacks": {
                "ran": self.callbacks_ran,
                "failed": self.callbacks_failed,
                "unknown_host_call_count": self.callback_unknown_host_calls,
            },
            "commands": {
                "ran": self.commands_ran,
                "refused": self.commands_refused,
                "unknown_host_call_count": self.command_unknown_host_calls,
                "translated": self.commands_translated,
                "refused_at_registration": self.commands_refused_at_registration,
            },
            "content": {
                "blocks": self.content_blocks,
                "items": self.content_items,
                "skipped": self.content_skipped.iter().map(|(namespace, (blocks, items))| {
                    json!({ "namespace": namespace, "blocks": blocks, "items": items })
                }).collect::<Vec<_>>(),
            },
        })
    }
}

impl std::fmt::Display for JarRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}) {} ({} host calls, {} unknown host calls",
            self.file,
            self.id,
            self.summary(),
            self.host_calls,
            self.unknown_host_calls
        )?;
        if let Some(reason) = &self.reason {
            write!(f, ", reason: {reason}")?;
        }
        f.write_str(")")
    }
}

/// The mod id a row names the jar by: its Fabric or NeoForge id, else its file stem.
pub(crate) fn jar_id(jar: &NativeJar, file: &str) -> String {
    jar.fabric_id
        .clone()
        .or_else(|| jar.neoforge_id.clone())
        .unwrap_or_else(|| file.strip_suffix(".jar").unwrap_or(file).to_owned())
}

/// Loads one jar the way a server would: every server-side entrypoint the real loader runs, in
/// the loader's order, in its own host, with the sibling jars it declares on that host's class
/// path. Anything the runtime does not model is counted by the host, which is what `ran-hollow`
/// means. The host is returned because the listeners it recorded need it for the rest of the
/// server's life.
///
/// Fabric runs `main` and then `server` (Hooks.startServer), so a jar that has both runs both.
/// A failing stage is recorded and the remaining stages still run: the failure is a limit of
/// this interpreter, not a reason to drop an entrypoint the jar ships for the server. Each stage
/// runs inside [`caught`], so an interpreter panic is that stage failing like any other error,
/// never a crashed game. The jar is refused only when every stage failed; a partial run is
/// `ran-partial` and its `reason` names the failed stages.
// The `Err` is the refused jar's report row, built once per jar at load: boxing it buys nothing.
#[allow(clippy::result_large_err)]
fn load_jar(
    file: String,
    jar: Arc<NativeJar>,
    links: &[(String, Arc<NativeJar>)],
) -> Result<LoadedJar, JarRow> {
    let id = jar_id(&jar, &file);
    let entrypoints = jar.role_entrypoints(false);
    if entrypoints.is_empty() {
        let reason = if jar.discover_entrypoints("client").is_empty() {
            "no main, server or neoforge entrypoint"
        } else {
            "client-only entrypoint: a server never runs it"
        };
        return Err(JarRow::refused(&file, id, reason.to_owned()));
    }
    let mut host = jar.seed_host(StdHost::new().with_environment_type("SERVER"));
    for (link, dependency) in links {
        if let Err(error) = host.link_jar(link.clone(), dependency) {
            return Err(JarRow::refused(
                &file,
                id,
                format!("linking {link} failed: {error}"),
            ));
        }
    }
    let mut calls = 0;
    let mut ran = 0;
    let mut failures = Vec::new();
    for (kind, entry) in &entrypoints {
        if let Some(adapter) = entry.adapter.as_deref() {
            failures.push(format!(
                "{}: entrypoint adapter {adapter} is not supported by the native path",
                entry.class
            ));
            continue;
        }
        let outcome = caught(
            &format!("{} entrypoint {} ({kind})", id, entry.class),
            || {
                jar.execute_with_budget(
                    &entry.class,
                    entry.method.as_deref(),
                    kind,
                    &mut host,
                    &runtime::ExecutionBudget::within(JAR_BUDGET),
                )
            },
        );
        match outcome {
            Ok(Ok(report)) => {
                calls += report.calls;
                ran += 1;
            }
            Ok(Err(error)) => failures.push(format!("{}: {error}", entry.class)),
            // A caught panic is this stage failing: the remaining stages still run, and it counts
            // toward `ran-partial` exactly as an interpreter error does.
            Err(panic) => failures.push(format!("{}: panicked: {panic}", entry.class)),
        }
    }
    if ran == 0 {
        return Err(JarRow {
            file,
            id,
            verdict: "refused",
            host_calls: calls,
            unknown_host_calls: host.unknown_host_call_count(),
            reason: Some(failures.join("; ")),
            callbacks_ran: 0,
            callbacks_failed: 0,
            callback_unknown_host_calls: 0,
            commands_ran: 0,
            commands_refused: 0,
            command_unknown_host_calls: 0,
            commands_translated: 0,
            commands_refused_at_registration: 0,
            content_blocks: 0,
            content_items: 0,
            content_skipped: BTreeMap::new(),
        });
    }
    let unknown = host.unknown_host_call_count();
    let registered = host.game_listeners(&jar, true);
    let (listeners, orderings) = (registered.listeners, registered.phase_orderings);
    Ok(LoadedJar {
        row: JarRow {
            file,
            id,
            verdict: runtime::verdict::verdict(ran, failures.len(), unknown),
            host_calls: calls,
            unknown_host_calls: unknown,
            reason: (!failures.is_empty())
                .then(|| format!("entrypoint(s) failed: {}", failures.join("; "))),
            callbacks_ran: 0,
            callbacks_failed: 0,
            callback_unknown_host_calls: 0,
            commands_ran: 0,
            commands_refused: 0,
            command_unknown_host_calls: 0,
            commands_translated: 0,
            commands_refused_at_registration: 0,
            content_blocks: 0,
            content_items: 0,
            content_skipped: BTreeMap::new(),
        },
        jar,
        host,
        listeners,
        orderings,
        commands: Vec::new(),
        native_commands: Vec::new(),
        commands_registered: false,
    })
}

/// Writes the report, replacing the previous one, and answers the path it was written to (as a
/// reader finds it under the game's `config/`).
fn write_report(host: &dyn Host, state: &State) -> Result<String, ConfigFileError> {
    let path = ConfigPath::new(REPORT)?;
    let rows: Vec<_> = state
        .jars
        .iter()
        .map(|jar| {
            let mut row = jar.row.json();
            let stats = jar.host.attachment_stats();
            if stats.types > 0 {
                row["attachments"] = attachments::stats_json(&stats);
            }
            row
        })
        .chain(state.refused.iter().map(JarRow::json))
        .collect();
    let report = json!({ "schema": SCHEMA, "jars": rows });
    let mut bytes = serde_json::to_vec_pretty(&report).unwrap_or_default();
    bytes.push(b'\n');
    host.config_files().write(&path, &bytes)?;
    Ok(format!("config/{REPORT}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mod_api::{DamageContainer, EntityRef, Identifier, PermissionLevel, WorldEpoch};

    /// A command block answers its own dimension (`CommandSource::CommandBlock.level`), the
    /// console the overworld (vanilla's console source), and the sources whose level the hook
    /// does not name stay unknown so `getLevel()` refuses and counts instead of acting on the
    /// overworld.
    #[test]
    fn a_command_blocks_level_is_its_own_and_unnamed_levels_stay_unknown() {
        let named = |key: DimensionKey| match key {
            DimensionKey::OVERWORLD => Some("minecraft:overworld".to_owned()),
            DimensionKey::NETHER => Some("minecraft:the_nether".to_owned()),
            _ => None,
        };
        let block = CommandSource::CommandBlock {
            position: [1, 2, 3],
            level: DimensionKey::NETHER,
            permission: PermissionLevel::Gamemasters,
        };
        assert_eq!(
            source_dimension(&block, named).as_deref(),
            Some("minecraft:the_nether")
        );
        // A level the host does not list is unknown, not the overworld.
        assert_eq!(source_dimension(&block, |_| None), None);
        let console = CommandSource::Console {
            permission: PermissionLevel::Owners,
        };
        assert_eq!(
            source_dimension(&console, named).as_deref(),
            Some("minecraft:overworld")
        );
        let cart = CommandSource::CommandBlockMinecart {
            entity: EntityRef::new(WorldEpoch::new(1), 9),
            permission: PermissionLevel::All,
        };
        assert_eq!(source_dimension(&cart, named), None);
        let function = CommandSource::Function {
            id: Identifier::parse("demo:probe").unwrap(),
            permission: PermissionLevel::All,
        };
        assert_eq!(source_dimension(&function, named), None);
    }

    /// A Java command runs on the server the report measured, not on an empty default: the
    /// façade carries the dedicated flag, the tick, the online players (with their levels) and
    /// the open levels. A player's level the host could not name stays `None`.
    #[test]
    fn the_command_facade_carries_the_measured_server_state() {
        let mut state = State {
            dedicated: true,
            tick_count: 512,
            ..State::default()
        };
        state.players.push(FacadePlayer {
            uuid: 5,
            name: "Alex".into(),
            level: Some("minecraft:the_nether".into()),
            entity_id: Some(3),
        });
        state.levels.push(FacadeLevel {
            id: "minecraft:the_nether".into(),
            game_time: 100,
            day_time: 40,
            world_border: None,
        });
        let facade = state.facade();
        assert!(facade.dedicated);
        assert_eq!(facade.tick_count, 512);
        assert_eq!(facade.players.len(), 1);
        assert_eq!(facade.players[0].name, "Alex");
        assert_eq!(
            facade.players[0].level.as_deref(),
            Some("minecraft:the_nether")
        );
        assert_eq!(facade.levels.len(), 1);
        assert_eq!(facade.levels[0].id, "minecraft:the_nether");
        assert_eq!(facade.levels[0].game_time, 100);
    }

    use mod_api::HurtSource;

    /// A jar whose listeners fail keeps the entrypoint verdict (`ran-clean`) but its summary says
    /// so: the log line and the report may never present it as an unqualified clean run.
    #[test]
    fn listener_failures_show_in_the_summary_and_the_log_line() {
        let mut row = JarRow::refused("calcmod.jar", "calcmod".to_owned(), String::new());
        row.verdict = "ran-clean";
        row.reason = None;
        assert_eq!(row.summary(), "ran-clean");
        row.callbacks_failed = 1;
        assert_eq!(row.summary(), "ran-clean, 1 listener failure");
        row.callbacks_failed = 12;
        assert_eq!(row.summary(), "ran-clean, 12 listener failures");
        assert!(row.to_string().contains("ran-clean, 12 listener failures"));
        assert_eq!(row.json()["summary"], "ran-clean, 12 listener failures");
        assert_eq!(row.json()["verdict"], "ran-clean");
    }

    /// Command-registration listeners are callbacks: their runs, failures, unmodelled host calls
    /// and refused usages land in the row, so `calcmod`'s failed registration is a counted
    /// failure in the report, not only a log line.
    /// Brigadier's four `EntityArgument` kinds, as the runtime records them from
    /// `EntityArgument.entity()/entities()/player()/players()` (`single`, `playersOnly`): the two
    /// player kinds are carried as the mod API's `player` kind and spelled so in the usage the
    /// game parses; the two entity kinds have no mod API counterpart and stay refused.
    #[test]
    fn only_the_player_selector_kinds_are_carried() {
        let kind = |single, players| ArgumentKind::Entity { single, players };
        assert_eq!(
            JavaArgument::from_kind(&kind(true, true)),
            Some(JavaArgument::Player)
        );
        assert_eq!(
            JavaArgument::from_kind(&kind(false, true)),
            Some(JavaArgument::Player)
        );
        assert_eq!(JavaArgument::from_kind(&kind(true, false)), None);
        assert_eq!(JavaArgument::from_kind(&kind(false, false)), None);
        assert_eq!(
            usage(
                &["carryon".to_owned(), "clear".to_owned()],
                &[("target".to_owned(), JavaArgument::Player)]
            ),
            "carryon clear <target:player>"
        );
        // The refusal log names the kind the jar asked for, not a Debug dump.
        assert_eq!(
            JavaArgument::describe(&kind(false, false)),
            "entity selector (EntityArgument.entities)"
        );
    }

    /// The player the game selected reaches the Java command as the runtime's one-player list,
    /// which `EntityArgument.getPlayer`/`getPlayers` turn into the `game/player/<uuid>` marker,
    /// and `getInput()` names that player. Without a name the input carries the UUID in
    /// `java.util.UUID.toString()` form (`new UUID(0x0123456789abcdefL, 0xfedcba9876543210L)`
    /// prints `01234567-89ab-cdef-fedc-ba9876543210`).
    #[test]
    fn a_selected_player_is_the_runtimes_one_player_list() {
        let uuid = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210_u128;
        assert_eq!(
            selected_player(uuid, Some("Alex".to_owned())),
            (
                brigadier::CommandArgument::Players(vec![uuid]),
                "Alex".to_owned()
            )
        );
        assert_eq!(
            selected_player(uuid, None).1,
            "01234567-89ab-cdef-fedc-ba9876543210"
        );
        // A value of another kind is not read as a player, nor a player as text.
        let text = CommandArgumentValue::String("Alex".to_owned());
        assert_eq!(JavaArgument::Player.runtime_value(&text, |_| None), None);
        assert_eq!(
            JavaArgument::Word.runtime_value(&text, |_| None),
            Some((
                brigadier::CommandArgument::Str("Alex".to_owned()),
                "Alex".to_owned()
            ))
        );
        let number = CommandArgumentValue::Integer(3);
        assert_eq!(JavaArgument::Player.runtime_value(&number, |_| None), None);
    }

    #[test]
    fn command_registration_counts_reach_the_row() {
        let mut row = JarRow::refused("calcmod.jar", "calcmod".to_owned(), String::new());
        row.verdict = "ran-clean";
        row.reason = None;
        count_registration(&mut row, (0, 1), 2, 3, 4, 5);
        assert_eq!(row.callbacks_ran, 0);
        assert_eq!(row.callbacks_failed, 1);
        assert_eq!(row.callback_unknown_host_calls, 2);
        assert_eq!(row.commands_translated, 3);
        assert_eq!(row.commands_refused_at_registration, 4);
        assert_eq!(row.command_unknown_host_calls, 5);
        assert_eq!(row.summary(), "ran-clean, 1 listener failure");
        let commands = &row.json()["commands"];
        assert_eq!(commands["translated"], 3);
        assert_eq!(commands["refused_at_registration"], 4);
    }

    /// A panic inside a Java callback is a named, counted failure: the game keeps running and
    /// the report says which callback threw. `Integer.parseInt("1", 40)` is the probe the audit
    /// used, and it aborts the Rust process without this wrapper.
    /// One entrypoint class: `public class <name> implements <interface>` with a no-argument
    /// constructor and `public void <method>()`. With `fails`, the method calls
    /// `Integer.parseInt("1", 40)`, which throws an uncaught `NumberFormatException` ("radix 40
    /// greater than Character.MAX_RADIX", as the JDK does since #47; it used to panic the
    /// interpreter); otherwise it just returns.
    fn entrypoint_class(name: &str, interface: &str, method: &str, fails: bool) -> Vec<u8> {
        fn u2(out: &mut Vec<u8>, value: u16) {
            out.extend_from_slice(&value.to_be_bytes());
        }
        fn utf8(out: &mut Vec<u8>, value: &str) {
            out.push(1);
            u2(out, value.len() as u16);
            out.extend_from_slice(value.as_bytes());
        }
        let mut out = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 52];
        u2(&mut out, 21); // pool slots 1..=20
        utf8(&mut out, name); // 1
        out.extend_from_slice(&[7, 0, 1]); // 2 Class name
        utf8(&mut out, "java/lang/Object"); // 3
        out.extend_from_slice(&[7, 0, 3]); // 4 Class Object
        utf8(&mut out, "<init>"); // 5
        utf8(&mut out, "()V"); // 6
        out.extend_from_slice(&[12, 0, 5, 0, 6]); // 7 NameAndType <init>()V
        out.extend_from_slice(&[10, 0, 4, 0, 7]); // 8 Methodref Object.<init>
        utf8(&mut out, "Code"); // 9
        utf8(&mut out, method); // 10
        utf8(&mut out, interface); // 11
        out.extend_from_slice(&[7, 0, 11]); // 12 Class interface
        utf8(&mut out, "1"); // 13
        out.extend_from_slice(&[8, 0, 13]); // 14 String "1"
        utf8(&mut out, "java/lang/Integer"); // 15
        out.extend_from_slice(&[7, 0, 15]); // 16 Class Integer
        utf8(&mut out, "parseInt"); // 17
        utf8(&mut out, "(Ljava/lang/String;I)I"); // 18
        out.extend_from_slice(&[12, 0, 17, 0, 18]); // 19 NameAndType parseInt
        out.extend_from_slice(&[10, 0, 16, 0, 19]); // 20 Methodref Integer.parseInt
        // public class, this #2, super #4, one interface (#12), no fields, two methods.
        out.extend_from_slice(&[0, 0x21, 0, 2, 0, 4, 0, 1, 0, 12, 0, 0, 0, 2]);
        let mut method_info = |name: u16, max_stack: u16, code: &[u8]| {
            u2(&mut out, 0x0001); // public
            u2(&mut out, name);
            u2(&mut out, 6); // ()V
            u2(&mut out, 1); // one attribute: Code
            u2(&mut out, 9);
            out.extend_from_slice(&(12 + code.len() as u32).to_be_bytes());
            u2(&mut out, max_stack);
            u2(&mut out, 1); // max_locals: `this`
            out.extend_from_slice(&(code.len() as u32).to_be_bytes());
            out.extend_from_slice(code);
            u2(&mut out, 0); // no exception table
            u2(&mut out, 0); // no attributes
        };
        // aload_0; invokespecial Object.<init>; return
        method_info(5, 1, &[0x2A, 0xB7, 0, 8, 0xB1]);
        if fails {
            // ldc "1"; bipush 40; invokestatic Integer.parseInt; pop; return
            method_info(10, 2, &[0x12, 14, 0x10, 40, 0xB8, 0, 20, 0x57, 0xB1]);
        } else {
            method_info(10, 0, &[0xB1]);
        }
        u2(&mut out, 0); // no class attributes
        out
    }

    /// A Fabric jar with a `main` stage (class `demo.Main`) and a `server` stage
    /// (`demo.Server`). `None` leaves that class out of the jar, so its stage fails to load;
    /// `Some(fails)` ships it, failing at runtime or not.
    fn two_stage_jar(main: Option<bool>, server: Option<bool>) -> Arc<NativeJar> {
        use std::io::Write;
        let mut bytes = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut bytes);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("fabric.mod.json", options).unwrap();
            zip.write_all(
                br#"{"schemaVersion":1,"id":"demo","version":"1",
                    "entrypoints":{"main":["demo.Main"],"server":["demo.Server"]}}"#,
            )
            .unwrap();
            let stages = [
                (
                    "demo/Main",
                    "net/fabricmc/api/ModInitializer",
                    "onInitialize",
                    main,
                ),
                (
                    "demo/Server",
                    "net/fabricmc/api/DedicatedServerModInitializer",
                    "onInitializeServer",
                    server,
                ),
            ];
            for (name, interface, method, shipped) in stages {
                if let Some(fails) = shipped {
                    zip.start_file(format!("{name}.class"), options).unwrap();
                    zip.write_all(&entrypoint_class(name, interface, method, fails))
                        .unwrap();
                }
            }
            zip.finish().unwrap();
        }
        Arc::new(NativeJar::from_bytes(&bytes.into_inner()).unwrap())
    }

    /// Two NeoForge jars listening to one event run as one bus: `EventPriority` first, then the
    /// jars' load order, so a cancellation in one jar decides what the other's listeners see.
    /// Each jar registers the runtime bus fixture's `BusDemo.cancel` listeners
    /// (`crates/horizon-native-java/tests/fixtures/neoforge_bus`); the expected lines are that
    /// fixture's `oracle.jsonl` case `cancel_two_mods` (the same two registrations on one real
    /// bus-8.0.5 bus, OpenJDK 25.0.5): `cancels, uncancels, skipped, uncancels, runs-again,
    /// runs-again, cancels-last`, split by jar (within one priority the first jar's listener comes
    /// first). Jar after jar, the second jar would have printed `uncancels, runs-again,
    /// cancels-last`.
    #[test]
    fn listeners_of_every_jar_run_in_one_priority_order() {
        use crate::runtime::neoforge_bus::fixture_tests::fixture_jar_bytes;
        use crate::runtime::{MethodRef, Value};
        let budget = ExecutionBudget::within(Duration::from_secs(20));
        let mut jars: Vec<LoadedJar> = ["first", "second"]
            .into_iter()
            .map(|id| {
                let jar = Arc::new(NativeJar::from_bytes(fixture_jar_bytes()).unwrap());
                let mut host = jar.seed_host(StdHost::new().with_environment_type("SERVER"));
                jar.invoke_persistent(
                    &MethodRef {
                        owner: "demo/BusDemo".into(),
                        name: "cancel".into(),
                        descriptor: "(Lnet/neoforged/bus/api/IEventBus;)V".into(),
                    },
                    None,
                    &[Value::Ref(Some(runtime::neoforge_bus::GAME_BUS.into()))],
                    &mut host,
                    &budget,
                )
                .unwrap();
                let registered = host.game_listeners(&jar, true);
                LoadedJar {
                    row: JarRow::refused(&format!("{id}.jar"), id.to_owned(), String::new()),
                    jar,
                    host,
                    listeners: registered.listeners,
                    orderings: registered.phase_orderings,
                    commands: Vec::new(),
                    native_commands: Vec::new(),
                    commands_registered: false,
                }
            })
            .collect();
        let mut firing = EventFiring::new(
            GameEvent::LivingAllowDeath,
            Arc::new(EventFacade::default()),
            EventContext::default(),
        );
        let order = listener_order(&jars, &firing, &|_| true);
        let counts = run_listeners(&mut jars, &order, &mut firing);
        let lines = |jar: &mut LoadedJar| -> Vec<String> {
            jar.host.take_stdout().lines().map(str::to_owned).collect()
        };
        assert_eq!(
            lines(&mut jars[0]),
            ["cancels", "uncancels", "runs-again", "cancels-last"]
        );
        assert_eq!(lines(&mut jars[1]), ["skipped", "uncancels", "runs-again"]);
        assert_eq!(counts, [(4, 0), (3, 0)]);
        assert!(firing.cancelled);
    }

    /// The verdict and reason `load_jar` gives a jar, whether it loaded or was refused.
    fn load_verdict(jar: Arc<NativeJar>) -> (&'static str, Option<String>) {
        match load_jar("demo.jar".to_owned(), jar, &[]) {
            Ok(loaded) => (loaded.row.verdict, loaded.row.reason),
            Err(row) => (row.verdict, row.reason),
        }
    }

    /// Fabric's `Hooks.startServer` runs `main` and then `server`; the bridge runs both, records
    /// a failed stage and keeps going. A stage fails by not loading or by an uncaught exception;
    /// a stage that panics the interpreter fails the same way through `caught` (pinned by
    /// `a_panicking_callback_is_a_named_failure_not_a_crash`, since no bytecode is known to panic
    /// the interpreter any more), never a crashed game.
    #[test]
    fn every_server_stage_runs_and_a_failed_stage_is_partial() {
        // Both stages ship and return: a full, clean run.
        assert_eq!(
            load_verdict(two_stage_jar(Some(false), Some(false))),
            ("ran-clean", None)
        );

        // `main` cannot load, `server` still runs: partial, and the reason names only `main`.
        let (verdict, reason) = load_verdict(two_stage_jar(None, Some(false)));
        assert_eq!(verdict, "ran-partial");
        let reason = reason.unwrap();
        assert!(
            reason.starts_with("entrypoint(s) failed: demo.Main: "),
            "{reason}"
        );
        assert!(!reason.contains("demo.Server"), "{reason}");

        // `main` throws an uncaught exception, `server` still runs after it: partial.
        let (verdict, reason) = load_verdict(two_stage_jar(Some(true), Some(false)));
        assert_eq!(verdict, "ran-partial");
        let reason = reason.unwrap();
        assert!(
            reason.contains("demo.Main: uncaught exception java/lang/NumberFormatException"),
            "{reason}"
        );
        assert!(!reason.contains("demo.Server"), "{reason}");

        // Every stage failed (one missing, one throwing): refused, with both named.
        let (verdict, reason) = load_verdict(two_stage_jar(None, Some(true)));
        assert_eq!(verdict, "refused");
        let reason = reason.unwrap();
        assert!(reason.contains("demo.Main: "), "{reason}");
        assert!(
            reason.contains("demo.Server: uncaught exception java/lang/NumberFormatException"),
            "{reason}"
        );
    }

    #[test]
    fn a_panicking_callback_is_a_named_failure_not_a_crash() {
        let answer: Result<u32, String> = caught("parseInt radix 40", || {
            panic!("radix must lie in the range [2, 36]")
        });
        assert_eq!(answer.unwrap_err(), "radix must lie in the range [2, 36]");
        let answer: Result<(), String> = caught("formatted", || panic!("{} {}", "bad", "value"));
        assert_eq!(answer.unwrap_err(), "bad value");
        let answer: Result<(), String> = caught("quiet", || std::panic::panic_any(7_u8));
        assert_eq!(answer.unwrap_err(), "panic with a non-string payload");
        assert_eq!(caught("runs", || 7).unwrap(), 7);
    }

    /// Only one hand holding the stack names the hand; two equal stacks cannot tell them apart
    /// (two empty hands included), so the hand is unknown rather than guessed.
    #[test]
    fn the_acting_hand_is_named_only_when_one_hand_holds_the_stack() {
        assert_eq!(
            one_hand(Some(&"stick"), Some(&"stick"), Some(&"paper")),
            Some(Hand::Main)
        );
        assert_eq!(
            one_hand(Some(&"paper"), Some(&"stick"), Some(&"paper")),
            Some(Hand::Off)
        );
        assert_eq!(
            one_hand::<&str>(None, Some(&"stick"), None),
            Some(Hand::Off)
        );
        assert_eq!(one_hand::<&str>(None, None, None), None);
        assert_eq!(
            one_hand(Some(&"stick"), Some(&"stick"), Some(&"stick")),
            None
        );
    }

    /// `PlayerList.respawn` passes `keepAllPlayerData = false` for a death respawn and `true`
    /// for the End return after the credits; Fabric's `COPY_FROM` forwards that flag as
    /// `alive` (`ServerPlayerMixin.onCopyFrom`, `iload_2`).
    #[test]
    fn copy_from_alive_is_the_keep_all_data_flag_not_the_death_flag() {
        assert!(!copy_from_alive(true), "a death respawn keeps no data");
        assert!(copy_from_alive(false), "leaving the End keeps all data");
        let event = PlayerCloneEvent {
            player: 7,
            name: "Alex",
            was_death: true,
        };
        assert!(!player_clone_context(&event).flag);
        let event = PlayerCloneEvent {
            was_death: false,
            ..event
        };
        assert!(player_clone_context(&event).flag);
    }

    /// Fabric's `afterDamage` gets `hurtServer`'s `originalDamage` and `damage` at `TAIL`, and
    /// `blocked`: the base amount, the dealt amount (after reductions, before absorption) and
    /// the block flag. The health damage after absorption is a third value and is not what
    /// either argument carries.
    #[test]
    fn after_damage_carries_the_base_amount_the_dealt_amount_and_the_block_flag() {
        // DamageContainer(original 10, blocked 4, Pre left 6, health lost 2): the two
        // arguments are the 10 (originalDamage) and the 6 (hurtServer's damage at TAIL), not
        // the 2 the health lost and not 10 twice.
        assert_eq!(after_damage_arguments(10.0, 6.0, 4.0), (10.0, 6.0, true));
        // No blocking: the flag is false, not a count or an amount.
        assert_eq!(after_damage_arguments(1.0, 1.0, 0.0), (1.0, 1.0, false));
        let source = HurtSource {
            damage_type: "minecraft:player_attack",
            causing: 3,
            direct: 3,
            position: None,
        };
        // A fresh snapshot has an original and nothing else set, so the context takes the base
        // from it and the dealt amount is the (unset here) inflicted value, not a copy.
        let container = DamageContainer::new(10.0);
        let event = LivingDamagePostEvent {
            entity: 7,
            kind: "minecraft:cow",
            source: &source,
            container: &container,
        };
        let context = living_damage_context(&event);
        assert_eq!(context.base_amount, Some(10.0));
        assert_eq!(context.amount, 0.0);
        assert!(!context.flag);
        assert_eq!(context.damage.as_deref(), Some("minecraft:player_attack"));
    }
}
