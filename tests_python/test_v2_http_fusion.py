"""HTTP end-to-end tests for the V6 ARCZ/Aedifex fusion API.

These tests use the real ``servidor.Handler`` and ``V2Router`` over a loopback
``ThreadingHTTPServer``. All persistent data is rooted in a temporary directory;
no provider, remote API or fake success path is used.
"""
from __future__ import annotations

from io import BytesIO
import http.server
import json
from pathlib import Path
import shutil
import threading
import urllib.error
import urllib.parse
import urllib.request

from PIL import Image
import pytest

import servidor
from arcz_server.v2_router import V2Router

ROOT = Path(__file__).resolve().parents[1]


def _active_region() -> dict:
    return {
        "request": {
            "schema_version": 1,
            "region_id": "http-region-v6",
            "bbox_wgs84": [-48.501, -27.151, -48.499, -27.149],
            "polygon_wgs84": [
                [-48.5005, -27.1505, 10.0],
                [-48.4995, -27.1505, 10.0],
                [-48.4995, -27.1495, 10.0],
                [-48.5005, -27.1495, 10.0],
            ],
            "focus": {"lat": -27.15, "lon": -48.5},
            "scale": "lote",
            "requested_radius_m": 100,
            "sources": {"osm": True, "overture": False, "dem": True, "imagery": False, "street": False},
            "generation_epoch": 3,
        },
        "context": {
            "schema_version": 1,
            "region_id": "http-region-v6",
            "crs_work": "ENU_LOCAL",
            "origin_wgs84": [-48.5, -27.15, 10.0],
            "terrain": {"min_m": 8.0, "max_m": 16.0, "mean_slope_deg": 5.0},
            "urban": {"density": "medium", "block_pattern": "irregular"},
            "environment": {"biome": "atlantic_forest_coastal", "climate_profile": "humid_subtropical"},
            "evidence": [],
            "warnings": [],
            "source_packages": [],
        },
        "generation_epoch": 3,
    }


def _scene(version: int = 1) -> dict:
    node_id = f"level-http-{version}"
    return {
        "nodes": {node_id: {"id": node_id, "type": "level", "name": f"Nível {version}"}},
        "rootNodeIds": [node_id],
        "sceneVersion": version,
        "metadata": {"source": "http_e2e"},
    }




def _minimal_glb() -> bytes:
    document = json.dumps({"asset": {"version": "2.0"}, "scene": 0, "scenes": [{}], "nodes": []}, separators=(",", ":")).encode("utf-8")
    document += b" " * ((4 - len(document) % 4) % 4)
    chunk = len(document).to_bytes(4, "little") + (0x4E4F534A).to_bytes(4, "little") + document
    total = 12 + len(chunk)
    return b"glTF" + (2).to_bytes(4, "little") + total.to_bytes(4, "little") + chunk

def _png() -> bytes:
    output = BytesIO()
    Image.new("RGB", (8, 6), (24, 48, 72)).save(output, "PNG")
    return output.getvalue()


class Api:
    def __init__(self, port: int, router: V2Router | None = None):
        self.origin = f"http://127.0.0.1:{port}"
        self.router = router

    def request(self, method: str, route: str, payload=None, headers=None, raw: bytes | None = None):
        body = raw
        request_headers = dict(headers or {})
        if payload is not None:
            body = json.dumps(payload).encode("utf-8")
            request_headers.setdefault("Content-Type", "application/json")
        request = urllib.request.Request(self.origin + route, data=body, headers=request_headers, method=method)
        try:
            with urllib.request.urlopen(request, timeout=15) as response:
                data = response.read()
                return response.status, json.loads(data.decode("utf-8"))
        except urllib.error.HTTPError as error:
            data = error.read()
            return error.code, json.loads(data.decode("utf-8"))

    def get(self, route: str):
        return self.request("GET", route)

    def post(self, route: str, payload: dict):
        return self.request("POST", route, payload=payload)

    def raw_get(self, route: str):
        request = urllib.request.Request(self.origin + route, method="GET")
        try:
            with urllib.request.urlopen(request, timeout=15) as response:
                return response.status, response.headers, response.read()
        except urllib.error.HTTPError as error:
            return error.code, error.headers, error.read()


@pytest.fixture(scope="module")
def api(tmp_path_factory):
    root = tmp_path_factory.mktemp("arcz_v6_http")
    shutil.copytree(ROOT / "schemas", root / "schemas")
    shutil.copytree(ROOT / "resources", root / "resources")
    (root / "integrations" / "aedifex").mkdir(parents=True)
    shutil.copy2(ROOT / "integrations" / "aedifex" / "UPSTREAM_LOCK.json", root / "integrations" / "aedifex" / "UPSTREAM_LOCK.json")
    (root / "TASKS.json").write_text(json.dumps({"schema_version": 1, "tasks": []}), encoding="utf-8")
    (root / "IMPLEMENTATION_STATUS.json").write_text(json.dumps({"schema_version": 1, "modules": []}), encoding="utf-8")
    for name in ("LEIA-PRIMEIRO.md", "AGENTS.md", "ROADMAP.md", "CHANGELOG.md"):
        (root / name).write_text("# V6 HTTP test\n", encoding="utf-8")

    previous = servidor.V2_API
    router = V2Router(root)
    servidor.V2_API = router
    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), servidor.Handler)
    thread = threading.Thread(target=httpd.serve_forever, daemon=True)
    thread.start()
    try:
        yield Api(httpd.server_address[1], router)
    finally:
        httpd.shutdown()
        httpd.server_close()
        router.jobs.stop()
        servidor.V2_API = previous
        thread.join(timeout=5)


def _create_context(api: Api) -> dict:
    status, context = api.post("/api/v2/floorplanner/context", {
        "active_region": _active_region(),
        "north_rotation_deg": 12.0,
        "vertical_offset_m": 0.75,
        "regional_profiles": ["br.sc.coastal.midrise.v1"],
        "constraints": {"max_floors": 8},
        "reference_media": [],
    })
    assert status == 201, context
    return context


def _create_project(api: Api, project_id: str) -> tuple[dict, dict]:
    context = _create_context(api)
    status, project = api.post("/api/v2/floorplanner/projects", {
        "id": project_id,
        "name": "Projeto HTTP V6",
        "context": context,
    })
    assert status == 201, project
    status, revision = api.post(f"/api/v2/floorplanner/projects/{urllib.parse.quote(project_id)}/revisions", {
        "scene": _scene(),
        "expected_revision": 0,
        "origin": "editor",
    })
    assert status == 201, revision
    return project, revision


def test_health_governance_and_aedifex_blocker_are_real(api: Api) -> None:
    status, health = api.get("/api/v2/health")
    assert status == 200
    assert health["ok"] is True
    assert health["network_mode"] == "offline_strict"

    status, governance = api.get("/api/governance/snapshot")
    assert status == 200
    assert governance["state"] in {"READY", "DEGRADED", "EMPTY"}

    status, aedifex = api.get("/api/v2/aedifex/status")
    assert status == 200
    assert aedifex["ready"] is False
    assert any(item["code"] == "AEDIFEX_UPSTREAM_MISSING" for item in aedifex["blockers"])


def test_floorplanner_context_project_revision_and_events_roundtrip(api: Api) -> None:
    _create_project(api, "fp-http-roundtrip")
    status, loaded = api.get("/api/v2/floorplanner/projects/fp-http-roundtrip?include_scene=1")
    assert status == 200
    assert loaded["current_revision"] == 1
    assert loaded["scene_revision"]["scene"] == _scene()

    status, events = api.get("/api/v2/floorplanner/projects/fp-http-roundtrip/events")
    assert status == 200
    assert [event["event_type"] for event in events] == ["project.created", "scene.committed"]

    status, conflict = api.post("/api/v2/floorplanner/projects/fp-http-roundtrip/revisions", {
        "scene": _scene(2),
        "expected_revision": 0,
        "origin": "editor",
    })
    assert status == 409
    assert conflict["error"]["code"] == "FLOORPLANNER_VERSION_CONFLICT"


def test_reference_media_binary_upload_and_prompt_catalog(api: Api) -> None:
    status, media = api.request(
        "POST",
        "/api/v2/reference-media/upload",
        raw=_png(),
        headers={
            "Content-Type": "image/png",
            "X-ARCZ-Filename": urllib.parse.quote("fachada referência.png"),
            "X-ARCZ-Roles": urllib.parse.quote(json.dumps(["facade", "style"])),
        },
    )
    assert status == 201, media
    assert media["original_name"] == "fachada referência.png"
    assert media["width"] == 8 and media["height"] == 6
    assert len(media["content_hash"]) == 64

    status, prompts = api.get("/api/v2/prompts?category=render")
    assert status == 200
    assert len(prompts) >= 3
    assert all(item["category"] == "render" for item in prompts)


def test_chat_and_prompt_enhancement_fail_honestly_without_local_model(api: Api) -> None:
    status, session = api.post("/api/v2/chat/sessions", {
        "id": "chat-http-v6",
        "title": "HTTP local",
        "scope": "global",
        "context": {"region_id": "http-region-v6"},
    })
    assert status == 201, session
    status, message = api.post("/api/v2/chat/sessions/chat-http-v6/messages", {
        "role": "user",
        "content": "Modele uma casa no lote selecionado.",
        "attachments": [],
    })
    assert status == 201, message

    status, response = api.post("/api/v2/chat/sessions/chat-http-v6/respond", {
        "content": "Continue usando o contexto real.",
    })
    assert status == 503
    assert response["error"]["code"] == "MODEL_NOT_INSTALLED"

    status, enhancement = api.post("/api/v2/prompts/enhance", {
        "text": "fachada moderna",
        "language": "pt-BR",
    })
    assert status == 503
    assert enhancement["error"]["code"] == "MODEL_NOT_INSTALLED"


def test_photoreal_preflight_reports_real_missing_worker_and_model(api: Api) -> None:
    _create_project(api, "fp-http-render")
    status, preflight = api.post("/api/v2/photoreal/preflight", {
        "schema_version": 1,
        "floorplanner_project_id": "fp-http-render",
        "revision": 1,
        "camera": {
            "position": [12, 8, 12],
            "target": [0, 2, 0],
            "focal_length_mm": 35,
            "aperture": 5.6,
            "focus_distance_m": 15,
        },
        "resolution": {"width": 7680, "height": 3291},
        "format": "png",
        "passes": ["beauty", "depth", "normals", "object_ids", "semantic_masks", "material_masks", "sky_mask"],
        "reference_media": [],
        "enhancement": {
            "mode": "full_photoreal",
            "model_id": None,
            "prompt": "cinematic architecture",
            "negative_prompt": "deformed geometry",
            "seed": 42,
            "geometry_guard_px": 2,
        },
        "output_name": "http-render",
    })
    assert status == 200, preflight
    assert preflight["ready"] is False
    codes = {item["code"] for item in preflight["blockers"]}
    assert {"MODEL_NOT_INSTALLED", "BLENDER_NOT_INSTALLED"}.issubset(codes)
    # O worker está registrado por manifesto, mas não se declara pronto sem o
    # executável Blender e os scripts locais efetivamente presentes.
    assert "PHOTOREAL_WORKER_NOT_INSTALLED" not in codes


def test_floorplanner_binary_export_is_validated_materialized_and_catalogued(api: Api) -> None:
    _, revision = _create_project(api, "fp-http-export-v9")
    manifest = {
        "schema_version": 1,
        "source": "aedifex_rendered_scene",
        "scene_hash": revision["scene_hash"],
        "geo_anchor": {
            "origin_wgs84": [-48.5, -27.15, 10.0],
            "north_rotation_deg": 12.0,
            "vertical_offset_m": 0.75,
            "axis_policy": "AEDIFEX_X_EAST_Y_UP_Z_SOUTH",
        },
    }
    headers = {
        "Content-Type": "model/gltf-binary",
        "X-ARCZ-Revision": "1",
        "X-ARCZ-Format": "glb",
        "X-ARCZ-Scene-Hash": revision["scene_hash"],
        "X-ARCZ-Semantic-Manifest": urllib.parse.quote(json.dumps(manifest, separators=(",", ":"))),
    }
    route = "/api/v2/floorplanner/projects/fp-http-export-v9/exports/upload"
    status, exported = api.request("POST", route, headers=headers, raw=_minimal_glb())
    assert status == 201, exported
    assert exported["path"].startswith("data/floorplanner/exports/")
    assert exported["url"] == "/" + exported["path"]
    assert len(exported["sha256"]) == 64
    assert exported["semantic_manifest"]["source"] == "aedifex_rendered_scene"

    status, duplicate = api.request("POST", route, headers=headers, raw=_minimal_glb())
    assert status == 201, duplicate
    assert duplicate["id"] == exported["id"]
    assert duplicate["deduplicated"] is True

    status, loaded = api.get("/api/v2/floorplanner/projects/fp-http-export-v9?include_scene=0")
    assert status == 200
    assert loaded["exports"][0]["sha256"] == exported["sha256"]

    bad_headers = dict(headers)
    bad_headers["X-ARCZ-Scene-Hash"] = "0" * 64
    status, mismatch = api.request("POST", route, headers=bad_headers, raw=_minimal_glb())
    assert status == 409
    assert mismatch["error"]["code"] == "FLOORPLANNER_EXPORT_SCENE_HASH_MISMATCH"

    status, invalid = api.request("POST", route, headers=headers, raw=b"not-a-glb")
    assert status == 400
    assert invalid["error"]["code"] == "FLOORPLANNER_EXPORT_GLB_INVALID"


def test_content_endpoints_stream_real_bytes_and_version_prompt_crud(api: Api) -> None:
    raw = _png()
    status, media = api.request(
        "POST",
        "/api/v2/reference-media/upload",
        raw=raw,
        headers={
            "Content-Type": "image/png",
            "X-ARCZ-Filename": urllib.parse.quote("material pedra.png"),
            "X-ARCZ-Roles": urllib.parse.quote(json.dumps(["material", "style"])),
        },
    )
    assert status == 201, media
    status, headers, streamed = api.raw_get(
        f"/api/v2/reference-media/{urllib.parse.quote(media['id'])}/content"
    )
    assert status == 200
    assert headers.get_content_type() == "image/png"
    assert int(headers["Content-Length"]) == len(raw)
    assert streamed == raw

    status, updated_media = api.post(
        f"/api/v2/reference-media/{urllib.parse.quote(media['id'])}/metadata",
        {
            "roles": ["material", "lighting"],
            "license": media["license"],
            "metadata": {"weight": 1.2, "notes": "preservar textura"},
        },
    )
    assert status == 200
    assert updated_media["roles"] == ["material", "lighting"]
    assert updated_media["integrity"]["ok"] is True

    slug = "http.user.prompt.v10"
    status, prompt = api.post("/api/v2/prompts", {
        "slug": slug,
        "title": "Prompt V10",
        "category": "render",
        "purpose": "photoreal",
        "language": "pt-BR",
        "template": "fachada {{style}}",
        "negative_template": "deformada",
        "tags": ["http", "v10"],
        "variables": {"style": {"required": True}},
    })
    assert status == 201, prompt
    status, version2 = api.post("/api/v2/prompts", {**prompt, "template": "fachada {{style}} em 8K"})
    assert status == 201 and version2["version"] == 2
    status, versions = api.get(f"/api/v2/prompts/{urllib.parse.quote(prompt['id'])}/versions")
    assert status == 200 and [item["version"] for item in versions] == [2, 1]
    status, duplicate = api.post(
        f"/api/v2/prompts/{urllib.parse.quote(prompt['id'])}/duplicate",
        {"slug": f"{slug}.copy", "language": "en-US"},
    )
    assert status == 201 and duplicate["builtin"] is False
    status, archived = api.post(
        f"/api/v2/prompts/{urllib.parse.quote(duplicate['id'])}/archive", {}
    )
    assert status == 200 and archived["active"] is False


def test_chat_tool_run_detail_and_reject_routes_use_correct_path_segments(api: Api) -> None:
    assert api.router is not None
    status, session = api.post("/api/v2/chat/sessions", {
        "id": "chat-tool-route-v10",
        "title": "Tool route",
        "scope": "global",
        "context": {"floorplanner_project_id": "project-route", "expected_revision": 2},
    })
    assert status == 201, session
    assistant = api.router.chat.append_message(session["id"], {
        "role": "assistant",
        "content": "Ação proposta.",
        "tool_calls": [],
        "metadata": {},
    })
    call = {"id": "call-route-v10", "name": "aedifex.apply_patch", "arguments": {"patches": []}}
    descriptor = {"name": call["name"], "side_effect": "mutate", "requires_approval": True}
    run = api.router.chat._create_tool_run(
        session["id"], assistant["id"], call, descriptor,
        {"project_id": "project-route", "expected_revision": 2},
    )
    run = api.router.chat._transition_tool_run(
        run["id"], "AWAITING_APPROVAL", preview={"changed": True, "diff": []},
        expected_from={"PROPOSED"},
    )

    status, loaded = api.get(f"/api/v2/chat/tool-runs/{urllib.parse.quote(run['id'])}")
    assert status == 200 and loaded["status"] == "AWAITING_APPROVAL"
    status, rejected = api.post(
        f"/api/v2/chat/tool-runs/{urllib.parse.quote(run['id'])}/reject",
        {"reason": "revisão do usuário"},
    )
    assert status == 200 and rejected["tool_run"]["status"] == "REJECTED"
