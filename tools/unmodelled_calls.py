#!/usr/bin/env python3
"""Rank the unmodelled host calls the native probe still answers with a neutral value.

    unmodelled_calls.py [--corpus name=DIR]... [--probe PATH] [--json OUT]
                        [--game-version 26.3] [--limit N] [--jobs N]

Runs `horizon-native-java --json --game-version 26.3 --link <dep>... <jar> <kind>`
once per jar, with the jar's declared dependency closure resolved against the other
corpus jars (the same resolution `java_mod_server_ledger.py --with-deps` uses), and
aggregates the probe's `unknown_host_calls`:

    call           owner#name descriptor, as the probe reports it
    jars           how many jars hit the call at least once
    calls          total invocations over every jar
    sole           jars where this call is the only distinct unknown left
                   (so modelling it alone would make them ran-clean)

A Fabric jar runs every server stage it declares, in the loader's order
(`Hooks.startServer`: `main`, then `server`), passed to the probe as
`main,server`; a NeoForge jar runs its `@Mod` classes. The verdict is the one
the game's mod gives (`verdict.rs`): `ran-clean` when every stage ran with no
unknown host call, `ran-hollow` with at least one, `ran-partial` when a stage
failed and another ran, `refused` when nothing ran. A jar with no server-side
entrypoint is `skipped` (client-only or a library, out of scope for a server
run).

Default corpora: the Fabric `~/horizon-corpus/{corpus-xl,mega}` and the NeoForge
jars pinned by `tools/real_mod_fixtures/neoforge_corpus.lock.json`. Nothing is
downloaded and no jar is copied into the checkout.

Exit 0 with a report; 1 when the probe binary or a corpus directory is missing.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import hybrid_pack  # noqa: E402
import java_mod_server_ledger as ledger  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
NEOFORGE_LOCK = ROOT / "tools" / "real_mod_fixtures" / "neoforge_corpus.lock.json"
HOME_CORPUS = Path.home() / "horizon-corpus"
DEFAULT_CORPORA = (
    f"corpus-xl={HOME_CORPUS / 'corpus-xl'}",
    f"mega={HOME_CORPUS / 'mega'}",
    f"neoforge={HOME_CORPUS / 'neoforge'}",
)
PROBE_TIMEOUT = 180.0
MAX_REPORTED = 400


def parse_corpus(value: str) -> tuple[str, Path]:
    name, _, directory = value.partition("=")
    if not name or not directory:
        raise argparse.ArgumentTypeError("a corpus is NAME=DIR")
    return name, Path(directory).expanduser()


def locked_neoforge() -> set[str]:
    if not NEOFORGE_LOCK.is_file():
        return set()
    data = json.loads(NEOFORGE_LOCK.read_text())
    rows = data.get("jars") if isinstance(data, dict) else None
    if not isinstance(rows, list):
        return set()
    return {str(row.get("file")) for row in rows if isinstance(row, dict) and row.get("file")}


def corpus_jars(name: str, directory: Path) -> list[Path]:
    if not directory.is_dir():
        raise SystemExit(f"{name}: {directory} is not a directory")
    jars = sorted(p for p in directory.iterdir() if p.suffix.lower() == ".jar" and p.is_file())
    if name == "neoforge":
        locked = locked_neoforge()
        if locked:
            jars = [jar for jar in jars if jar.name in locked]
    return jars


SERVER_STAGES = ("main", "server")


def entrypoint_kind(path: Path, corpus: str) -> str | None:
    """The stages a dedicated server runs for this jar, comma-joined in loader order.

    Fabric Loader's `Hooks.startServer` invokes every `main` entrypoint, then
    every `server` one, so a jar with both runs both (the game's mod does the
    same since #31). A jar without Fabric server stages runs its NeoForge
    `@Mod` classes.
    """
    if corpus == "neoforge":
        return "neoforge"
    entrypoints = ledger.read_jar_entrypoints(path)
    if entrypoints is None:
        return None
    stages = [kind for kind in SERVER_STAGES if kind in entrypoints.kinds]
    if stages:
        return ",".join(stages)
    if entrypoints.neoforge:
        return "neoforge"
    return None


def row_verdict(result: dict) -> str:
    """The label of one probe run that returned `status: ok`.

    The probe reports the game's own verdict (`ran-partial` included); an older
    probe without one is labelled from its unknown-call count.
    """
    verdict = result.get("verdict")
    if verdict in {"ran-clean", "ran-hollow", "ran-partial", "refused"}:
        return verdict
    return "ran-clean" if result.get("unknown_host_call_count") == 0 else "ran-hollow"


def probe_one(probe: Path, jar: Path, kind: str, deps: list[Path],
              game_version: str) -> dict:
    command = [str(probe), "--json", "--game-version", game_version]
    for dep in deps:
        command += ["--link", str(dep)]
    command += [str(jar), kind]
    try:
        result = subprocess.run(command, capture_output=True, text=True,
                                timeout=PROBE_TIMEOUT, check=False)
    except (OSError, subprocess.SubprocessError) as error:
        return {"status": "refused", "stage": "probe", "error": str(error)}
    for raw in reversed(result.stdout.splitlines()):
        line = raw.strip()
        if line.startswith("{"):
            try:
                report = json.loads(line)
            except ValueError:
                continue
            if isinstance(report, dict):
                return report
    return {"status": "refused", "stage": "probe",
            "error": (result.stderr or result.stdout).strip()[-400:]}


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--corpus", action="append", type=parse_corpus,
                        help="NAME=DIR, repeatable (default: the three measured corpora)")
    parser.add_argument("--probe", type=Path,
                        default=ROOT / "target" / "fast" / "horizon-native-java")
    parser.add_argument("--game-version", default="26.3")
    parser.add_argument("--json", type=Path, help="write the full per-jar report here")
    parser.add_argument("--limit", type=int, default=0,
                        help="probe at most N jars per corpus (0 = all)")
    parser.add_argument("--jobs", type=int, default=4)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    corpora = args.corpus or [parse_corpus(value) for value in DEFAULT_CORPORA]
    probe = args.probe.resolve()
    if not probe.is_file():
        print(f"missing probe binary {probe}", file=sys.stderr)
        return 1

    # Providers live in the corpora under test and in the pools the mega corpus
    # was fetched with, exactly as `java_mod_server_ledger.DEFAULT_DEP_DIRS`.
    # A NeoForge jar resolves inside the NeoForge corpus only: the Fabric corpora
    # carry Fabric builds of the same libraries (TerraBlender, GlitchCore), and
    # linking those beside a NeoForge mod measures the wrong class path.
    indexes: dict[str, dict] = {}
    for name, directory in corpora:
        if name in indexes:
            continue
        if name == "neoforge":
            provider_dirs = [directory]
        else:
            provider_dirs = [path for other, path in corpora if other != "neoforge"]
            provider_dirs += [path for path in ledger.DEFAULT_DEP_DIRS
                              if str(path) not in {str(d) for d in provider_dirs}]
        indexes[name] = ledger.provider_index(ledger.corpus_jars(provider_dirs))

    report: dict[str, dict] = {}
    for name, directory in corpora:
        index = indexes[name]
        jars = corpus_jars(name, directory)
        if args.limit:
            jars = jars[:args.limit]
        rows: list[dict] = []
        print(f"{name}: {len(jars)} jars", file=sys.stderr)
        todo = []
        for jar in jars:
            kind = entrypoint_kind(jar, name)
            if kind is None:
                rows.append({"jar": jar.name, "verdict": "skipped",
                             "reason": "no server entrypoint"})
                continue
            closure = ledger.dependency_closure(jar, index)
            todo.append((jar, kind, list(closure.jars), [key for key, _ in closure.missing]))

        def run(item):
            jar, kind, deps, missing = item
            return jar, kind, deps, missing, probe_one(probe, jar, kind, deps,
                                                      args.game_version)

        with ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
            for jar, kind, deps, missing, result in pool.map(run, todo):
                status = result.get("status")
                if status == "ok":
                    unknown = result.get("unknown_host_call_count")
                    calls = result.get("unknown_host_calls")
                    if not isinstance(unknown, int) or not isinstance(calls, list):
                        calls = []
                    row = {
                        "jar": jar.name,
                        "verdict": row_verdict(result),
                        "kind": kind,
                        "unknown": unknown,
                        "calls": calls,
                        "deps": [dep.name for dep in deps],
                        "missing": missing,
                    }
                    if result.get("failed"):
                        row["failed"] = [str(item)[:400] for item in result["failed"]]
                    rows.append(row)
                else:
                    rows.append({
                        "jar": jar.name,
                        "verdict": "refused",
                        "kind": kind,
                        "stage": result.get("stage"),
                        "error": str(result.get("error"))[:400],
                        "deps": [dep.name for dep in deps],
                        "missing": missing,
                    })
        report[name] = {"directory": str(directory), "rows": rows}

    totals, ranked = aggregate({name: data["rows"] for name, data in report.items()})
    print(json.dumps({"totals": totals, "ranked": ranked[:MAX_REPORTED],
                      "corpora": report}, indent=2))
    if args.json:
        args.json.write_text(json.dumps({"totals": totals, "ranked": ranked,
                                        "corpora": report}, indent=2))
    return 0


def aggregate(corpora: dict[str, list[dict]]) -> tuple[dict, list[dict]]:
    """Per-corpus label counts and the ranked unknown-call table.

    An unknown call is ranked by how many jars it blocks, then by the jars it
    is the only distinct unknown of, then by its total volume. A jar counts
    once per call however many times the call ran inside it.
    """
    hits: dict[str, dict] = defaultdict(lambda: {"jars": 0, "calls": 0, "sole": 0,
                                                 "corpora": set()})
    totals = {}
    for name, rows in corpora.items():
        counts = defaultdict(int)
        for row in rows:
            counts[row["verdict"]] += 1
            calls = row.get("calls") or []
            distinct = {str(call) for call, _ in calls}
            for call, count in calls:
                hit = hits[str(call)]
                hit["jars"] += 1
                hit["calls"] += int(count)
                hit["corpora"].add(name)
                if len(distinct) == 1:
                    hit["sole"] += 1
        totals[name] = {
            **dict(counts),
            "jars": len(rows),
            "unknown_total": sum(row.get("unknown") or 0 for row in rows),
        }
    ranked = sorted(
        ({"call": call, **{key: value for key, value in hit.items() if key != "corpora"},
          "corpora": sorted(hit["corpora"])}
         for call, hit in hits.items()),
        key=lambda entry: (-entry["sole"], -entry["jars"], -entry["calls"], entry["call"]),
    )
    return totals, ranked


if __name__ == "__main__":
    raise SystemExit(main())
