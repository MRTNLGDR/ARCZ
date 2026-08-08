"""Migrações puras e atômicas do ``projeto.json`` do ARCZ Earth.

Contrato para implementadores futuros
-------------------------------------
* Nunca edite um projeto antigo in-place antes de produzir backup válido.
* Migrações precisam ser idempotentes: aplicar duas vezes produz os mesmos bytes
  canônicos, exceto campos de salvamento alterados explicitamente pelo usuário.
* Uma versão de schema mais nova que o runtime é erro, não oportunidade para
  descartar campos desconhecidos.
* Segredos de conectores nunca pertencem ao projeto persistente.
* Dados derivados não são inventados durante migração.
"""
from __future__ import annotations

from copy import deepcopy
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any

from .atomic_io import atomic_write_json, read_json
from .errors import ApiError
from .hashing import canonical_json_hash
from .schema_validation import SchemaRegistry

CURRENT_PROJECT_SCHEMA = 2
NETWORK_MODES = frozenset({"offline_strict", "local_lan", "import_assisted"})


@dataclass(frozen=True, slots=True)
class MigrationReport:
    source_schema: int
    target_schema: int
    changed: bool
    applied: tuple[str, ...]
    warnings: tuple[str, ...]

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


def _stable_seed(value: dict[str, Any]) -> int:
    """Deriva seed 63-bit sem relógio/ordem assíncrona."""
    digest = canonical_json_hash(value)
    return max(1, int(digest[:16], 16) & ((1 << 63) - 1))


def _legacy_schema(value: dict[str, Any]) -> int:
    raw = value.get("schema_version")
    if raw is None:
        return 1
    if isinstance(raw, bool) or not isinstance(raw, int) or raw < 1:
        raise ApiError("PROJECT_SCHEMA_INVALID", f"schema_version inválido: {raw!r}", status=400)
    if raw > CURRENT_PROJECT_SCHEMA:
        raise ApiError(
            "PROJECT_SCHEMA_NEWER_THAN_RUNTIME",
            f"Projeto schema {raw}; runtime suporta até {CURRENT_PROJECT_SCHEMA}",
            status=409,
            details={"project_schema": raw, "runtime_schema": CURRENT_PROJECT_SCHEMA},
        )
    return raw


def _normalize_list(value: Any, field: str) -> list[Any]:
    if value is None:
        return []
    if isinstance(value, list):
        return deepcopy(value)
    if isinstance(value, dict):
        # Formato legado: dicionário indexado por id/nome.
        return [deepcopy(item) for item in value.values()]
    raise ApiError("PROJECT_FIELD_TYPE_INVALID", f"{field} precisa ser lista/dicionário legado", status=400)


def _normalize_dict(value: Any, field: str) -> dict[str, Any]:
    if value is None:
        return {}
    if isinstance(value, dict):
        return deepcopy(value)
    raise ApiError("PROJECT_FIELD_TYPE_INVALID", f"{field} precisa ser objeto", status=400)


def _migrate_legacy_position(project: dict[str, Any], applied: list[str]) -> None:
    position = project.get("posicao")
    if not isinstance(position, dict) or not isinstance(position.get("lugar"), dict):
        return
    place = position.get("lugar", {})
    scene = position.get("cena", {}) if isinstance(position.get("cena"), dict) else {}
    legacy_camera = position.get("camera") if isinstance(position.get("camera"), dict) else None
    lod = scene.get("qualidade", "equilibrado")
    project["posicao"] = {
        "lat": place.get("lat"),
        "lon": place.get("lon"),
        "alt": place.get("alt"),
        "rumo": place.get("rumo"),
        "escala": place.get("escala"),
        "colar": place.get("colar"),
        "lod": "original" if lod == "original" else lod,
    }
    environment = _normalize_dict(project.get("ambiente"), "ambiente")
    for target, legacy_key in (
        ("imagery", "imagery"),
        ("relevo", "relevo"),
        ("hora", "hora"),
        ("sombra", "sombra"),
        ("qualidade", "qualidade"),
    ):
        if legacy_key in scene:
            environment[target] = scene[legacy_key]
    project["ambiente"] = environment
    if legacy_camera is not None and not isinstance(project.get("camera"), dict):
        project["camera"] = deepcopy(legacy_camera)
    elif legacy_camera is not None and not project.get("camera"):
        project["camera"] = deepcopy(legacy_camera)
    applied.append("v1.flatten_position_scene_camera")


def migrate_project(
    value: dict[str, Any],
    *,
    project_seed: int | None = None,
    schemas: SchemaRegistry | None = None,
) -> tuple[dict[str, Any], MigrationReport]:
    """Retorna uma cópia V2 validada; nunca modifica ``value``.

    ``project_seed`` é usado somente para um projeto que ainda não possui seed.
    Sem esse argumento, a migração deriva seed estável do conteúdo legado,
    tornando o resultado puro e reproduzível.
    """
    if not isinstance(value, dict):
        raise ApiError("PROJECT_ROOT_INVALID", "projeto precisa ser objeto JSON", status=400)
    original = deepcopy(value)
    project = deepcopy(value)
    source_schema = _legacy_schema(project)
    applied: list[str] = []
    warnings: list[str] = []

    _migrate_legacy_position(project, applied)

    environment = _normalize_dict(project.get("ambiente"), "ambiente")
    if "token_mapbox" in environment:
        environment.pop("token_mapbox", None)
        applied.append("security.remove_legacy_mapbox_token")
    environment.setdefault("imagery", "naturalearth_local")
    environment.setdefault("relevo", "ellipsoid")
    project["ambiente"] = environment

    mode = project.get("network_mode", "offline_strict")
    if mode not in NETWORK_MODES:
        mode = "offline_strict"
        warnings.append("network_mode inválido foi reduzido a offline_strict")
        applied.append("security.normalize_network_mode")
    project["network_mode"] = mode

    project["posicao"] = _normalize_dict(project.get("posicao"), "posicao")
    project["camera"] = _normalize_dict(project.get("camera"), "camera")
    project["corte"] = _normalize_dict(project.get("corte"), "corte")
    project["recorte"] = _normalize_dict(project.get("recorte"), "recorte")
    for field in ("takes", "pecas", "lugares"):
        project[field] = _normalize_list(project.get(field), field)

    project.setdefault("versao", 1)
    project["schema_version"] = CURRENT_PROJECT_SCHEMA
    seed = project.get("project_seed")
    if isinstance(seed, bool) or not isinstance(seed, int) or seed < 0:
        seed = project_seed if project_seed is not None else _stable_seed(original)
        project["project_seed"] = int(seed)
        applied.append("v2.add_project_seed")

    project.setdefault("active_region", None)
    project["region_profiles"] = _normalize_dict(project.get("region_profiles"), "region_profiles")
    project["plugins"] = _normalize_dict(project.get("plugins"), "plugins")
    project["overrides"] = _normalize_dict(project.get("overrides"), "overrides")
    project["procedural_layers"] = _normalize_list(project.get("procedural_layers"), "procedural_layers")
    project["generation_manifests"] = _normalize_list(project.get("generation_manifests"), "generation_manifests")
    project["tombstones"] = _normalize_list(project.get("tombstones"), "tombstones")
    project["render_jobs"] = _normalize_list(project.get("render_jobs"), "render_jobs")
    project["source_registry"] = _normalize_list(project.get("source_registry"), "source_registry")
    project["floorplanner_projects"] = _normalize_list(project.get("floorplanner_projects"), "floorplanner_projects")
    project["floorplanner_derivatives"] = _normalize_list(
        project.get("floorplanner_derivatives"), "floorplanner_derivatives"
    )
    primary_model = project.get("primary_model")
    if primary_model is not None and not isinstance(primary_model, dict):
        raise ApiError("PROJECT_FIELD_TYPE_INVALID", "primary_model precisa ser objeto/null", status=400)
    project["primary_model"] = deepcopy(primary_model) if primary_model is not None else None
    project["chat_sessions"] = _normalize_list(project.get("chat_sessions"), "chat_sessions")
    project["reference_media"] = _normalize_list(project.get("reference_media"), "reference_media")
    workspace_mode = str(project.get("workspace_mode", "globo"))
    project["workspace_mode"] = workspace_mode if workspace_mode in {"globo", "floorplanner", "render", "walk"} else "globo"
    project["floorplanner_north_rotation_deg"] = float(project.get("floorplanner_north_rotation_deg", 0.0))
    project["floorplanner_vertical_offset_m"] = float(project.get("floorplanner_vertical_offset_m", 0.0))
    project["floorplanner_constraints"] = _normalize_dict(project.get("floorplanner_constraints"), "floorplanner_constraints")
    project["floorplanner_context_layers"] = _normalize_list(
        project.get("floorplanner_context_layers"), "floorplanner_context_layers"
    )
    active_floorplanner = project.get("active_floorplanner_project_id")
    if active_floorplanner is not None and not isinstance(active_floorplanner, str):
        raise ApiError("PROJECT_FIELD_TYPE_INVALID", "active_floorplanner_project_id precisa ser texto/null", status=400)
    project["active_floorplanner_project_id"] = active_floorplanner
    panel_layout = project.get("panel_layout")
    if panel_layout is None:
        panel_layout = {"schema_version": 1, "panels": {}}
        applied.append("v2.add_panel_layout")
    elif not isinstance(panel_layout, dict):
        raise ApiError("PROJECT_FIELD_TYPE_INVALID", "panel_layout precisa ser objeto", status=400)
    project["panel_layout"] = deepcopy(panel_layout)
    earth_presentation = project.get("earth_presentation")
    if earth_presentation is None:
        earth_presentation = {
            "schema_version": 1, "enabled": True, "duration_ms": 6500,
            "start_altitude_m": 24000000, "end_altitude_m": 1500000,
            "orbit_altitude_m": 1500000, "clouds": True, "atmosphere": True,
            "stars": True, "sun": True, "moon": True, "fog": True,
            "fog_density": 0.00018, "hue_shift": 0.0,
            "saturation_shift": -0.05, "brightness_shift": -0.03,
            "orbit_heading_delta_deg": 14.0,
            "skip_on_reduced_motion": True,
        }
        applied.append("v2.add_earth_presentation")
    elif not isinstance(earth_presentation, dict):
        raise ApiError("PROJECT_FIELD_TYPE_INVALID", "earth_presentation precisa ser objeto", status=400)
    earth_presentation = deepcopy(earth_presentation)
    earth_presentation.setdefault("schema_version", 1)
    earth_presentation.setdefault("enabled", True)
    earth_presentation.setdefault("duration_ms", 6500)
    earth_presentation.setdefault("start_altitude_m", 24000000)
    earth_presentation.setdefault("end_altitude_m", earth_presentation.get("orbit_altitude_m", 1500000))
    earth_presentation.setdefault("orbit_altitude_m", earth_presentation.get("end_altitude_m", 1500000))
    earth_presentation.setdefault("clouds", True)
    earth_presentation.setdefault("atmosphere", True)
    earth_presentation.setdefault("stars", True)
    earth_presentation.setdefault("sun", True)
    earth_presentation.setdefault("moon", True)
    earth_presentation.setdefault("fog", True)
    earth_presentation.setdefault("fog_density", 0.00018)
    earth_presentation.setdefault("hue_shift", 0.0)
    earth_presentation.setdefault("saturation_shift", -0.05)
    earth_presentation.setdefault("brightness_shift", -0.03)
    earth_presentation.setdefault("orbit_heading_delta_deg", 14.0)
    earth_presentation.setdefault("skip_on_reduced_motion", True)
    project["earth_presentation"] = earth_presentation

    timeline = project.get("timeline")
    if timeline is None:
        timeline = {"schema_version": 1, "fps": 30, "duration_frames": 300, "tracks": []}
        applied.append("v2.add_timeline")
    elif not isinstance(timeline, dict):
        raise ApiError("PROJECT_FIELD_TYPE_INVALID", "timeline precisa ser objeto", status=400)
    project["timeline"] = deepcopy(timeline)

    revision = project.get("save_revision", 0)
    if isinstance(revision, bool) or not isinstance(revision, int) or revision < 0:
        raise ApiError("PROJECT_SAVE_REVISION_INVALID", repr(revision), status=400)
    project["save_revision"] = revision

    # Hash armazenado deve refletir o documento migrado, nunca bytes antigos.
    project.pop("content_hash", None)
    project["content_hash"] = canonical_json_hash(project)

    if schemas is not None:
        schemas.validate("project-v2.schema.json", project)

    changed = canonical_json_hash(original) != canonical_json_hash(project)
    if source_schema < CURRENT_PROJECT_SCHEMA and "v1_to_v2" not in applied:
        applied.insert(0, "v1_to_v2")
    report = MigrationReport(
        source_schema=source_schema,
        target_schema=CURRENT_PROJECT_SCHEMA,
        changed=changed,
        applied=tuple(dict.fromkeys(applied)),
        warnings=tuple(warnings),
    )
    return project, report


def migrate_project_file(
    path: Path,
    *,
    schemas: SchemaRegistry,
) -> tuple[dict[str, Any], MigrationReport]:
    """Lê, migra, valida e persiste por replace atômico com ``.bak``.

    Em qualquer erro antes do replace, o arquivo original permanece intacto.
    """
    raw = read_json(path)
    if raw is None:
        raise ApiError("PROJECT_FILE_MISSING", str(path), status=404)
    migrated, report = migrate_project(raw, schemas=schemas)
    if report.changed:
        atomic_write_json(path, migrated, backup=True, indent=2)
    return migrated, report
