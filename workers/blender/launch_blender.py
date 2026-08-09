#!/usr/bin/env python3
from __future__ import annotations

"""Launcher do pipeline fotorreal local.

Etapa 1: Blender gera a cena, beauty e passes técnicos.
Etapa 2: quando solicitado, o Local AI Broker executa o modelo de difusão
instalado e a saída só substitui o beauty se passar pelo geometry guard.

O executável Blender NÃO é descoberto pelo PATH. Ele chega congelado no request
pelo preflight do ARCZ, deve estar dentro de ``vendor/blender`` e precisa manter
o SHA-256 registrado no manifesto local. Assim o worker executa exatamente a
mesma dependência que foi auditada antes da criação do job.
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


def _within_repo(path: Path, root: Path, label: str) -> Path:
    resolved = path.expanduser().resolve()
    try:
        resolved.relative_to(root.resolve())
    except ValueError as error:
        raise RuntimeError(f"{label} escaped ARCZ repository: {resolved}") from error
    if not resolved.is_file() or resolved.is_symlink():
        raise RuntimeError(f"{label} is missing or is not a regular file: {resolved}")
    return resolved


def _resolve_blender(wrapper: dict, root: Path) -> Path:
    request = wrapper.get("request")
    if not isinstance(request, dict):
        raise RuntimeError("worker request envelope has no request object")
    frozen = request.get("resolved_blender")
    if not isinstance(frozen, dict) or frozen.get("verified_repo_local") is not True:
        raise RuntimeError("BLENDER_NOT_VERIFIED: job has no frozen repo-local Blender")

    executable_value = str(frozen.get("executable") or "").strip()
    if not executable_value:
        raise RuntimeError("BLENDER_NOT_VERIFIED: frozen executable is empty")
    executable = _within_repo(Path(executable_value), root, "Blender executable")

    vendor_root = (root / "vendor" / "blender").resolve()
    try:
        executable.relative_to(vendor_root)
    except ValueError as error:
        raise RuntimeError(
            f"BLENDER_PATH_ESCAPE: executable is outside vendor/blender: {executable}"
        ) from error

    manifest_value = str(frozen.get("manifest") or "").strip()
    manifest_path = _within_repo(
        Path(manifest_value) if manifest_value else root / "vendor" / "blender" / "manifest.json",
        root,
        "Blender manifest",
    )
    if manifest_path.parent != vendor_root:
        raise RuntimeError(f"BLENDER_MANIFEST_ESCAPE: {manifest_path}")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("schema_version") != 1 or manifest.get("dependency") != "Blender":
        raise RuntimeError("BLENDER_MANIFEST_INVALID: unexpected contract")
    if manifest.get("runtime_network_required") is not False:
        raise RuntimeError("BLENDER_MANIFEST_INVALID: runtime must be offline")

    expected = str((manifest.get("integrity") or {}).get("executable_sha256") or "")
    if len(expected) != 64:
        raise RuntimeError("BLENDER_MANIFEST_INVALID: executable SHA-256 missing")
    actual = _sha256(executable)
    if actual != expected:
        raise RuntimeError(
            f"BLENDER_HASH_MISMATCH: expected {expected}, got {actual}"
        )
    manifest_executable = (vendor_root / str(manifest.get("executable") or "")).resolve()
    if manifest_executable != executable:
        raise RuntimeError(
            "BLENDER_EXECUTABLE_MISMATCH: frozen executable differs from vendor manifest"
        )
    return executable


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
    try:
        blender = _resolve_blender(wrapper, root)
    except Exception as error:
        print(f"BLENDER_NOT_VERIFIED: {error}", file=sys.stderr)
        return 127
    script = Path(__file__).with_name("render_floor_scene.py").resolve()
    if not script.is_file():
        print(f"worker script missing: {script}", file=sys.stderr)
        return 2
    command = [
        str(blender), "--background", "--factory-startup", "--python", str(script),
        "--", str(request), str(output),
    ]
    completed = subprocess.run(
        command,
        shell=False,
        env={
            **os.environ,
            "ARCZ_BLENDER": str(blender),
            "ARCZ_NETWORK_MODE": "offline_strict",
            "NO_PROXY": "*",
            "no_proxy": "*",
        },
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
