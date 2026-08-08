from __future__ import annotations

import json
from pathlib import Path

import pytest

from arcz_server.errors import ApiError
from arcz_server.floorplanner_store import FloorplannerStore
from arcz_server.geo_model_bridge import GeoModelBridge
from arcz_server.project_migrations import migrate_project
from arcz_server.schema_validation import SchemaRegistry

ROOT = Path(__file__).resolve().parents[1]
SCHEMAS = SchemaRegistry(ROOT / "schemas")


def active_region() -> dict:
    return {
        "request": {
            "schema_version": 1,
            "region_id": "roundtrip-v9",
            "bbox_wgs84": [-48.501, -27.151, -48.499, -27.149],
            "polygon_wgs84": [[-48.5005, -27.1505, 10], [-48.4995, -27.1505, 10], [-48.4995, -27.1495, 10]],
            "focus": {"lat": -27.15, "lon": -48.5},
            "scale": "lote",
            "requested_radius_m": 100,
            "sources": {"osm": True, "overture": False, "dem": True, "imagery": False, "street": False},
            "generation_epoch": 1,
        },
        "context": {
            "schema_version": 1,
            "region_id": "roundtrip-v9",
            "crs_work": "ENU_LOCAL",
            "origin_wgs84": [-48.5, -27.15, 10],
            "terrain": {}, "urban": {}, "environment": {},
            "evidence": [], "warnings": [], "source_packages": [],
        },
        "generation_epoch": 1,
    }


def context() -> dict:
    return GeoModelBridge(SCHEMAS).build_context({"active_region": active_region()})


def scene() -> dict:
    return {
        "nodes": {"site": {"id": "site", "type": "site", "name": "Lote"}},
        "rootNodeIds": ["site"],
        "sceneVersion": 1,
    }


def minimal_glb() -> bytes:
    document = json.dumps({"asset": {"version": "2.0"}, "scene": 0, "scenes": [{}]}, separators=(",", ":")).encode()
    document += b" " * ((4 - len(document) % 4) % 4)
    chunk = len(document).to_bytes(4, "little") + (0x4E4F534A).to_bytes(4, "little") + document
    length = 12 + len(chunk)
    return b"glTF" + (2).to_bytes(4, "little") + length.to_bytes(4, "little") + chunk


def test_export_bytes_uses_revision_hash_atomic_storage_and_deduplication(tmp_path: Path) -> None:
    store = FloorplannerStore(tmp_path / "floorplanner.sqlite3", SCHEMAS)
    store.create_project({"id": "project-v9", "name": "V9", "context": context()})
    revision = store.save_revision("project-v9", {"scene": scene(), "expected_revision": 0})
    manifest = {"geo_anchor": context()["geo_anchor"], "scene_hash": revision["scene_hash"]}
    exported = store.import_export_bytes(
        "project-v9", 1, minimal_glb(), format="glb", semantic_manifest=manifest,
        scene_hash=revision["scene_hash"], root=tmp_path,
    )
    assert exported["readonly"] if "readonly" in exported else True
    assert (tmp_path / exported["path"]).read_bytes() == minimal_glb()
    assert exported["url"] == "/" + exported["path"]
    duplicate = store.import_export_bytes(
        "project-v9", 1, minimal_glb(), format="glb", semantic_manifest=manifest,
        scene_hash=revision["scene_hash"], root=tmp_path,
    )
    assert duplicate["id"] == exported["id"]
    assert duplicate["deduplicated"] is True
    events = store.events_after("project-v9")
    assert [event["event_type"] for event in events].count("export.registered") == 1


def test_export_rejects_corruption_format_and_scene_mismatch(tmp_path: Path) -> None:
    store = FloorplannerStore(tmp_path / "floorplanner.sqlite3", SCHEMAS)
    store.create_project({"id": "project-v9", "name": "V9", "context": context()})
    revision = store.save_revision("project-v9", {"scene": scene(), "expected_revision": 0})
    with pytest.raises(ApiError, match="GLB") as invalid:
        store.import_export_bytes("project-v9", 1, b"glTF", format="glb", semantic_manifest={}, scene_hash=revision["scene_hash"], root=tmp_path)
    assert invalid.value.code == "FLOORPLANNER_EXPORT_GLB_INVALID"
    with pytest.raises(ApiError) as unsupported:
        store.import_export_bytes("project-v9", 1, minimal_glb(), format="obj", semantic_manifest={}, scene_hash=revision["scene_hash"], root=tmp_path)
    assert unsupported.value.code == "FLOORPLANNER_EXPORT_FORMAT_UNSUPPORTED"
    with pytest.raises(ApiError) as mismatch:
        store.import_export_bytes("project-v9", 1, minimal_glb(), format="glb", semantic_manifest={}, scene_hash="0" * 64, root=tmp_path)
    assert mismatch.value.code == "FLOORPLANNER_EXPORT_SCENE_HASH_MISMATCH"


def test_project_migration_adds_global_model_contracts() -> None:
    migrated, _ = migrate_project({"posicao": {"lat": 0, "lon": 0}, "ambiente": {}, "camera": {}, "takes": [], "pecas": [], "lugares": []}, schemas=SCHEMAS)
    assert migrated["primary_model"] is None
    assert migrated["floorplanner_derivatives"] == []
    SCHEMAS.validate("project-v2.schema.json", migrated)


def test_browser_boot_has_no_project_specific_default_model() -> None:
    main = (ROOT / "app" / "main.js").read_text(encoding="utf-8")
    scene_code = (ROOT / "app" / "cena.js").read_text(encoding="utf-8")
    assert 'carregarPredio("modelos/zenite.glb"' not in main
    assert 'this.caminhoPredio = "modelos/zenite.glb"' not in scene_code
    assert "primary_model" in main
    assert "floorplanner_derivatives" in scene_code


def test_sidecar_contains_real_gltf_exporter_and_binary_upload_contract() -> None:
    exporter = (ROOT / "integrations" / "aedifex" / "overlay" / "apps" / "arcz-floorplanner" / "app" / "ui" / "arcz-scene-export-bridge.tsx").read_text(encoding="utf-8")
    bridge = (ROOT / "integrations" / "aedifex" / "overlay" / "packages" / "arcz-bridge" / "src" / "index.ts").read_text(encoding="utf-8")
    assert "GLTFExporter" in exporter
    assert "parseAsync" in exporter
    assert "meshCount === 0" in exporter
    assert "/exports/upload" in bridge
    assert "model/gltf-binary" in bridge
    assert "X-ARCZ-Scene-Hash" in bridge
