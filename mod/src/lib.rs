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

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mod_api::{
    AttackEntityEvent, Authority, BreakBlockEvent, ClientHost, ClientSetupEvent, Command,
    CommandArgumentValue, CommandSource, ConfigEntryKind, ConfigFileError, ConfigPath,
    EntityInteractEvent, EntityJoinLevelEvent, EntityLeaveLevelEvent, Flow, Hand, Hooks, Host,
    HostSide, InteractionResult, ItemStack, LeftClickBlockEvent, LevelDimension, LevelLoadEvent,
    LevelRandomHost, LevelTickPostEvent, LevelTickPreEvent, LevelUnloadEvent, LivingDamagePostEvent,
    LivingDeathEvent, LivingDeathPostEvent, LivingIncomingDamageEvent, PlayerChangedDimensionEvent,
    PlayerCloneEvent, PlayerLoggedInEvent, PlayerLoggedOutEvent, PlayerRespawnedEvent,
    RegisterCommandsEvent, RightClickBlockEvent, RightClickItemEvent, RunModCommandEvent,
    ServerAboutToStartEvent, ServerHost, ServerStartedEvent, ServerStartingEvent, ServerStoppedEvent,
    ServerStoppingEvent, ServerTickEvent, ServerTickPhase, Text,
};
use runtime::brigadier::{
    self, ArgumentKind, CommandInvocation as RuntimeInvocation, CommandMessage, CommandOutcome,
    CommandSource as RuntimeSource, CommandText, MessageKind, NodeKind, Requirement,
};
use runtime::events::{
    EventContext, EventFacade, EventFiring, FacadeEntity, FacadePlayer, GameEvent, GameListener,
};
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
    /// Players whose join this mod heard, as much of them as the hook gave:
    /// a Java player marker answers its name and UUID, and refuses the level
    /// and entity id it does not know.
    players: Vec<FacadePlayer>,
    /// `MinecraftServer.getTickCount()` as the last server tick reported it.
    tick_count: i32,
    /// Whether the open server is a dedicated one, from `server_about_to_start`.
    dedicated: bool,
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
    jar: NativeJar,
    host: StdHost,
    listeners: Vec<GameListener>,
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
#[derive(Clone, Copy, PartialEq, Eq)]
enum JavaArgument {
    Word,
    Text,
    Integer,
}

impl JavaArgument {
    fn usage_kind(self) -> &'static str {
        match self {
            Self::Word => "word",
            Self::Text => "text",
            Self::Integer => "integer",
        }
    }

    /// The kinds the runtime's façade records that this bridge carries. Everything else is
    /// refused by name rather than approximated.
    fn from_kind(kind: &ArgumentKind) -> Option<Self> {
        match kind {
            ArgumentKind::Word | ArgumentKind::Quoted => Some(Self::Word),
            ArgumentKind::Greedy => Some(Self::Text),
            ArgumentKind::Integer { .. } => Some(Self::Integer),
            _ => None,
        }
    }

    fn describe(kind: &ArgumentKind) -> String {
        match kind {
            ArgumentKind::Word => "word".to_owned(),
            ArgumentKind::Quoted => "quoted string".to_owned(),
            ArgumentKind::Greedy => "greedy string".to_owned(),
            ArgumentKind::Integer { .. } => "integer".to_owned(),
            other => format!("{other:?}"),
        }
    }
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
        *state = State {
            dedicated: event.dedicated,
            ..State::default()
        };
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

    /// One jar changed its commands (`Commands::refresh_commands`): the Java registrations cannot
    /// change at runtime, so the same set is offered again.
    fn register_commands(&self, _host: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        let mut state = self.lock();
        let (dedicated, tick_count, players) =
            (state.dedicated, state.tick_count, state.players.clone());
        for loaded in &mut state.jars {
            if !loaded.commands_registered {
                loaded.commands_registered = true;
                let mut firing = EventFiring::new(
                    GameEvent::RegisterCommands,
                    Arc::new(EventFacade {
                        dedicated,
                        tick_count,
                        players: players.clone(),
                        ..EventFacade::default()
                    }),
                    EventContext::default(),
                );
                let (ran, failed) = run_listeners(
                    &loaded.jar,
                    &mut loaded.host,
                    &loaded.listeners,
                    &mut firing,
                    &|_| true,
                );
                log::info!(
                    "{LOG}: {}: command registration: {ran} callback(s) ran, {failed} failed",
                    loaded.row.id
                );
                let (native, commands, refused) =
                    translate_commands(&loaded.jar, &mut loaded.host, &loaded.row.id);
                log::info!(
                    "{LOG}: {}: {} command usage(s) translated, {} refused",
                    loaded.row.id,
                    native.len(),
                    refused,
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
                match (kind, event.command.argument(name)) {
                    (
                        JavaArgument::Word | JavaArgument::Text,
                        Some(CommandArgumentValue::String(text)),
                    ) => {
                        arguments
                            .push((name.clone(), brigadier::CommandArgument::Str(text.clone())));
                        words.push(text.clone());
                    }
                    (JavaArgument::Integer, Some(CommandArgumentValue::Integer(value))) => {
                        arguments.push((name.clone(), brigadier::CommandArgument::Int(*value)));
                        words.push(value.to_string());
                    }
                    (_, Some(_)) => {
                        return refuse(
                            host,
                            &command.usage,
                            format!("argument '{name}' is not the kind this bridge carries"),
                        );
                    }
                    (_, None) => {
                        return refuse(
                            host,
                            &command.usage,
                            format!("argument '{name}' is missing"),
                        );
                    }
                }
            }
            let source = runtime_source(&event.source, host);
            let invocation = RuntimeInvocation {
                input: words.join(" "),
                source,
                arguments,
            };
            let before = loaded.host.unknown_host_call_count();
            let run = loaded.jar.run_command(
                &command.run,
                &invocation,
                Arc::new(EventFacade::default()),
                &mut loaded.host,
                &ExecutionBudget::within(JAR_BUDGET),
            );
            let unknown = loaded.host.unknown_host_call_count().saturating_sub(before);
            loaded.row.command_unknown_host_calls += unknown;
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
    fn server_tick(&self, _host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        let game = match event.phase {
            ServerTickPhase::ServerTickStart { .. } => GameEvent::ServerTickStart,
            ServerTickPhase::ServerTickEnd { .. } => GameEvent::ServerTickEnd,
            // A phase this build does not know: nothing to fire rather than a guess.
            _ => return,
        };
        let tick_count = event.context.tick.get().min(i32::MAX as u64) as i32;
        self.lock().tick_count = tick_count;
        let dedicated = event.context.authority == Authority::DedicatedServer;
        self.fire_event(game, dedicated, EventContext::default(), |_| true);
    }

    /// A server level's tick starts (Fabric's `START_LEVEL_TICK`, NeoForge's
    /// `LevelTickEvent.Pre`).
    fn level_tick_pre(&self, _host: HostSide<'_>, event: &mut LevelTickPreEvent) {
        let context = EventContext {
            level: Some(event.level.dimension_id().to_owned()),
            ..EventContext::default()
        };
        self.fire_event(
            GameEvent::LevelTickStart,
            self.dedicated(),
            context,
            |_| true,
        );
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
    fn level_load(&self, _host: &mut dyn ServerHost, event: &mut LevelLoadEvent<'_>) {
        let context = EventContext {
            level: Some(event.dimension.to_owned()),
            ..EventContext::default()
        };
        self.fire_event(GameEvent::LevelLoad, self.dedicated(), context, |_| true);
    }

    /// A server level closed (Fabric's `ServerLevelEvents.UNLOAD`, NeoForge's
    /// `LevelEvent.Unload`).
    fn level_unload(&self, _host: &mut dyn ServerHost, event: &mut LevelUnloadEvent<'_>) {
        let context = EventContext {
            level: Some(event.dimension.to_owned()),
            ..EventContext::default()
        };
        self.fire_event(GameEvent::LevelUnload, self.dedicated(), context, |_| true);
    }

    /// A player joined (Fabric's `ServerPlayConnectionEvents.JOIN` and
    /// `ServerPlayerEvents.JOIN`, NeoForge's `PlayerLoggedInEvent`).
    fn player_logged_in(&self, _host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        {
            let mut state = self.lock();
            state.players.retain(|player| player.uuid != event.player);
            state.players.push(FacadePlayer {
                uuid: event.player,
                name: event.name.to_owned(),
                level: None,
                entity_id: None,
            });
        }
        let context = EventContext {
            player: Some(event.player),
            ..EventContext::default()
        };
        self.fire_event(GameEvent::PlayerJoin, self.dedicated(), context, |_| true);
    }

    /// A player left (Fabric's `ServerPlayConnectionEvents.DISCONNECT` and
    /// `ServerPlayerEvents.LEAVE`, NeoForge's `PlayerLoggedOutEvent`). The player is already off
    /// the list when a listener hears it, as the hook says.
    fn player_logged_out(&self, _host: &mut dyn ServerHost, event: &mut PlayerLoggedOutEvent<'_>) {
        self.lock()
            .players
            .retain(|player| player.uuid != event.player);
        let context = EventContext {
            player: Some(event.player),
            ..EventContext::default()
        };
        self.fire_event(GameEvent::PlayerLeave, self.dedicated(), context, |_| true);
    }

    /// A player respawned (Fabric's `ServerPlayerEvents.AFTER_RESPAWN`, NeoForge's
    /// `PlayerRespawnEvent`).
    fn player_respawned(&self, _host: &mut dyn ServerHost, event: &mut PlayerRespawnedEvent<'_>) {
        let context = EventContext {
            player: Some(event.player),
            ..EventContext::default()
        };
        self.fire_event(GameEvent::PlayerRespawn, self.dedicated(), context, |_| true);
    }

    /// A player's data was copied to their new player (Fabric's `COPY_FROM`, NeoForge's
    /// `PlayerEvent.Clone`); `was_death` says a death caused it.
    fn player_clone(&self, _host: &mut dyn ServerHost, event: &mut PlayerCloneEvent<'_>) {
        let context = EventContext {
            player: Some(event.player),
            flag: event.was_death,
            ..EventContext::default()
        };
        self.fire_event(GameEvent::PlayerCopy, self.dedicated(), context, |_| true);
    }

    /// A player moved to another dimension (Fabric's `AFTER_PLAYER_CHANGE_LEVEL`, NeoForge's
    /// `PlayerChangedDimensionEvent`).
    fn player_changed_dimension(
        &self,
        _host: &mut dyn ServerHost,
        event: &mut PlayerChangedDimensionEvent<'_>,
    ) {
        let context = EventContext {
            player: Some(event.player),
            origin: Some(event.from.to_owned()),
            level: Some(event.to.to_owned()),
            ..EventContext::default()
        };
        self.fire_event(
            GameEvent::PlayerChangeLevel,
            self.dedicated(),
            context,
            |_| true,
        );
    }

    /// A living entity took damage (Fabric's `ServerLivingEntityEvents.AFTER_DAMAGE`, NeoForge's
    /// `LivingDamageEvent.Post`).
    fn living_damage_post(
        &self,
        _level: &mut dyn LevelRandomHost,
        event: &mut LivingDamagePostEvent<'_>,
    ) {
        let context = EventContext {
            entity: Some(FacadeEntity {
                id: event.entity,
                kind: event.kind.to_owned(),
                level: None,
            }),
            damage: Some(event.source.damage_type.to_owned()),
            amount: event.container.health_damage(),
            ..EventContext::default()
        };
        self.fire_event(
            GameEvent::LivingAfterDamage,
            self.dedicated(),
            context,
            |_| true,
        );
    }

    /// An entity left a server level (Fabric's `ServerEntityEvents.ENTITY_UNLOAD`, NeoForge's
    /// `EntityLeaveLevelEvent`).
    fn entity_leave_level(
        &self,
        _host: &mut dyn ServerHost,
        event: &mut EntityLeaveLevelEvent<'_>,
    ) {
        let context = EventContext {
            level: Some(event.dimension.to_owned()),
            entity: Some(FacadeEntity {
                id: event.entity as i32,
                kind: event.kind.to_owned(),
                level: Some(event.dimension.to_owned()),
            }),
            ..EventContext::default()
        };
        self.fire_event(GameEvent::EntityUnload, self.dedicated(), context, |_| true);
    }

    /// A player broke the block at `pos` (Fabric's `PlayerBlockBreakEvents.BEFORE`, NeoForge's
    /// `BreakBlockEvent`). A cancel keeps the block, and the Fabric `CANCELED` listeners hear it.
    fn break_block(&self, host: &mut dyn ServerHost, event: &mut BreakBlockEvent<'_>) -> Flow {
        let context = self.block_context(
            host,
            event.pos,
            event.state,
            None,
            None,
        );
        let fired = self.fire_event(
            GameEvent::BlockBreakBefore,
            self.dedicated(),
            context.clone(),
            |_| true,
        );
        if fired.cancelled {
            self.fire_event(
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
        let fired = self.fire_event(
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
            off_hand: click.hand == Hand::Off,
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
        if !matches!(host, HostSide::Server(_)) {
            return Flow::Continue;
        }
        let click = event.click;
        let context = EventContext {
            level: Some(dimension_id(click.dimension).to_owned()),
            player: Some(click.player),
            pos: Some(click.pos),
            off_hand: click.hand == Hand::Off,
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
        let fired = self.fire_event(
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
            off_hand: matches!(self.acting_hand(host, event.stack), Some(Hand::Off)),
            entity: Some(FacadeEntity {
                id: event.entity,
                kind: event.kind.to_owned(),
                level: None,
            }),
            ..EventContext::default()
        };
        let fired = self.fire_event(GameEvent::UseEntity, self.dedicated(), context, |_| true);
        if fired.cancelled {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }

    /// A living entity's death has finished (Fabric's `ServerLivingEntityEvents.AFTER_DEATH`): the
    /// loot, experience and death message went out. The runtime's `living_after_death` event; a
    /// death `living_death` cancelled never reaches it.
    fn living_death_post(&self, _host: &mut dyn ServerHost, event: &mut LivingDeathPostEvent<'_>) {
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
        self.fire_event(
            GameEvent::LivingAfterDeath,
            self.dedicated(),
            context,
            |_| true,
        );
    }

    /// A living entity is about to die (Fabric's `ServerLivingEntityEvents.ALLOW_DEATH`, NeoForge's
    /// `LivingDeathEvent`).
    fn living_death(&self, _host: &mut dyn ServerHost, event: &mut LivingDeathEvent<'_>) -> Flow {
        let context = EventContext {
            entity: Some(FacadeEntity {
                id: event.entity as i32,
                kind: event.kind.to_owned(),
                level: None,
            }),
            damage: Some(event.damage_type.to_owned()),
            ..EventContext::default()
        };
        let fired = self.fire_event(
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
        _host: &mut dyn ServerHost,
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
        let fired = self.fire_event(GameEvent::EntityLoad, self.dedicated(), context, |_| true);
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
    /// it matches exactly one of the player's two hands. When both hold the same stack (both
    /// empty counts) vanilla tries the main hand first, which is what `None` means to the caller.
    fn acting_hand(&self, host: &dyn ServerHost, stack: Option<&ItemStack>) -> Option<Hand> {
        let player = host.players().acting_player()?;
        let living = host.living(player.entity())?;
        let main = living.hand_item(host, Hand::Main);
        let off = living.hand_item(host, Hand::Off);
        let holds = |hand: &Option<ItemStack>| match (stack, hand.as_ref()) {
            (Some(held), Some(hand)) => held == hand,
            (Some(_), None) => false,
            (None, hand) => hand.is_none(),
        };
        match (holds(&main), holds(&off)) {
            (true, false) => Some(Hand::Main),
            (false, true) => Some(Hand::Off),
            _ => None,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Fires one runtime event to every jar's recorded listeners, and says what happened.
    fn fire(&self, event: GameEvent, dedicated: bool, wanted: impl Fn(&GameListener) -> bool) {
        self.fire_event(event, dedicated, EventContext::default(), wanted);
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
        let mut state = self.lock();
        let facade = Arc::new(EventFacade {
            dedicated,
            tick_count: state.tick_count,
            players: state.players.clone(),
            ..EventFacade::default()
        });
        let mut firing = EventFiring::new(event, facade, context);
        let (mut ran, mut failed, mut unknown) = (0, 0, 0);
        for loaded in &mut state.jars {
            let id = loaded.row.id.clone();
            let before = loaded.host.unknown_host_call_count();
            let (jar_ran, jar_failed) = run_listeners(
                &loaded.jar,
                &mut loaded.host,
                &loaded.listeners,
                &mut firing,
                &wanted,
            );
            let jar_unknown = loaded
                .host
                .unknown_host_call_count()
                .saturating_sub(before);
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
            off_hand: off_hand.unwrap_or(false),
            ..EventContext::default()
        }
    }
}

/// The dimension id of a hook's `LevelDimension`.
fn dimension_id(dimension: LevelDimension) -> &'static str {
    dimension.key().id()
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

/// Runs the listeners of this jar that `wanted` keeps, in registration order, against one shared
/// firing.
fn run_listeners(
    jar: &NativeJar,
    host: &mut StdHost,
    listeners: &[GameListener],
    firing: &mut EventFiring,
    wanted: &dyn Fn(&GameListener) -> bool,
) -> (usize, usize) {
    let (mut ran, mut failed) = (0, 0);
    let event = firing.event;
    for listener in listeners
        .iter()
        .filter(|listener| listener.event == event && wanted(listener))
    {
        match jar.run_game_listener(
            listener,
            firing,
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
        if let MessageKind::Player(uuid) = message.kind {
            if let Some(player) = host.player(uuid) {
                let _ = player.tell(host, text);
                continue;
            }
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

/// The runtime's view of the command source.
///
/// Every non-player source runs as the console (the façade has no separate block, cart or
/// function origin). Its level is the overworld, which is vanilla's server-console source level:
/// correct, not a guess.
///
/// A player's dimension is no mod API read (the fork seam in `docs/mod-plan.md`), so a player
/// source's level is unknown. `CommandSourceStack.getLevel()` then goes through the runtime's
/// unknown-host-call path — counted and failing the command visibly — instead of quietly acting on
/// the overworld while the player stands in the Nether.
fn runtime_source(source: &CommandSource, host: &dyn Host) -> RuntimeSource {
    let level = source.permission().level();
    match source {
        CommandSource::Player { player, .. } => RuntimeSource {
            name: player.name(host).unwrap_or_else(|| "Player".to_owned()),
            level,
            player: Some(player.uuid()),
            dimension: None,
        },
        _ => RuntimeSource {
            name: "Server".to_owned(),
            level,
            player: None,
            dimension: Some("minecraft:overworld".to_owned()),
        },
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
                    match self
                        .jar
                        .requirement_level(requirement, true, self.host, &self.budget)
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
    /// Commands that ran or were refused, and the unmodelled host calls their bodies made (a
    /// player source's `getLevel()`, for one).
    commands_ran: usize,
    commands_refused: usize,
    command_unknown_host_calls: usize,
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
            "commands": {
                "ran": self.commands_ran,
                "refused": self.commands_refused,
                "unknown_host_call_count": self.command_unknown_host_calls,
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
                    commands_ran: 0,
                    commands_refused: 0,
                    command_unknown_host_calls: 0,
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
            commands_ran: 0,
            commands_refused: 0,
            command_unknown_host_calls: 0,
        },
        jar,
        host,
        listeners,
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
        .map(|jar| jar.row.json())
        .chain(state.refused.iter().map(JarRow::json))
        .collect();
    let report = json!({ "schema": SCHEMA, "jars": rows });
    let mut bytes = serde_json::to_vec_pretty(&report).unwrap_or_default();
    bytes.push(b'\n');
    host.config_files().write(&path, &bytes)?;
    Ok(format!("config/{REPORT}"))
}
