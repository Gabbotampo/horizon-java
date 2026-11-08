#!/usr/bin/env python3
"""The small slice of Horizon's ``tools/lab.py`` the Java-mod corpus tools use.

The corpus tools were written inside the Horizon tree, where ``lab`` also owns
the pinned Minecraft import, decompilation and the parity harness helpers. None
of that belongs here, so this module keeps only what these tools call:
``cargo_target()`` and ``game_env()``. Their behaviour is the upstream
behaviour, copied verbatim.
"""

from __future__ import annotations

import atexit
import os
import shutil
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def cargo_target():
    """Cargo's target directory, honouring CARGO_TARGET_DIR."""
    return ROOT / os.environ.get("CARGO_TARGET_DIR", "target")


_SCRATCH_DIRS = []


def _scratch_dir(prefix):
    path = tempfile.mkdtemp(prefix=prefix)
    _SCRATCH_DIRS.append(path)
    return path


@atexit.register
def _remove_scratch_dirs():
    for path in _SCRATCH_DIRS:
        shutil.rmtree(path, ignore_errors=True)


def game_env(base=None, **extra):
    """The environment for a game run: no audio, no Discord presence, and fresh
    scratch folders for the game's home, the launcher's home and the XDG folders,
    so a run never writes into a real player's folders. A folder the caller
    already has in ``base`` (default ``os.environ``) or passes in ``extra`` is
    kept. The scratch folders are removed when this process exits."""
    env = dict(os.environ if base is None else base)
    env.update({key: str(value) for key, value in extra.items()})
    env["HORIZON_NO_AUDIO"] = "1"
    env["HORIZON_LAUNCHER_NO_SELF_UPDATE"] = "1"
    env.setdefault("HORIZON_TEST_LOOPBACK", "1")
    if not env.get("HORIZON_DISCORD_IPC_DIR"):
        env["HORIZON_NO_DISCORD"] = "1"
    for key, prefix in (
        ("HORIZON_HOME", "horizon-home-"),
        ("HORIZON_LAUNCHER_HOME", "horizon-launcher-home-"),
        ("XDG_DATA_HOME", "horizon-xdg-data-"),
        ("XDG_CONFIG_HOME", "horizon-xdg-config-"),
    ):
        if not env.get(key):
            env[key] = _scratch_dir(prefix)
    return env
