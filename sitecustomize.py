"""ARCZ Python process bootstrap.

Python imports ``sitecustomize`` during interpreter startup when this repository
is the script/import root (including ``python servidor.py``). Keep only process-
wide safety defaults here; no network, file mutation or dependency install.

The legacy server still contains a historical external fallback string, but it
is no longer reachable in repository execution: ARCZ_BANCO is normalized before
that module is evaluated and an external override is rejected back to the
repo-local asset root.
"""
from __future__ import annotations

import os
from pathlib import Path

_ROOT = Path(__file__).resolve().parent
_DEFAULT_ASSET_BANK = (_ROOT / "resources" / "assets").resolve()


def _repo_local_asset_bank() -> Path:
    raw = os.environ.get("ARCZ_BANCO", "").strip()
    candidate = Path(raw).expanduser().resolve() if raw else _DEFAULT_ASSET_BANK
    try:
        candidate.relative_to(_ROOT)
    except ValueError:
        return _DEFAULT_ASSET_BANK
    return candidate


# User-facing runtime assets are always inside the ARCZ repository. An external
# ARCZ_BANCO value cannot silently reintroduce a machine-specific dependency.
os.environ["ARCZ_BANCO"] = str(_repo_local_asset_bank())
os.environ.setdefault("ARCZ_NETWORK_MODE", "offline_strict")
