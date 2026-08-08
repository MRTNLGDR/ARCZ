from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import zipfile

import pytest

import servidor

ROOT = Path(__file__).resolve().parents[1]


def load_vendor_tool():
    path = ROOT / "tools" / "vendor_cesium.py"
    spec = importlib.util.spec_from_file_location("arcz_vendor_cesium_test", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def create_minimal_cesium_tree(root: Path) -> Path:
    (root / "Widgets").mkdir(parents=True)
    (root / "Assets" / "Textures" / "NaturalEarthII").mkdir(parents=True)
    (root / "Workers").mkdir(parents=True)
    (root / "ThirdParty").mkdir(parents=True)
    (root / "Cesium.js").write_text("/* real-file fixture for validator */", encoding="utf-8")
    (root / "Widgets" / "widgets.css").write_text("body{}", encoding="utf-8")
    (root / "Assets" / "Textures" / "NaturalEarthII" / "tilemapresource.xml").write_text(
        "<TileMap/>", encoding="utf-8"
    )
    (root / "Workers" / "worker.js").write_text("self.onmessage=()=>{}", encoding="utf-8")
    return root


def test_project_defaults_are_local_first_and_strip_provider_secret():
    data = servidor.Handler._defaults_projeto_v2({
        "ambiente": {"imagery": "satelite", "relevo": "dem", "token_mapbox": "secret"},
        "network_mode": "invalid",
    })
    assert data["network_mode"] == "offline_strict"
    assert "token_mapbox" not in data["ambiente"]
    # Valores antigos explícitos permanecem para migração; o front aplica o
    # guard de modo e volta à base local quando a fonte remota não é autorizada.
    assert data["ambiente"]["imagery"] == "satelite"


def test_dem_route_never_downloads_missing_provider_tile(tmp_path, monkeypatch):
    monkeypatch.setattr(servidor, "CACHE_DEM", tmp_path)
    assert servidor.obter_tile_dem(14, 123, 456) is None
    tile = tmp_path / "14" / "123" / "456.png"
    tile.parent.mkdir(parents=True)
    tile.write_bytes(b"local-tile")
    assert servidor.obter_tile_dem(14, 123, 456) == b"local-tile"


def test_vendor_cesium_validator_accepts_complete_local_tree(tmp_path):
    tool = load_vendor_tool()
    root = create_minimal_cesium_tree(tmp_path / "Build" / "Cesium")
    assert tool.find_cesium_root(tmp_path) == root.resolve()
    tool.validate_tree(root)


def test_vendor_cesium_validator_rejects_incomplete_tree(tmp_path):
    tool = load_vendor_tool()
    root = tmp_path / "Cesium"
    root.mkdir()
    with pytest.raises(FileNotFoundError):
        tool.validate_tree(root)


def test_vendor_zip_rejects_path_traversal(tmp_path):
    tool = load_vendor_tool()
    archive = tmp_path / "bad.zip"
    with zipfile.ZipFile(archive, "w") as zf:
        zf.writestr("../escape.txt", "not allowed")
    with pytest.raises(ValueError, match="escapa"):
        tool.safe_extract_zip(archive, tmp_path / "out")


def load_tool(name: str):
    path = ROOT / "tools" / f"{name}.py"
    spec = importlib.util.spec_from_file_location(f"arcz_{name}_test", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_smoke_generator_aliases_are_explicit_and_deduplicated():
    tool = load_tool("smoke_generation")
    assert tool.resolve_generator_kinds(None) == list(tool.DEFAULT_KINDS)
    assert tool.resolve_generator_kinds(["all"]) == list(tool.DEFAULT_KINDS)
    assert tool.resolve_generator_kinds(["houses", "terrain", "houses"]) == [
        "houses.generate",
        "terrain.generate",
    ]
    with pytest.raises(ValueError, match="gerador inválido"):
        tool.resolve_generator_kinds(["invented"])


def test_offline_acceptance_denies_remote_and_allows_loopback():
    tool = load_tool("offline_acceptance")
    policy = tool.NetworkPolicy(mode=tool.NetworkMode.OFFLINE_STRICT)
    checks = {item["name"]: item for item in tool._check_network_policy(policy)}
    assert checks["remote_denied:example.com"]["status"] == "PASSED"
    assert checks["remote_denied:8.8.8.8"]["status"] == "PASSED"
    assert checks["loopback_allowed:127.0.0.1"]["status"] == "PASSED"
    assert checks["loopback_allowed:localhost"]["status"] == "PASSED"
    assert checks["loopback_allowed:::1"]["status"] == "PASSED"


def test_offline_acceptance_marks_missing_runtime_dependencies_blocked(tmp_path):
    tool = load_tool("offline_acceptance")
    cesium = tool._check_cesium_vendor(tmp_path)
    worker = tool._check_generation_worker(tmp_path)
    assert cesium["status"] == "BLOCKED"
    assert worker["status"] == "BLOCKED"
    assert cesium["missing"]


def test_project_migration_v1_to_v2_is_pure_idempotent_and_strips_secret():
    from arcz_server.project_migrations import migrate_project
    from arcz_server.schema_validation import SchemaRegistry

    legacy = {
        "posicao": {
            "lugar": {"lat": -27.15, "lon": -48.5, "alt": 10, "rumo": 90, "escala": 1, "colar": False},
            "cena": {"imagery": "naturalearth_local", "relevo": "ellipsoid", "qualidade": "equilibrado"},
            "camera": {"lat": -27.15, "lon": -48.5, "alt": 100},
        },
        "ambiente": {"token_mapbox": "must-not-survive"},
        "takes": {"take-a": {"id": "take-a"}},
        "pecas": {},
        "lugares": {},
    }
    original = json.loads(json.dumps(legacy))
    schemas = SchemaRegistry(ROOT / "schemas")
    migrated, report = migrate_project(legacy, schemas=schemas)
    migrated_again, second = migrate_project(migrated, schemas=schemas)

    assert legacy == original  # função não muta entrada
    assert migrated == migrated_again
    assert report.changed is True
    assert second.changed is False
    assert migrated["schema_version"] == 2
    assert migrated["network_mode"] == "offline_strict"
    assert migrated["posicao"]["lat"] == -27.15
    assert migrated["camera"]["alt"] == 100
    assert migrated["takes"] == [{"id": "take-a"}]
    assert "token_mapbox" not in migrated["ambiente"]


def test_project_migration_file_creates_backup_and_keeps_original_bytes(tmp_path):
    from arcz_server.project_migrations import migrate_project_file
    from arcz_server.schema_validation import SchemaRegistry

    path = tmp_path / "projeto.json"
    legacy = {"posicao": {"lat": 1, "lon": 2}, "takes": [], "pecas": [], "lugares": []}
    original_bytes = json.dumps(legacy, ensure_ascii=False).encode("utf-8")
    path.write_bytes(original_bytes)
    migrated, report = migrate_project_file(path, schemas=SchemaRegistry(ROOT / "schemas"))

    assert report.changed is True
    assert path.with_name("projeto.json.bak").read_bytes() == original_bytes
    assert json.loads(path.read_text(encoding="utf-8")) == migrated
    assert migrated["schema_version"] == 2


def test_project_migration_rejects_newer_schema_without_overwriting(tmp_path):
    from arcz_server.errors import ApiError
    from arcz_server.project_migrations import migrate_project_file
    from arcz_server.schema_validation import SchemaRegistry

    path = tmp_path / "projeto.json"
    raw = json.dumps({"schema_version": 999, "posicao": {"lat": 1}}, ensure_ascii=False).encode("utf-8")
    path.write_bytes(raw)
    with pytest.raises(ApiError, match="runtime suporta"):
        migrate_project_file(path, schemas=SchemaRegistry(ROOT / "schemas"))
    assert path.read_bytes() == raw
    assert not path.with_name("projeto.json.bak").exists()
