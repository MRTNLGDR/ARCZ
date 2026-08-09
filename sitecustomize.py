"""ARCZ Python process bootstrap.

Python imports ``sitecustomize`` during interpreter startup when this repository
is the script/import root (including ``python servidor.py``). Keep only process-
wide safety defaults here; no network, file mutation or dependency install.

The legacy server still contains a historical external fallback string, but it
is no longer reachable in normal repository execution because ARCZ_BANCO is set
before that module is evaluated.
"""
from __future__ import annotations

import os
from pathlib import Path

_ROOT = Path(__file__).resolve().parent
_DEFAULT_ASSET_BANK = (_ROOT / "resources" / "assets").resolve()

# Never overwrite an explicit deployment value here; launchers and service
# managers may set it. The official ARCZ launchers set this exact repo path.
os.environ.setdefault("ARCZ_BANCO", str(_DEFAULT_ASSET_BANK))
os.environ.setdefault("ARCZ_NETWORK_MODE", "offline_strict")
