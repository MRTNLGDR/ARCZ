#!/usr/bin/env python3
from __future__ import annotations

"""Render a tiny real ARCZ scene through the production Blender worker.

This is intentionally not a Blender availability probe. It invokes the same
``workers/blender/render_floor_scene.py`` used by photoreal jobs, asks Cycles to
render a small scene, and validates the PNG, .blend and manifest hashes. The
smoke is CPU-only, 128x128 and four samples so it is suitable for a clean
Windows installation gate.
"""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

from PIL import Image, ImageStat

ROOT = Path(__file__).resolve().parents[1]
BLENDER_MANIFEST = ROOT / "vendor/blender/manifest.json"
WORKER = ROOT / "workers/blender/render_floor_scene.py"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def inside(path: Path, root: Path) -> bool:
    try:
        path.resolve().relative_to(root.resolve())
        return True
    except ValueError:
        return False


def blender_executable() -> tuple[Path, dict]:
    manifest = json.loads(BLENDER_MANIFEST.read_text(encoding="utf-8"))
    if manifest.get("dependency") != "Blender" or manifest.get("runtime_network_required") is not False:
        raise RuntimeError("Blender vendor manifest is not the expected local-only contract")
    relative = str(manifest.get("executable") or "")
    executable = (BLENDER_MANIFEST.parent / relative).resolve()
    if not executable.is_file() or not inside(executable, BLENDER_MANIFEST.parent):
        raise RuntimeError(f"Blender executable missing/escaped vendor: {executable}")
    expected = str((manifest.get("integrity") or {}).get("executable_sha256") or "")
    actual = sha256(executable)
    if len(expected) != 64 or actual != expected:
        raise RuntimeError("Blender executable SHA-256 does not match vendor manifest")
    return executable, manifest


def request_payload() -> dict:
    nodes = {
        "level-0": {
            "id": "level-0",
            "type": "level",
            "name": "Ground floor",
            "elevation": 0.0,
        },
        "slab-0": {
            "id": "slab-0",
            "type": "slab",
            "name": "Real smoke slab",
            "parentId": "level-0",
            "polygon": [[-2.5, -2.0], [2.5, -2.0], [2.5, 2.0], [-2.5, 2.0]],
            "thickness": 0.15,
        },
        "wall-0": {
            "id": "wall-0",
            "type": "wall",
            "name": "Real smoke wall",
            "parentId": "level-0",
            "start": [-2.5, -2.0],
            "end": [2.5, -2.0],
            "height": 2.7,
            "thickness": 0.18,
        },
        "column-0": {
            "id": "column-0",
            "type": "column",
            "name": "Real smoke column",
            "parentId": "level-0",
            "position": [0.0, 0.0],
            "height": 2.7,
            "width": 0.35,
            "depth": 0.35,
        },
    }
    return {
        "job_id": "arcz-cycles-smoke",
        "generation_epoch": 1,
        "request": {
            "scene_document": {
                "schema_version": 1,
                "revision": 1,
                "nodes": nodes,
                "rootNodeIds": ["level-0"],
            },
            "scene_hash": hashlib.sha256(
                json.dumps(nodes, sort_keys=True, separators=(",", ":")).encode("utf-8")
            ).hexdigest(),
            "resolution": {"width": 128, "height": 128},
            "format": "png",
            "quality": "draft",
            "engine": "cycles",
            "passes": ["beauty"],
            "output_name": "cycles-smoke",
            "render_settings": {
                "samples": 4,
                "denoise": False,
                "device": "cpu",
                "tile_size": 128,
                "transparent_background": False,
                "color_management": "AgX",
                "look": "AgX - Medium High Contrast",
            },
            "camera": {
                "position": [6.0, 4.0, 6.0],
                "target": [0.0, 1.0, 0.0],
                "focal_length_mm": 35.0,
                "aperture": 64.0,
                "focus_distance_m": 8.0,
            },
            "environment": {
                "world_mode": "nishita",
                "strength": 0.8,
                "sun_elevation_deg": 32.0,
                "sun_rotation_deg": -35.0,
                "sun_energy": 3.0,
                "haze": 1.0,
                "ground_color": [0.12, 0.14, 0.11],
            },
            "enhancement": {"seed": 17},
        },
    }


def validate_output(output: Path, blender_manifest: dict) -> dict:
    manifest_path = output / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("generator") != "arcz.render.blender@2.0.0":
        raise RuntimeError(f"unexpected render generator: {manifest.get('generator')}")
    metrics = manifest.get("metrics") or {}
    if metrics.get("engine") != "cycles":
        raise RuntimeError(f"smoke did not render with Cycles: {metrics}")
    if metrics.get("resolution") != [128, 128]:
        raise RuntimeError(f"unexpected render resolution: {metrics.get('resolution')}")
    if int(metrics.get("meshes") or 0) < 3:
        raise RuntimeError(f"production worker produced too few real meshes: {metrics}")

    outputs = manifest.get("outputs")
    if not isinstance(outputs, list):
        raise RuntimeError("render manifest outputs missing")
    records = {str(item.get("kind")): item for item in outputs if isinstance(item, dict)}
    for kind in ("beauty", "scene", "object-index-map", "semantic-index-map", "material-index-map"):
        record = records.get(kind)
        if not record:
            raise RuntimeError(f"render output record missing: {kind}")
        path = Path(str(record.get("path") or "")).resolve()
        if not path.is_file() or not inside(path, output):
            raise RuntimeError(f"render output missing/escaped smoke directory: {kind} -> {path}")
        if sha256(path) != record.get("sha256"):
            raise RuntimeError(f"render output SHA mismatch: {kind}")
        if path.stat().st_size != int(record.get("bytes") or -1):
            raise RuntimeError(f"render output size mismatch: {kind}")

    beauty = Path(str(records["beauty"]["path"]))
    with Image.open(beauty) as image:
        image.load()
        if image.size != (128, 128):
            raise RuntimeError(f"beauty PNG dimensions are {image.size}")
        rgb = image.convert("RGB")
        extrema = rgb.getextrema()
        if not extrema or all(low == high for low, high in extrema):
            raise RuntimeError("beauty PNG is constant/blank; Cycles did not produce a useful image")
        stats = ImageStat.Stat(rgb)
        if max(stats.var or [0.0]) <= 0.01:
            raise RuntimeError("beauty PNG has no measurable visual variance")

    return {
        "ok": True,
        "blender_version": blender_manifest.get("version"),
        "engine": metrics.get("engine"),
        "samples": metrics.get("samples"),
        "resolution": metrics.get("resolution"),
        "meshes": metrics.get("meshes"),
        "render_seconds": metrics.get("render_seconds"),
        "beauty_sha256": records["beauty"]["sha256"],
        "beauty_bytes": records["beauty"]["bytes"],
        "source": metrics.get("source"),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--keep-output", type=Path)
    args = parser.parse_args()

    if not WORKER.is_file():
        raise FileNotFoundError(WORKER)
    executable, blender_manifest = blender_executable()

    temp = tempfile.TemporaryDirectory(prefix="arcz-cycles-smoke-") if args.keep_output is None else None
    root = Path(temp.name) if temp else args.keep_output.resolve()
    root.mkdir(parents=True, exist_ok=True)
    request_path = root / "request.json"
    output_dir = root / "output"
    if output_dir.exists():
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True)
    request_path.write_text(
        json.dumps(request_payload(), ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    completed = subprocess.run(
        [
            str(executable),
            "--background",
            "--factory-startup",
            "--python",
            str(WORKER),
            "--",
            str(request_path),
            str(output_dir),
        ],
        cwd=executable.parent,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=True,
        timeout=180,
        check=False,
        shell=False,
        env={**__import__("os").environ, "BLENDER_USER_CONFIG": str(root / "blender-config")},
    )
    if completed.returncode != 0:
        raise RuntimeError(
            "production Blender worker failed\n"
            f"STDOUT:\n{completed.stdout[-6000:]}\nSTDERR:\n{completed.stderr[-6000:]}"
        )
    result = validate_output(output_dir, blender_manifest)
    print(json.dumps(result, ensure_ascii=False))
    if temp:
        temp.cleanup()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
