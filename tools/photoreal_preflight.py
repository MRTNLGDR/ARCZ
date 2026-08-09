#!/usr/bin/env python3
from __future__ import annotations

"""Preflight dedicado ao render fotorreal base do ARCZ.

Este gate não exige modelo generativo: Cycles precisa funcionar sozinho. Modelos
locais de enhancement são verificados pelo preflight da requisição apenas quando
o usuário escolhe enhancement != none.
"""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from tools.runtime_preflight import _blender_check

WORKER_FILES = (
    ROOT / "resources/workers/render.photoreal.worker.json",
    ROOT / "workers/blender/launch_blender.py",
    ROOT / "workers/blender/render_floor_scene.py",
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    blender = _blender_check(ROOT)
    workers = [
        {
            "path": path.relative_to(ROOT).as_posix(),
            "ok": path.is_file() and path.stat().st_size > 0,
            "bytes": path.stat().st_size if path.is_file() else 0,
        }
        for path in WORKER_FILES
    ]
    ready = blender["status"] == "READY" and all(item["ok"] for item in workers)

    executable = (blender.get("detail") or {}).get("executable")
    probe = None
    if ready and executable:
        completed = subprocess.run(
            [str(executable), "--version"],
            cwd=ROOT,
            env={**os.environ, "ARCZ_NETWORK_MODE": "offline_strict"},
            shell=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=30,
        )
        probe = {
            "returncode": completed.returncode,
            "output": ((completed.stdout or "") + "\n" + (completed.stderr or "")).strip()[:2000],
        }
        ready = completed.returncode == 0 and "Blender" in probe["output"]

    report = {
        "schema_version": 1,
        "capability": "render.photoreal.cycles",
        "network_mode": "offline_strict",
        "ready": ready,
        "blender": blender,
        "workers": workers,
        "probe": probe,
        "enhancement_model_required": False,
        "note": "Modelos generativos só são exigidos quando enhancement != none.",
    }
    text = json.dumps(report, ensure_ascii=False, indent=2)
    print(text)
    return 0 if ready else 2


if __name__ == "__main__":
    raise SystemExit(main())
