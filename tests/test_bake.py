"""The bake turns a Java jar's own assets and data into a content package the game loads.

Milestone 3 puts a jar's blocks and items in the game, but a native mod may not own another
namespace's files, so the jar's models, textures, names, blockstates and loot need a content
`.hmod` of their own (docs/m3-content-design.md, recommendation 3). These tests drive the tool
with synthetic jars and check the observable result: the package tree, its manifest, what the
report says was left out, and the zip-slip refusal.
"""

import importlib.util
import json
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BAKE = ROOT / "tools" / "bake.py"


def load_bake():
    spec = importlib.util.spec_from_file_location("horizon_bake", BAKE)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


BAKE_MODULE = load_bake()

# A 1x1 PNG is enough for the copy; the fixture tool generates the real 16x16 art.
PNG = bytes.fromhex(
    "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4"
    "890000000d49444154789c6360f8cfc000000301010018dd8db10000000049454e44ae426082"
)


def make_jar(path: Path, entries: dict[str, bytes]) -> Path:
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in entries.items():
            archive.writestr(name, data)
    return path


def ruby_jar(entries: dict[str, bytes] | None = None) -> dict[str, bytes]:
    files = {
        "fabric.mod.json": b'{"id":"ruby_mod","version":"1.2.3"}',
        "ruby_mod/Entry.class": b"\xca\xfe\xba\xbe",
        "assets/ruby_mod/textures/item/ruby.png": PNG,
        "assets/ruby_mod/items/ruby.json": (
            b'{"model":{"type":"minecraft:model","model":"ruby_mod:item/ruby"}}'
        ),
        "assets/ruby_mod/models/item/ruby.json": b'{"parent":"minecraft:item/generated"}',
        "assets/ruby_mod/lang/en_us.json": b'{"item.ruby_mod.ruby":"Ruby"}',
        "data/ruby_mod/loot_table/blocks/ruby_block.json": b'{"pools":[]}',
        "data/minecraft/tags/block/mineable/pickaxe.json": b'{"values":["ruby_mod:ruby_block"]}',
    }
    if entries:
        files.update(entries)
    return files


class BakeTests(unittest.TestCase):
    def setUp(self) -> None:
        self.scratch = tempfile.TemporaryDirectory()
        self.root = Path(self.scratch.name)
        self.game = self.root / "game"
        self.mods = self.game / "config" / "horizon_java" / "java-mods"
        self.mods.mkdir(parents=True)

    def tearDown(self) -> None:
        self.scratch.cleanup()

    def bake(self) -> dict:
        report = BAKE_MODULE.bake_directory(self.mods, self.game)
        return report.as_json()

    def test_assets_lang_and_loot_bake_into_the_namespace_package(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()

        self.assertEqual(report["enabled"], ["ruby_mod"])
        package = self.game / "horizon-mods" / "ruby_mod.hmod"
        self.assertTrue(package.is_file())
        with zipfile.ZipFile(package) as archive:
            names = archive.namelist()
            manifest = archive.read("horizon-mod.toml").decode()
        # The package is the jar's content tree plus the manifest, and nothing else: no classes,
        # no Fabric metadata, no arbitrary files.
        self.assertEqual(names[0], "horizon-mod.toml")
        self.assertEqual(
            names[1:],
            sorted(
                name
                for name in ruby_jar()
                if name.startswith(("assets/", "data/"))
            ),
        )
        self.assertIn('id = "ruby_mod"', manifest)
        self.assertIn('api = "0.2"', manifest)
        self.assertIn('side = "both"', manifest)
        self.assertIn('capabilities = ["registries"]', manifest)
        # The jar's own loot table, at the 26.3 path, comes through byte for byte.
        with zipfile.ZipFile(package) as archive:
            self.assertEqual(
                archive.read("data/ruby_mod/loot_table/blocks/ruby_block.json"), b'{"pools":[]}'
            )
        # The report names the files it did not copy, with reasons.
        skipped = {
            entry["reason"]: entry for entry in report["jars"][0]["skipped"]
        }
        self.assertIn("not an assets/ or data/ file", skipped)
        examples = [path for entry in skipped.values() for path in entry["paths"]]
        self.assertIn("ruby_mod/Entry.class", examples)
        # The package is deterministic: a second bake writes the same bytes.
        before = package.read_bytes()
        self.bake()
        self.assertEqual(package.read_bytes(), before)
        # `horizon-mods.txt` turns it on without disturbing other lines.
        mods_file = (self.game / "horizon-mods.txt").read_text()
        self.assertEqual(mods_file, "mod.ruby_mod:true\n")

    def test_unsafe_entry_paths_refuse_the_jar_whole(self):
        unsafe_paths = (
            "../escape.json",
            "/absolute.json",
            "assets/ruby_mod/../../x.json",
            "a\\b.json",
        )
        for unsafe in unsafe_paths:
            with self.subTest(unsafe=unsafe):
                entries = {"assets/ruby_mod/lang/en_us.json": b"{}", unsafe: b"{}"}
                make_jar(self.mods / "evil.jar", entries)
                report = self.bake()
                row = report["jars"][0]
                self.assertIsNotNone(row["error"])
                self.assertIn("unsafe entry path", row["error"])
                self.assertEqual(row["packages"], [])
                self.assertFalse((self.game / "horizon-mods" / "evil.hmod").exists())
                self.assertFalse((self.root / "escape.json").exists())
                self.assertFalse((self.root / "game" / "escape.json").exists())

    def test_unsupported_kinds_are_skipped_and_reported(self):
        make_jar(
            self.mods / "ruby_mod.jar",
            ruby_jar(
                {
                    "assets/ruby_mod/classes/Ruby.class": b"\xca\xfe\xba\xbe",
                    "assets/ruby_mod/nonsense/thing.bin": b"x",
                    "data/ruby_mod/horizon/block/ruby_block.json": b"{}",
                    "assets/minecraft/models/item/stone.json": b"{}",
                    "assets/ruby_mod/sounds.json": b"{}",
                }
            ),
        )
        report = self.bake()
        with zipfile.ZipFile(self.game / "horizon-mods" / "ruby_mod.hmod") as archive:
            names = set(archive.namelist())

        self.assertIn("assets/ruby_mod/sounds.json", names)
        self.assertNotIn("assets/ruby_mod/classes/Ruby.class", names)
        self.assertNotIn("assets/ruby_mod/nonsense/thing.bin", names)
        self.assertNotIn("data/ruby_mod/horizon/block/ruby_block.json", names)
        self.assertNotIn("assets/minecraft/models/item/stone.json", names)

        reasons = {entry["reason"] for entry in report["jars"][0]["skipped"]}
        self.assertIn("code or an executable, never copied", reasons)
        self.assertIn("unsupported asset kind", reasons)
        self.assertIn("vanilla's namespace: only tag and atlas additions are a mod's", reasons)
        self.assertIn(
            "content declarations: the runtime declares this jar's blocks and items", reasons
        )

    def test_a_namespace_that_cannot_be_a_package_id_is_refused_by_name(self):
        make_jar(
            self.mods / "ruby-mod.jar",
            {
                "assets/ruby-mod/lang/en_us.json": b'{"item.ruby-mod.ruby":"Ruby"}',
                "fabric.mod.json": b'{"id":"ruby-mod"}',
            },
        )
        report = self.bake()
        row = report["jars"][0]
        self.assertEqual(row["packages"], [])
        self.assertIn("not a content package id", row["error"])
        self.assertFalse((self.game / "horizon-mods" / "ruby-mod.hmod").exists())

    def test_class_files_are_never_copied_even_under_an_allowed_kind(self):
        make_jar(
            self.mods / "ruby_mod.jar",
            ruby_jar(
                {
                    "assets/ruby_mod/textures/item/Ruby.class": b"\xca\xfe\xba\xbe",
                    "assets/ruby_mod/models/item/ruby.jar": b"PK\x05\x06",
                }
            ),
        )
        self.bake()
        with zipfile.ZipFile(self.game / "horizon-mods" / "ruby_mod.hmod") as archive:
            names = archive.namelist()
        self.assertNotIn("assets/ruby_mod/textures/item/Ruby.class", names)
        self.assertNotIn("assets/ruby_mod/models/item/ruby.jar", names)


if __name__ == "__main__":
    unittest.main()
