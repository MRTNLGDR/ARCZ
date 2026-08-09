#!/usr/bin/env python3
from __future__ import annotations

"""Canonical ARCZ local runtime entrypoint.

This module applies the user-facing runtime boundaries explicitly before the
legacy HTTP server is loaded:

- network defaults to ``offline_strict``;
- the asset bank is always ``resources/assets`` inside this repository;
- no machine-specific external asset directory can leak into the official
  launcher or CI smoke path.

Preparation commands use separate setup entrypoints and may explicitly run in
``import_assisted``; this runtime entrypoint never turns network access on.
"""

import os
from pathlib import Path
import runpy
import sys
from typing import Mapping

ROOT = Path(__file__).resolve().parent
ASSET_BANK = (ROOT / "resources" / "assets").resolve()
SERVER = ROOT / "servidor.py"


def runtime_environment(source: Mapping[str, str] | None = None) -> dict[str, str]:
    """Return the canonical environment for the normal local ARCZ runtime."""
    env = dict(source if source is not None else os.environ)
    env["ARCZ_NETWORK_MODE"] = "offline_strict"
    env["ARCZ_BANCO"] = str(ASSET_BANK)
    return env


def apply_runtime_environment() -> dict[str, str]:
    """Apply and return the canonical local runtime environment."""
    ASSET_BANK.mkdir(parents=True, exist_ok=True)
    env = runtime_environment()
    os.environ.update(env)
    return {
        "ARCZ_NETWORK_MODE": os.environ["ARCZ_NETWORK_MODE"],
        "ARCZ_BANCO": os.environ["ARCZ_BANCO"],
    }


def main() -> int:
    if not SERVER.is_file():
        print(f"ARCZ server entrypoint missing: {SERVER}", file=sys.stderr)
        return 2
    applied = apply_runtime_environment()
    try:
        Path(applied["ARCZ_BANCO"]).resolve().relative_to(ROOT)
    except ValueError:
        print("ARCZ_BANCO escaped repository after normalization", file=sys.stderr)
        return 3

    # Keep positional arguments intact (for example ``8123``) because
    # servidor.py consumes sys.argv directly. Execute it only after the runtime
    # boundary above is established.
    runpy.run_path(str(SERVER), run_name="__main__")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
