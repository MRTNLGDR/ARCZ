#!/usr/bin/env python3
"""Remove exclusivamente artefatos voláteis antes de empacotar o handoff."""
from __future__ import annotations
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]
DIRECTORIES = [".pytest_cache", "__pycache__"]
GLOBS = [
    "**/__pycache__", "**/*.pyc", "**/*.pyo", ".pytest_cache",
    "jobs/*.sqlite3*", "data/**/*.sqlite3*", "data/registry.sqlite3*",
    "scene/staging/*", "data/floorplanner/exports/*", "data/media/content/*",
    "logs/*", "cache/*", "cache_*/*",
]

for name in DIRECTORIES:
    path = ROOT / name
    if path.exists(): shutil.rmtree(path)
for pattern in GLOBS:
    for path in ROOT.glob(pattern):
        if path.is_dir(): shutil.rmtree(path)
        else: path.unlink(missing_ok=True)
for directory in [
    ROOT / "jobs", ROOT / "scene" / "staging", ROOT / "data" / "indexes",
    ROOT / "data" / "floorplanner" / "exports", ROOT / "data" / "media" / "content",
    ROOT / "logs",
    ROOT / "cache", ROOT / "cache_dem", ROOT / "cache_entorno", ROOT / "cache_geo",
    ROOT / "cache_glb", ROOT / "cache_overpass",
]:
    directory.mkdir(parents=True, exist_ok=True)
    (directory / ".gitkeep").touch()
print("árvore limpa")
