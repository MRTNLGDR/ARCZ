from __future__ import annotations

from io import BytesIO
import json
import math
from pathlib import Path
import shutil
from typing import Any

from PIL import Image
import pytest

from arcz_server.aedifex_registry import AedifexRegistry
from arcz_server.aedifex_runtime import AedifexRuntimeManager
from arcz_server.ai_broker import LocalAIBroker, ModelRegistry
from arcz_server.chat_workspace import ChatWorkspace
from arcz_server.errors import ApiError
from arcz_server.floorplanner_store import FloorplannerStore
from arcz_server.geo_model_bridge import GeoAnchor, GeoModelBridge, GeoModelTransform
from arcz_server.governance import GovernanceSnapshot
from arcz_server.jobs import JobManager
from arcz_server.network_policy import NetworkMode, NetworkPolicy
from arcz_server.panoramas import PanoramaRegistry
from arcz_server.photoreal import PhotorealRenderService
from arcz_server.project_migrations import migrate_project
from arcz_server.prompt_library import PromptLibrary
from arcz_server.reference_media import ReferenceMediaStore
from arcz_server.schema_validation import SchemaRegistry
from arcz_server.hashing import sha256_file


ROOT = Path(__file__).resolve().parents[1]
SCHEMAS = SchemaRegistry(ROOT / "schemas")


def api_error(code: str, fn) -> ApiError:
    with pytest.raises(ApiError) as caught:
        fn()
    assert caught.value.code == code
    return caught.value


def png_bytes(size: tuple[int, int] = (3, 2)) -> bytes:
    output = BytesIO()
    Image.new("RGB", size, (16, 32, 64)).save(output, "PNG")
    return output.getvalue()


def active_region() -> dict[str, Any]:
    return {
        "request": {
            "schema_version": 1,
            "region_id": "region-v6",
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
            "generation_epoch": 9,
        },
        "context": {
            "schema_version": 1,
            "region_id": "region-v6",
            "crs_work": "ENU_LOCAL",
            "origin_wgs84": [-48.5, -27.15, 10.0],
            "terrain": {"min_m": 8.0, "max_m": 16.0, "mean_slope_deg": 5.0},
            "urban": {"density": "medium", "block_pattern": "irregular"},
            "environment": {"biome": "atlantic_forest_coastal", "climate_profile": "humid_subtropical"},
            "evidence": [],
            "warnings": [],
            "source_packages": [],
        },
        "generation_epoch": 9,
    }


def modeling_context(*, reference_media: list[str] | None = None) -> dict[str, Any]:
    return GeoModelBridge(SCHEMAS).build_context({
        "active_region": active_region(),
        "north_rotation_deg": 23.5,
        "vertical_offset_m": 1.25,
        "regional_profiles": ["br.sc.coastal.midrise.v1"],
        "constraints": {"max_floors": 8, "setback_front_m": 4.0},
        "reference_media": reference_media or [],
    })


def scene(version: int = 1) -> dict[str, Any]:
    node_id = f"level-{version}"
    return {
        "nodes": {node_id: {"id": node_id, "type": "level", "name": f"Nível {version}"}},
        "rootNodeIds": [node_id],
        "sceneVersion": version,
        "metadata": {"fixture": True},
    }


def empty_ai(root: Path) -> tuple[ModelRegistry, LocalAIBroker]:
    registry = ModelRegistry([root / "resources" / "models", root / "data" / "models"], SCHEMAS)
    (root / "jobs").mkdir(parents=True, exist_ok=True)
    broker = LocalAIBroker(
        root,
        registry,
        SCHEMAS,
        NetworkPolicy(mode=NetworkMode.OFFLINE_STRICT),
    )
    return registry, broker


def prepare_prompt_root(root: Path) -> None:
    target = root / "resources" / "prompts"
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(ROOT / "resources" / "prompts", target)


def test_geo_transform_roundtrip_for_multiple_north_rotations() -> None:
    for rotation in (0.0, 23.5, 90.0, 180.0, -45.0):
        transform = GeoModelTransform(GeoAnchor(
            origin_wgs84=(-48.5, -27.15, 10.0),
            north_rotation_deg=rotation,
            vertical_offset_m=2.75,
        ))
        for point in ([0.0, 0.0, 0.0], [12.5, -6.25, 4.0], [-100.0, 220.0, -3.5]):
            restored = transform.aedifex_to_enu(transform.enu_to_aedifex(point))
            assert restored == pytest.approx(point, abs=1e-9)
        wgs = [-48.4998, -27.1497, 18.5]
        assert transform.aedifex_to_wgs84(transform.wgs84_to_aedifex(wgs)) == pytest.approx(wgs, abs=1e-8)


def test_geo_bridge_builds_valid_hashed_modeling_context() -> None:
    context = modeling_context()
    assert context["region_id"] == "region-v6"
    assert context["scale"] == "lote"
    assert context["geo_anchor"]["axis_policy"] == "AEDIFEX_X_EAST_Y_UP_Z_SOUTH"
    assert len(context["selection"]["parcel_polygon_aedifex_xyz_m"]) == 4
    assert len(context["context_hash"]) == 64
    SCHEMAS.validate("modeling-context-package.schema.json", context)


def test_geo_bridge_rejects_conflicting_coordinate_sources() -> None:
    api_error("PARCEL_COORDINATE_CONFLICT", lambda: GeoModelBridge(SCHEMAS).build_context({
        "active_region": active_region(),
        "selection": {
            "parcel_polygon_wgs84": [[-48.5, -27.15], [-48.499, -27.15], [-48.499, -27.149]],
            "parcel_polygon_enu_m": [[0, 0], [10, 0], [10, 10]],
        },
    }))


def test_floorplanner_revisions_are_versioned_idempotent_and_conflict_safe(tmp_path: Path) -> None:
    store = FloorplannerStore(tmp_path / "floorplanner.sqlite3", SCHEMAS)
    created = store.create_project({"id": "fp-v6", "name": "Casa V6", "context": modeling_context()})
    assert created["current_revision"] == 0
    first = store.save_revision("fp-v6", {"scene": scene(1), "expected_revision": 0, "origin": "editor"})
    assert first["changed"] is True and first["current_revision"] == 1
    same = store.save_revision("fp-v6", {"scene": scene(1), "expected_revision": 1, "origin": "editor"})
    assert same["changed"] is False and same["current_revision"] == 1
    conflict = api_error("FLOORPLANNER_VERSION_CONFLICT", lambda: store.save_revision(
        "fp-v6", {"scene": scene(2), "expected_revision": 0, "origin": "editor"}
    ))
    assert conflict.details["current_revision"] == 1
    second = store.save_revision("fp-v6", {"scene": scene(2), "expected_revision": 1, "origin": "mcp"})
    assert second["current_revision"] == 2
    events = store.events_after("fp-v6")
    assert [item["event_type"] for item in events] == ["project.created", "scene.committed", "scene.committed"]
    assert store.get_project("fp-v6", include_scene=True)["scene_revision"]["scene"] == scene(2)


def test_reference_media_browser_upload_preserves_name_hash_and_dimensions(tmp_path: Path) -> None:
    store = ReferenceMediaStore(tmp_path, SCHEMAS)
    raw = png_bytes((7, 5))
    record = store.import_bytes("fachada-referencia.png", raw)
    assert record["original_name"] == "fachada-referencia.png"
    assert record["category"] == "image"
    assert (record["width"], record["height"]) == (7, 5)
    assert len(record["content_hash"]) == 64
    assert (tmp_path / record["stored_path"]).read_bytes() == raw
    duplicate = store.import_bytes("outro-nome.png", raw)
    assert duplicate["id"] == record["id"]
    api_error("MEDIA_FILENAME_INVALID", lambda: store.import_bytes("../escape.png", raw))
    api_error("MEDIA_IMAGE_INVALID", lambda: store.import_bytes("corrupt.png", b"not-a-png"))


def test_reference_media_rejects_symlink_component(tmp_path: Path) -> None:
    store = ReferenceMediaStore(tmp_path, SCHEMAS)
    outside = tmp_path / "outside"
    outside.mkdir()
    (outside / "file.png").write_bytes(png_bytes())
    link = store.inbox / "linked"
    try:
        link.symlink_to(outside, target_is_directory=True)
    except (OSError, NotImplementedError):
        pytest.skip("symlink indisponível neste sistema")
    api_error("MEDIA_SYMLINK_DENIED", lambda: store.import_from_inbox({"path": "linked/file.png"}))


def test_prompt_library_loads_compiles_and_fails_honestly_without_model(tmp_path: Path) -> None:
    prepare_prompt_root(tmp_path)
    _, broker = empty_ai(tmp_path)
    library = PromptLibrary(tmp_path, SCHEMAS, broker)
    rows = library.list(category="render")
    assert len(rows) >= 3
    compiled = library.compile("render.archviz.exterior.photoreal", {
        "project": {"name": "Zenite"},
        "region": {"profile": "litoral sul"},
        "environment": {"time": "golden hour", "weather": "céu parcialmente nublado"},
        "camera": {"focal_length_mm": 35},
        "output": {"width": 7680, "height": 3291},
    })
    assert "Zenite" in compiled["prompt"]
    assert "7680x3291" in compiled["prompt"]
    api_error("PROMPT_VARIABLES_MISSING", lambda: library.compile("render.archviz.exterior.photoreal", {}))
    api_error("MODEL_NOT_INSTALLED", lambda: library.enhance({"text": "melhore", "language": "pt-BR"}))
    api_error("MODEL_NOT_INSTALLED", lambda: library.translate({"text": "casa", "target_language": "en"}))


def test_chat_workspace_persists_attachments_and_reports_missing_model(tmp_path: Path) -> None:
    _, broker = empty_ai(tmp_path)
    media = ReferenceMediaStore(tmp_path, SCHEMAS)
    image = media.import_bytes("ref.png", png_bytes())
    chat = ChatWorkspace(tmp_path, SCHEMAS, broker, media)
    session = chat.create_session({"id": "chat-v6", "title": "ARCZ", "scope": "floorplanner", "context": {"project": "fp"}})
    message = chat.append_message(session["id"], {"role": "user", "content": "Use a referência", "attachments": [image["content_hash"]]})
    assert message["attachments"] == [image["content_hash"]]
    assert chat.get_session(session["id"])["messages"][0]["content"] == "Use a referência"
    api_error("MODEL_NOT_INSTALLED", lambda: chat.respond(session["id"], {"content": "Modele"}, tool_catalog=[]))
    # The real user instruction is retained even when inference cannot run.
    assert [m["role"] for m in chat.get_session(session["id"])["messages"]] == ["user", "user"]


def test_photoreal_preflight_requires_real_worker_and_model(tmp_path: Path) -> None:
    models, _ = empty_ai(tmp_path)
    media = ReferenceMediaStore(tmp_path, SCHEMAS)
    floorplanner = FloorplannerStore(tmp_path / "floorplanner.sqlite3", SCHEMAS)
    floorplanner.create_project({"id": "fp-render", "name": "Render", "context": modeling_context()})
    floorplanner.save_revision("fp-render", {"scene": scene(), "expected_revision": 0, "origin": "editor"})
    jobs = JobManager(tmp_path, SCHEMAS, workers=1)
    service = PhotorealRenderService(tmp_path, SCHEMAS, models, media, floorplanner, jobs)
    request = {
        "schema_version": 1,
        "floorplanner_project_id": "fp-render",
        "revision": 1,
        "camera": {"position": [12, 8, 12], "target": [0, 2, 0], "focal_length_mm": 35, "aperture": 5.6, "focus_distance_m": 15},
        "resolution": {"width": 7680, "height": 3291},
        "format": "png",
        "passes": ["beauty", "depth", "normals", "object_ids", "semantic_masks", "material_masks", "sky_mask"],
        "reference_media": [],
        "enhancement": {"mode": "full_photoreal", "model_id": None, "prompt": "cinematic", "negative_prompt": "deformed", "seed": 1, "geometry_guard_px": 2},
        "output_name": "test-render",
    }
    blocked = service.preflight(request)
    codes = {item["code"] for item in blocked["blockers"]}
    assert {"MODEL_NOT_INSTALLED", "PHOTOREAL_WORKER_NOT_INSTALLED", "BLENDER_NOT_INSTALLED"}.issubset(codes)
    request["enhancement"]["mode"] = "none"
    jobs.register("render.photoreal", lambda context, value: {})
    still_blocked = service.preflight(request)
    assert still_blocked["ready"] is False
    assert {item["code"] for item in still_blocked["blockers"]} == {"BLENDER_NOT_INSTALLED"}
    jobs.stop()


def test_panorama_registry_exposes_only_same_origin_data_path(tmp_path: Path) -> None:
    root = tmp_path / "data" / "panoramas"
    sequence_dir = root / "bombinhas" / "seq-1"
    sequence_dir.mkdir(parents=True)
    image = sequence_dir / "0001.png"
    image.write_bytes(png_bytes())
    manifest = {
        "schema_version": 1,
        "sequence_id": "seq-1",
        "license": {"id": "LicenseRef-Test"},
        "frames": [{
            "id": "f1", "image": "0001.png", "lat": -27.15, "lon": -48.5,
            "heading": 90.0, "timestamp": "2026-08-06T12:00:00Z", "sha256": sha256_file(image), "next": [],
        }],
    }
    (sequence_dir / "sequence.json").write_text(json.dumps(manifest), encoding="utf-8")
    registry = PanoramaRegistry(root, SCHEMAS)
    listed = registry.list()[0]
    assert listed["base_url"] == "/data/panoramas/bombinhas/seq-1/"
    loaded = registry.get("seq-1", verify_images=True)
    assert loaded["frames"][0]["id"] == "f1"
    assert not loaded["base_url"].startswith("http")


def test_aedifex_registry_and_runtime_never_claim_missing_build_ready(tmp_path: Path) -> None:
    integration = tmp_path / "integrations" / "aedifex"
    integration.mkdir(parents=True)
    lock = json.loads((ROOT / "integrations" / "aedifex" / "UPSTREAM_LOCK.json").read_text(encoding="utf-8"))
    (integration / "UPSTREAM_LOCK.json").write_text(json.dumps(lock), encoding="utf-8")
    registry = AedifexRegistry(tmp_path)
    status = registry.status()
    assert status["ready"] is False
    assert {item["code"] for item in status["blockers"]}.issuperset({"AEDIFEX_UPSTREAM_MISSING", "AEDIFEX_FORK_MISSING", "AEDIFEX_BRIDGE_BUILD_MISSING"})
    runtime = AedifexRuntimeManager(tmp_path, registry)
    runtime_status = runtime.status()
    assert runtime_status["ready"] is False
    assert runtime_status["runtime"]["healthy"] is False
    api_error("AEDIFEX_RUNTIME_NOT_READY", lambda: runtime.start(wait_seconds=0.5))


def test_project_v2_migration_includes_fusion_state_and_relative_schema_refs() -> None:
    migrated, _ = migrate_project({
        "posicao": {"lat": -27.15, "lon": -48.5},
        "ambiente": {}, "camera": {}, "takes": [], "pecas": [], "lugares": [],
        "floorplanner_projects": [{"id": "fp", "region_id": "r", "name": "P", "current_revision": 2}],
    }, schemas=SCHEMAS)
    assert migrated["workspace_mode"] == "globo"
    assert migrated["active_floorplanner_project_id"] is None
    assert migrated["panel_layout"] == {"schema_version": 1, "panels": {}}
    assert migrated["earth_presentation"]["clouds"] is True
    assert migrated["floorplanner_projects"][0]["current_revision"] == 2
    SCHEMAS.validate("project-v2.schema.json", migrated)


def test_governance_snapshot_is_derived_from_real_handoff_files(tmp_path: Path) -> None:
    (tmp_path / "TASKS.json").write_text(json.dumps({"tasks": [
        {"id": "T1", "module": "fusion", "title": "Done", "state": "DONE"},
        {"id": "T2", "module": "fusion", "title": "Blocked", "state": "BLOCKED"},
    ]}), encoding="utf-8")
    (tmp_path / "IMPLEMENTATION_STATUS.json").write_text(json.dumps({"modules": [
        {"id": "fusion", "name": "Fusion", "status": "BLOCKED", "limitations": ["toolchain"]},
    ]}), encoding="utf-8")
    for name in ("LEIA-PRIMEIRO.md", "AGENTS.md", "ROADMAP.md", "CHANGELOG.md"):
        (tmp_path / name).write_text("## V6\n", encoding="utf-8")
    snapshot = GovernanceSnapshot(tmp_path).build()
    assert snapshot["state"] == "DEGRADED"
    assert snapshot["summary"]["doneTasks"] == 1
    assert snapshot["summary"]["pendingTasks"] == 1
    assert snapshot["alerts"][0]["kind"] == "BLOCKED"


def _minimal_scene_glb() -> bytes:
    document = json.dumps({"asset": {"version": "2.0"}, "scene": 0, "scenes": [{}]}, separators=(",", ":")).encode()
    document += b" " * ((4 - len(document) % 4) % 4)
    chunk = len(document).to_bytes(4, "little") + (0x4E4F534A).to_bytes(4, "little") + document
    length = 12 + len(chunk)
    return b"glTF" + (2).to_bytes(4, "little") + length.to_bytes(4, "little") + chunk


def _photoreal_request(project_id: str, quality: str) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "floorplanner_project_id": project_id,
        "revision": 1,
        "quality": quality,
        "engine": "cycles",
        "camera": {
            "position": [12, 8, 12], "target": [0, 2, 0],
            "focal_length_mm": 35, "aperture": 5.6, "focus_distance_m": 15,
        },
        "resolution": {"width": 3840, "height": 2160},
        "format": "png",
        "passes": ["beauty", "depth", "normals", "object_ids", "semantic_masks"],
        "reference_media": [],
        "enhancement": {
            "mode": "none", "model_id": None, "prompt": "", "negative_prompt": "",
            "seed": 1, "geometry_guard_px": 2,
        },
        "render_settings": {"samples": 128, "denoise": True, "device": "auto"},
        "output_name": f"{project_id}-{quality}",
    }


def test_final_photoreal_requires_real_aedifex_glb_but_preview_fallback_is_explicit(tmp_path: Path) -> None:
    models, _ = empty_ai(tmp_path)
    media = ReferenceMediaStore(tmp_path, SCHEMAS)
    floorplanner = FloorplannerStore(tmp_path / "floorplanner.sqlite3", SCHEMAS)
    floorplanner.create_project({"id": "fp-final", "name": "Final", "context": modeling_context()})
    revision = floorplanner.save_revision("fp-final", {
        "scene": scene(), "expected_revision": 0, "origin": "editor",
    })
    jobs = JobManager(tmp_path, SCHEMAS, workers=1)
    jobs.register("render.photoreal", lambda context, value: {"ok": True})
    service = PhotorealRenderService(tmp_path, SCHEMAS, models, media, floorplanner, jobs)
    service._blender_status = lambda: {
        "installed": True, "executable": "/local/blender", "launcher": "worker",
        "render_script": "worker", "launcher_exists": True, "render_script_exists": True,
    }

    high = service.preflight(_photoreal_request("fp-final", "high"))
    assert high["ready"] is False
    assert "AEDIFEX_GLB_EXPORT_MISSING" in {item["code"] for item in high["blockers"]}

    preview = service.preflight(_photoreal_request("fp-final", "preview"))
    assert preview["ready"] is True
    assert "AEDIFEX_GLB_EXPORT_MISSING" in {item["code"] for item in preview["warnings"]}

    exported = floorplanner.import_export_bytes(
        "fp-final", 1, _minimal_scene_glb(), format="glb",
        semantic_manifest={"scene_hash": revision["scene_hash"], "geo_anchor": modeling_context()["geo_anchor"]},
        scene_hash=revision["scene_hash"], root=tmp_path,
    )
    final = service.preflight(_photoreal_request("fp-final", "ultra"))
    assert final["ready"] is True
    assert final["scene"]["glb_export"]["id"] == exported["id"]
    assert final["scene"]["glb_export"]["sha256"] == exported["sha256"]
    jobs.stop()
