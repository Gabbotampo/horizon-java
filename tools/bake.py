#!/usr/bin/env python3
"""Bake a Java mod jar's assets and data into a Horizon content package.

A native mod may not own another namespace's files or declarations (`Content::check_namespace`
and the generated declaration fence in the fork), so a jar's own models, textures, names,
blockstates, loot, blocks and items never reach the game by themselves. This tool closes that gap at
install time: it runs each jar's server entrypoints through the offline runtime
(``horizon-native-java --json <jar>``, the code the mod vendors), reads the typed blocks and items
it recorded, keeps the resource kinds a content package accepts, and writes one ``.hmod`` per
namespace into ``<game-dir>/horizon-mods/``. The package declares its own blocks and items
(``data/<ns>/horizon/{block,item}/*.json``) next to the jar's assets, and the game loads it as an
ordinary content mod; the mod's runtime declaration for that namespace is skipped.

    python3 tools/bake.py --probe target/debug/horizon-native-java --game-dir /tmp/m3b-env/game-dir
    python3 tools/bake.py --game-dir SCRATCH --java-mods SCRATCH/config/horizon_java/java-mods

The probe is ``--probe``, ``$HORIZON_NATIVE_JAVA`` or a cargo build under ``target/``; without one
the bake refuses the jar rather than writing a package whose items do not exist.

The bake is deterministic: same jar bytes, same package bytes. A jar with an unsafe entry path
(zip-slip) is refused whole and named in the report; nothing is extracted from it. Class files,
nested jars and executable payloads are never copied, and every entry left out is in
``config/horizon_java/bake-report.json`` with its reason.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import zipfile
from dataclasses import dataclass, field
from pathlib import Path

# The folder the game reads content packages from, beside `options.txt`
# (horizon-mod-package `mod_list::DIR`), and the file that turns one on (`FILE`).
MODS_DIR = "horizon-mods"
MODS_FILE = "horizon-mods.txt"
REPORT = Path("config") / "horizon_java" / "bake-report.json"

# The content fence's path rule: `assets/` or `data/`, then `/`-separated parts of
# `[a-z0-9_.-]`, none of them `.` or `..` (horizon-mod-package `content::check_path`).
PLAIN_PART = re.compile(r"^[a-z0-9_.-]+$")
NAMESPACE = re.compile(r"^[a-z0-9_.-]{1,64}$")
# A `.hmod` manifest id is narrower than a namespace: `[a-z0-9_]{1,64}` and not a Windows
# device name (horizon-mod-package `manifest::valid_mod_id`).
PACKAGE_ID = re.compile(r"^[a-z0-9_]{1,64}$")
RESERVED_IDS = {"con", "prn", "aux", "nul"} | {
    f"{prefix}{digit}" for prefix in ("com", "lpt") for digit in "123456789"
}

# What a content package may carry under `assets/<namespace>/`: the pack-layer kinds a Java mod
# ships for its blocks, items and sounds. `sounds.json` is the one file at the namespace root.
ASSET_KINDS = set(
    """
    atlases blockstates equipment font items lang models particles post_effect shaders sounds
    texts textures waypoint_style
    """.split()
)
ASSET_FILES = {"sounds.json"}

# `data/<namespace>/` kinds the fork reads (vanilla 26.3's data folders, minus `horizon/`: those
# are content declarations, which the runtime already supplies for this jar).
DATA_KINDS = set(
    """
    advancement banner_pattern cat_sound_variant cat_variant chat_type chicken_sound_variant
    chicken_variant cow_sound_variant cow_variant damage_type decorated_pot_pattern dialog
    dimension_type enchantment enchantment_provider frog_variant instrument item_modifier
    jukebox_song loot_table painting_variant pig_sound_variant pig_variant predicate recipe
    structure tags test_environment test_instance trade_set trial_spawner trim_material
    trim_pattern villager_trade wolf_sound_variant wolf_variant world_clock worldgen
    zombie_nautilus_variant
    """.split()
)

# Vanilla's and the convention tags a mod may add to whichever namespace package it owns.
SHARED_NAMESPACES = {"minecraft", "c"}

# Never copied, whatever folder they sit in: code and executables. The kind allowlist already
# leaves them out; naming them gives the report the real reason.
REFUSED_SUFFIXES = {
    ".class",
    ".jar",
    ".zip",
    ".wasm",
    ".exe",
    ".dll",
    ".dylib",
    ".so",
    ".bat",
    ".cmd",
    ".sh",
    ".py",
}

MAX_FILE = 16 << 20  # the fork's `content::MAX_FILE`
MAX_TOTAL = 512 << 20  # the fork's `content::MAX_TOTAL`
MAX_FILES = 65_536  # the fork's `content::MAX_FILES`

# The runtime probe (`crates/horizon-native-java`) runs a jar's entrypoints offline and prints its
# typed blocks and items as content declarations. The bake never re-implements the interpreter.
PROBE_ENV = "HORIZON_NATIVE_JAVA"
PROBE = "horizon-native-java"
SERVER_KINDS = ("main", "server", "neoforge")

# The fixed entry timestamp: a package's bytes must not depend on when the bake ran.
ZIP_TIME = (1980, 1, 1, 0, 0, 0)
ZIP_MODE = 0o644 << 16


class UnsafeJar(Exception):
    """A jar entry could escape the package (zip-slip) or is not a plain file."""


class ProbeError(Exception):
    """The offline runtime could not say what content a jar declares."""


@dataclass
class Skipped:
    """One reason entries were left out of a jar's bake, with a few example paths."""

    reason: str
    count: int = 0
    paths: list[str] = field(default_factory=list)

    def add(self, path: str) -> None:
        self.count += 1
        if len(self.paths) < 8:
            self.paths.append(path)

    def as_json(self) -> dict:
        return {"reason": self.reason, "count": self.count, "paths": self.paths}


@dataclass
class Package:
    id: str
    namespace: str
    path: Path
    files: list[str]
    bytes: int
    blocks: int = 0
    items: int = 0

    def as_json(self, game_dir: Path) -> dict:
        return {
            "id": self.id,
            "namespace": self.namespace,
            "path": _relative(self.path, game_dir),
            "files": len(self.files),
            "bytes": self.bytes,
            "declarations": {"blocks": self.blocks, "items": self.items},
        }


@dataclass
class JarBake:
    jar: str
    packages: list[Package] = field(default_factory=list)
    skipped: dict[str, Skipped] = field(default_factory=dict)
    error: str | None = None
    namespaces: list[str] = field(default_factory=list)

    def skip(self, reason: str, path: str) -> None:
        self.skipped.setdefault(reason, Skipped(reason)).add(path)

    def as_json(self, game_dir: Path) -> dict:
        return {
            "jar": self.jar,
            "namespaces": self.namespaces,
            "packages": [package.as_json(game_dir) for package in self.packages],
            "skipped": [skipped.as_json() for skipped in self.skipped.values()],
            "error": self.error,
        }


@dataclass
class BakeReport:
    java_mods: Path
    game_dir: Path
    out: Path
    jars: list[JarBake] = field(default_factory=list)
    enabled: list[str] = field(default_factory=list)

    @property
    def failed(self) -> bool:
        return any(jar.error for jar in self.jars)

    def as_json(self) -> dict:
        return {
            "schema": 1,
            "java_mods": _relative(self.java_mods, self.game_dir),
            "out": _relative(self.out, self.game_dir),
            "enabled": self.enabled,
            "jars": [jar.as_json(self.game_dir) for jar in self.jars],
        }


def _relative(path: Path, base: Path) -> str:
    try:
        return path.resolve().relative_to(base.resolve()).as_posix()
    except ValueError:
        return path.as_posix()


def safe_entry(name: str) -> bool:
    """Whether a zip entry name is a plain relative path: no absolute path, no `..`, no `\\`."""
    if not name or "\x00" in name or "\\" in name:
        return False
    if name.startswith("/") or (len(name) > 1 and name[1] == ":"):
        return False
    parts = name.rstrip("/").split("/")
    return all(part not in ("", ".", "..") for part in parts)


def plain_path(name: str) -> bool:
    """The fork's content path rule (`content::check_path`), without the `assets|data` prefix."""
    return all(PLAIN_PART.match(part) for part in name.split("/"))


def _refused_suffix(name: str) -> bool:
    lower = name.lower()
    return any(lower.endswith(suffix) for suffix in REFUSED_SUFFIXES)


def _is_symlink(info: zipfile.ZipInfo) -> bool:
    return (info.external_attr >> 16) & 0o170000 == 0o120000


def _zip_entries(jar: Path, bake: JarBake) -> list[tuple[str, bytes]]:
    """Reads the jar's plain files, refusing the whole jar on an unsafe entry.

    Unsafe means a path that could escape the package (`..`, absolute, a drive letter, a
    backslash, NUL) or an entry that is not a regular file (a symlink). Refusing the jar whole is
    deliberate: a jar that tries any of these is not one to extract half of.
    """
    entries: list[tuple[str, bytes]] = []
    total = 0
    try:
        with zipfile.ZipFile(jar) as archive:
            for info in archive.infolist():
                name = info.filename
                if not safe_entry(name):
                    raise UnsafeJar(f"unsafe entry path: {name!r}")
                if _is_symlink(info):
                    raise UnsafeJar(f"symlink entry: {name!r}")
                if info.is_dir():
                    continue
                if info.flag_bits & 0x1:
                    raise UnsafeJar(f"encrypted entry: {name!r}")
                if info.file_size > MAX_FILE:
                    bake.skip("over the content file cap (16 MiB)", name)
                    continue
                total += info.file_size
                if total > MAX_TOTAL:
                    raise UnsafeJar("content over the package total cap (512 MiB)")
                entries.append((name, archive.read(info)))
    except zipfile.BadZipFile as error:
        raise UnsafeJar(f"not a readable jar: {error}") from error
    if len(entries) > MAX_FILES:
        raise UnsafeJar("more files than the content file cap")
    return entries


def _classify(name: str) -> tuple[str, str, str] | None:
    """`assets|data`, namespace, and the path under it, for a name inside a content tree."""
    parts = name.split("/")
    if len(parts) < 3 or parts[0] not in ("assets", "data"):
        return None
    return parts[0], parts[1], "/".join(parts[2:])


def _allowed(top: str, namespace: str, rest: str, path: str, bake: JarBake) -> bool:
    if namespace in SHARED_NAMESPACES:
        # Only tag additions and vanilla atlas entries are a mod's to make in a shared namespace
        # (`content::check_namespace`); everything else stays vanilla's.
        if top == "data" and rest.startswith("tags/"):
            return True
        if top == "assets" and namespace == "minecraft" and rest.startswith("atlases/"):
            return True
        bake.skip("vanilla's namespace: only tag and atlas additions are a mod's", path)
        return False
    if not NAMESPACE.match(namespace):
        bake.skip("not a content namespace ([a-z0-9_.-]{1,64})", path)
        return False
    kind, _, _ = rest.partition("/")
    if top == "assets":
        if kind in ASSET_KINDS or rest in ASSET_FILES:
            return True
        bake.skip("unsupported asset kind", path)
        return False
    if kind == "horizon":
        bake.skip("content declarations: the runtime declares this jar's blocks and items", path)
        return False
    if kind in DATA_KINDS:
        return True
    bake.skip("unsupported data kind", path)
    return False


def _manifest(namespace: str, jar: str) -> str:
    return (
        f'id = "{namespace}"\n'
        f'name = "{namespace}"\n'
        'version = "1.0.0"\n'
        f'description = "Assets and data baked from {jar} for the {namespace} namespace."\n'
        "authors = []\n"
        'api = "0.2"\n'
        'side = "both"\n'
        'capabilities = ["registries"]\n'
    )


def find_probe(explicit: Path | None) -> Path | None:
    """The runtime probe: the flag, `$HORIZON_NATIVE_JAVA`, or a cargo build in this checkout."""
    if explicit is not None:
        return explicit if explicit.is_file() else None
    configured = os.environ.get(PROBE_ENV)
    if configured:
        path = Path(configured)
        if path.is_file():
            return path
    for folder in ("target/release", "target/debug", ".local/parity-target/release", ".local/parity-target/debug"):
        path = Path(folder) / PROBE
        if path.is_file():
            return path
    return None


def probe_declarations(probe: Path, jar: Path) -> list[tuple[str, str, dict]]:
    """Runs the jar's server entrypoints offline and returns its (kind, id, content file) blocks
    and items. The probe is the same runtime the mod vendors, so what it records is exactly what
    the mod would have declared at Registration."""
    last = "no output"
    for kind in SERVER_KINDS:
        try:
            run = subprocess.run(
                [str(probe), "--json", str(jar), kind],
                capture_output=True,
                text=True,
                timeout=120,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            raise ProbeError(str(error)) from error
        lines = [line for line in run.stdout.splitlines() if line.strip()]
        if run.returncode != 0 or not lines:
            errors = [line for line in run.stderr.splitlines() if line.strip()]
            last = errors[-1] if errors else f"exit status {run.returncode}"
            continue
        try:
            report = json.loads(lines[-1])
        except json.JSONDecodeError as error:
            last = f"not JSON: {error}"
            continue
        if report.get("status") != "ok":
            last = str(report.get("error", report.get("status")))
            continue
        if not report.get("entrypoints"):
            last = "no server entrypoint"
            continue
        content = report.get("content") or {}
        declarations = []
        for name, key in (("block", "blocks"), ("item", "items")):
            for entry in content.get(key, []):
                declarations.append((name, entry["id"], entry["file"]))
        return declarations
    raise ProbeError(f"no server entrypoint ran ({last})")


def write_package(path: Path, namespace: str, jar: str, files: list[tuple[str, bytes]]) -> int:
    """Writes a deterministic `.hmod` (a zip: `horizon-mod.toml` then the sorted files)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    with zipfile.ZipFile(temporary, "w", zipfile.ZIP_DEFLATED) as archive:
        manifest = _manifest(namespace, jar)
        info = zipfile.ZipInfo("horizon-mod.toml", ZIP_TIME)
        info.external_attr = ZIP_MODE
        info.compress_type = zipfile.ZIP_DEFLATED
        archive.writestr(info, manifest.encode("utf-8"))
        for name, data in sorted(files):
            info = zipfile.ZipInfo(name, ZIP_TIME)
            info.external_attr = ZIP_MODE
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data)
    os.replace(temporary, path)
    return path.stat().st_size


def _declaration_bytes(file: dict) -> bytes:
    """The declaration file, deterministic (sorted keys, compact) and newline-terminated."""
    return json.dumps(file, sort_keys=True, separators=(",", ":")).encode() + b"\n"


def bake_jar(jar: Path, out: Path, claimed: dict[str, str], probe: Path | None) -> JarBake:
    """Bakes one jar into one package per namespace; `claimed` refuses a shared namespace."""
    bake = JarBake(jar=jar.name)
    try:
        entries = _zip_entries(jar, bake)
    except UnsafeJar as error:
        bake.error = str(error)
        return bake
    if probe is None:
        bake.error = (
            f"no runtime probe: pass --probe <{PROBE}> or set {PROBE_ENV}, so the jar's blocks "
            "and items can be declared in its package"
        )
        return bake
    try:
        declarations = probe_declarations(probe, jar)
    except ProbeError as error:
        bake.error = f"the runtime could not run the jar's entrypoints: {error}"
        return bake

    by_namespace: dict[str, list[tuple[str, bytes]]] = {}
    shared: list[tuple[str, bytes]] = []
    for name, data in entries:
        if _refused_suffix(name):
            bake.skip("code or an executable, never copied", name)
            continue
        if not plain_path(name):
            bake.skip("not a plain content path", name)
            continue
        classified = _classify(name)
        if classified is None:
            bake.skip("not an assets/ or data/ file", name)
            continue
        top, namespace, rest = classified
        if not _allowed(top, namespace, rest, name, bake):
            continue
        if namespace in SHARED_NAMESPACES:
            shared.append((name, data))
        else:
            by_namespace.setdefault(namespace, []).append((name, data))

    # The runtime's typed registrations become the package's own declarations
    # (`data/<namespace>/horizon/{block,item}/<path>.json`), which the content loader reads without
    # the native ownership fence. A block with `item: true` declares its own item; an item file for
    # the same id would refuse the namespace.
    block_items = {
        declaration_id
        for kind, declaration_id, file in declarations
        if kind == "block" and file.get("item", True)
    }
    declared = set()
    for kind, declaration_id, file in declarations:
        namespace, path = declaration_id.split(":", 1) if ":" in declaration_id else ("", "")
        if not namespace or not path:
            bake.skip("declaration id is not namespace:path", declaration_id)
            continue
        if namespace == "minecraft":
            bake.skip("vanilla's namespace: a mod does not declare vanilla content", declaration_id)
            continue
        if not NAMESPACE.match(namespace) or not plain_path(path):
            bake.skip("declaration id is not a content namespace and path", declaration_id)
            continue
        if not PACKAGE_ID.match(namespace) or namespace in RESERVED_IDS:
            # A manifest id is `[a-z0-9_]`; a package id its declarations can join has the same
            # shape (`mod_content::accept_declared` matches the id). `-` and `.` have no package.
            bake.error = (
                f"namespace {namespace!r} is not a content package id ([a-z0-9_], not a device "
                "name): its declarations cannot join a package (docs/mod-plan.md, "
                "Fork seams needed)"
            )
            continue
        if kind == "item" and declaration_id in block_items:
            bake.skip("the block's own item (its block declares it)", declaration_id)
            continue
        if declaration_id in declared:
            bake.skip("declared more than once in the jar", declaration_id)
            continue
        declared.add(declaration_id)
        target = f"data/{namespace}/horizon/{kind}/{path}.json"
        by_namespace.setdefault(namespace, []).append(
            (target, _declaration_bytes(file))
        )

    bake.namespaces = sorted(by_namespace)
    if not bake.namespaces:
        if shared:
            bake.skip(
                "vanilla tag or atlas additions with no jar namespace to bake into", shared[0][0]
            )
        return bake

    for namespace in bake.namespaces:
        if not PACKAGE_ID.match(namespace) or namespace in RESERVED_IDS:
            # Runtime declarations join a package by id (`mod_content::accept_declared`), and a
            # manifest id is `[a-z0-9_]`; a `-` or `.` namespace has no package shape that joins.
            bake.error = (
                f"namespace {namespace!r} is not a content package id ([a-z0-9_], not a device "
                "name): a runtime declaration cannot join its assets (docs/mod-plan.md, "
                "Fork seams needed)"
            )
            continue
        if namespace in claimed:
            bake.error = (
                f"namespace {namespace} is already baked from {claimed[namespace]}; "
                "two content mods may not own one namespace"
            )
            continue
        files = by_namespace[namespace]
        if namespace == bake.namespaces[0]:
            files = files + shared
        target = out / f"{namespace}.hmod"
        size = write_package(target, namespace, jar.name, files)
        prefix = f"data/{namespace}/horizon/"
        package = Package(
            id=namespace,
            namespace=namespace,
            path=target,
            files=[name for name, _ in sorted(files)],
            bytes=size,
            blocks=sum(1 for name, _ in files if name.startswith(prefix + "block/")),
            items=sum(1 for name, _ in files if name.startswith(prefix + "item/")),
        )
        bake.packages.append(package)
        claimed[namespace] = jar.name
    if shared and not bake.packages:
        bake.skip("vanilla tag or atlas additions with no package to carry them", shared[0][0])
    return bake


def enable(game_dir: Path, ids: list[str]) -> list[str]:
    """Turns the packages on in `horizon-mods.txt`; returns the ids it wrote."""
    if not ids:
        return []
    path = game_dir / MODS_FILE
    lines = path.read_text(encoding="utf-8").splitlines() if path.exists() else []
    written = []
    for mod_id in ids:
        line = f"mod.{mod_id}:true"
        wanted = f"mod.{mod_id}:"
        replaced = False
        for index, existing in enumerate(lines):
            if existing.strip().startswith(wanted):
                if existing.strip() != line:
                    lines[index] = line
                replaced = True
                break
        if not replaced:
            lines.append(line)
        written.append(mod_id)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return written


def bake_directory(
    java_mods: Path,
    game_dir: Path,
    out: Path | None = None,
    turn_on: bool = True,
    probe: Path | None = None,
) -> BakeReport:
    """Bakes every `*.jar` under `java_mods` into `out` (default `<game_dir>/horizon-mods`)."""
    out = out if out is not None else game_dir / MODS_DIR
    report = BakeReport(java_mods=java_mods, game_dir=game_dir, out=out)
    jars = sorted(java_mods.glob("*.jar")) if java_mods.is_dir() else []
    probe = find_probe(probe)
    claimed: dict[str, str] = {}
    for jar in jars:
        report.jars.append(bake_jar(jar, out, claimed, probe))
    if turn_on:
        ids = [package.id for jar in report.jars for package in jar.packages]
        report.enabled = enable(game_dir, ids)
    return report


def _write_report(report: BakeReport, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(report.as_json(), indent=2, sort_keys=False)
    path.write_text(text + "\n", encoding="utf-8")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--game-dir",
        type=Path,
        default=Path("."),
        help="the game folder: packages go in its horizon-mods/ (default: .)",
    )
    parser.add_argument(
        "--java-mods",
        type=Path,
        help="the jars to bake (default: <game-dir>/config/horizon_java/java-mods)",
    )
    parser.add_argument(
        "--out", type=Path, help="where the .hmod files go (default: <game-dir>/horizon-mods)"
    )
    parser.add_argument(
        "--no-enable",
        action="store_true",
        help="do not add mod.<id>:true lines to horizon-mods.txt",
    )
    parser.add_argument(
        "--report",
        type=Path,
        help="the report file (default: config/horizon_java/bake-report.json)",
    )
    parser.add_argument(
        "--probe",
        type=Path,
        help=f"the {PROBE} runtime probe (default: ${PROBE_ENV} or a cargo build)",
    )
    args = parser.parse_args(argv)

    game_dir = args.game_dir.resolve()
    java_mods = args.java_mods or game_dir / "config" / "horizon_java" / "java-mods"
    out = args.out or game_dir / MODS_DIR
    report = bake_directory(java_mods, game_dir, out, not args.no_enable, args.probe)
    report_path = args.report or game_dir / REPORT
    _write_report(report, report_path)

    for jar in report.jars:
        if jar.error:
            print(f"{jar.jar}: refused: {jar.error}", file=sys.stderr)
            continue
        names = ", ".join(package.id for package in jar.packages) or "no packages"
        skipped = sum(skipped.count for skipped in jar.skipped.values())
        print(f"{jar.jar}: {len(jar.packages)} package(s) [{names}], {skipped} file(s) skipped")
    print(f"report: {report_path}")
    return 1 if report.failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
