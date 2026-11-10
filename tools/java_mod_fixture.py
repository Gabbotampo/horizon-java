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
``CommandRegistrationCallback``, which milestone 2b runs in the game. ``content`` registers one
plain item and one plain block, which milestone 3 turns into game content (the runtime infers a
``minecraft:stick`` item and a ``minecraft:stone`` block), and ships the jar's own assets, name
and loot for the milestone 3b bake: a generated 16x16 ruby texture and ruby block texture, their
item definitions, models and blockstate, ``lang/en_us.json`` (Ruby, Block of Ruby), and the block's
loot table dropping one ruby.

The loader and game classes are compiled as stubs so ``javac`` accepts the mod, but only the
mod's own classes go into the jar: the native host answers the API classes, exactly as it does
for a real jar. The jar is a test input, never a committed artifact.
"""

from __future__ import annotations

import argparse
import json
import shutil
import struct
import subprocess
import sys
import tempfile
import zipfile
import zlib
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

CONTENT_STUBS: dict[str, str] = {
    "net/minecraft/core/Registry.java": """
package net.minecraft.core;
public interface Registry<T> {
    static <V, T extends V> T register(Registry<V> registry,
        net.minecraft.resources.Identifier location, T value) { return value; }
}
""",
    "net/minecraft/core/registries/Registries.java": """
package net.minecraft.core.registries;
public final class Registries {
    public static final net.minecraft.core.Registry<net.minecraft.world.item.Item> ITEM = null;
    public static final net.minecraft.core.Registry<net.minecraft.world.level.block.Block> BLOCK = null;
}
""",
    "net/minecraft/resources/Identifier.java": """
package net.minecraft.resources;
public final class Identifier {
    public static Identifier fromNamespaceAndPath(String namespace, String path) { return null; }
}
""",
    "net/minecraft/world/item/Item.java": """
package net.minecraft.world.item;
public class Item {
    public Item(Item.Properties properties) {}
    public static class Properties {}
}
""",
    "net/minecraft/world/level/block/state/BlockBehaviour.java": """
package net.minecraft.world.level.block.state;
public class BlockBehaviour {
    public static class Properties {
        public static BlockBehaviour.Properties of() { return null; }
    }
}
""",
    "net/minecraft/world/level/block/Block.java": """
package net.minecraft.world.level.block;
public class Block {
    public Block(net.minecraft.world.level.block.state.BlockBehaviour.Properties properties) {}
}
""",
}

# The 16x16 art the content fixture ships, generated here so no binary is committed. The game's
# content bake copies it; the proof checks that this exact colour reaches the hotbar.
RUBY_BASE = (0xB3, 0x1F, 0x3E, 255)
RUBY_DARK = (0x5E, 0x0B, 0x1F, 255)
RUBY_LIGHT = (0xE8, 0x5A, 0x78, 255)


def png(pixels: list[list[tuple[int, int, int, int]]]) -> bytes:
    """A minimal RGBA PNG: the fixture's own texture, deterministic and dependency-free."""
    height, width = len(pixels), len(pixels[0])
    raw = b"".join(b"\x00" + b"".join(bytes(pixel) for pixel in row) for row in pixels)

    def chunk(kind: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def ruby_item_texture() -> bytes:
    """A 16x16 cut gem: dark outline, ruby fill, a light diagonal facet."""
    pixels = []
    for y in range(16):
        row = []
        for x in range(16):
            edge = x in (2, 15) or y in (1, 14) or x - y >= 13 or y - x >= 13
            if edge:
                row.append(RUBY_DARK)
            elif y == x or y == x + 1:
                row.append(RUBY_LIGHT)
            else:
                row.append(RUBY_BASE)
        pixels.append(row)
    return png(pixels)


def ruby_block_texture() -> bytes:
    """A 16x16 ruby block: a framed face with a faceted middle, the same palette."""
    pixels = []
    for y in range(16):
        row = []
        for x in range(16):
            border = x in (0, 15) or y in (0, 15)
            inner_border = x in (1, 14) or y in (1, 14)
            facet = x in (7, 8) or y in (7, 8)
            if border:
                row.append(RUBY_DARK)
            elif inner_border or facet:
                row.append(RUBY_LIGHT)
            else:
                row.append(RUBY_BASE)
        pixels.append(row)
    return png(pixels)


def content_assets() -> dict[str, bytes]:
    """The content fixture's jar assets: model, item definition, texture, name and loot."""
    cube = {
        "parent": "minecraft:block/cube_all",
        "textures": {"all": "m3_fixture:block/ruby_block"},
    }
    return {
        "assets/m3_fixture/textures/item/ruby.png": ruby_item_texture(),
        "assets/m3_fixture/textures/block/ruby_block.png": ruby_block_texture(),
        "assets/m3_fixture/items/ruby.json": (
            b'{"model":{"type":"minecraft:model","model":"m3_fixture:item/ruby"}}'
        ),
        "assets/m3_fixture/items/ruby_block.json": (
            b'{"model":{"type":"minecraft:model","model":"m3_fixture:block/ruby_block"}}'
        ),
        "assets/m3_fixture/models/item/ruby.json": (
            b'{"parent":"minecraft:item/generated",'
            b'"textures":{"layer0":"m3_fixture:item/ruby"}}'
        ),
        "assets/m3_fixture/models/block/ruby_block.json": json.dumps(cube).encode(),
        "assets/m3_fixture/blockstates/ruby_block.json": (
            b'{"variants":{"":{"model":"m3_fixture:block/ruby_block"}}}'
        ),
        "assets/m3_fixture/lang/en_us.json": (
            b'{"item.m3_fixture.ruby":"Ruby",'
            b'"block.m3_fixture.ruby_block":"Block of Ruby"}'
        ),
        # A stone-template block keeps stone's "needs the right tool" rule, so the jar names its
        # own block as pickaxe-mineable; without this a pickaxe would break it and drop nothing.
        "data/minecraft/tags/block/mineable/pickaxe.json": (
            b'{"values":["m3_fixture:ruby_block"]}'
        ),
        "data/m3_fixture/loot_table/blocks/ruby_block.json": json.dumps(
            {
                "type": "minecraft:block",
                "pools": [
                    {
                        "rolls": 1,
                        "entries": [{"type": "minecraft:item", "name": "m3_fixture:ruby"}],
                    }
                ],
                "random_sequence": "m3_fixture:blocks/ruby_block",
            }
        ).encode(),
    }


# The mod id, entrypoint class and jar name per fixture; the mod's log line names the id.
FIXTURES: dict[str, dict] = {
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
    "content": {
        "id": "m3_fixture",
        "entry": "m3fixture.Content",
        "jar": "m3_fixture.jar",
        "stubs": CONTENT_STUBS,
        "assets": content_assets(),
        "source": """
package m3fixture;

import net.minecraft.core.Registry;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockBehaviour;

public final class Content {
    public void onInitialize() {
        // Milestone 3: one plain item and one plain block, each registered the way a real mod
        // registers content. The native runtime infers the declaration from the constructor's
        // property chain and the bridge turns it into the game's content.
        Registry.register(Registries.ITEM,
            Identifier.fromNamespaceAndPath("m3_fixture", "ruby"),
            new Item(new Item.Properties()));
        Registry.register(Registries.BLOCK,
            Identifier.fromNamespaceAndPath("m3_fixture", "ruby_block"),
            new Block(BlockBehaviour.Properties.of()));
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
            for name, data in sorted(fixture.get("assets", {}).items()):
                archive.writestr(name, data)
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
