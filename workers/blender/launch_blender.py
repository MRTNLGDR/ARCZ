#!/usr/bin/env python3
from __future__ import annotations

"""Launcher do pipeline fotorreal local.

Etapa 1: Blender gera a cena, beauty e passes técnicos.
Etapa 2: quando solicitado, o Local AI Broker executa o modelo de difusão
instalado e a saída só substitui o beauty se passar pelo geometry guard.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


def _sha256(path: Path) -> str:
    import hashlib
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _within(path: Path, root: Path) -> Path:
    resolved = path.resolve()
    try:
        resolved.relative_to(root.resolve())
    except ValueError as error:
        raise RuntimeError(f"enhancement output escaped staging: {resolved}") from error
    return resolved


def _enhance(request_path: Path, output: Path, root: Path) -> None:
    wrapper = json.loads(request_path.read_text(encoding="utf-8"))
    request = wrapper["request"]
    enhancement = request.get("enhancement", {})
    if enhancement.get("mode", "none") == "none":
        return

    sys.path.insert(0, str(root))
    from arcz_server.ai_broker import LocalAIBroker, ModelRegistry
    from arcz_server.image_guard import compare_structure
    from arcz_server.network_policy import NetworkMode, NetworkPolicy
    from arcz_server.schema_validation import SchemaRegistry

    manifest_path = output / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    beauty_entries = [item for item in manifest.get("outputs", []) if item.get("kind") == "beauty"]
    if not beauty_entries:
        raise RuntimeError("Blender manifest has no beauty output")
    base = Path(beauty_entries[-1]["path"]).resolve()
    references = [
        item.get("path") or item.get("absolute_path")
        for item in request.get("reference_media_records", [])
        if isinstance(item, dict)
    ]
    references = [str(Path(value).resolve()) for value in references if value]
    enhanced_dir = output / "enhancement"
    enhanced_dir.mkdir(parents=True, exist_ok=True)
    target = enhanced_dir / "beauty-enhanced.png"
    registry = ModelRegistry([root / "resources" / "models", root / "data" / "models"], SchemaRegistry(root / "schemas"))
    policy = NetworkPolicy(mode=NetworkMode.OFFLINE_STRICT, allow_loopback=True)
    broker = LocalAIBroker(root, registry, SchemaRegistry(root / "schemas"), policy)
    envelope = broker.request(
        "render-diffusion",
        {
            "schema_version": 1,
            "mode": enhancement.get("mode"),
            "base_image": str(base),
            "technical_passes": manifest.get("technical_passes", {}),
            "reference_media": references,
            "prompt": enhancement.get("prompt", ""),
            "negative_prompt": enhancement.get("negative_prompt", ""),
            "seed": int(enhancement.get("seed", 0)),
            "output_path": str(target),
            "preserve_geometry": True,
        },
        model_id=enhancement.get("model_id"),
    )
    result = envelope.get("result")
    if not isinstance(result, dict):
        raise RuntimeError("render-diffusion result must be an object")
    result_path = result.get("output_path") or result.get("image") or str(target)
    candidate = _within(Path(str(result_path)), output)
    if not candidate.is_file():
        raise RuntimeError(f"render-diffusion output missing: {candidate}")
    guard = compare_structure(
        base,
        candidate,
        guard_px=int(float(enhancement.get("geometry_guard_px", 2))),
    )
    manifest.setdefault("metrics", {})["geometry_guard"] = guard
    if not guard["ok"]:
        rejected = enhanced_dir / "rejected-geometry.png"
        if candidate != rejected:
            shutil.move(str(candidate), rejected)
            candidate = rejected
        manifest.setdefault("warnings", []).append(
            f"ENHANCEMENT_GEOMETRY_REJECTED ratio={guard['mismatch_ratio']:.6f} path={candidate}"
        )
    else:
        final = output / "render" / "beauty-enhanced.png"
        if candidate != final:
            shutil.copy2(candidate, final)
        manifest["outputs"].append({
            "path": str(final),
            "sha256": _sha256(final),
            "bytes": final.stat().st_size,
            "kind": "beauty-enhanced",
        })
        manifest.setdefault("metrics", {})["enhancement"] = {
            "model": envelope.get("model"),
            "cache_key": envelope.get("cache_key"),
            "mode": enhancement.get("mode"),
        }
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8")


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: launch_blender.py request.json output_dir", file=sys.stderr)
        return 2
    request = Path(sys.argv[1]).resolve()
    output = Path(sys.argv[2]).resolve()
    output.mkdir(parents=True, exist_ok=True)
    wrapper = json.loads(request.read_text(encoding="utf-8"))
    root = Path(wrapper.get("root") or Path(__file__).resolve().parents[2]).resolve()
    blender = os.environ.get("ARCZ_BLENDER") or shutil.which("blender")
    if not blender:
        print("BLENDER_NOT_INSTALLED: set ARCZ_BLENDER or install Blender", file=sys.stderr)
        return 127
    script = Path(__file__).with_name("render_floor_scene.py").resolve()
    if not script.is_file():
        print(f"worker script missing: {script}", file=sys.stderr)
        return 2
    command = [
        blender, "--background", "--factory-startup", "--python", str(script),
        "--", str(request), str(output),
    ]
    completed = subprocess.run(
        command,
        shell=False,
        env={**os.environ, "ARCZ_NETWORK_MODE": "offline_strict", "NO_PROXY": "*", "no_proxy": "*"},
    )
    if completed.returncode != 0:
        return completed.returncode
    try:
        _enhance(request, output, root)
    except Exception as error:
        print(f"LOCAL_ENHANCEMENT_FAILED: {error}", file=sys.stderr)
        return 3
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
