from __future__ import annotations

from datetime import datetime, timezone
import json
import os
from pathlib import Path
import time
from typing import Any
import urllib.parse

from .ai_broker import LocalAIBroker, ModelRegistry
from .aedifex_registry import AedifexRegistry
from .aedifex_runtime import AedifexRuntimeManager
from .chat_workspace import ChatWorkspace
from .floorplanner_store import FloorplannerStore
from .geo_model_bridge import GeoModelBridge
from .governance import GovernanceSnapshot
from .budget import BudgetEngine, Resources
from .diagnostics import diagnostics
from .errors import ApiError, as_api_error
from .generation_workers import RustGenerationWorker, CommandJobWorker
from .generator_contracts import GeneratorContracts
from .geocoder import LocalGeocoder
from .jobs import JobContext, JobManager, TERMINAL_STATUSES
from .input_assembler import LocalInputAssembler
from .network_policy import NetworkPolicy, install_egress_guard
from .panoramas import PanoramaRegistry
from .photoreal import PhotorealRenderService
from .prompt_library import PromptLibrary
from .reference_media import ReferenceMediaStore
from .plugin_catalog import PluginCatalog
from .profiles import ProfileStore
from .region_service import RegionService
from .schema_validation import SchemaRegistry
from .sheets import SheetComposer
from .source_registry import SourceRegistry
from .tiles import TilePlanner
from .tool_catalog import GlobalToolCatalog


class V2Router:
    """API local V2 para ser embutida no `servidor.py` existente.

    Não abre servidor paralelo nem muda a topologia do ARCZ. Toda rota é local,
    e o guard de egress bloqueia sockets externos no modo padrão.
    """

    def __init__(self, root: Path):
        self.root = root.resolve()
        self.schemas = SchemaRegistry(self.root / "schemas")
        self.policy = NetworkPolicy.from_environment()
        install_egress_guard(self.policy)
        self.sources = SourceRegistry(self.root / "data", self.schemas)
        self.geocoder = LocalGeocoder(self.root / "data" / "indexes" / "geocoder.sqlite3")
        self.profiles = ProfileStore([
            self.root / "resources" / "profiles", self.root / "data" / "profiles"
        ], self.schemas)
        self.models = ModelRegistry([
            self.root / "resources" / "models", self.root / "data" / "models"
        ], self.schemas)
        self.ai = LocalAIBroker(self.root, self.models, self.schemas, self.policy)
        self.regions = RegionService(self.schemas, self.geocoder, self.sources, self.profiles, self.ai)
        self.tiles = TilePlanner()
        self.budget = BudgetEngine(self.root / "jobs" / "budget.sqlite3")
        self.contracts = GeneratorContracts(self.schemas)
        self.inputs = LocalInputAssembler(self.schemas, self.sources, self.contracts)
        self.jobs = JobManager(self.root, self.schemas, workers=int(os.environ.get("ARCZ_JOB_WORKERS", "1")))
        self.plugins = PluginCatalog([self.root / "resources" / "plugins"], self.schemas)
        self.panoramas = PanoramaRegistry(self.root / "data" / "panoramas", self.schemas)
        self.sheets = SheetComposer(self.root)
        # Floorplanner/Aedifex é um núcleo de autoria local subordinado à
        # georreferência do ARCZ; não cria outro mundo ou outro estado territorial.
        self.aedifex = AedifexRegistry(self.root)
        self.aedifex_runtime = AedifexRuntimeManager(self.root, self.aedifex)
        self.geo_models = GeoModelBridge(self.schemas, self.root)
        self.floorplanner = FloorplannerStore(self.root / "data" / "floorplanner" / "floorplanner.sqlite3", self.schemas)
        self.media = ReferenceMediaStore(self.root, self.schemas)
        self.prompts = PromptLibrary(self.root, self.schemas, self.ai)
        self.chat = ChatWorkspace(self.root, self.schemas, self.ai, self.media)
        self.tools = GlobalToolCatalog(self.root, self.aedifex, self.aedifex_runtime)
        self.photoreal = PhotorealRenderService(self.root, self.schemas, self.models, self.media, self.floorplanner, self.jobs)
        self.governance = GovernanceSnapshot(self.root)
        rust = RustGenerationWorker(self.root)
        for kind in (
            "terrain.generate", "parcels.generate", "roads.generate", "houses.generate",
            "buildings.generate", "vegetation.generate", "materials.generate",
            "region.context.generate", "tiles.generate",
        ):
            self.jobs.register(kind, rust)
        self.jobs.register("sheets.compose", self._sheet_worker)
        self._register_command_workers()

    def _register_command_workers(self) -> None:
        root = self.root / "resources" / "workers"
        if not root.is_dir():
            return
        for path in sorted(root.glob("*.worker.json")):
            try:
                manifest = json.loads(path.read_text(encoding="utf-8"))
                self.schemas.validate("local-worker-manifest.schema.json", manifest)
                kind = str(manifest["kind"])
                command = [str(item) for item in manifest["command"]]
                self.jobs.register(kind, CommandJobWorker(self.root, command,
                                                          timeout_seconds=int(manifest.get("timeout_seconds", 3600))))
            except Exception as error:
                # Falha de manifesto opcional fica no diagnóstico; não derruba o globo.
                print(f"[ARCZ/v2] worker ignorado {path}: {error}")

    def _tool_services(self) -> dict[str, Any]:
        return {
            "floorplanner_list": self.floorplanner.list_projects,
            "floorplanner_get": self.floorplanner.get_project,
            "prompt_list": self.prompts.list,
            "prompt_compile": self.prompts.compile,
            "prompt_enhance": self.prompts.enhance,
            "prompt_translate": self.prompts.translate,
            "media_list": self.media.list,
            "photoreal_preflight": self.photoreal.preflight,
            "photoreal_submit": self.photoreal.submit,
            "aedifex_status": self.aedifex_runtime.status,
        }

    def _invoke_chat_tool(self, name: str, arguments: dict[str, Any], context: dict[str, Any]) -> Any:
        return self.tools.invoke(name, arguments, services=self._tool_services(), context=context)

    def _sheet_worker(self, context: JobContext, request: dict[str, Any]):
        context.update("VALIDATE_REQUEST", 0.05, message="Validando fontes da prancha")
        destination = context.staging_dir / "sheet.svg"
        output = self.sheets.compose_svg(request, destination)
        context.update("PERSIST", 0.95, message="Prancha SVG gravada")
        from .hashing import canonical_json_hash
        now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
        return {
            "schema_version": 1, "job_id": context.job_id,
            "generator": "arcz.sheets.compose@1.0.0",
            "inputs_hash": canonical_json_hash(request),
            "profile_hash": "0" * 64, "seed": int(request.get("seed", 0)),
            "source_versions": {}, "outputs": [output], "warnings": [],
            "metrics": {"viewports": len(request.get("viewports", []))},
            "created_at": now, "deterministic": True,
            "generation_epoch": context.job["generation_epoch"],
        }

    def handle_get(self, handler, route: str, query: str) -> bool:
        if not route.startswith("/api/v2/") and route not in {"/api/v2", "/api/governance/snapshot"}:
            return False
        try:
            params = urllib.parse.parse_qs(query)
            if route in {"/api/v2", "/api/v2/health"}:
                return self._json(handler, {"ok": True, "api": "2", "network_mode": self.policy.mode.value,
                                            "job_kinds": self.jobs.supported_kinds()})
            if route == "/api/v2/diagnostics":
                return self._json(handler, diagnostics(self.root, policy=self.policy, jobs=self.jobs,
                                                       sources=self.sources, models=self.models))
            if route == "/api/v2/network":
                return self._json(handler, {"mode": self.policy.mode.value,
                                            "allow_loopback": self.policy.allow_loopback,
                                            "local_lan_cidrs": list(self.policy.local_lan_cidrs),
                                            "import_allowlist": sorted(self.policy.import_allowlist)})
            if route == "/api/governance/snapshot" or route == "/api/v2/governance/snapshot":
                return self._json(handler, self.governance.build())
            if route == "/api/v2/aedifex/status":
                return self._json(handler, self.aedifex_runtime.status())
            if route == "/api/v2/floorplanner/projects":
                return self._json(handler, self.floorplanner.list_projects(
                    region_id=params.get("region_id", [None])[0],
                    limit=int(params.get("limit", [100])[0])))
            if route.startswith("/api/v2/floorplanner/projects/"):
                parts = [urllib.parse.unquote(part) for part in route.strip("/").split("/")]
                # api/v2/floorplanner/projects/{id}/events
                if len(parts) == 6 and parts[-1] == "events":
                    project_id = parts[-2]
                    after = int(params.get("after", [0])[0])
                    if params.get("stream", ["0"])[0] in {"1", "true", "yes"}:
                        return self._floorplanner_sse(handler, project_id, after)
                    return self._json(handler, self.floorplanner.events_after(project_id, after=after,
                                                                               limit=int(params.get("limit", [200])[0])))
                # api/v2/floorplanner/projects/{id}/revisions/{n}
                if len(parts) == 7 and parts[-2] == "revisions":
                    return self._json(handler, self.floorplanner.get_revision(parts[-3], int(parts[-1])))
                if len(parts) == 5:
                    revision = params.get("revision", [None])[0]
                    return self._json(handler, self.floorplanner.get_project(
                        parts[-1], include_scene=params.get("include_scene", ["1"])[0] not in {"0", "false"},
                        revision=int(revision) if revision is not None else None))
            if route == "/api/v2/reference-media":
                return self._json(handler, self.media.list(category=params.get("category", [None])[0],
                                                            limit=int(params.get("limit", [200])[0])))
            if route.startswith("/api/v2/reference-media/"):
                parts = [urllib.parse.unquote(part) for part in route.strip("/").split("/")]
                if len(parts) == 5 and parts[-1] == "content":
                    path, mime, size = self.media.content_path(parts[-2])
                    return self._file(handler, path, mime, size)
                if len(parts) == 4:
                    return self._json(handler, self.media.get(parts[-1], verify=True))
            if route == "/api/v2/prompts":
                return self._json(handler, self.prompts.list(query=params.get("q", [None])[0],
                    category=params.get("category", [None])[0], language=params.get("language", [None])[0],
                    limit=int(params.get("limit", [200])[0])))
            if route.startswith("/api/v2/prompts/"):
                parts = [urllib.parse.unquote(part) for part in route.strip("/").split("/")]
                if len(parts) == 5 and parts[-1] == "versions":
                    return self._json(handler, self.prompts.versions(parts[-2], limit=int(params.get("limit", [100])[0])))
                if len(parts) == 5:
                    return self._json(handler, self.prompts.get(parts[-1]))
            if route == "/api/v2/chat/sessions":
                return self._json(handler, self.chat.list_sessions(limit=int(params.get("limit", [100])[0])))
            if route.startswith("/api/v2/chat/sessions/"):
                parts = [urllib.parse.unquote(part) for part in route.strip("/").split("/")]
                if len(parts) == 5:
                    return self._json(handler, self.chat.get_session(parts[-1], include_messages=True,
                                                                      limit=int(params.get("limit", [500])[0])))
            if route == "/api/v2/chat/tools":
                return self._json(handler, {
                    "tools": self.tools.list(include_unavailable=True),
                    "aedifex": self.aedifex_runtime.status(),
                })
            if route == "/api/v2/chat/tool-runs":
                return self._json(handler, self.chat.list_tool_runs(
                    session_id=params.get("session_id", [None])[0],
                    status=params.get("status", [None])[0],
                    limit=int(params.get("limit", [200])[0]),
                ))
            if route.startswith("/api/v2/chat/tool-runs/"):
                parts = [urllib.parse.unquote(part) for part in route.strip("/").split("/")]
                if len(parts) == 5:
                    return self._json(handler, self.chat.get_tool_run(parts[-1]))
            if route == "/api/v2/profiles":
                return self._json(handler, self.profiles.list())
            if route.startswith("/api/v2/profiles/"):
                return self._json(handler, self.profiles.get(urllib.parse.unquote(route.rsplit("/", 1)[1])))
            if route == "/api/v2/plugins":
                return self._json(handler, self.plugins.list())
            if route.startswith("/api/v2/plugins/"):
                return self._json(handler, self.plugins.get(urllib.parse.unquote(route.rsplit("/", 1)[1])))
            if route == "/api/v2/sources":
                kind = params.get("kind", [None])[0]
                return self._json(handler, self.sources.list(kind))
            if route == "/api/v2/models":
                return self._json(handler, self.models.list(verify=True))
            if route == "/api/v2/panoramas":
                return self._json(handler, self.panoramas.list())
            if route.startswith("/api/v2/panoramas/"):
                sequence_id = urllib.parse.unquote(route.rsplit("/", 1)[1])
                return self._json(handler, self.panoramas.get(sequence_id, verify_images=True))
            if route == "/api/v2/render/jobs":
                limit = int(params.get("limit", [100])[0])
                jobs = [job for job in self.jobs.store.list(limit=limit) if job["kind"].startswith("render.")]
                return self._json(handler, jobs)
            if route.endswith("/events") and route.startswith("/api/v2/render/jobs/"):
                job_id = route.split("/")[-2]
                job = self.jobs.store.get(job_id)
                if not job["kind"].startswith("render."):
                    raise ApiError("RENDER_JOB_NOT_FOUND", job_id, status=404)
                return self._sse(handler, job_id, params)
            if route.startswith("/api/v2/render/jobs/"):
                job_id = urllib.parse.unquote(route.rsplit("/", 1)[1])
                job = self.jobs.store.get(job_id)
                if not job["kind"].startswith("render."):
                    raise ApiError("RENDER_JOB_NOT_FOUND", job_id, status=404)
                return self._json(handler, job)
            if route == "/api/v2/generation/jobs":
                status = params.get("status", [None])[0]
                limit = int(params.get("limit", [100])[0])
                return self._json(handler, self.jobs.store.list(status=status, limit=limit))
            if route.endswith("/events") and route.startswith("/api/v2/generation/jobs/"):
                job_id = route.split("/")[-2]
                return self._sse(handler, job_id, params)
            if route.startswith("/api/v2/generation/jobs/"):
                job_id = urllib.parse.unquote(route.rsplit("/", 1)[1])
                return self._json(handler, self.jobs.store.get(job_id))
            raise ApiError("ROUTE_NOT_FOUND", route, status=404)
        except BaseException as error:
            self._error(handler, error)
            return True

    def handle_post(self, handler, route: str, body: bytes) -> bool:
        if not route.startswith("/api/v2/"):
            return False
        try:
            if route.startswith("/api/v2/floorplanner/projects/") and route.endswith("/exports/upload"):
                parts = [urllib.parse.unquote(part) for part in route.strip("/").split("/")]
                if len(parts) != 7:
                    raise ApiError("ROUTE_NOT_FOUND", route, status=404)
                project_id = parts[-3]
                revision_raw = handler.headers.get("X-ARCZ-Revision")
                if revision_raw is None:
                    raise ApiError("FLOORPLANNER_EXPORT_REVISION_REQUIRED", "X-ARCZ-Revision", status=400)
                try:
                    revision = int(revision_raw)
                except ValueError as error:
                    raise ApiError("FLOORPLANNER_EXPORT_REVISION_INVALID", revision_raw, status=400) from error
                if revision <= 0:
                    raise ApiError("FLOORPLANNER_EXPORT_REVISION_INVALID", revision_raw, status=400)
                manifest_raw = handler.headers.get("X-ARCZ-Semantic-Manifest") or "%7B%7D"
                try:
                    semantic_manifest = json.loads(urllib.parse.unquote(manifest_raw))
                except Exception as error:
                    raise ApiError("FLOORPLANNER_EXPORT_MANIFEST_INVALID", "X-ARCZ-Semantic-Manifest", status=400) from error
                return self._json(handler, self.floorplanner.import_export_bytes(
                    project_id, revision, body,
                    format=str(handler.headers.get("X-ARCZ-Format") or "glb"),
                    semantic_manifest=semantic_manifest,
                    scene_hash=handler.headers.get("X-ARCZ-Scene-Hash"),
                    root=self.root,
                ), status=201)
            if route == "/api/v2/reference-media/upload":
                filename = urllib.parse.unquote(str(handler.headers.get("X-ARCZ-Filename") or ""))
                def header_json(name: str, default):
                    raw = handler.headers.get(name)
                    if not raw:
                        return default
                    try:
                        return json.loads(urllib.parse.unquote(raw))
                    except Exception as error:
                        raise ApiError("MEDIA_HEADER_INVALID", name, status=400) from error
                metadata = {
                    "roles": header_json("X-ARCZ-Roles", ["reference"]),
                    "license": header_json("X-ARCZ-License", {"id": "LicenseRef-UserProvided", "redistribution_allowed": False}),
                    "provenance": header_json("X-ARCZ-Provenance", {"source": "browser_upload", "source_ref": filename}),
                    "metadata": header_json("X-ARCZ-Metadata", {}),
                }
                return self._json(handler, self.media.import_bytes(filename, body, metadata), status=201)
            payload = self._decode_json(body)
            if route == "/api/v2/aedifex/start":
                return self._json(handler, self.aedifex_runtime.start(
                    wait_seconds=float(payload.get("wait_seconds", 20.0))))
            if route == "/api/v2/aedifex/stop":
                return self._json(handler, self.aedifex_runtime.stop(
                    grace_seconds=float(payload.get("grace_seconds", 5.0))))
            if route == "/api/v2/floorplanner/context":
                return self._json(handler, self.geo_models.build_context(payload), status=201)
            if route == "/api/v2/floorplanner/projects":
                return self._json(handler, self.floorplanner.create_project(payload), status=201)
            if route.startswith("/api/v2/floorplanner/projects/"):
                parts = [urllib.parse.unquote(part) for part in route.strip("/").split("/")]
                if len(parts) == 6 and parts[-1] == "revisions":
                    self.schemas.validate("floorplanner-scene-revision.schema.json", payload)
                    return self._json(handler, self.floorplanner.save_revision(parts[-2], payload), status=201)
                if len(parts) == 6 and parts[-1] == "exports":
                    return self._json(handler, self.floorplanner.register_export(
                        parts[-2], int(payload["revision"]), payload, self.root), status=201)
            if route == "/api/v2/reference-media/import":
                return self._json(handler, self.media.import_from_inbox(payload), status=201)
            if route.startswith("/api/v2/reference-media/"):
                parts = [urllib.parse.unquote(part) for part in route.strip("/").split("/")]
                if len(parts) == 5 and parts[-1] == "metadata":
                    return self._json(handler, self.media.update_metadata(parts[-2], payload))
            if route == "/api/v2/prompts":
                return self._json(handler, self.prompts.upsert(payload), status=201)
            if route == "/api/v2/prompts/compile":
                identifier = payload.get("identifier", payload.get("template"))
                if not identifier:
                    raise ApiError("PROMPT_IDENTIFIER_REQUIRED", "identifier obrigatório", status=400)
                return self._json(handler, self.prompts.compile(str(identifier), payload.get("variables", {}),
                                                                context=payload.get("context")))
            if route == "/api/v2/prompts/export":
                return self._json(handler, self.prompts.export_bundle(payload))
            if route == "/api/v2/prompts/import":
                bundle = payload.get("bundle", payload)
                return self._json(handler, self.prompts.import_bundle(
                    bundle, conflict=str(payload.get("conflict", "duplicate")) if isinstance(payload, dict) else "duplicate"
                ), status=201)
            if route == "/api/v2/prompts/enhance":
                return self._json(handler, self.prompts.enhance(payload))
            if route == "/api/v2/prompts/translate":
                return self._json(handler, self.prompts.translate(payload))
            if route.startswith("/api/v2/prompts/"):
                parts = [urllib.parse.unquote(part) for part in route.strip("/").split("/")]
                if len(parts) == 5 and parts[-1] == "duplicate":
                    return self._json(handler, self.prompts.duplicate(parts[-2], payload), status=201)
                if len(parts) == 5 and parts[-1] == "archive":
                    return self._json(handler, self.prompts.archive(parts[-2]))
            if route == "/api/v2/chat/sessions":
                return self._json(handler, self.chat.create_session(payload), status=201)
            if route.startswith("/api/v2/chat/sessions/"):
                parts = [urllib.parse.unquote(part) for part in route.strip("/").split("/")]
                if len(parts) == 6 and parts[-1] == "messages":
                    return self._json(handler, self.chat.append_message(parts[-2], payload), status=201)
                if len(parts) == 6 and parts[-1] == "respond":
                    return self._json(handler, self.chat.respond(
                        parts[-2], payload, tool_catalog=self.tools.list(),
                        invoke_tool=self._invoke_chat_tool,
                    ), status=201)
                if len(parts) == 6 and parts[-1] == "continue":
                    return self._json(handler, self.chat.continue_after_tools(
                        parts[-2], payload, tool_catalog=self.tools.list(),
                        invoke_tool=self._invoke_chat_tool,
                    ), status=201)
            if route.startswith("/api/v2/chat/tool-runs/"):
                parts = [urllib.parse.unquote(part) for part in route.strip("/").split("/")]
                if len(parts) == 6 and parts[-1] == "approve":
                    return self._json(handler, self.chat.approve_tool_run(
                        parts[-2], payload, invoke_tool=self._invoke_chat_tool,
                    ))
                if len(parts) == 6 and parts[-1] in {"reject", "cancel"}:
                    return self._json(handler, self.chat.reject_tool_run(parts[-2], payload))
            if route == "/api/v2/chat/tools/invoke":
                context = payload.get("context", {})
                if not isinstance(context, dict):
                    raise ApiError("CHAT_TOOL_CONTEXT_INVALID", "context precisa ser objeto", status=400)
                return self._json(handler, {"result": self._invoke_chat_tool(
                    str(payload["name"]), payload.get("arguments", {}), context,
                )})
            if route == "/api/v2/photoreal/preflight":
                return self._json(handler, self.photoreal.preflight(payload))
            if route == "/api/v2/photoreal/jobs":
                return self._json(handler, self.photoreal.submit(payload), status=202)
            if route == "/api/v2/regions/resolve":
                return self._json(handler, self.regions.resolve(payload))
            if route == "/api/v2/regions/context":
                return self._json(handler, self.regions.build_context(payload))
            if route == "/api/v2/profiles/compose":
                return self._json(handler, self.profiles.compose(payload.get("profile_ids", []), payload.get("override")))
            if route == "/api/v2/profiles/infer":
                return self._json(handler, self.regions.recommend_profiles(payload["context"],
                                                                            allow_ai=bool(payload.get("allow_ai", False))))
            if route == "/api/v2/tiles/plan":
                return self._json(handler, self.tiles.plan(
                    payload["focus"], float(payload["radius_m"]), int(payload["zoom"]),
                    int(payload.get("generation_epoch", 0)), payload.get("rings_m")))
            if route == "/api/v2/budget":
                request = Resources.from_dict(payload.get("requested", payload))
                return self._json(handler, self.budget.evaluate(request, payload.get("profile", "EQUILIBRADO"),
                                                                reserve=bool(payload.get("reserve", True))))
            if route == "/api/v2/budget/release":
                state = str(payload.get("state", "RELEASED")).upper()
                if state not in {"RELEASED", "COMMITTED"}:
                    raise ApiError("BUDGET_RELEASE_STATE_INVALID", state, status=400)
                released = self.budget.release(str(payload["reservation_id"]), state=state)
                if not released:
                    raise ApiError("BUDGET_RESERVATION_NOT_FOUND", str(payload["reservation_id"]), status=404)
                return self._json(handler, {"ok": True, "reservation_id": payload["reservation_id"], "state": state})
            if route == "/api/v2/generation/inputs/resolve":
                kind = str(payload["kind"])
                region = payload.get("region")
                params = payload.get("params", {})
                if not isinstance(region, dict) or not isinstance(params, dict):
                    raise ApiError("INPUT_RESOLVE_INVALID", "region e params precisam ser objetos", status=400)
                return self._json(handler, self.inputs.resolve(kind, region, params))
            if route == "/api/v2/render/jobs":
                self.schemas.validate("render-job.schema.json", payload)
                return self._json(handler, self.jobs.create("render.sequence", payload, 0), status=202)
            if route.endswith("/cancel") and route.startswith("/api/v2/render/jobs/"):
                job_id = route.split("/")[-2]
                job = self.jobs.store.get(job_id)
                if not job["kind"].startswith("render."):
                    raise ApiError("RENDER_JOB_NOT_FOUND", job_id, status=404)
                return self._json(handler, self.jobs.cancel(job_id, str(payload.get("reason", "cancelled_by_user"))))
            if route == "/api/v2/generation/jobs":
                kind = str(payload["kind"])
                request = payload.get("request", {})
                if not isinstance(request, dict):
                    raise ApiError("JOB_REQUEST_INVALID", "request precisa ser objeto", status=400)
                self.contracts.validate_job(kind, request)
                job = self.jobs.create(kind, request,
                                       int(payload.get("generation_epoch", 0)))
                return self._json(handler, job, status=202)
            if route.endswith("/cancel") and route.startswith("/api/v2/generation/jobs/"):
                job_id = route.split("/")[-2]
                return self._json(handler, self.jobs.cancel(job_id, str(payload.get("reason", "cancelled_by_user"))))
            if route == "/api/v2/ai/tools":
                return self._json(handler, self.ai.request(str(payload["task"]), payload.get("input", {}),
                                                           model_id=payload.get("model_id"),
                                                           timeout_seconds=payload.get("timeout_seconds")))
            if route == "/api/v2/sources/import":
                # Por segurança, só importa de data/imports/inbox. O usuário ou
                # instalador materializa o pacote ali antes de solicitar a API.
                relative = str(payload["directory"]).strip("/\\")
                inbox = (self.root / "data" / "imports" / "inbox").resolve()
                source = (inbox / relative).resolve()
                try: source.relative_to(inbox)
                except ValueError as error:
                    raise ApiError("IMPORT_PATH_ESCAPE", relative, status=403) from error
                return self._json(handler, self.sources.import_directory(source), status=201)
            if route == "/api/v2/geocoder/import":
                source_package = str(payload["source_package"])
                count = self.geocoder.import_records(payload.get("records", []), source_package)
                return self._json(handler, {"ok": True, "imported": count}, status=201)
            if route == "/api/v2/panoramas/verify":
                return self._json(handler, self.panoramas.get(str(payload["sequence_id"]), verify_images=True))
            if route == "/api/v2/sheets/compose":
                return self._json(handler, self.jobs.create("sheets.compose", payload,
                                                            int(payload.get("generation_epoch", 0))), status=202)
            if route.startswith("/api/v2/plugins/") and route.endswith("/validate"):
                plugin_id = urllib.parse.unquote(route.split("/")[-2])
                return self._json(handler, {"ok": True, "manifest": self.plugins.get(plugin_id)})
            raise ApiError("ROUTE_NOT_FOUND", route, status=404)
        except BaseException as error:
            self._error(handler, error)
            return True

    def _floorplanner_sse(self, handler, project_id: str, after: int = 0) -> bool:
        self.floorplanner.get_project(project_id, include_scene=False)
        handler.send_response(200)
        handler.send_header("Content-Type", "text/event-stream; charset=utf-8")
        handler.send_header("Cache-Control", "no-cache")
        handler.send_header("Connection", "keep-alive")
        handler.end_headers()
        last_heartbeat = 0.0
        try:
            while True:
                events = self.floorplanner.events_after(project_id, after=after, limit=200)
                for event in events:
                    after = int(event["seq"])
                    raw = json.dumps(event, ensure_ascii=False, separators=(",", ":"))
                    handler.wfile.write(f"id: {after}\ndata: {raw}\n\n".encode("utf-8"))
                if events:
                    handler.wfile.flush()
                now = time.monotonic()
                if now - last_heartbeat >= 10:
                    handler.wfile.write(b": heartbeat\n\n"); handler.wfile.flush(); last_heartbeat = now
                time.sleep(0.25)
        except (BrokenPipeError, ConnectionResetError):
            return True

    @staticmethod
    def _decode_json(body: bytes) -> dict[str, Any]:
        try:
            value = json.loads(body.decode("utf-8")) if body else {}
        except Exception as error:
            raise ApiError("JSON_INVALID", str(error), status=400) from error
        if not isinstance(value, dict):
            raise ApiError("JSON_OBJECT_REQUIRED", "Corpo precisa ser um objeto JSON", status=400)
        return value

    @staticmethod
    def _file(handler, path: Path, mime: str, size: int) -> bool:
        handler.send_response(200)
        handler.send_header("Content-Type", mime)
        handler.send_header("Content-Length", str(size))
        handler.send_header("Cache-Control", "private, max-age=31536000, immutable")
        handler.send_header("X-Content-Type-Options", "nosniff")
        handler.send_header("Content-Disposition", "inline")
        handler.end_headers()
        with path.open("rb") as stream:
            while True:
                chunk = stream.read(1024 * 1024)
                if not chunk:
                    break
                handler.wfile.write(chunk)
        return True

    @staticmethod
    def _json(handler, value: Any, status: int = 200) -> bool:
        raw = json.dumps(value, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode("utf-8")
        handler.send_response(status)
        handler.send_header("Content-Type", "application/json; charset=utf-8")
        handler.send_header("Content-Length", str(len(raw)))
        handler.send_header("Cache-Control", "no-store")
        handler.end_headers()
        handler.wfile.write(raw)
        return True

    def _error(self, handler, error: BaseException) -> None:
        api = as_api_error(error)
        self._json(handler, api.payload(), status=api.status)

    def _sse(self, handler, job_id: str, params: dict[str, list[str]]) -> bool:
        self.jobs.store.get(job_id)
        after = int(handler.headers.get("Last-Event-ID") or params.get("after", [0])[0] or 0)
        handler.send_response(200)
        handler.send_header("Content-Type", "text/event-stream; charset=utf-8")
        handler.send_header("Cache-Control", "no-cache")
        handler.send_header("Connection", "keep-alive")
        handler.end_headers()
        last_heartbeat = 0.0
        try:
            while True:
                events = self.jobs.store.events_after(job_id, after, limit=200)
                for event in events:
                    after = event["seq"]
                    raw = json.dumps(event, ensure_ascii=False, separators=(",", ":"))
                    handler.wfile.write(f"id: {after}\ndata: {raw}\n\n".encode("utf-8"))
                if events:
                    handler.wfile.flush()
                job = self.jobs.store.get(job_id)
                if job["status"] in TERMINAL_STATUSES and not self.jobs.store.events_after(job_id, after, limit=1):
                    return True
                now = time.monotonic()
                if now - last_heartbeat >= 10:
                    handler.wfile.write(b": heartbeat\n\n")
                    handler.wfile.flush()
                    last_heartbeat = now
                time.sleep(0.25)
        except (BrokenPipeError, ConnectionResetError):
            return True
