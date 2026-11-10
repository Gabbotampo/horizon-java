# Game events and their mod API hooks

The runtime records listeners for 41 game events (`GameEvent` in
`crates/horizon-native-java/src/events.rs`): Fabric API event fields plus the NeoForge/Forge
event classes, each mapped to one runtime event. This page says, for every one of them, which
mod API hook (`arismoko/horizon`, crate `horizon-mod-api`, library `mod_api`) the mod fires it
from at the matching moment, what the hook's contract lets a Java listener do, and whether it is
wired. Moments are matched from the hook docs, not the names.

**Statuses:** *wired* — the mod fires the Java listeners from that hook, carrying the cancel or
result the hook exposes. *missing on fork* — the runtime models the moment but the mod API has no
hook for it. *can't map* — the mod API has a hook, but this mod cannot reach it faithfully.

| Runtime event | Mod API hook (NeoForge counterpart) | Contract | Status |
|---|---|---|---|
| `server_starting` | `server_about_to_start` (`ServerAboutToStartEvent`) | listen | wired (M2a) |
| `server_started` | `server_starting` (`ServerStartingEvent`), `server_started` (`ServerStartedEvent`) | listen | wired (M2a), split by the listener's recorded source |
| `server_stopping` | `server_stopping` (`ServerStoppingEvent`) | listen | wired (M2a) |
| `server_stopped` | `server_stopped` (`ServerStoppedEvent`) | listen | wired (M2a) |
| `server_tick_start` | `server_tick`, `ServerTickStart` phase (`ServerTickEvent.Pre`) | listen | wired |
| `server_tick_end` | `server_tick`, `ServerTickEnd` phase (`ServerTickEvent.Post`) | listen | wired |
| `level_tick_start` | `level_tick_pre` (`LevelTickEvent.Pre`) | listen | wired |
| `level_tick_end` | `level_tick_post` (`LevelTickEvent.Post`) | listen | wired |
| `level_load` | `level_load` (`LevelEvent.Load`) | listen | wired |
| `level_unload` | `level_unload` (`LevelEvent.Unload`) | listen | wired |
| `player_join` | `player_logged_in` (`PlayerEvent.PlayerLoggedInEvent`) | listen | wired |
| `player_leave` | `player_logged_out` (`PlayerEvent.PlayerLoggedOutEvent`) | listen | wired |
| `entity_load` | `entity_join_level` (`EntityJoinLevelEvent`) | cancel | pending (cancel PR) |
| `living_allow_death` | `living_death` (`LivingDeathEvent`) | cancel | pending (cancel PR) |
| `living_after_death` | — (Fabric `ServerLivingEntityEvents.AFTER_DEATH`) | listen | missing on fork |
| `use_block` | `right_click_block` (`PlayerInteractEvent.RightClickBlock`) | cancel + result | pending (cancel PR) |
| `use_item` | `right_click_item` (`PlayerInteractEvent.RightClickItem`) | cancel + result | pending (cancel PR) |
| `attack_entity` | `attack_entity` (`AttackEntityEvent`) | cancel | pending (cancel PR) |
| `block_break_before` | `break_block` (`BreakBlockEvent`) | cancel | pending (cancel PR) |
| `block_break_after` | — (Fabric `PlayerBlockBreakEvents.AFTER`) | listen | missing on fork |
| `attack_block` | `left_click_block` (`PlayerInteractEvent.LeftClickBlock`) | cancel | pending (cancel PR) |
| `use_entity` | `entity_interact` (`PlayerInteractEvent.EntityInteract`) | cancel | pending (cancel PR) |
| `block_break_canceled` | `break_block`'s cancel path (Fabric `PlayerBlockBreakEvents.CANCELED`) | listen | pending (cancel PR) |
| `entity_unload` | `entity_leave_level` (`EntityLeaveLevelEvent`) | listen | wired |
| `living_allow_damage` | `living_incoming_damage` (`LivingIncomingDamageEvent`) | cancel | pending (cancel PR) |
| `living_after_damage` | `living_damage_post` (`LivingDamageEvent.Post`) | listen | wired |
| `player_respawn` | `player_respawned` (`PlayerEvent.PlayerRespawnEvent`) | listen | wired |
| `player_copy` | `player_clone` (`PlayerEvent.Clone`) | listen | wired |
| `player_change_level` | `player_changed_dimension` (`PlayerEvent.PlayerChangedDimensionEvent`) | listen | wired |
| `register_commands` | `register_commands` (`RegisterCommandsEvent`) | listen | wired (M2b) |
| `client_started` | `client_setup` (`FMLClientSetupEvent`) | listen | can't map: no client jar runtime |
| `client_stopping` | `client_stopping` (`ClientStoppingEvent`) | listen | can't map: no client jar runtime |
| `client_tick_start` | `client_tick_pre` (`ClientTickEvent.Pre`) | listen | can't map: no client jar runtime |
| `client_tick_end` | `client_tick_post` (`ClientTickEvent.Post`) | listen | can't map: no client jar runtime |
| `client_level_tick_start` | — (Fabric `ClientTickEvents.START_LEVEL_TICK`; NeoForge has no per-level client tick) | listen | can't map: no client jar runtime |
| `client_level_tick_end` | — (Fabric `ClientTickEvents.END_LEVEL_TICK`) | listen | can't map: no client jar runtime |
| `client_join` | `client_player_logging_in` (`ClientPlayerNetworkEvent.LoggingIn`) | listen | can't map: no client jar runtime |
| `client_disconnect` | `client_player_logging_out` (`ClientPlayerNetworkEvent.LoggingOut`) | listen | can't map: no client jar runtime |
| `client_register_key_mappings` | — (`RegisterKeyMappingsEvent` is a call, not a hook) | listen | can't map: no client jar runtime |
| `client_register_gui_layers` | `register_gui_layers` (`RegisterGuiLayersEvent`) | collect | can't map: registration context, no client jar runtime |
| `client_item_tooltip` | `item_tooltip` (`ItemTooltipEvent`) | collect | can't map: no client jar runtime |

**This PR wires 18** (the five M2a lifecycle/command rows and 13 listen-only events); the ten
cancellable rows follow in the next PR, then read: 28 wired, 2 missing on fork, 11 can't map.

## How a wired event reaches a Java listener

The mod builds one `EventFiring` per hook call and runs every jar's recorded listeners of that
event against it, in registration order, so a Fabric `false`/`InteractionResult` or a NeoForge
`setCanceled(true)` carries from one listener to the next as the loaders deliver it. Every event
wired here is listen-only: every listener runs and nothing comes back. The ten cancellable events
(the two click hooks, item use, block break and its `CANCELED`, block and entity attack, living
death and damage, entity join) follow in the next PR, carrying the firing's `Flow::Cancel` and the
`InteractionResult` that cancelled it.

## Arguments

A hook's own arguments become the runtime's façade values: the player UUID and name, the block
position, the face, the block id (a `BlockState` answers `getBlock()` and `state.is(Blocks.STONE)`),
the dimension and the damage type. A hook that does not give an argument passes a marker whose
every member refuses; the Java callback still runs, and each call that needs the argument is
counted in that jar's `callbacks.unknown_host_call_count` instead of failing the listener or
inventing a value. The in-game proof uses that rule: a block break names the block and position
but no level, so a listener that ignores its `Level` still cancels the break.

## Departures from the loaders' behaviour

- **`break_block` does not carry `exp_to_drop`.** The runtime's `BlockBreakBefore` has no
  experience model; the hook's `exp_to_drop` is left `None`, so vanilla samples experience.
- **`entity_interact` runs as the main hand.** The mod API event does not name the hand that
  clicked; the runtime hands the Java callback the main hand for both hands' callbacks.
- **A player who joins has a name and UUID, not a level or entity id.** The join hook gives those
  two facts; `getId()` and `level()` refuse (and are counted) until the fork reads them.
- **The tick events log only when something went wrong.** Twenty firings a second would drown the
  log; the report still counts every callback.

## The in-game proof

`tools/java_mod_fixture.py --kind hooks` builds a jar whose tick and join listeners print, and the
run's log shows the forwarded events and the report's totals:

```
Horizon Java: m2c_fixture: m2c: joined Player89790
Horizon Java: player_join (dedicated=false): 1 callback(s) ran, 0 failed, 0 unknown host call(s)
Horizon Java: m2c_fixture: m2c: 100 server ticks
```

The stopped report's row is `"callbacks": {"failed": 0, "ran": 254, "unknown_host_call_count": 0}`
over the run (one join plus the ticks).

## Fork seams

The gaps above and the arguments the runtime cannot fill are listed under **Fork seams needed** in
[docs/mod-plan.md](mod-plan.md), with the Fabric/NeoForge counterpart of each.
