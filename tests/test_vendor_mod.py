"""The native mod's vendored runtime copy is what `tools/vendor.py` produces, byte for byte.

`mod/` is one crate (a Horizon native mod cannot have a path dependency), so it carries a copy of
`crates/horizon-native-java/src`. This checks that copy against its source and its lock, and that a
single byte edited by hand is caught: a silently stale copy would run a different runtime than the
one the crate's tests cover.
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


def load_vendor():
    spec = importlib.util.spec_from_file_location("horizon_vendor", VENDOR)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class VendorModTests(unittest.TestCase):
    def test_the_committed_copy_matches_the_runtime_crate(self):
        result = subprocess.run(
            [sys.executable, str(VENDOR), "--check"],
            text=True,
            capture_output=True,
            timeout=120,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("vendored file(s) match", result.stdout)

    def test_one_edited_byte_fails_the_check(self):
        with tempfile.TemporaryDirectory() as scratch:
            tree = Path(scratch) / "tree"
            # The whole tree the tool reads: the runtime's sources, the vendored copy and its lock.
            for relative in ("crates/horizon-native-java/src", "mod/src/runtime", "mod/vendor.lock"):
                source = ROOT / relative
                target = tree / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                if source.is_dir():
                    shutil.copytree(source, target)
                else:
                    shutil.copy2(source, target)
            code, stderr = check(tree)
            self.assertEqual(code, 0, "the copied tree starts clean")
            self.assertNotIn("drift:", stderr)

            edited = tree / "mod" / "src" / "runtime" / "brigadier.rs"
            original = edited.read_bytes()
            # One byte, in the middle of the file, from a ':' to a ';'.
            offset = original.index(b":", len(original) // 2)
            edited.write_bytes(original[:offset] + b";" + original[offset + 1 :])
            code, stderr = check(tree)
            self.assertEqual(code, 1)
            self.assertIn("brigadier.rs", stderr)
            self.assertIn("edited by hand", stderr)

    def test_a_source_change_fails_the_check(self):
        with tempfile.TemporaryDirectory() as scratch:
            tree = Path(scratch) / "tree"
            for relative in ("crates/horizon-native-java/src", "mod/src/runtime", "mod/vendor.lock"):
                source = ROOT / relative
                target = tree / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                if source.is_dir():
                    shutil.copytree(source, target)
                else:
                    shutil.copy2(source, target)
            # A change the vendored copy does not have: the crate grew, the copy did not.
            source_file = tree / "crates" / "horizon-native-java" / "src" / "type_switch.rs"
            source_file.write_bytes(source_file.read_bytes() + b"\n// a new behaviour\n")
            code, stderr = check(tree)
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


def check(tree: Path) -> tuple[int, str]:
    result = subprocess.run(
        [sys.executable, str(VENDOR), "--check", "--repo", str(tree)],
        text=True,
        capture_output=True,
        timeout=120,
        check=False,
    )
    return result.returncode, result.stderr


if __name__ == "__main__":
    unittest.main()
