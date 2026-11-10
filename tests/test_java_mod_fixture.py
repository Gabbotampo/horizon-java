"""The milestone-1 fixture jar is a `ran-clean` jar by the mod's own definition.

The proof that the mod runs a jar inside the game uses a fixture built by
`tools/java_mod_fixture.py`. This pins what "ran-clean" means for it against the probe, which
shares the runtime the mod carries: the entrypoint runs, it makes host calls, and none of them is
unmodelled. A fixture that stopped exercising the host, or one the runtime started answering
neutrally, would fail here before anyone reads a game log.
"""

import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class JavaModFixtureTests(unittest.TestCase):
    def test_the_fixture_runs_without_an_unmodelled_host_call(self):
        missing = [tool for tool in ("javac", "cargo") if shutil.which(tool) is None]
        if missing:
            self.skipTest(f"{', '.join(missing)} required for the fixture run")
        with tempfile.TemporaryDirectory() as scratch:
            jar = subprocess.run(
                [sys.executable, str(ROOT / "tools" / "java_mod_fixture.py"), "--out", scratch],
                text=True,
                capture_output=True,
                timeout=180,
                check=False,
            )
            self.assertEqual(jar.returncode, 0, jar.stderr)
            path = Path(jar.stdout.strip().splitlines()[-1])
            self.assertTrue(path.is_file(), f"{path} was not written")

            probe = subprocess.run(
                [
                    "cargo",
                    "run",
                    "--quiet",
                    "-p",
                    "horizon-native-java",
                    "--",
                    "--json",
                    str(path),
                ],
                cwd=ROOT,
                text=True,
                capture_output=True,
                timeout=900,
                check=False,
            )
            self.assertEqual(probe.returncode, 0, probe.stderr or probe.stdout)
            report = json.loads(probe.stdout.strip().splitlines()[-1])
            self.assertEqual(report["status"], "ok")
            # Fabric's common entrypoint, which both sides run.
            self.assertEqual(report["kind"], "main")
            self.assertEqual(report["entrypoints"], ["m0fixture.Hello"])
            # The fixture reaches the host (so the row is not a hollow zero) and
            # every call it makes is modelled (so the mod's verdict is `ran-clean`).
            self.assertGreater(report["host_calls"], 0)
            self.assertEqual(report["unknown_host_call_count"], 0, report["unknown_host_calls"])


if __name__ == "__main__":
    unittest.main()
