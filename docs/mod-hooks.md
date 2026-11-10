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
| `entity_load` | `entity_join_level` (`EntityJoinLevelEvent`) | cancel | wired |
| `living_allow_death` | `living_death` (`LivingDeathEvent`) | cancel | wired |
| `living_after_death` | `living_death_post` (no NeoForge counterpart; `LivingDeathEvent` fires before the death) | listen | wired |
| `use_block` | `right_click_block` (`PlayerInteractEvent.RightClickBlock`) | cancel + result | wired |
| `use_item` | `right_click_item` (`PlayerInteractEvent.RightClickItem`) | cancel + result | wired |
| `attack_entity` | `attack_entity` (`AttackEntityEvent`) | cancel | wired |
| `block_break_before` | `break_block` (`BreakBlockEvent`) | cancel | wired |
| `block_break_after` | — (Fabric `PlayerBlockBreakEvents.AFTER`) | listen | missing on fork |
| `attack_block` | `left_click_block` (`PlayerInteractEvent.LeftClickBlock`) | cancel | wired |
| `use_entity` | `entity_interact` (`PlayerInteractEvent.EntityInteract`) | cancel | wired |
| `block_break_canceled` | `break_block`'s cancel path (Fabric `PlayerBlockBreakEvents.CANCELED`) | listen | wired |
| `entity_unload` | `entity_leave_level` (`EntityLeaveLevelEvent`) | listen | wired |
| `living_allow_damage` | `living_incoming_damage` (`LivingIncomingDamageEvent`) | cancel | wired |
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

**Counts: 29 wired, 1 missing on fork, 11 can't map.** The five M2a lifecycle/command rows are
included; M2c adds 23 (13 listen-only, 10 cancellable), and `living_after_death` closes one of the
two gaps M2c left (M2d).

## How a wired event reaches a Java listener

The mod builds one `EventFiring` per hook call and runs every jar's recorded listeners of that
event against it, in registration order, so a Fabric `false`/`InteractionResult` or a NeoForge
`setCanceled(true)` carries from one listener to the next as the loaders deliver it. What a
listener may cancel, and what it may answer, is the hook's contract:

- **Cancel** (`break_block`, `left_click_block`, `right_click_block`, `right_click_item`,
  `attack_entity`, `entity_interact`, `living_death`, `living_incoming_damage`,
  `entity_join_level`): the `Flow::Cancel` return is the firing's cancellation. `break_block` and
  the two click hooks also fire the Fabric `InteractionResult` that cancelled them: `SUCCESS`,
  `SUCCESS_SERVER`, `CONSUME` and `FAIL` become the mod API's `InteractionResult`; a
  NeoForge cancellation with no result stays `PASS`, the hook's documented default.
- **Listen** (`server_tick`, the level ticks, the level and player lifecycle events,
  `living_damage_post`, `living_death_post`, `entity_leave_level`): every listener runs; nothing
  comes back.

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
- **`entity_interact`'s hand is read from the held stack.** `entity_interact` fires once per hand
  and names the stack in that hand (`self.hand` in its contract), but the pin's event carries the
  stack and not the hand. The mod compares the stack with the acting player's two hands
  (`LivingRef::hand_item`): a stack only one hand holds names that hand. When both hands hold the
  same stack — both empty counts — the Java callback gets the main hand, vanilla's first attempt.
  This is a read, not a guess, but an off-hand click with two identical stacks is indistinguishable
  from a main-hand one to this hook.
- **A player who joins has a name and UUID, not a level or entity id.** The join hook gives those
  two facts; `getId()` and `level()` refuse (and are counted) until the fork reads them.
- **The tick events log only when something went wrong.** Twenty firings a second would drown the
  log; the report still counts every callback.

## The in-game proof

`tools/java_mod_fixture.py --kind hooks` builds a jar whose listeners print, and the run's log
shows the forwarded events:

```
Horizon Java: m2c_fixture: m2c: joined Player89790
Horizon Java: player_join (dedicated=false): 1 callback(s) ran, 0 failed, 0 unknown host call(s)
Horizon Java: m2c_fixture: m2c: 100 server ticks
Horizon Java: m2c_fixture: m2c: kept stone at -7,65,0
Horizon Java: block_break_before (dedicated=false): 1 callback(s) ran, 0 failed, 0 unknown host call(s)
Horizon Java: m2c_fixture: m2c: break canceled at -7,65,0
Horizon Java: block_break_canceled (dedicated=false): 1 callback(s) ran, 0 failed, 0 unknown host call(s)
```

`horizon-ctl … break -7 65 0` on the stone answers `{"block":"minecraft:stone",
"block_before":"minecraft:stone"}` and `block -7 65 0` still reports the stone. Replacing it with
dirt and breaking it answers `{"block":"minecraft:air","block_before":"minecraft:dirt"}`, so the
cancel is scoped to the block the listener named. The jar's report row carries `callbacks.ran`,
`callbacks.failed` and `callbacks.unknown_host_call_count` over every event.

## Fork seams

The gaps above and the arguments the runtime cannot fill are listed under **Fork seams needed** in
[docs/mod-plan.md](mod-plan.md), with the Fabric/NeoForge counterpart of each.
