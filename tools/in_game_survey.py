#!/usr/bin/env python3
"""Run batches of real Java mod jars in a game, and summarise what happened.

This is the driver behind `docs/in-game-survey.md`. It does not install the game, prepare
resources or fetch jars: give it a game built at the pinned commit, the `horizon-ctl` beside it,
a `horizon-native-java` probe, prepared resources and the corpus root. Each batch gets a fresh
scratch folder (its own homes, game folder and `config/horizon_java/java-mods`), a bake, and one
integrated-server run driven through the control socket.

    python3 tools/in_game_survey.py --game target/debug/horizon-client \\
        --ctl target/debug/horizon-ctl --probe target/debug/horizon-native-java \\
        --resources .local/horizon-client/26.3 --corpus ~/horizon-corpus \\
        --out /tmp/in-game-survey --sample

`--sample` runs the 40 jars the survey selected (see `SAMPLE`); without it, `--manifest FILE`
gives `{"batch": "b1", "jars": [{"file": "...", "command": "root or null"}]}` rows and each
`file` is found under `--corpus` by name.

Nothing is committed from a run except the JSON summaries; jars are copied into the scratch game
folder and deleted with it. Every game process runs in its own systemd unit with a 10 GiB memory
cap (`--memory`), so a bad jar cannot take the machine with it.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
import zipfile
from pathlib import Path

# The survey's sample: (batch, [(short id, file name under a corpus, command root or None)]).
# Selection: every distinct mod among the census's server-side `ran-clean` jars that registers a
# command or a listener, plus two declared dependencies that are themselves mods, plus sixteen
# no-hook control jars. See docs/in-game-survey.md, "The sample".
SAMPLE = [
    ("b1", [
        ("fancymenu", "fancymenu_neoforge_3.9.14_MC_26.3.jar", "fmvariable get test"),
        ("konkrete", "konkrete_neoforge_1.11.1_MC_26.3.jar", None),
        ("jade", "Jade-mc26.3-NeoForge-26.3.5.jar", "jade"),
        ("midnightlib", "midnightlib-neoforge-1.10.0+26.3.jar", None),
    ]),
    ("b2", [
        ("carryon", "carryon-neoforge-26.3-2.12.1.jar", "carryon debug"),
        ("fzzy_config", "fzzy_config-0.7.7+fix3+26.3+neoforge.jar", "configure_update test list"),
        ("iceberg_nf", "Iceberg-26.3-neoforge-1.4.3.2.jar", None),
        ("searchables", "Searchables-neoforge-26.3-1.0.3.jar", None),
    ]),
    ("b3", [
        ("connectivity", "470193-8893473.jar", "connectivity packetsSummary"),
        ("limitedchunks", "456640-8026052.jar", "limitedchunks listnonplayerchunktickets"),
        ("cupboard", "326652-8890593.jar", None),
        ("structureessentials", "832882-8896904.jar", "structureessentials showGenerationTimes"),
    ]),
    ("b4", [
        ("chunky", "fc1f2f374c-Chunky-Fabric-1.5.3.jar", "chunky"),
        ("respackopts", "respackopts-26.3.0.jar", None),
        ("modelbrowser", "c661609de9-modelbrowser-1.5.1.jar", "modelbrowser items_always_equippable"),
        ("baguettelib", "baguettelib-26.3-Fabric-2.0.7.jar", None),
    ]),
    ("b5", [
        ("spark", "361579-8918571.jar", "spark"),
        ("mcwifipnp", "mcwifipnp-2.1.4-26.3-fabric.jar", "uuidfixer list"),
        # `stackrefill` declares `collective`: since cross-jar linking (#33) a dependency must
        # sit in the same `java-mods`, so the two share a batch (the first survey ran
        # `stackrefill` in b6 and `otterlib` here; nothing else about either changed).
        ("stackrefill", "9f12a0dffd-stackrefill-26.3.0-5.1.jar", None),
        ("collective", "bdc7223ca2-collective-26.3.0-8.67.jar", None),
    ]),
    ("b6", [
        ("otterlib", "1079f41d14-otterlib-0.4.0.2+26.3-fabric.jar", None),
        ("calcmod", "d198676d8b-calcmod-1.6.0+fabric.26.3.jar", None),
        ("entity-view-distance", "09ee9afdd2-entity-view-distance-1.9.0+26.2.jar", None),
        ("structure_pool_api", "1b0a4ed5f7-structure_pool_api-fabric-1.3.0+26.3.jar", None),
    ]),
    ("b7", [
        ("supermartijn642configlib", "supermartijn642configlib-1.1.8-neoforge-mc26.3.jar", None),
        ("bookshelf", "Bookshelf-neoforge-MC26.3-26.3.0.4.jar", None),
        ("cloth-config", "cloth-config-neoforge-26.3.159.jar", None),
        ("geckolib", "geckolib-neoforge-26.3-5.5.7.jar", None),
    ]),
    ("b8", [
        ("yacl", "yet_another_config_lib_v3-3.9.7+26.3-neoforge.jar", None),
        ("forgeconfigapiport", "ForgeConfigAPIPort-v26.3.1-mc26.3.x-NeoForge.jar", None),
        ("fast-ip-ping", "fast-ip-ping-v1.0.12-mc26.3.jar", None),
        ("clumps", "Clumps-neoforge-26.3-26.3.2.jar", None),
    ]),
    ("b9", [
        ("packetfixer", "PacketFixer-neoforge-3.3.7.jar", None),
        ("nochatreports", "NoChatReports-NEOFORGE-26.3-v2.21.0.jar", None),
        ("mossylib", "MossyLib-1.6.0+26.3+fabric.jar", None),
        ("resourcefullib", "825dce0c0b-ResourcefulLib-6.0.1.jar", None),
    ]),
    ("b10", [
        ("krypton", "krypton-0.3.2.jar", None),
        ("asynclogger", "asynclogger-2.2.2+26.1.2-fabric.jar", None),
        ("c2me", "c2me-fabric-mc26.3-0.4.2-alpha.0.106.jar", None),
        ("structure_layout_optimizer", "structure_layout_optimizer-1.1.4+26.3-fabric.jar", None),
    ]),
]

CORPUS_DIRS = ("neoforge", "corpus-xl", "mega")
SERVER_KINDS = ("main", "server", "neoforge")
VERDICT = re.compile(r"\(([^)]+)\) (ran-clean|ran-hollow|ran-partial|refused)")
REGISTRATION = re.compile(r"command registration: (\d+) callback\(s\) ran, (\d+) failed")
USAGES = re.compile(r"(\d+) command usage\(s\) translated, (\d+) refused")


def parse_reply(text: str):
    """The JSON object on the last such line of a `horizon-ctl` reply, or None."""
    for line in reversed((text or "").splitlines()):
        line = line.strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except ValueError:
                continue
    return None


def summary_of(jar: dict) -> str | None:
    """The row's verdict plus its listener failures (`ran-clean, 12 listener failures`).

    The report writes this line itself; an older report without it is summarised here from the
    callback count, so a jar whose listeners threw never reads as an unqualified clean run.
    """
    if jar.get("summary"):
        return jar["summary"]
    verdict = jar.get("verdict")
    failed = (jar.get("callbacks") or {}).get("failed") or 0
    if verdict is None or not failed:
        return verdict
    noun = "listener failure" if failed == 1 else "listener failures"
    return f"{verdict}, {failed} {noun}"


def summarise(report: dict | None, log_lines: list[str]) -> list[dict]:
    """One row per jar in the runtime report, with what the log says about it.

    The report is authoritative for verdicts, host-call counts, callbacks and commands; the log
    adds the command-registration totals and the reasons a command or listener failed, which the
    report does not carry. A jar missing from the report (the run died before the report) still
    gets a row with `verdict: None`. `summary` is what a reader should judge: the entrypoint
    verdict plus any listener failures after it.
    """
    rows = []
    for jar in (report or {}).get("jars", []):
        iid = jar.get("id") or jar.get("file")
        text = [line for line in log_lines if f"({iid})" in line]
        mine = [line for line in log_lines if f": {iid}:" in line]
        usages = (0, 0)
        callbacks = (0, 0)
        reasons = []
        for line in mine:
            match = USAGES.search(line)
            if match:
                usages = (int(match.group(1)), int(match.group(2)))
            match = REGISTRATION.search(line)
            if match:
                callbacks = (int(match.group(1)), int(match.group(2)))
            if " refused: " in line or (" listener from " in line and " failed:" in line):
                reasons.append(line.split("Horizon Java: ", 1)[-1])
        rows.append({
            "id": iid,
            "file": jar.get("file"),
            "verdict": jar.get("verdict"),
            "summary": summary_of(jar),
            "host_calls": jar.get("host_calls"),
            "unknown_host_call_count": jar.get("unknown_host_call_count"),
            "reason": jar.get("reason"),
            "callbacks": jar.get("callbacks"),
            "commands": jar.get("commands"),
            "content": jar.get("content"),
            "usages": {"translated": usages[0], "refused": usages[1]},
            "registration": {"ran": callbacks[0], "failed": callbacks[1]},
            "log_reasons": reasons[:8],
            "verdict_line": text[0] if text else None,
        })
    return rows


def table(rows: list[dict]) -> str:
    """One batch as a markdown table: the jar, its entrypoint verdict and its summary.

    `summary` is the column to read: `ran-clean` alone hides a jar whose listeners all threw, and
    the survey's own table showed exactly that before this column existed.
    """
    lines = ["| jar | verdict | summary |", "|---|---|---|"]
    for row in rows:
        lines.append(f"| `{row['file']}` | {row['verdict']} | {row['summary']} |")
    return "\n".join(lines)


def content_items(game_dir: Path) -> dict[str, list[str]]:
    """The item ids each jar's baked packages declare, keyed by the jar's file name.

    `tools/bake.py` writes one `.hmod` per namespace with the runtime's declarations as
    `data/<ns>/horizon/item/<path>.json` and `data/<ns>/horizon/block/<path>.json` (a block whose
    declaration says `"item": true` brings its block item). These are the ids a player can be
    given, so the survey gives one of them for every content jar. A jar with no declared items
    is absent; an unreadable package is skipped (its bake error is already in the bake report).
    """
    report_file = game_dir / "config" / "horizon_java" / "bake-report.json"
    try:
        report = json.loads(report_file.read_text())
    except (OSError, ValueError):
        return {}
    found: dict[str, list[str]] = {}
    for jar in report.get("jars", []):
        ids = []
        for package in jar.get("packages", []):
            declared = package.get("declarations") or {}
            if not (declared.get("items") or declared.get("blocks")):
                continue
            try:
                with zipfile.ZipFile(game_dir / package["path"]) as archive:
                    names = sorted(archive.namelist())
                    for name in names:
                        parts = name.split("/")
                        if (len(parts) != 5 or parts[0] != "data" or parts[2] != "horizon"
                                or not parts[4].endswith(".json")):
                            continue
                        item_id = f"{parts[1]}:{parts[4][:-len('.json')]}"
                        if parts[3] == "item":
                            ids.append(item_id)
                        elif parts[3] == "block":
                            try:
                                body = json.loads(archive.read(name))
                            except ValueError:
                                continue
                            if body.get("item"):
                                ids.append(item_id)
            except (OSError, KeyError, zipfile.BadZipFile):
                continue
        if ids:
            found[jar["jar"]] = ids
    return found


def find_jar(corpus: Path, name: str) -> Path:
    """A jar by file name under one of the corpora, or a manifest's absolute path as given.

    The absolute form is for jars that are not in a corpus (the repo's own fixtures, built into a
    scratch folder by `tools/java_mod_fixture.py`)."""
    if Path(name).is_absolute():
        if Path(name).is_file():
            return Path(name)
        raise SystemExit(f"{name}: not found")
    for directory in CORPUS_DIRS:
        candidate = corpus / directory / name
        if candidate.is_file():
            return candidate
    raise SystemExit(f"{name}: not found under {corpus}/{{{','.join(CORPUS_DIRS)}}}")


def run(cmd, timeout=180, env=None):
    started = time.time()
    try:
        done = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, env=env)
        return {"cmd": [str(part) for part in cmd], "code": done.returncode,
                "out": done.stdout[-4000:], "err": done.stderr[-4000:],
                "seconds": round(time.time() - started, 2)}
    except subprocess.TimeoutExpired:
        return {"cmd": [str(part) for part in cmd], "code": None, "out": "", "err": "timeout",
                "seconds": round(time.time() - started, 2)}


class Session:
    def __init__(self, args):
        self.args = args
        self.control = args.ctl

    def call(self, sock, *words, timeout=180):
        command = [str(self.control), "--socket", str(sock), "--timeout", str(timeout), *words]
        return run(command, timeout=timeout + 20)

    def reply(self, sock, *words, timeout=180):
        return parse_reply(self.call(sock, *words, timeout=timeout)["out"])

    def env(self, scratch):
        env = dict(os.environ)
        env.update({
            "HORIZON_HOME": str(scratch / "game-home"),
            "HORIZON_LAUNCHER_HOME": str(scratch / "launcher"),
            "XDG_DATA_HOME": str(scratch / "data"),
            "XDG_CONFIG_HOME": str(scratch / "config"),
            "HORIZON_LAUNCHER_TRASH": str(scratch / "trash"),
            "HORIZON_LAUNCHER_CLIPBOARD": str(scratch / "clipboard"),
            "HORIZON_LAUNCHER_DESKTOP": str(scratch / "desktop"),
            "HORIZON_LAUNCHER_NO_SELF_UPDATE": "1",
            "HORIZON_NO_AUDIO": "1",
        })
        return env

    def run_batch(self, batch, jars, break_block="minecraft:dirt"):
        args = self.args
        scratch = args.out / "runs" / batch
        if scratch.exists():
            shutil.rmtree(scratch)
        for name in ["game-home", "launcher", "data", "config", "trash", "clipboard",
                     "desktop", "game-dir"]:
            (scratch / name).mkdir(parents=True, exist_ok=True)
        game_dir = scratch / "game-dir"
        mods = game_dir / "config" / "horizon_java" / "java-mods"
        mods.mkdir(parents=True, exist_ok=True)
        for short, path, _ in jars:
            if not path.is_file():
                raise SystemExit(f"{short}: {path} is missing")
            shutil.copyfile(path, mods / path.name)
        baked = run([sys.executable, str(args.repo / "tools" / "bake.py"),
                     "--probe", str(args.probe), "--game-dir", str(game_dir)], timeout=300)
        env = self.env(scratch)
        unit = f"in-game-survey-{batch}"
        sock = scratch / "ctl.sock"
        log = scratch / "game.log"
        subprocess.run(["systemctl", "--user", "reset-failed", unit], capture_output=True)
        launch = ["systemd-run", "--user", f"--unit={unit}",
                  "-p", f"MemoryMax={args.memory}", "-p", "MemorySwapMax=0",
                  f"--working-directory={game_dir}",
                  "-p", f"StandardOutput=append:{log}", "-p", f"StandardError=append:{log}"]
        for key in ["HORIZON_HOME", "HORIZON_LAUNCHER_HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME",
                    "HORIZON_LAUNCHER_TRASH", "HORIZON_LAUNCHER_CLIPBOARD",
                    "HORIZON_LAUNCHER_DESKTOP", "HORIZON_LAUNCHER_NO_SELF_UPDATE",
                    "HORIZON_NO_AUDIO"]:
            launch += ["--setenv", f"{key}={env[key]}"]
        launch += [args.game, "--game-dir", str(game_dir), "--resources", str(args.resources),
                   "--offscreen", "--world", batch.upper(), "--creative", "--control",
                   "--control-socket", str(sock)]
        started = run(launch, timeout=60)
        result = {"batch": batch, "launch": started,
                  "jars": [{"short": short, "file": path.name, "source": str(path),
                            "command": command} for short, path, command in jars],
                  "bake": {"code": baked["code"], "out": baked["out"], "err": baked["err"]},
                  "actions": {}, "ready": False, "ticks": None, "report": None, "log_version": None,
                  "content_items": content_items(game_dir)}
        if not wait_for(sock, args.startup):
            result["error"] = "no control socket"
        else:
            ready = self.reply(sock, "wait-ready", str(args.ready))
            result["ready"] = ready is not None
            state = None
            for _ in range(30):
                state = self.reply(sock, "state")
                if (state or {}).get("player"):
                    break
                time.sleep(1)
            position = ((state or {}).get("player") or {}).get("position")
            if position:
                x, y, z = (int(value) for value in position)
                result["actions"]["setblock"] = self.reply(
                    sock, "setblock", str(x), str(y + 2), str(z + 1), break_block)
                result["actions"]["block_visible_after_reads"] = wait_for_block(
                    lambda: self.reply(sock, "block", str(x), str(y + 2), str(z + 1)),
                    break_block)
                result["actions"]["break"] = self.reply(
                    sock, "break", str(x), str(y + 2), str(z + 1), timeout=120)
                # A cancelled break (the m2c fixture keeps stone) leaves the block standing.
                result["actions"]["block_after_break"] = self.reply(
                    sock, "block", str(x), str(y + 2), str(z + 1))
            result["actions"]["give"] = self.reply(sock, "give", "minecraft:bread", "2")
            # One item from every content jar, by the id its baked package declares.
            result["content_gives"] = {}
            for jar_file, ids in result["content_items"].items():
                result["content_gives"][jar_file] = {
                    "item": ids[0], "declared": len(ids),
                    "reply": self.reply(sock, "give", ids[0], "1")}
            result["actions"]["use-item"] = self.reply(sock, "use-item", "--hold", "1.7")
            moved = self.reply(sock, "move", "forward", "2")
            if not moved or (moved.get("moved") or 0) < 0.5:
                moved = self.reply(sock, "move", "left", "2")
            result["actions"]["move"] = moved
            result["commands"] = {}
            for short, _, command in jars:
                if command:
                    result["commands"][short] = self.reply(
                        sock, "command", f"/{command}", timeout=90)
            waited = self.reply(sock, "wait", str(args.play_ticks), timeout=180)
            result["ticks"] = (waited or {}).get("ticks")
            result["actions"]["quit"] = self.reply(sock, "quit", timeout=120)
            for _ in range(60):
                if systemd_state(unit) != "active":
                    break
                time.sleep(1)
            subprocess.run(["systemctl", "--user", "stop", unit], capture_output=True)
        if log.exists():
            text = log.read_text(errors="replace")
            for line in text.splitlines():
                if line.startswith("Horizon 0."):
                    result["log_version"] = line.strip()
                    break
            result["log_lines"] = [line for line in text.splitlines() if "Horizon Java" in line]
        report_file = game_dir / "config" / "horizon_java" / "java-runtime.json"
        if report_file.exists():
            try:
                result["report"] = json.loads(report_file.read_text())
            except ValueError as error:
                result["report_error"] = str(error)
        result["rows"] = summarise(result["report"], result.get("log_lines") or [])
        (args.out / f"{batch}.json").write_text(json.dumps(result, indent=1) + "\n")
        return result


def wait_for_block(read, block: str, tries: int = 40, delay: float = 0.25, sleep=time.sleep):
    """Poll `read()` (a `horizon-ctl block X Y Z` reply) until the client sees `block` there.

    `setblock` runs on the server thread and `break` checks the window's own copy of the world,
    so a `break` sent straight after `setblock` can find air and answer "there is no block"
    before the block update arrives (it did on the first elysium run). The break is only
    meaningful once the client shows the block. Returns the number of reads it took, or None if
    the block never appeared (the batch then records the failed break as it is).
    """
    for attempt in range(1, tries + 1):
        reply = read() or {}
        if block in (reply.get("block"), reply.get("state")):
            return attempt
        sleep(delay)
    return None


def wait_for(path: Path, seconds: float) -> bool:
    started = time.time()
    while time.time() - started < seconds:
        if path.exists():
            return True
        time.sleep(0.5)
    return False


def systemd_state(unit: str) -> str:
    done = subprocess.run(["systemctl", "--user", "is-active", unit],
                          capture_output=True, text=True)
    return done.stdout.strip()


def manifest(path: Path) -> list[tuple[str, list[tuple[str, str, str | None]]]]:
    """Batches from a JSON file. A batch may name `break_block`, the block placed and broken
    (default dirt); the fixture batch uses stone, which the m2c fixture's listener keeps."""
    data = json.loads(path.read_text())
    batches = []
    for entry in data:
        batches.append((entry["batch"], [(jar["file"], jar["file"], jar.get("command"))
                                          for jar in entry["jars"]]))
        BREAK_BLOCKS[entry["batch"]] = entry.get("break_block", "minecraft:dirt")
    return batches


# The block each manifest batch places and breaks (filled by `manifest`); `--sample` uses dirt.
BREAK_BLOCKS: dict[str, str] = {}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--game", type=Path, required=True, help="the game executable")
    parser.add_argument("--ctl", type=Path, required=True, help="the horizon-ctl beside it")
    parser.add_argument("--probe", type=Path, required=True, help="horizon-native-java")
    parser.add_argument("--resources", type=Path, required=True, help="prepared 26.3 resources")
    parser.add_argument("--corpus", type=Path, default=Path.home() / "horizon-corpus")
    parser.add_argument("--out", type=Path, default=Path("/tmp/in-game-survey"))
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--manifest", type=Path, help="batch definitions; default: --sample")
    parser.add_argument("--sample", action="store_true", help="run the survey's 40-jar sample")
    parser.add_argument("--only", action="append", default=[], help="batch id to run")
    parser.add_argument("--play-ticks", type=int, default=600)
    parser.add_argument("--ready", type=int, default=240, help="seconds wait-ready may take")
    parser.add_argument("--startup", type=float, default=180, help="seconds for the socket")
    parser.add_argument("--memory", default="10G")
    args = parser.parse_args(argv)
    if not args.sample and not args.manifest:
        parser.error("give --sample or --manifest")
    batches = SAMPLE if args.sample else manifest(args.manifest)
    args.out.mkdir(parents=True, exist_ok=True)
    session = Session(args)
    for batch, jars in batches:
        if args.only and batch not in args.only:
            continue
        resolved = [(short, find_jar(args.corpus, name), command) for short, name, command in jars]
        result = session.run_batch(batch, resolved,
                                   BREAK_BLOCKS.get(batch, "minecraft:dirt"))
        rows = result.get("rows") or []
        verdicts = {}
        for row in rows:
            verdicts[row["verdict"]] = verdicts.get(row["verdict"], 0) + 1
        print(f"{batch}: ready={result['ready']} rows={len(rows)} verdicts={verdicts}")
        print(table(rows))
    return 0


if __name__ == "__main__":
    sys.exit(main())
