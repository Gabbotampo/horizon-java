"""The native mod's vendored runtime copy is what its lock and `tools/vendor.py` say.

`mod/` is one crate (a Horizon native mod cannot have a path dependency), so it carries a copy of
`crates/horizon-native-java/src`, recorded per file in `mod/vendor.lock`. The checks are split so a
runtime-only change does not tax its own PR:

* the committed copy is worth what its lock says (`--check-lock`): always true, tested here;
* the committed copy also matches the runtime source as it is now (`--check`): required of a PR
  that touches `mod/**` or `tools/vendor.py`, and run by CI only for those (`.github/workflows/ci.yml`).

A one-byte hand edit must fail the first; a runtime edit the mod does not carry must fail the second.
A data file a source embeds (`include_str!`/`include_bytes!`) is vendored and locked like a source,
so the mod's crate builds without it being copied by hand.
"""

import importlib.util
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VENDOR = ROOT / "tools" / "vendor.py"
# Everything the tool reads, so a temp tree can be checked on its own.
TREE = ("crates/horizon-native-java/src", "mod/src/runtime", "mod/vendor.lock")


def load_vendor():
    spec = importlib.util.spec_from_file_location("horizon_vendor", VENDOR)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def copy_tree(root: Path) -> Path:
    for relative in TREE:
        source = ROOT / relative
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if source.is_dir():
            shutil.copytree(source, target)
        else:
            shutil.copy2(source, target)
    return root


def check(tree: Path, *flags: str) -> tuple[int, str]:
    result = subprocess.run(
        [sys.executable, str(VENDOR), *flags, "--repo", str(tree)],
        text=True,
        capture_output=True,
        timeout=120,
        check=False,
    )
    return result.returncode, result.stderr


class VendorModTests(unittest.TestCase):
    def test_the_committed_copy_matches_its_lock(self):
        # The check every PR runs, whatever it touches: the copy is not edited by hand.
        code, stderr = check(ROOT, "--check-lock")
        self.assertEqual(code, 0, stderr)
        self.assertNotIn("drift:", stderr)
        same = subprocess.run(
            [sys.executable, str(VENDOR), "--check-lock"],
            text=True,
            capture_output=True,
            timeout=120,
            check=False,
        )
        self.assertEqual(same.returncode, 0, same.stderr)
        self.assertIn("vendored file(s) match", same.stdout)

    def test_one_edited_byte_fails_the_lock_check(self):
        with tempfile.TemporaryDirectory() as scratch:
            tree = copy_tree(Path(scratch) / "tree")
            self.assertEqual(check(tree, "--check-lock")[0], 0, "the copied tree starts clean")

            edited = tree / "mod" / "src" / "runtime" / "brigadier.rs"
            original = edited.read_bytes()
            # One byte, in the middle of the file, from a ':' to a ';'.
            offset = original.index(b":", len(original) // 2)
            edited.write_bytes(original[:offset] + b";" + original[offset + 1 :])
            code, stderr = check(tree, "--check-lock")
            self.assertEqual(code, 1)
            self.assertIn("brigadier.rs", stderr)
            self.assertIn("edited by hand", stderr)

    def test_a_runtime_change_is_caught_only_by_the_full_check(self):
        # A runtime PR that does not touch the mod: its own change must not fail the lock check,
        # which is why CI runs the full check only for `mod/**` and `tools/vendor.py`.
        with tempfile.TemporaryDirectory() as scratch:
            tree = copy_tree(Path(scratch) / "tree")
            source_file = tree / "crates" / "horizon-native-java" / "src" / "type_switch.rs"
            source_file.write_bytes(source_file.read_bytes() + b"\n// a new behaviour\n")

            code, stderr = check(tree, "--check-lock")
            self.assertEqual(code, 0, "a runtime change alone is not the mod's drift")
            self.assertNotIn("drift:", stderr)

            code, stderr = check(tree, "--check")
            self.assertEqual(code, 1)
            self.assertIn("changed since", stderr)

    def test_the_rewrite_keeps_relative_paths_and_moves_the_crate_root(self):
        module = load_vendor()
        files = module.expected(ROOT)
        # `lib.rs` is the module the mod declares, and it no longer sits at a crate root.
        self.assertIn("mod.rs", files)
        root = files["mod.rs"][1].decode()
        # `lib.rs` reaches one of its own items by the crate path, which moves under `runtime`.
        self.assertIn("crate::runtime::return_desc", root)
        self.assertNotIn("crate::return_desc", root)
        # The probe's binary is not part of the library copy.
        self.assertNotIn("main.rs", files)
        # A `#[path]` submodule of `client_surface.rs` keeps its relative module paths.
        surface = files["client_surface.rs"][1].decode()
        self.assertIn('#[path = "client_items.rs"]', surface)

    def test_an_embedded_data_file_is_vendored_and_locked(self):
        with tempfile.TemporaryDirectory() as scratch:
            tree = copy_tree(Path(scratch) / "tree")
            src = tree / "crates" / "horizon-native-java" / "src"
            (src / "tables").mkdir()
            (src / "tables" / "sample.bin").write_bytes(b"\x00\xffbytes, not text\n")
            (src / "embeds_sample.rs").write_text(
                'pub(crate) const SAMPLE: &[u8] = include_bytes!("tables/sample.bin");\n'
            )
            write = subprocess.run(
                [sys.executable, str(VENDOR), "--repo", str(tree)],
                text=True, capture_output=True, timeout=120, check=False,
            )
            self.assertEqual(write.returncode, 0, write.stderr)
            vendored = tree / "mod" / "src" / "runtime" / "tables" / "sample.bin"
            # Copied byte for byte beside its vendored includer, where the macro looks for it.
            self.assertEqual(vendored.read_bytes(), b"\x00\xffbytes, not text\n")
            self.assertEqual(check(tree, "--check")[0], 0)

            # The lock covers it: an edit fails the lock check, and so does a deletion.
            vendored.write_bytes(b"\x01\xffbytes, not text\n")
            code, stderr = check(tree, "--check-lock")
            self.assertEqual(code, 1)
            self.assertIn("tables/sample.bin", stderr)
            vendored.unlink()
            code, stderr = check(tree, "--check-lock")
            self.assertEqual(code, 1)
            self.assertIn("missing", stderr)

            # A changed source data file is drift for the full check only.
            subprocess.run([sys.executable, str(VENDOR), "--repo", str(tree)], check=True,
                           capture_output=True, timeout=120)
            (src / "tables" / "sample.bin").write_bytes(b"new\n")
            self.assertEqual(check(tree, "--check-lock")[0], 0)
            code, stderr = check(tree, "--check")
            self.assertEqual(code, 1)
            self.assertIn("sample.bin", stderr)

            # A file in the copy that nothing vendors is not the tool's output.
            (tree / "mod" / "src" / "runtime" / "stray.json").write_text("{}")
            code, stderr = check(tree, "--check-lock")
            self.assertEqual(code, 1)
            self.assertIn("stray.json", stderr)

    def test_an_include_the_tool_cannot_follow_is_refused(self):
        for line, message in (
            ('const A: &str = include_str!(concat!("a", ".json"));', "non-literal"),
            ('const A: &str = include_str!("../../../README.md");', "outside"),
            ('const A: &str = include_str!("missing.json");', "does not exist"),
        ):
            with self.subTest(line=line), tempfile.TemporaryDirectory() as scratch:
                tree = copy_tree(Path(scratch) / "tree")
                src = tree / "crates" / "horizon-native-java" / "src"
                (src / "bad_include.rs").write_text(line + "\n")
                (tree / "README.md").write_text("outside the runtime source\n")
                result = subprocess.run(
                    [sys.executable, str(VENDOR), "--repo", str(tree)],
                    text=True, capture_output=True, timeout=120, check=False,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)
                self.assertIn("bad_include.rs", result.stderr)


if __name__ == "__main__":
    unittest.main()
