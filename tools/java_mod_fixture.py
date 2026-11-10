#!/usr/bin/env python3
"""Build a tiny Fabric mod jar whose ``onInitialize`` runs on the native runtime.

The milestone-1 proof needs a real jar that does something the runtime models, without shipping a
jar in the repository or downloading one: this writes one small Java source, compiles it with the
``javac`` on ``PATH`` and zips it with Fabric metadata. The class does a bounded loop, a string
concatenation, ``Optional.filter``, and a ``HashMap`` round trip, so its run reports host calls and
no unmodelled one (the definition of ``ran-clean`` in the mod's report).

    python3 tools/java_mod_fixture.py --out /tmp/config/horizon_java/java-mods

The jar is a test input, never a committed artifact.
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

# The mod id and class the jar declares; the mod's log line names this id.
MOD_ID = "m0_fixture"
ENTRY = "m0fixture.Hello"
JAR = "m0_fixture.jar"

SOURCE = """
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
"""

METADATA = {
    "schemaVersion": 1,
    "id": MOD_ID,
    "version": "1.0.0",
    "name": "M0 fixture",
    "depends": {"minecraft": "26.3", "fabricloader": "*"},
    "entrypoints": {"main": [ENTRY]},
}


def build(out: Path) -> Path:
    """Compiles the fixture and writes `<out>/m0_fixture.jar`; returns the jar."""
    if shutil.which("javac") is None:
        raise SystemExit("javac is required to build the fixture")
    out.mkdir(parents=True, exist_ok=True)
    jar = out / JAR
    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        source = root / "src" / "m0fixture" / "Hello.java"
        source.parent.mkdir(parents=True)
        source.write_text(SOURCE.lstrip(), encoding="utf-8")
        classes = root / "classes"
        classes.mkdir()
        result = subprocess.run(
            ["javac", "--release", "17", "-d", str(classes), str(source)],
            text=True,
            capture_output=True,
        )
        if result.returncode:
            raise SystemExit(result.stderr or result.stdout)
        # A Fabric mod jar is a zip: the entrypoint class plus `fabric.mod.json` at its root.
        with zipfile.ZipFile(jar, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("fabric.mod.json", json.dumps(METADATA, indent=2))
            for class_file in sorted(classes.rglob("*.class")):
                archive.write(class_file, class_file.relative_to(classes).as_posix())
    return jar


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, required=True, help="where m0_fixture.jar goes")
    args = parser.parse_args(argv)
    jar = build(args.out)
    print(jar)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
