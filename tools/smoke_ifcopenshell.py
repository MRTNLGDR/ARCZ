#!/usr/bin/env python3
from __future__ import annotations

"""Functional IfcOpenShell smoke for ARCZ.

Creates an ARCZ scene with a real wall, slab and column, exports it through the
production IFC worker, reopens the IFC, validates generated geometry, and
inspects ARCZ tags. No fixture IFC is copied into the result.
"""

import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
WORKER = ROOT / "workers/ifc/ifc_worker.py"


def run_worker(*args: str) -> dict:
    completed = subprocess.run(
        [sys.executable, str(WORKER), *args],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=True,
        check=False,
        timeout=120,
        shell=False,
    )
    if completed.returncode:
        raise RuntimeError(
            f"IFC worker failed ({completed.returncode})\n"
            f"STDOUT:\n{completed.stdout[-4000:]}\nSTDERR:\n{completed.stderr[-4000:]}"
        )
    lines = [line for line in completed.stdout.splitlines() if line.strip()]
    if not lines:
        raise RuntimeError("IFC worker returned no JSON")
    return json.loads(lines[-1])


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="arcz-ifc-smoke-") as temp_name:
        temp = Path(temp_name)
        scene = temp / "scene.json"
        output = temp / "arcz-smoke.ifc"
        scene.write_text(
            json.dumps(
                {
                    "name": "ARCZ IFC Smoke",
                    "nodes": {
                        "level-0": {
                            "id": "level-0",
                            "type": "level",
                            "name": "Ground Floor",
                            "elevation": 0.0,
                        },
                        "wall-smoke": {
                            "id": "wall-smoke",
                            "type": "wall",
                            "name": "Smoke Wall",
                            "parentId": "level-0",
                            "start": [0.0, 0.0],
                            "end": [5.0, 0.0],
                            "height": 3.0,
                            "thickness": 0.2,
                        },
                        "slab-smoke": {
                            "id": "slab-smoke",
                            "type": "slab",
                            "name": "Smoke Slab",
                            "parentId": "level-0",
                            "polygon": [[0.0, 0.0], [5.0, 0.0], [5.0, 4.0], [0.0, 4.0]],
                            "thickness": 0.18,
                        },
                        "column-smoke": {
                            "id": "column-smoke",
                            "type": "column",
                            "name": "Smoke Column",
                            "parentId": "level-0",
                            "position": [2.5, 2.0],
                            "height": 3.0,
                            "width": 0.35,
                            "depth": 0.35,
                        },
                    },
                },
                ensure_ascii=False,
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )

        exported = run_worker("export-arcz", str(scene), str(output))
        if exported.get("schema") != "IFC4":
            raise RuntimeError(f"unexpected IFC schema: {exported}")
        classes = exported.get("classes") or {}
        expected = {"IfcWall": 1, "IfcSlab": 1, "IfcColumn": 1}
        for ifc_class, minimum in expected.items():
            if int(classes.get(ifc_class) or 0) < minimum:
                raise RuntimeError(f"missing {ifc_class} after export: {classes}")
        if int(exported.get("geometric_products") or 0) < 3:
            raise RuntimeError(f"IFC export lacks real geometry: {exported}")
        if not output.is_file() or output.stat().st_size < 1024:
            raise RuntimeError("exported IFC is missing or implausibly small")

        inspected = run_worker("inspect", str(output))
        tags = {item.get("tag") for item in inspected.get("elements") or []}
        required_tags = {"wall-smoke", "slab-smoke", "column-smoke"}
        if not required_tags.issubset(tags):
            raise RuntimeError(f"ARCZ semantic tags were not preserved: {tags}")

        validated = run_worker("validate", str(output))
        if validated.get("sha256") != exported.get("sha256"):
            raise RuntimeError("IFC hash changed between export and validation")

        print(
            json.dumps(
                {
                    "ok": True,
                    "ifcopenshell_version": exported.get("ifcopenshell_version"),
                    "schema": exported.get("schema"),
                    "entities": exported.get("entities"),
                    "products": exported.get("products"),
                    "geometric_products": exported.get("geometric_products"),
                    "classes": classes,
                    "sha256": exported.get("sha256"),
                    "bytes": exported.get("bytes"),
                    "semantic_tags": sorted(required_tags),
                },
                ensure_ascii=False,
            )
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
