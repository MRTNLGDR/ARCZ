from __future__ import annotations

import json
from pathlib import Path

import pytest

from arcz_server.errors import ApiError
from arcz_server.geo_model_bridge import GeoModelBridge
from arcz_server.hashing import sha256_file
from arcz_server.schema_validation import SchemaRegistry

ROOT = Path(__file__).resolve().parents[1]
SCHEMAS = SchemaRegistry(ROOT / "schemas")


def active_region() -> dict:
    return {
        "request": {
            "schema_version": 1,
            "region_id": "region-context-v10",
            "bbox_wgs84": [-48.501, -27.151, -48.499, -27.149],
            "polygon_wgs84": [
                [-48.5005, -27.1505, 10.0],
                [-48.4995, -27.1505, 10.0],
                [-48.4995, -27.1495, 10.0],
            ],
            "focus": {"lat": -27.15, "lon": -48.5},
            "scale": "lote",
            "requested_radius_m": 100,
            "sources": {"osm": True, "overture": False, "dem": True, "imagery": False, "street": False},
            "generation_epoch": 12,
        },
        "context": {
            "schema_version": 1,
            "region_id": "region-context-v10",
            "crs_work": "ENU_LOCAL",
            "origin_wgs84": [-48.5, -27.15, 10.0],
            "terrain": {"min_m": 8.0, "max_m": 16.0, "mean_slope_deg": 5.0},
            "urban": {"density": "medium", "block_pattern": "irregular"},
            "environment": {"biome": "atlantic_forest_coastal", "climate_profile": "humid_subtropical"},
            "evidence": [],
            "warnings": [],
            "source_packages": [],
        },
        "generation_epoch": 12,
    }


def payload(layer: dict) -> dict:
    return {
        "active_region": active_region(),
        "north_rotation_deg": 31.0,
        "vertical_offset_m": 2.0,
        "context_layers": [layer],
    }


def test_context_layer_is_local_hashed_readonly_and_schema_valid(tmp_path: Path) -> None:
    path = tmp_path / "data" / "context" / "roads.geojson"
    path.parent.mkdir(parents=True)
    path.write_text(json.dumps({
        "type": "FeatureCollection",
        "features": [{
            "type": "Feature",
            "id": "road-1",
            "geometry": {"type": "LineString", "coordinates": [[0, 0, 0], [20, 5, 0]]},
            "properties": {},
        }],
    }), encoding="utf-8")
    digest = sha256_file(path)
    context = GeoModelBridge(SCHEMAS, tmp_path).build_context(payload({
        "id": "roads:verified",
        "role": "roads",
        "format": "geojson",
        "asset_path": "/data/context/roads.geojson",
        "sha256": digest,
        "coordinate_space": "ENU_LOCAL",
        "transform": {
            "position_m": [10, 20, 0],
            "rotation_euler_rad": [0, 0, 0],
            "scale": [1, 1, 1],
        },
        "visible": True,
        "opacity": 0.8,
        "provenance": {"source": "fixture", "license": "LicenseRef-Test"},
    }))
    layer = context["context_layers"][0]
    assert layer["readonly"] is True
    assert layer["sha256"] == digest
    assert layer["asset_path"] == "/data/context/roads.geojson"
    assert layer["coordinate_space"] == "ENU_LOCAL"
    assert context["context_hash"]
    SCHEMAS.validate("modeling-context-package.schema.json", context)


def test_context_layer_rejects_missing_file_hash_mismatch_and_remote_path(tmp_path: Path) -> None:
    bridge = GeoModelBridge(SCHEMAS, tmp_path)
    base = {
        "id": "terrain:test",
        "role": "terrain",
        "format": "glb",
        "coordinate_space": "AEDIFEX_LOCAL",
        "visible": True,
        "opacity": 1,
        "transform": {"position_m": [0, 0, 0], "rotation_euler_rad": [0, 0, 0], "scale": [1, 1, 1]},
        "provenance": {},
    }
    cases = [
        ({**base, "asset_path": "/data/context/missing.glb", "sha256": "a" * 64}, "CONTEXT_LAYER_ASSET_MISSING"),
        ({**base, "asset_path": "https://provider.invalid/terrain.glb", "sha256": "a" * 64}, "CONTEXT_LAYER_PATH_INVALID"),
        ({**base, "asset_path": "/data/context/missing.glb", "sha256": "bad"}, "CONTEXT_LAYER_HASH_REQUIRED"),
    ]
    for layer, code in cases:
        with pytest.raises(ApiError) as caught:
            bridge.build_context(payload(layer))
        assert caught.value.code == code

    path = tmp_path / "data" / "context" / "terrain.glb"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"glTF" + b"\0" * 64)
    with pytest.raises(ApiError) as caught:
        bridge.build_context(payload({**base, "asset_path": "/data/context/terrain.glb", "sha256": "a" * 64}))
    assert caught.value.code == "CONTEXT_LAYER_HASH_MISMATCH"
