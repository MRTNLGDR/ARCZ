from __future__ import annotations

"""Preflight e submissão segura do render fotorreal local.

A solicitação pública contém apenas IDs/revisões. No submit, o serviço congela
uma cópia da revisão Aedifex, resolve o GLB derivado real quando disponível e
materializa somente caminhos já verificados dentro da raiz do ARCZ. Nenhum
worker recebe URL externa, segredo ou caminho escolhido livremente pelo cliente.

Blender também é uma dependência materializada: o runtime aceita somente o
vendor auditado em ``vendor/blender`` (ou ARCZ_BLENDER apontando exatamente para
um executável dentro da raiz do repo e coberto pelo mesmo manifesto). PATH
externo não é fallback de produção.
"""

from copy import deepcopy
import json
import os
from pathlib import Path
from typing import Any

from .ai_broker import ModelRegistry
from .errors import ApiError
from .floorplanner_store import FloorplannerStore
from .hashing import sha256_file
from .jobs import JobManager
from .reference_media import ReferenceMediaStore
from .schema_validation import SchemaRegistry


QUALITY_SAMPLES = {
    "draft": 16,
    "preview": 64,
    "balanced": 128,
    "high": 256,
    "ultra": 512,
}


class PhotorealRenderService:
    def __init__(self, root: Path, schemas: SchemaRegistry, models: ModelRegistry,
                 media: ReferenceMediaStore, floorplanner: FloorplannerStore, jobs: JobManager):
        self.root = root.resolve(); self.schemas = schemas; self.models = models
        self.media = media; self.floorplanner = floorplanner; self.jobs = jobs

    def _inside_root(self, path: Path) -> bool:
        try:
            path.resolve().relative_to(self.root)
            return True
        except (ValueError, OSError):
            return False

    def _blender_status(self) -> dict[str, Any]:
        launcher = self.root / "workers" / "blender" / "launch_blender.py"
        render_script = self.root / "workers" / "blender" / "render_floor_scene.py"
        vendor_root = self.root / "vendor" / "blender"
        manifest_path = vendor_root / "manifest.json"
        manifest: dict[str, Any] | None = None
        executable: Path | None = None
        blockers: list[str] = []

        try:
            parsed = json.loads(manifest_path.read_text(encoding="utf-8"))
            if not isinstance(parsed, dict):
                raise ValueError("manifesto não é objeto")
            manifest = parsed
        except (OSError, json.JSONDecodeError, ValueError) as error:
            blockers.append(f"manifesto Blender local inválido/ausente: {error}")

        if manifest is not None:
            if manifest.get("schema_version") != 1 or manifest.get("dependency") != "Blender":
                blockers.append("manifesto Blender possui contrato desconhecido")
            if manifest.get("runtime_network_required") is not False:
                blockers.append("vendor Blender não declara runtime offline")
            relative_executable = str(manifest.get("executable") or "").strip()
            if not relative_executable:
                blockers.append("manifesto Blender não declara executável")
            else:
                candidate = (vendor_root / relative_executable).resolve()
                if not self._inside_root(candidate) or not candidate.is_file() or candidate.is_symlink():
                    blockers.append("executável Blender não existe dentro do repo")
                else:
                    expected_hash = str((manifest.get("integrity") or {}).get("executable_sha256") or "")
                    if len(expected_hash) != 64:
                        blockers.append("manifesto Blender sem SHA-256 válido")
                    else:
                        actual_hash = sha256_file(candidate)
                        if actual_hash != expected_hash:
                            blockers.append("SHA-256 do executável Blender diverge do manifesto")
                        else:
                            executable = candidate

        configured = os.environ.get("ARCZ_BLENDER", "").strip()
        configured_path: Path | None = None
        configured_inside_repo = False
        if configured:
            try:
                configured_path = Path(configured).expanduser().resolve()
                configured_inside_repo = self._inside_root(configured_path)
            except OSError:
                configured_path = None
            if not configured_inside_repo:
                blockers.append("ARCZ_BLENDER aponta para fora da raiz do ARCZ")
            elif executable is not None and configured_path != executable:
                blockers.append("ARCZ_BLENDER não corresponde ao executável coberto pelo vendor manifest")
            elif executable is None:
                blockers.append("ARCZ_BLENDER local não substitui manifesto/hash obrigatório")

        installed = bool(
            executable
            and not blockers
            and launcher.is_file()
            and render_script.is_file()
        )
        return {
            "installed": installed,
            "verified_repo_local": bool(executable and not blockers),
            "executable": str(executable) if executable else None,
            "manifest": str(manifest_path),
            "manifest_version": manifest.get("version") if manifest else None,
            "configured": str(configured_path) if configured_path else configured or None,
            "configured_inside_repo": configured_inside_repo if configured else None,
            "launcher": str(launcher),
            "render_script": str(render_script),
            "launcher_exists": launcher.is_file(),
            "render_script_exists": render_script.is_file(),
            "blockers": blockers,
        }

    def _resolve_scene_export(self, project_id: str, revision: int,
                              requested_id: str | None = None) -> dict[str, Any] | None:
        project = self.floorplanner.get_project(project_id, include_scene=False)
        candidates = [
            item for item in project.get("exports", [])
            if int(item.get("revision", 0)) == int(revision) and str(item.get("format", "")).lower() == "glb"
        ]
        if requested_id:
            candidates = [item for item in candidates if str(item.get("id")) == str(requested_id)]
            if not candidates:
                raise ApiError(
                    "FLOORPLANNER_EXPORT_NOT_FOUND",
                    f"{project_id}@{revision}/{requested_id}",
                    status=404,
                )
        if not candidates:
            return None
        candidates.sort(key=lambda item: str(item.get("created_at", "")), reverse=True)
        record = dict(candidates[0])
        raw_path = Path(str(record.get("path", "")))
        path = raw_path.resolve() if raw_path.is_absolute() else (self.root / raw_path).resolve()
        try:
            relative = path.relative_to(self.root).as_posix()
        except ValueError as error:
            raise ApiError("FLOORPLANNER_EXPORT_PATH_ESCAPE", str(path), status=403) from error
        if not path.is_file() or path.is_symlink():
            raise ApiError("FLOORPLANNER_EXPORT_MISSING", relative, status=409)
        actual_hash = sha256_file(path)
        expected_hash = str(record.get("sha256") or "")
        if actual_hash != expected_hash:
            raise ApiError(
                "FLOORPLANNER_EXPORT_HASH_MISMATCH",
                relative,
                status=409,
                details={"expected": expected_hash, "actual": actual_hash},
            )
        record["path"] = relative
        record["absolute_path"] = str(path)
        record["integrity"] = {"ok": True, "sha256": actual_hash}
        return record

    def preflight(self, request: dict[str, Any]) -> dict[str, Any]:
        self.schemas.validate("photoreal-render-request.schema.json", request)
        blockers: list[dict[str, Any]] = []; warnings: list[dict[str, Any]] = []
        width, height = int(request["resolution"]["width"]), int(request["resolution"]["height"])
        pixels = width * height
        quality = str(request.get("quality") or "balanced")
        engine = str(request.get("engine") or "cycles")
        render_settings = request.get("render_settings") if isinstance(request.get("render_settings"), dict) else {}
        samples = int(render_settings.get("samples") or QUALITY_SAMPLES.get(quality, 128))
        revision = None
        scene_export = None
        try:
            revision = self.floorplanner.get_revision(
                str(request["floorplanner_project_id"]), int(request["revision"]),
            )
            scene_export = self._resolve_scene_export(
                str(request["floorplanner_project_id"]), int(request["revision"]),
                str(request.get("scene_export_id")) if request.get("scene_export_id") else None,
            )
        except ApiError as error:
            blockers.append({"code": error.code, "message": error.message, "details": error.details})

        if scene_export is None:
            item = {
                "code": "AEDIFEX_GLB_EXPORT_MISSING",
                "message": "A revisão ainda não possui GLB real do viewport Aedifex; o worker só poderá reconstruir os tipos paramétricos suportados.",
            }
            if quality in {"high", "ultra"}:
                blockers.append(item | {"message": item["message"] + " Exporte o GLB antes do render final."})
            else:
                warnings.append(item)

        refs = []
        for identifier in request.get("reference_media", []):
            try:
                record = self.media.get(str(identifier), verify=True); refs.append(record)
                if not record.get("integrity", {}).get("ok"):
                    blockers.append({"code": "REFERENCE_MEDIA_CORRUPT", "id": identifier})
            except ApiError as error:
                blockers.append({"code": error.code, "id": identifier, "message": error.message})

        mode = request.get("enhancement", {}).get("mode", "none")
        model = None
        if mode != "none":
            try:
                found = self.models.find(
                    task="render-diffusion",
                    model_id=request.get("enhancement", {}).get("model_id"),
                )
                model = {
                    "id": found.manifest["id"],
                    "version": found.manifest["version"],
                    "backend": found.manifest["backend"],
                    "manifest_path": str(found.manifest_path),
                }
            except ApiError as error:
                blockers.append({"code": error.code, "message": error.message})
            if not str(request.get("enhancement", {}).get("prompt") or "").strip():
                warnings.append({"code": "ENHANCEMENT_PROMPT_EMPTY", "message": "O modelo local receberá apenas passes e referências."})

        if pixels > 8192 * 8192 or max(width, height) > 8192:
            warnings.append({"code": "TILED_RENDER_REQUIRED", "pixels": pixels})
        if request.get("format") == "exr":
            warnings.append({"code": "EXR_OUTPUT_REQUIRES_VIEWER_SUPPORT"})
        if engine == "eevee" and quality in {"high", "ultra"}:
            warnings.append({"code": "EEVEE_FINAL_QUALITY_LIMIT", "message": "Cycles é recomendado para a entrega final."})
        if "render.photoreal" not in self.jobs.supported_kinds():
            blockers.append({"code": "PHOTOREAL_WORKER_NOT_INSTALLED"})
        blender = self._blender_status()
        if not blender["installed"]:
            blockers.append({
                "code": "BLENDER_NOT_INSTALLED",
                "message": (
                    "Materialize uma distribuição Blender portátil auditada em vendor/blender com "
                    "tools/vendor_blender.py; nenhum PATH externo ou render fictício será usado."
                ),
                "details": blender,
            })

        # Beauty 4 B/px; passes EXR assumem 16 B/px. O multiplicador de
        # framebuffer cobre color, depth, denoise e buffers temporários.
        pass_count = max(1, len(request.get("passes", ["beauty"])))
        uncompressed_bytes = pixels * (4 + max(0, pass_count - 1) * 16)
        framebuffer_bytes = pixels * (48 if engine == "cycles" else 24)
        max_memory_mb = render_settings.get("max_memory_mb")
        if max_memory_mb and framebuffer_bytes > int(max_memory_mb) * 1024 * 1024:
            blockers.append({
                "code": "RENDER_MEMORY_BUDGET_EXCEEDED",
                "message": "Estimativa de framebuffer excede o orçamento configurado.",
                "details": {"estimated_bytes": framebuffer_bytes, "max_memory_mb": int(max_memory_mb)},
            })
        return {
            "ready": not blockers,
            "blockers": blockers,
            "warnings": warnings,
            "model": model,
            "blender": blender,
            "scene": None if revision is None else {
                "project_id": revision["project_id"],
                "revision": revision["revision"],
                "scene_hash": revision["scene_hash"],
                "glb_export": None if scene_export is None else {
                    "id": scene_export["id"],
                    "path": scene_export["path"],
                    "sha256": scene_export["sha256"],
                    "bytes": scene_export["bytes"],
                },
            },
            "reference_count": len(refs),
            "estimate": {
                "pixels": pixels,
                "samples": samples,
                "engine": engine,
                "quality": quality,
                "uncompressed_pass_bytes": uncompressed_bytes,
                "framebuffer_bytes": framebuffer_bytes,
                "requires_tiling": pixels > 8192 * 8192 or max(width, height) > 8192,
            },
        }

    def submit(self, request: dict[str, Any]) -> dict[str, Any]:
        preflight = self.preflight(request)
        if not preflight["ready"]:
            raise ApiError(
                "PHOTOREAL_PREFLIGHT_FAILED", "Render bloqueado pelo preflight",
                status=422, details=preflight,
            )
        revision = self.floorplanner.get_revision(
            str(request["floorplanner_project_id"]), int(request["revision"]),
        )
        scene_export = self._resolve_scene_export(
            str(request["floorplanner_project_id"]), int(request["revision"]),
            str(request.get("scene_export_id")) if request.get("scene_export_id") else None,
        )
        frozen = deepcopy(request)
        frozen["scene_document"] = revision["scene"]
        frozen["scene_hash"] = revision["scene_hash"]
        frozen["scene_revision_metadata"] = revision.get("metadata", {})
        frozen["resolved_scene_export"] = scene_export
        resolved_references = []
        for identifier in request.get("reference_media", []):
            record = self.media.get(str(identifier), verify=True)
            path, _mime, _size = self.media.content_path(str(identifier))
            record["absolute_path"] = str(path)
            resolved_references.append(record)
        frozen["reference_media_records"] = resolved_references
        frozen["resolved_enhancement_model"] = preflight.get("model")
        frozen["resolved_blender"] = preflight.get("blender")
        return self.jobs.create(
            "render.photoreal", frozen, int(request.get("generation_epoch", 0)),
        )
