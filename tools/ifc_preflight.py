#!/usr/bin/env python3
from __future__ import annotations

"""Fast offline IfcOpenShell integrity preflight.

Unlike the functional smoke this does not create geometry on every launch. It
proves that the exact verified wheel evidence, copied license, repo-local Python
module and production IFC worker are still present. The canonical BAT uses this
to decide whether the expensive import-assisted setup must be repaired.
"""

import hashlib
import importlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
VENDOR = ROOT / "vendor/ifcopenshell"
MANIFEST = VENDOR / "manifest.json"
WORKER = ROOT / "workers/ifc/ifc_worker.py"
VERSION = "0.8.5"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def inside_repo(path: Path) -> bool:
    try:
        path.resolve().relative_to(ROOT.resolve())
        return True
    except ValueError:
        return False


def check() -> dict:
    blockers: list[str] = []
    manifest = None
    try:
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        blockers.append("vendor/ifcopenshell/manifest.json missing or invalid")

    if manifest:
        if manifest.get("schema_version") != 1:
            blockers.append("IfcOpenShell manifest schema_version mismatch")
        if manifest.get("dependency") != "IfcOpenShell" or manifest.get("version") != VERSION:
            blockers.append("IfcOpenShell manifest version mismatch")
        if manifest.get("license") != "LGPL-3.0-or-later":
            blockers.append("IfcOpenShell license declaration mismatch")
        if manifest.get("runtime_network_required") is not False:
            blockers.append("IfcOpenShell manifest does not declare offline runtime")

        for field, label in (("wheel", "wheel"), ("license_file", "license")):
            record = manifest.get(field)
            if not isinstance(record, dict):
                blockers.append(f"IfcOpenShell {label} evidence missing")
                continue
            relative = str(record.get("path") or "")
            expected = str(record.get("sha256") or "")
            path = (ROOT / relative).resolve() if relative else Path()
            if not relative or not path.is_file() or not inside_repo(path):
                blockers.append(f"IfcOpenShell {label} file missing/escaped repo")
            elif len(expected) != 64 or sha256(path) != expected:
                blockers.append(f"IfcOpenShell {label} SHA-256 mismatch")
            elif path.stat().st_size != int(record.get("bytes") or -1):
                blockers.append(f"IfcOpenShell {label} size mismatch")

    if not WORKER.is_file():
        blockers.append("production IFC worker missing")

    module_path = None
    try:
        module = importlib.import_module("ifcopenshell")
        if str(getattr(module, "version", "")) != VERSION:
            blockers.append(f"loaded IfcOpenShell version is {getattr(module, 'version', None)!r}")
        module_path = Path(module.__file__).resolve()
        if not inside_repo(module_path):
            blockers.append(f"IfcOpenShell Python module escaped repo: {module_path}")
        if ".venv" not in {part.lower() for part in module_path.parts}:
            blockers.append(f"IfcOpenShell Python module is not in repo .venv: {module_path}")
        model = module.file(schema="IFC4")
        if str(model.schema).upper() != "IFC4":
            blockers.append("IfcOpenShell cannot create IFC4 file")
    except Exception as error:
        blockers.append(f"IfcOpenShell import/probe failed: {error.__class__.__name__}: {error}")

    return {
        "schema_version": 1,
        "dependency": "IfcOpenShell",
        "version": VERSION,
        "ready": not blockers,
        "network_mode": "offline_strict",
        "module": str(module_path) if module_path else None,
        "worker": str(WORKER),
        "manifest": str(MANIFEST),
        "blockers": blockers,
    }


def main() -> int:
    result = check()
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0 if result["ready"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
