"""The fixture jars do what the mod's proofs need them to do, by the probe's own reading.

Milestone 1's jar is `ran-clean` by the mod's definition; milestone 2a's jar records
`SERVER_STARTED`/`SERVER_STOPPING` listeners and milestone 2b's registers a Brigadier command.
The probe shares the runtime the mod carries, so it sees the same listeners and tree the game
will; a fixture that stopped registering them would fail here before anyone reads a game log.
"""

import json
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def build(kind: str, out: Path) -> Path:
    built = subprocess.run(
        [
            sys.executable,
            str(ROOT / "tools" / "java_mod_fixture.py"),
            "--kind",
            kind,
            "--out",
            str(out),
        ],
        text=True,
        capture_output=True,
        timeout=180,
        check=False,
    )
    assert built.returncode == 0, built.stderr
    return Path(built.stdout.strip().splitlines()[-1])


def probe(jar: Path) -> dict:
    result = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "horizon-native-java",
            "--",
            "--json",
            str(jar),
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=900,
        check=False,
    )
    assert result.returncode == 0, result.stderr or result.stdout
    return json.loads(result.stdout.strip().splitlines()[-1])


class JavaModFixtureTests(unittest.TestCase):
    def require_tools(self) -> None:
        missing = [tool for tool in ("javac", "cargo") if shutil.which(tool) is None]
        if missing:
            self.skipTest(f"{', '.join(missing)} required for the fixture run")

    def test_the_fixture_runs_without_an_unmodelled_host_call(self):
        self.require_tools()
        with tempfile.TemporaryDirectory() as scratch:
            report = probe(build("m0", Path(scratch)))
        self.assertEqual(report["status"], "ok")
        # Fabric's common entrypoint, which both sides run.
        self.assertEqual(report["kind"], "main")
        self.assertEqual(report["entrypoints"], ["m0fixture.Hello"])
        # The fixture reaches the host (so the row is not a hollow zero) and
        # every call it makes is modelled (so the mod's verdict is `ran-clean`).
        self.assertGreater(report["host_calls"], 0)
        self.assertEqual(report["unknown_host_call_count"], 0, report["unknown_host_calls"])

    def test_the_lifecycle_fixture_registers_the_two_proof_events(self):
        self.require_tools()
        with tempfile.TemporaryDirectory() as scratch:
            report = probe(build("lifecycle", Path(scratch)))
        self.assertEqual(report["status"], "ok")
        events = report["game_listeners"]["events"]
        self.assertEqual(events.get("server_started"), 1)
        self.assertEqual(events.get("server_stopping"), 1)

    def test_the_hook_fixture_registers_the_proof_listeners(self):
        self.require_tools()
        with tempfile.TemporaryDirectory() as scratch:
            report = probe(build("hooks", Path(scratch)))
        self.assertEqual(report["status"], "ok")
        events = report["game_listeners"]["events"]
        # The tick, join and break callbacks milestone 2c forwards from the game's hooks.
        self.assertEqual(events.get("server_tick_start"), 1)
        self.assertEqual(events.get("player_join"), 1)
        self.assertEqual(events.get("block_break_before"), 1)
        self.assertEqual(events.get("block_break_canceled"), 1)
        self.assertEqual(events.get("block_break_after"), 1)

    def test_the_command_fixture_registers_javahello_with_an_optional_name(self):
        self.require_tools()
        with tempfile.TemporaryDirectory() as scratch:
            report = probe(build("command", Path(scratch)))
        self.assertEqual(report["status"], "ok")
        commands = report["commands"]
        self.assertEqual(commands["roots"], ["javahello", "javawhere"])
        # `javahello`, `javahello <name>` and `javawhere` execute.
        self.assertEqual(commands["executable"], 3)
        self.assertEqual(
            commands["samples"], ["javahello", "javawhere", "javahello test"]
        )

    def test_the_content_fixture_registers_a_typed_item_and_block(self):
        self.require_tools()
        with tempfile.TemporaryDirectory() as scratch:
            report = probe(build("content", Path(scratch)))
        self.assertEqual(report["status"], "ok")
        # The ids and registries are the fixture's own (m3fixture.Content): the runtime records
        # this jar's two Registry.register calls, and both carry a typed declaration the bridge can
        # transcribe.
        rows = {(row["registry"], row["id"]): row for row in report["registrations"]}
        self.assertIn(("minecraft:item", "m3_fixture:ruby"), rows)
        self.assertIn(("minecraft:block", "m3_fixture:ruby_block"), rows)
        self.assertTrue(rows[("minecraft:item", "m3_fixture:ruby")]["typed"])
        self.assertTrue(rows[("minecraft:block", "m3_fixture:ruby_block")]["typed"])
        self.assertEqual(report["unknown_host_call_count"], 0, report["unknown_host_calls"])

    def test_the_content_fixture_ships_its_own_assets_name_and_loot(self):
        """The jar milestone 3b bakes: a 16x16 texture, item models, names, tag and block loot."""
        self.require_tools()
        with tempfile.TemporaryDirectory() as scratch:
            jar = build("content", Path(scratch))
            with zipfile.ZipFile(jar) as archive:
                names = set(archive.namelist())
                texture = archive.read("assets/m3_fixture/textures/item/ruby.png")
                lang = json.loads(archive.read("assets/m3_fixture/lang/en_us.json"))
                pickaxe = json.loads(
                    archive.read("data/minecraft/tags/block/mineable/pickaxe.json")
                )
                loot = json.loads(
                    archive.read("data/m3_fixture/loot_table/blocks/ruby_block.json")
                )
        for name in (
            "assets/m3_fixture/items/ruby.json",
            "assets/m3_fixture/items/ruby_block.json",
            "assets/m3_fixture/models/item/ruby.json",
            "assets/m3_fixture/models/block/ruby_block.json",
            "assets/m3_fixture/blockstates/ruby_block.json",
            "assets/m3_fixture/textures/block/ruby_block.png",
        ):
            self.assertIn(name, names)
        # An actual PNG, 16x16 RGBA (`IHDR`: width, height, bit depth, colour type).
        self.assertEqual(texture[:8], b"\x89PNG\r\n\x1a\n")
        width, height, depth, colour = struct.unpack(">IIBB", texture[16:26])
        self.assertEqual((width, height, depth, colour), (16, 16, 8, 6))
        self.assertEqual(lang["item.m3_fixture.ruby"], "Ruby")
        self.assertEqual(lang["block.m3_fixture.ruby_block"], "Block of Ruby")
        self.assertEqual(pickaxe["values"], ["m3_fixture:ruby_block"])
        # One roll, one ruby: what the in-game break must drop.
        self.assertEqual(loot["pools"][0]["rolls"], 1)
        self.assertEqual(loot["pools"][0]["entries"][0]["name"], "m3_fixture:ruby")


if __name__ == "__main__":
    unittest.main()
