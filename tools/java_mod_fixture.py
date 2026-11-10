#!/usr/bin/env python3
"""Build a tiny Fabric mod jar whose entrypoint runs on the native runtime.

Each proof needs a real jar that does something the runtime models, without shipping a jar in the
repository or downloading one: this writes one small Java source, compiles it with the ``javac``
on ``PATH`` and zips it with Fabric metadata.

    python3 tools/java_mod_fixture.py --out /tmp/config/horizon_java/java-mods
    python3 tools/java_mod_fixture.py --kind lifecycle --out ...
    python3 tools/java_mod_fixture.py --kind command --out ...

``m0`` (the default) is milestone 1's jar: a bounded loop, a string concatenation,
``Optional.filter`` and a ``HashMap`` round trip, so its run reports host calls and no unmodelled
one (the definition of ``ran-clean`` in the mod's report). ``lifecycle`` registers Fabric
``SERVER_STARTED`` and ``SERVER_STOPPING`` listeners that print a line, which milestone 2a shows
in the game log. ``command`` registers ``/javahello [name]`` through Fabric's
``CommandRegistrationCallback``, which milestone 2b runs in the game.

The loader and game classes are compiled as stubs so ``javac`` accepts the mod, but only the
mod's own classes go into the jar: the native host answers the API classes, exactly as it does
for a real jar. The jar is a test input, never a committed artifact.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

# Common stubs every fixture compiles against; none of them enter the jar.
EVENT_STUB = """
package net.fabricmc.fabric.api.event;
public abstract class Event<T> {
    public void register(T listener) {}
}
"""

STUBS: dict[str, str] = {
    "net/fabricmc/fabric/api/event/Event.java": EVENT_STUB,
    "net/minecraft/server/MinecraftServer.java": """
package net.minecraft.server;
public class MinecraftServer {
    public int getTickCount() { return 0; }
}
""",
}

LIFECYCLE_STUBS: dict[str, str] = {
    "net/fabricmc/fabric/api/event/lifecycle/v1/ServerLifecycleEvents.java": """
package net.fabricmc.fabric.api.event.lifecycle.v1;
import net.fabricmc.fabric.api.event.Event;
import net.minecraft.server.MinecraftServer;
public final class ServerLifecycleEvents {
    public static final Event<ServerStarted> SERVER_STARTED = null;
    public static final Event<ServerStopping> SERVER_STOPPING = null;
    public interface ServerStarted { void onServerStarted(MinecraftServer server); }
    public interface ServerStopping { void onServerStopping(MinecraftServer server); }
}
""",
}

# The server tick, player join and block break callbacks milestone 2c fires from the game's hooks.
HOOK_STUBS: dict[str, str] = {
    "net/fabricmc/fabric/api/event/lifecycle/v1/ServerTickEvents.java": """
package net.fabricmc.fabric.api.event.lifecycle.v1;
import net.fabricmc.fabric.api.event.Event;
import net.minecraft.server.MinecraftServer;
public final class ServerTickEvents {
    public static final Event<StartTick> START_SERVER_TICK = null;
    public interface StartTick { void onStartTick(MinecraftServer server); }
}
""",
    "net/fabricmc/fabric/api/entity/event/v1/ServerPlayerEvents.java": """
package net.fabricmc.fabric.api.entity.event.v1;
import net.fabricmc.fabric.api.event.Event;
import net.minecraft.server.level.ServerPlayer;
public final class ServerPlayerEvents {
    public static final Event<Join> JOIN = null;
    public interface Join { void onJoin(ServerPlayer player); }
}
""",
    "net/fabricmc/fabric/api/event/player/PlayerBlockBreakEvents.java": """
package net.fabricmc.fabric.api.event.player;
import net.fabricmc.fabric.api.event.Event;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
public final class PlayerBlockBreakEvents {
    public static final Event<BeforeBlockBreak> BEFORE = null;
    public static final Event<AfterBlockBreak> AFTER = null;
    public static final Event<CanceledBlockBreak> CANCELED = null;
    public interface BeforeBlockBreak {
        boolean beforeBlockBreak(Level world, Player player, BlockPos pos, BlockState state,
            BlockEntity entity);
    }
    public interface AfterBlockBreak {
        void afterBlockBreak(Level world, Player player, BlockPos pos, BlockState state,
            BlockEntity entity);
    }
    public interface CanceledBlockBreak {
        void onBlockBreakCanceled(Level world, Player player, BlockPos pos, BlockState state,
            BlockEntity entity);
    }
}
""",
    "net/minecraft/server/level/ServerPlayer.java": """
package net.minecraft.server.level;
import net.minecraft.world.entity.player.Player;
public class ServerPlayer extends Player {
    public String getScoreboardName() { return null; }
}
""",
    "net/minecraft/world/entity/player/Player.java": """
package net.minecraft.world.entity.player;
public class Player {}
""",
    "net/minecraft/world/level/Level.java": """
package net.minecraft.world.level;
public class Level {}
""",
    "net/minecraft/core/BlockPos.java": """
package net.minecraft.core;
public class BlockPos {
    public int getX() { return 0; }
    public int getY() { return 0; }
    public int getZ() { return 0; }
}
""",
    "net/minecraft/world/level/block/Block.java": """
package net.minecraft.world.level.block;
public class Block {}
""",
    "net/minecraft/world/level/block/Blocks.java": """
package net.minecraft.world.level.block;
public final class Blocks {
    public static final Block STONE = null;
}
""",
    "net/minecraft/world/level/block/state/BlockState.java": """
package net.minecraft.world.level.block.state;
import net.minecraft.world.level.block.Block;
public class BlockState {
    public boolean is(Block block) { return false; }
}
""",
    "net/minecraft/world/level/block/entity/BlockEntity.java": """
package net.minecraft.world.level.block.entity;
public class BlockEntity {}
""",
}

COMMAND_STUBS: dict[str, str] = {
    "net/fabricmc/fabric/api/command/v2/CommandRegistrationCallback.java": """
package net.fabricmc.fabric.api.command.v2;
import net.fabricmc.fabric.api.event.Event;
public interface CommandRegistrationCallback {
    Event<CommandRegistrationCallback> EVENT = null;
    void register(com.mojang.brigadier.CommandDispatcher<net.minecraft.commands.CommandSourceStack> dispatcher,
        net.minecraft.commands.CommandBuildContext context,
        net.minecraft.commands.Commands.CommandSelection selection);
}
""",
    "com/mojang/brigadier/CommandDispatcher.java": """
package com.mojang.brigadier;
public class CommandDispatcher<S> {
    public com.mojang.brigadier.tree.LiteralCommandNode<S> register(
        com.mojang.brigadier.builder.LiteralArgumentBuilder<S> builder) { return null; }
}
""",
    "com/mojang/brigadier/Command.java": """
package com.mojang.brigadier;
public interface Command<S> {
    int run(com.mojang.brigadier.context.CommandContext<S> context)
        throws com.mojang.brigadier.exceptions.CommandSyntaxException;
}
""",
    "com/mojang/brigadier/exceptions/CommandSyntaxException.java": """
package com.mojang.brigadier.exceptions;
public class CommandSyntaxException extends Exception {}
""",
    "com/mojang/brigadier/context/CommandContext.java": """
package com.mojang.brigadier.context;
public class CommandContext<S> { public S getSource() { return null; } }
""",
    "com/mojang/brigadier/tree/LiteralCommandNode.java": """
package com.mojang.brigadier.tree;
public class LiteralCommandNode<S> {}
""",
    "com/mojang/brigadier/builder/ArgumentBuilder.java": """
package com.mojang.brigadier.builder;
public abstract class ArgumentBuilder<S, T extends ArgumentBuilder<S, T>> {
    public T then(ArgumentBuilder<S, ?> child) { return null; }
    public T executes(com.mojang.brigadier.Command<S> command) { return null; }
    public T requires(java.util.function.Predicate<S> predicate) { return null; }
}
""",
    "com/mojang/brigadier/builder/LiteralArgumentBuilder.java": """
package com.mojang.brigadier.builder;
public class LiteralArgumentBuilder<S> extends ArgumentBuilder<S, LiteralArgumentBuilder<S>> {}
""",
    "com/mojang/brigadier/builder/RequiredArgumentBuilder.java": """
package com.mojang.brigadier.builder;
public class RequiredArgumentBuilder<S, T> extends ArgumentBuilder<S, RequiredArgumentBuilder<S, T>> {}
""",
    "com/mojang/brigadier/arguments/ArgumentType.java": """
package com.mojang.brigadier.arguments;
public interface ArgumentType<T> {}
""",
    "com/mojang/brigadier/arguments/StringArgumentType.java": """
package com.mojang.brigadier.arguments;
public class StringArgumentType implements ArgumentType<String> {
    public static StringArgumentType word() { return null; }
    public static String getString(com.mojang.brigadier.context.CommandContext<?> context, String name) {
        return null;
    }
}
""",
    "net/minecraft/commands/CommandBuildContext.java": """
package net.minecraft.commands;
public interface CommandBuildContext {}
""",
    "net/minecraft/commands/Commands.java": """
package net.minecraft.commands;
import com.mojang.brigadier.builder.*;
public class Commands {
    public enum CommandSelection { ALL, DEDICATED, INTEGRATED }
    public static LiteralArgumentBuilder<CommandSourceStack> literal(String name) { return null; }
    public static <T> RequiredArgumentBuilder<CommandSourceStack, T> argument(
        String name, com.mojang.brigadier.arguments.ArgumentType<T> type) { return null; }
}
""",
    "net/minecraft/commands/CommandSourceStack.java": """
package net.minecraft.commands;
public class CommandSourceStack {
    public void sendSuccess(java.util.function.Supplier<net.minecraft.network.chat.Component> supplier,
        boolean broadcast) {}
    public net.minecraft.server.level.ServerLevel getLevel() { return null; }
}
""",
    "net/minecraft/server/level/ServerLevel.java": """
package net.minecraft.server.level;
public class ServerLevel {}
""",
    "net/minecraft/network/chat/Component.java": """
package net.minecraft.network.chat;
public interface Component {
    static MutableComponent literal(String text) { return null; }
}
""",
    "net/minecraft/network/chat/MutableComponent.java": """
package net.minecraft.network.chat;
public class MutableComponent implements Component {}
""",
}

# The mod id, entrypoint class and jar name per fixture; the mod's log line names the id.
FIXTURES: dict[str, dict[str, str]] = {
    "m0": {
        "id": "m0_fixture",
        "entry": "m0fixture.Hello",
        "jar": "m0_fixture.jar",
        "stubs": {},
        "source": """
package m0fixture;

import java.util.HashMap;
import java.util.Map;
import java.util.Optional;

public final class Hello {
    public void onInitialize() {
        // A bounded loop: arithmetic and control flow the runtime interprets.
        int sum = 0;
        for (int i = 1; i <= 6; i++) {
            sum += i;
        }
        // JDK calls the runtime models through its host, so the row is not empty.
        Optional<String> text = Optional.ofNullable("m0-" + sum);
        boolean ok = text.filter(value -> value.length() == 5).isPresent();
        Map<String, Integer> seen = new HashMap<>();
        seen.put("sum", sum);
        if (!ok || seen.getOrDefault("sum", 0) != 21) {
            throw new IllegalStateException("the fixture did not run");
        }
    }
}
""",
    },
    "lifecycle": {
        "id": "m2a_fixture",
        "entry": "m2afixture.Lifecycle",
        "jar": "m2a_fixture.jar",
        "stubs": LIFECYCLE_STUBS,
        "source": """
package m2afixture;

import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;

public final class Lifecycle {
    public void onInitialize() {
        ServerLifecycleEvents.SERVER_STARTED.register(server ->
            System.out.println("m2a: SERVER_STARTED ran"));
        ServerLifecycleEvents.SERVER_STOPPING.register(server ->
            System.out.println("m2a: SERVER_STOPPING ran"));
    }
}
""",
    },
    "command": {
        "id": "m2b_fixture",
        "entry": "m2bfixture.HelloCommand",
        "jar": "m2b_fixture.jar",
        "stubs": COMMAND_STUBS,
        "source": """
package m2bfixture;

import com.mojang.brigadier.arguments.StringArgumentType;
import net.fabricmc.fabric.api.command.v2.CommandRegistrationCallback;
import net.minecraft.commands.Commands;
import net.minecraft.network.chat.Component;

public final class HelloCommand {
    public void onInitialize() {
        CommandRegistrationCallback.EVENT.register((dispatcher, context, selection) -> {
            dispatcher.register(Commands.literal("javahello")
                .executes(command -> {
                    command.getSource().sendSuccess(() -> Component.literal("Hello, world!"), false);
                    return 1;
                })
                .then(Commands.argument("name", StringArgumentType.word())
                    .executes(command -> {
                        String name = StringArgumentType.getString(command, "name");
                        command.getSource().sendSuccess(
                            () -> Component.literal("Hello, " + name + "!"), false);
                        return 1;
                    })));
            // Reads the source's level, which a player source cannot answer (no dimension read):
            // the command must fail with the unknown host call counted, never act on a guessed
            // level.
            dispatcher.register(Commands.literal("javawhere").executes(command -> {
                Object level = command.getSource().getLevel();
                command.getSource().sendSuccess(() -> Component.literal("level " + level), false);
                return 1;
            }));
        });
    }
}
""",
    },
    "hooks": {
        "id": "m2c_fixture",
        "entry": "m2cfixture.Events",
        "jar": "m2c_fixture.jar",
        "stubs": HOOK_STUBS,
        "source": """
package m2cfixture;

import net.fabricmc.fabric.api.entity.event.v1.ServerPlayerEvents;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerTickEvents;
import net.fabricmc.fabric.api.event.player.PlayerBlockBreakEvents;
import net.minecraft.world.level.block.Blocks;

public final class Events {
    private static int ticks;

    public void onInitialize() {
        // Count server ticks here and say so once, so a bounded run shows the
        // tick hook firing every tick without a line for each one.
        ServerTickEvents.START_SERVER_TICK.register(server -> {
            ticks++;
            if (ticks == 100) {
                System.out.println("m2c: 100 server ticks");
            }
        });
        ServerPlayerEvents.JOIN.register(player ->
            System.out.println("m2c: joined " + player.getScoreboardName()));
        // Keep stone: the cancelled break leaves the block where it was.
        PlayerBlockBreakEvents.BEFORE.register((world, player, pos, state, entity) -> {
            if (state.is(Blocks.STONE)) {
                System.out.println("m2c: kept stone at " + pos.getX() + "," + pos.getY()
                    + "," + pos.getZ());
                return false;
            }
            return true;
        });
        PlayerBlockBreakEvents.CANCELED.register((world, player, pos, state, entity) ->
            System.out.println("m2c: break canceled at " + pos.getX() + "," + pos.getY()
                + "," + pos.getZ()));
        // Fabric's AFTER has no mod API hook yet (docs/mod-hooks.md), so this
        // listener is recorded but the mod never fires it.
        PlayerBlockBreakEvents.AFTER.register((world, player, pos, state, entity) ->
            System.out.println("m2c: broke a block at " + pos.getX() + "," + pos.getY()
                + "," + pos.getZ()));
    }
}
""",
    },
    # Milestone 2d: the death-after hook, and the hand an entity click carries.
    "m2d": {
        "id": "m2d_fixture",
        "entry": "m2dfixture.Events",
        "jar": "m2d_fixture.jar",
        "stubs": {
            **HOOK_STUBS,
            "net/fabricmc/fabric/api/entity/event/v1/ServerLivingEntityEvents.java": """
package net.fabricmc.fabric.api.entity.event.v1;
import net.fabricmc.fabric.api.event.Event;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.LivingEntity;
public final class ServerLivingEntityEvents {
    public static final Event<AfterDeath> AFTER_DEATH = null;
    public interface AfterDeath { void afterDeath(LivingEntity entity, DamageSource source); }
}
""",
            "net/fabricmc/fabric/api/event/player/UseEntityCallback.java": """
package net.fabricmc.fabric.api.event.player;
import net.fabricmc.fabric.api.event.Event;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.EntityHitResult;
public final class UseEntityCallback {
    public static final Event<UseEntity> EVENT = null;
    public interface UseEntity {
        InteractionResult interact(Player player, Level world, InteractionHand hand, Entity entity,
            EntityHitResult hit);
    }
}
""",
            "net/minecraft/world/InteractionHand.java": """
package net.minecraft.world;
public enum InteractionHand { MAIN_HAND, OFF_HAND }
""",
            "net/minecraft/world/InteractionResult.java": """
package net.minecraft.world;
public class InteractionResult {
    public static final InteractionResult PASS = null;
}
""",
            "net/minecraft/world/entity/Entity.java": """
package net.minecraft.world.entity;
public class Entity {
    public int getId() { return 0; }
}
""",
            "net/minecraft/world/entity/LivingEntity.java": """
package net.minecraft.world.entity;
public class LivingEntity extends Entity {}
""",
            "net/minecraft/world/damagesource/DamageSource.java": """
package net.minecraft.world.damagesource;
public class DamageSource {}
""",
            "net/minecraft/world/phys/EntityHitResult.java": """
package net.minecraft.world.phys;
public class EntityHitResult {}
""",
        },
        "source": """
package m2dfixture;

import net.fabricmc.fabric.api.entity.event.v1.ServerLivingEntityEvents;
import net.fabricmc.fabric.api.event.player.UseEntityCallback;
import net.minecraft.world.InteractionResult;

public final class Events {
    public void onInitialize() {
        // Fabric's AFTER_DEATH: the mob's id after its loot and death message.
        ServerLivingEntityEvents.AFTER_DEATH.register((entity, source) ->
            System.out.println("m2d: after death id=" + entity.getId()));
        // Read the hand: a named hand answers its name; an unknown one is counted by the
        // runtime and the listener still runs (the name reads as null).
        UseEntityCallback.EVENT.register((player, world, hand, entity, hit) -> {
            System.out.println("m2d: interact hand=" + hand.name()
                + " id=" + entity.getId());
            return InteractionResult.PASS;
        });
    }
}
""",
    },
}

# Kept for callers that imported milestone 1's names.
MOD_ID = FIXTURES["m0"]["id"]
ENTRY = FIXTURES["m0"]["entry"]
JAR = FIXTURES["m0"]["jar"]


def build(out: Path, kind: str = "m0") -> Path:
    """Compiles the ``kind`` fixture and writes its jar under ``out``; returns the jar."""
    fixture = FIXTURES[kind]
    if shutil.which("javac") is None:
        raise SystemExit("javac is required to build the fixture")
    out.mkdir(parents=True, exist_ok=True)
    jar = out / fixture["jar"]
    sources = {**STUBS, **fixture["stubs"]}
    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        paths = []
        for name, text in sources.items():
            source = root / "src" / name
            source.parent.mkdir(parents=True, exist_ok=True)
            source.write_text(text.lstrip(), encoding="utf-8")
            paths.append(source)
        stubs = root / "stubs"
        stubs.mkdir()
        compiled = subprocess.run(
            ["javac", "--release", "17", "-d", str(stubs), *map(str, paths)],
            text=True,
            capture_output=True,
        )
        if compiled.returncode:
            raise SystemExit(compiled.stderr or compiled.stdout)
        entry = fixture["entry"].replace(".", "/")
        source = root / "src" / f"{entry}.java"
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text(fixture["source"].lstrip(), encoding="utf-8")
        classes = root / "classes"
        classes.mkdir()
        compiled = subprocess.run(
            [
                "javac",
                "--release",
                "17",
                "-cp",
                str(stubs),
                "-d",
                str(classes),
                str(source),
            ],
            text=True,
            capture_output=True,
        )
        if compiled.returncode:
            raise SystemExit(compiled.stderr or compiled.stdout)
        metadata = {
            "schemaVersion": 1,
            "id": fixture["id"],
            "version": "1.0.0",
            "name": f"{kind} fixture",
            "depends": {"minecraft": "26.3", "fabricloader": "*"},
            "entrypoints": {"main": [fixture["entry"]]},
        }
        # A Fabric mod jar is a zip: the entrypoint class plus `fabric.mod.json` at its root.
        # The stubs are compile-time only; the native host answers those classes.
        with zipfile.ZipFile(jar, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("fabric.mod.json", json.dumps(metadata, indent=2))
            for class_file in sorted(classes.rglob("*.class")):
                archive.write(class_file, class_file.relative_to(classes).as_posix())
    return jar


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, required=True, help="where the fixture jar goes")
    parser.add_argument(
        "--kind",
        choices=sorted(FIXTURES),
        default="m0",
        help="which fixture to build (default: m0)",
    )
    args = parser.parse_args(argv)
    jar = build(args.out, args.kind)
    print(jar)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
