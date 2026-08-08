from __future__ import annotations

from datetime import datetime, timezone
import json
import os
from pathlib import Path
import sys
import time
from typing import Any

import pytest

from arcz_server.ai_broker import LocalAIBroker, ModelRegistry
from arcz_server.errors import ApiError
from arcz_server.generator_contracts import GeneratorContracts
from arcz_server.geocoder import LocalGeocoder
from arcz_server.hashing import canonical_json_hash, sha256_file
from arcz_server.input_assembler import LocalInputAssembler
from arcz_server.jobs import JobContext, JobManager, JobStore, utc_now
from arcz_server.network_policy import NetworkMode, NetworkPolicy
from arcz_server.schema_validation import SchemaRegistry
from arcz_server.source_registry import SourceRegistry


ROOT = Path(__file__).resolve().parents[1]
SCHEMAS = SchemaRegistry(ROOT / "schemas")


def assert_api_error(code: str, fn) -> ApiError:
    with pytest.raises(ApiError) as captured:
        fn()
    assert captured.value.code == code
    return captured.value


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")),
        encoding="utf-8",
    )


def package_manifest(
    directory: Path,
    *,
    package_id: str,
    version: str,
    kind: str,
    bbox: list[float],
    input_data: dict[str, Any],
) -> dict[str, Any]:
    input_path = directory / "arcz-generator-inputs.json"
    write_json(input_path, input_data)
    manifest = {
        "schema_version": 1,
        "package_id": package_id,
        "version": version,
        "kind": kind,
        "license": {
            "id": "LicenseRef-TestFixture",
            "name": "ARCZ deterministic contract fixture",
            "attribution_required": False,
            "redistribution_allowed": True,
        },
        "provenance": {
            "source": "test-contract-fixture",
            "import_method": "local_test",
            "imported_by": "pytest",
        },
        "created_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "files": [{
            "path": input_path.name,
            "sha256": sha256_file(input_path),
            "bytes": input_path.stat().st_size,
            "mime": "application/json",
        }],
        "bbox_wgs84": bbox,
        "immutable": True,
        "metadata": {"arcz_generator_inputs": input_path.name},
    }
    write_json(directory / "package.json", manifest)
    return manifest


def active_region(origin: list[float] | None = None) -> dict[str, Any]:
    origin = origin or [-48.0, -27.0, 0.0]
    return {
        "request": {
            "schema_version": 1,
            "region_id": "region-test",
            "bbox_wgs84": [-48.001, -27.001, -47.999, -26.999],
            "polygon_wgs84": [],
            "focus": {"lat": origin[1], "lon": origin[0]},
            "scale": "quarteirao",
            "requested_radius_m": 250,
            "sources": {"osm": True, "overture": False, "dem": True, "imagery": False, "street": False},
            "generation_epoch": 7,
        },
        "context": {
            "schema_version": 1,
            "region_id": "region-test",
            "crs_work": "ENU_LOCAL",
            "origin_wgs84": origin,
            "terrain": {
                "min_m": None,
                "max_m": None,
                "mean_slope_deg": None,
                "slope_classes": {},
                "confidence": 0.0,
                "vertical_error_m": None,
            },
            "urban": {
                "density": "unknown",
                "block_pattern": "unknown",
                "road_hierarchy": {},
                "building_height_distribution": {},
                "landuse_distribution": {},
            },
            "environment": {"biome": "unknown", "climate_profile": "unknown", "soil_profile": "unknown"},
            "evidence": [],
            "warnings": [],
            "source_packages": [],
        },
    }


def test_generator_contracts_reject_missing_or_silent_fields() -> None:
    contracts = GeneratorContracts(SCHEMAS)
    assert_api_error(
        "GENERATOR_INPUT_MISSING",
        lambda: contracts.validate_job("terrain.generate", {"params": {}}),
    )
    assert_api_error(
        "SCHEMA_INVALID",
        lambda: contracts.validate_job(
            "terrain.generate",
            {"params": {
                "allow_flat_terrain_fallback": True,
                "flat_terrain": {"bounds_enu_m": [0, 0, 10, 10]},
                "silently_ignored": 1,
            }},
        ),
    )
    assert_api_error(
        "GENERATOR_NON_FINITE",
        lambda: contracts.validate_job(
            "roads.generate",
            {"params": {"roads": [{"id": "r", "centerline_enu_m": [[0, 0], [1, float("nan")]], "width_m": 4}]}},
        ),
    )
    contracts.validate_job(
        "terrain.generate",
        {"params": {
            "allow_flat_terrain_fallback": True,
            "flat_terrain": {"bounds_enu_m": [0, 0, 10, 10], "elevation_m": 2},
        }},
    )


def test_source_registry_is_immutable_and_does_not_leave_conflict_orphan(tmp_path: Path) -> None:
    registry = SourceRegistry(tmp_path / "data", SCHEMAS)
    first = tmp_path / "first"
    first.mkdir()
    manifest = package_manifest(
        first,
        package_id="br.test.osm",
        version="1.0.0",
        kind="osm",
        bbox=[-48.01, -27.01, -47.99, -26.99],
        input_data={
            "schema_version": 1,
            "coordinate_system": "ENU_LOCAL",
            "origin_wgs84": [-48.0, -27.0, 0.0],
            "data": {"parcels": [{"id": "p1", "polygon_enu_m": [[0, 0], [10, 0], [10, 10], [0, 10]]}]},
        },
    )
    imported = registry.import_directory(first)
    assert imported["manifest"] == manifest
    assert registry.verify(imported["content_hash"])["ok"] is True
    assert len(list((tmp_path / "data" / "packages").iterdir())) == 1

    second = tmp_path / "second"
    second.mkdir()
    package_manifest(
        second,
        package_id="br.test.osm",
        version="1.0.0",
        kind="osm",
        bbox=[-48.01, -27.01, -47.99, -26.99],
        input_data={
            "schema_version": 1,
            "coordinate_system": "ENU_LOCAL",
            "origin_wgs84": [-48.0, -27.0, 0.0],
            "data": {"parcels": [{"id": "p1", "polygon_enu_m": [[0, 0], [20, 0], [20, 20], [0, 20]]}]},
        },
    )
    assert_api_error("PACKAGE_CONFLICT", lambda: registry.import_directory(second))
    assert len(list((tmp_path / "data" / "packages").iterdir())) == 1


def test_local_input_assembler_materializes_wgs84_and_preserves_provenance(tmp_path: Path) -> None:
    registry = SourceRegistry(tmp_path / "data", SCHEMAS)
    package = tmp_path / "source"
    package.mkdir()
    package_manifest(
        package,
        package_id="br.test.parcels",
        version="1.0.0",
        kind="osm",
        bbox=[-48.01, -27.01, -47.99, -26.99],
        input_data={
            "schema_version": 1,
            "coordinate_system": "WGS84",
            "data": {
                "parcels": [{
                    "id": "parcel-1",
                    "polygon_enu_m": [
                        [-48.0, -27.0],
                        [-47.9999, -27.0],
                        [-47.9999, -26.9999],
                        [-48.0, -26.9999],
                    ],
                }]
            },
        },
    )
    imported = registry.import_directory(package)
    assembler = LocalInputAssembler(SCHEMAS, registry, GeneratorContracts(SCHEMAS))
    result = assembler.resolve("parcels.generate", active_region(), {})

    parcel = result["params"]["parcels"][0]
    assert parcel["id"] == "parcel-1"
    assert abs(parcel["polygon_enu_m"][0][0]) < 0.01
    assert abs(parcel["polygon_enu_m"][0][1]) < 0.01
    assert 8 < parcel["polygon_enu_m"][1][0] < 12
    assert 9 < parcel["polygon_enu_m"][2][1] < 13
    assert parcel["source"]["source"].startswith("package:br.test.parcels@")
    assert parcel["source"]["source_ref"] == imported["content_hash"]
    assert result["offline"] is True
    assert result["source_versions"]["br.test.parcels"].endswith(imported["content_hash"])
    assert result["inputs_hash"] == canonical_json_hash(result["params"])


def test_local_input_assembler_rejects_regular_grid_with_wrong_enu_origin(tmp_path: Path) -> None:
    registry = SourceRegistry(tmp_path / "data", SCHEMAS)
    package = tmp_path / "dem"
    package.mkdir()
    package_manifest(
        package,
        package_id="br.test.dem",
        version="1.0.0",
        kind="dem",
        bbox=[-48.01, -27.01, -47.99, -26.99],
        input_data={
            "schema_version": 1,
            "coordinate_system": "ENU_LOCAL",
            "origin_wgs84": [-47.5, -27.0, 0.0],
            "data": {
                "terrain": {
                    "origin_enu_m": [0, 0],
                    "columns": 2,
                    "rows": 2,
                    "cell_size_m": [1, 1],
                    "heights_m": [0, 0, 0, 0],
                }
            },
        },
    )
    registry.import_directory(package)
    assembler = LocalInputAssembler(SCHEMAS, registry, GeneratorContracts(SCHEMAS))
    assert_api_error(
        "GRID_REPROJECTION_REQUIRED",
        lambda: assembler.resolve("terrain.generate", active_region(), {}),
    )


def test_local_geocoder_is_searchable_without_provider(tmp_path: Path) -> None:
    geocoder = LocalGeocoder(tmp_path / "geocoder.sqlite3")
    assert_api_error("DATASET_NOT_INSTALLED", lambda: geocoder.search("Bombinhas"))
    assert geocoder.import_records([
        {
            "id": "place-1",
            "display_name": "Rua Onça Pintada, Bombinhas, Santa Catarina",
            "lat": -27.151,
            "lon": -48.498,
            "bbox_wgs84": [-48.499, -27.152, -48.497, -27.150],
            "scale": "endereco",
        }
    ], "br.test.geocoder@1") == 1
    result = geocoder.search("Onca Pintada")
    assert result[0]["id"] == "place-1"
    assert result[0]["source_package"] == "br.test.geocoder@1"


def test_network_policy_denies_external_hosts_by_default() -> None:
    policy = NetworkPolicy(mode=NetworkMode.OFFLINE_STRICT)
    assert policy.allows_host("127.0.0.1") is True
    assert policy.allows_host("localhost") is True
    assert policy.allows_host("8.8.8.8") is False
    assert policy.allows_host("example.com") is False
    assert_api_error("NETWORK_EGRESS_DENIED", lambda: policy.assert_url("https://example.com/data"))


def test_local_ai_broker_uses_verified_model_and_content_cache(tmp_path: Path) -> None:
    model_dir = tmp_path / "models" / "fixture"
    model_dir.mkdir(parents=True)
    script = model_dir / "adapter.py"
    script.write_text(
        """from pathlib import Path
import json
import sys

inp, out = map(Path, sys.argv[1:3])
counter = Path(__file__).with_name("runs.txt")
n = int(counter.read_text() or "0") + 1 if counter.exists() else 1
counter.write_text(str(n))
payload = json.loads(inp.read_text())
out.write_text(json.dumps({"echo": payload, "run": n}, sort_keys=True))
""",
        encoding="utf-8",
    )
    manifest = {
        "schema_version": 1,
        "id": "fixture.local.command",
        "version": "1.0.0",
        "task": "style-classifier",
        "backend": "command",
        "license": "LicenseRef-TestFixture",
        "files": [{"path": "adapter.py", "sha256": sha256_file(script), "bytes": script.stat().st_size}],
        "requirements": {"ram_mb": 1, "vram_mb": 0, "devices": ["cpu"]},
        "input_contract": {"type": "object"},
        "output_contract": {"type": "object"},
        "command": [sys.executable, "adapter.py", "{input}", "{output}"],
        "fallback": "procedural",
        "timeout_seconds": 30,
    }
    write_json(model_dir / "model.json", manifest)
    (tmp_path / "jobs").mkdir()
    registry = ModelRegistry([tmp_path / "models"], SCHEMAS)
    broker = LocalAIBroker(tmp_path, registry, SCHEMAS, NetworkPolicy())

    first = broker.request("style-classifier", {"region": "x"})
    second = broker.request("style-classifier", {"region": "x"})
    assert first == second
    assert first["result"]["run"] == 1
    assert (model_dir / "runs.txt").read_text() == "1"


def test_job_manager_requeues_persisted_queued_job_after_worker_registration(tmp_path: Path) -> None:
    root = tmp_path / "arcz"
    (root / "jobs").mkdir(parents=True)
    store = JobStore(root / "jobs" / "jobs.sqlite3")
    queued = store.create("fixture.generate", {"value": 7}, generation_epoch=3)

    manager = JobManager(root, SCHEMAS, workers=1)

    def worker(context: JobContext, request: dict[str, Any]) -> dict[str, Any]:
        context.update("GENERATE", 0.5, message="Gerando artefato determinístico de contrato")
        output = context.staging_dir / "result.txt"
        output.write_text(str(request["value"]), encoding="utf-8")
        return {
            "schema_version": 1,
            "job_id": context.job_id,
            "generator": "fixture.generate@1.0.0",
            "inputs_hash": canonical_json_hash(request),
            "profile_hash": "0" * 64,
            "seed": 0,
            "source_versions": {},
            "outputs": [{
                "path": output.relative_to(root).as_posix(),
                "sha256": sha256_file(output),
                "bytes": output.stat().st_size,
                "kind": "text",
            }],
            "warnings": [],
            "metrics": {},
            "created_at": utc_now(),
            "deterministic": True,
            "generation_epoch": context.job["generation_epoch"],
        }

    try:
        # Antes do registro, o job continua QUEUED e não é perdido por corrida.
        time.sleep(0.05)
        assert manager.store.get(queued["id"])["status"] == "QUEUED"
        manager.register("fixture.generate", worker)
        completed = manager.wait(queued["id"], timeout=5)
        assert completed["status"] == "COMPLETED"
        assert completed["generation_epoch"] == 3
        assert (root / completed["result_manifest"]).is_file()
    finally:
        manager.stop()


def test_job_manager_cancels_cooperatively(tmp_path: Path) -> None:
    root = tmp_path / "arcz"
    manager = JobManager(root, SCHEMAS, workers=1)

    def cancellable(context: JobContext, _request: dict[str, Any]) -> dict[str, Any]:
        for index in range(200):
            context.check_cancelled()
            if index % 10 == 0:
                context.update("GENERATE", min(0.9, index / 220))
            time.sleep(0.005)
        raise AssertionError("worker deveria ter sido cancelado")

    try:
        manager.register("fixture.cancel", cancellable)
        job = manager.create("fixture.cancel", {})
        deadline = time.monotonic() + 2
        while manager.store.get(job["id"])["status"] == "QUEUED" and time.monotonic() < deadline:
            time.sleep(0.01)
        manager.cancel(job["id"], "test_cancel")
        terminal = manager.wait(job["id"], timeout=5)
        assert terminal["status"] == "CANCELLED"
        assert terminal["cancel_reason"] == "test_cancel"
    finally:
        manager.stop()
