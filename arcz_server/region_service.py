from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path
from typing import Any
import uuid

from .ai_broker import LocalAIBroker
from .errors import ApiError
from .geocoder import LocalGeocoder
from .profiles import ProfileStore
from .schema_validation import SchemaRegistry
from .source_registry import SourceRegistry


def now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


class RegionService:
    def __init__(self, schemas: SchemaRegistry, geocoder: LocalGeocoder,
                 sources: SourceRegistry, profiles: ProfileStore,
                 ai: LocalAIBroker | None = None):
        self.schemas = schemas
        self.geocoder = geocoder
        self.sources = sources
        self.profiles = profiles
        self.ai = ai

    def resolve(self, request: dict[str, Any]) -> dict[str, Any] | list[dict[str, Any]]:
        if "query" in request:
            return self.geocoder.search(str(request["query"]), limit=int(request.get("limit", 8)),
                                        scale=request.get("scale"))
        normalized = dict(request)
        normalized.setdefault("region_id", uuid.uuid4().hex)
        normalized.setdefault("generation_epoch", 0)
        self.schemas.validate("region-request.schema.json", normalized)
        west, south, east, north = normalized["bbox_wgs84"]
        if west > east or south > north:
            raise ApiError("REGION_BBOX_INVALID", "bbox precisa seguir west,south,east,north", status=400)
        return normalized

    def build_context(self, request: dict[str, Any]) -> dict[str, Any]:
        self.schemas.validate("region-request.schema.json", request)
        bbox = request["bbox_wgs84"]
        package_rows: list[dict[str, Any]] = []
        source_map = {"osm": "osm", "overture": "overture", "dem": "dem",
                      "imagery": "imagery", "street": "panorama"}
        for flag, kind in source_map.items():
            if request["sources"].get(flag):
                package_rows.extend(self.sources.resolve_bbox(kind, bbox))
        manifests = [self.sources.manifest(row["content_hash"]) for row in package_rows]
        warnings: list[str] = []
        evidence: list[dict[str, Any]] = []

        terrain_stats = self._first_metadata(manifests, "terrain_stats")
        if terrain_stats is None:
            terrain = {"min_m": None, "max_m": None, "mean_slope_deg": None,
                       "slope_classes": {}, "confidence": 0.0, "vertical_error_m": None}
            if request["sources"].get("dem"):
                warnings.append("DEM solicitado, mas nenhum pacote local com terrain_stats cobre a região")
        else:
            terrain = {
                "min_m": terrain_stats.get("min_m"), "max_m": terrain_stats.get("max_m"),
                "mean_slope_deg": terrain_stats.get("mean_slope_deg"),
                "slope_classes": terrain_stats.get("slope_classes", {}),
                "confidence": float(terrain_stats.get("confidence", 1.0)),
                "vertical_error_m": terrain_stats.get("vertical_error_m"),
            }
            evidence.append(self._evidence("terrain", terrain, "package_metadata",
                                           str(terrain_stats.get("source_ref", "terrain_stats")),
                                           terrain["confidence"]))

        urban_meta = self._first_metadata(manifests, "urban_context") or {}
        urban = {
            "density": urban_meta.get("density", "unknown"),
            "block_pattern": urban_meta.get("block_pattern", "unknown"),
            "road_hierarchy": urban_meta.get("road_hierarchy", {}),
            "building_height_distribution": urban_meta.get("building_height_distribution", {}),
            "landuse_distribution": urban_meta.get("landuse_distribution", {}),
        }
        if not urban_meta and (request["sources"].get("osm") or request["sources"].get("overture")):
            warnings.append("Pacote urbano presente sem urban_context pré-calculado; contexto permanece desconhecido")
        if urban_meta:
            evidence.append(self._evidence("urban", urban, "package_metadata",
                                           str(urban_meta.get("source_ref", "urban_context")),
                                           float(urban_meta.get("confidence", 1.0))))

        env_meta = self._first_metadata(manifests, "environment") or {}
        environment = {
            "biome": env_meta.get("biome", "unknown"),
            "climate_profile": env_meta.get("climate_profile", "unknown"),
            "soil_profile": env_meta.get("soil_profile", "unknown"),
        }
        if env_meta:
            evidence.append(self._evidence("environment", environment, "package_metadata",
                                           str(env_meta.get("source_ref", "environment")),
                                           float(env_meta.get("confidence", 1.0))))

        context = {
            "schema_version": 1,
            "region_id": request["region_id"],
            "crs_work": "ENU_LOCAL",
            "origin_wgs84": [request["focus"]["lon"], request["focus"]["lat"], 0.0],
            "terrain": terrain, "urban": urban, "environment": environment,
            "evidence": evidence, "warnings": warnings,
            "source_packages": sorted({row["content_hash"] for row in package_rows}),
        }
        self.schemas.validate("region-context.schema.json", context)
        return context

    def recommend_profiles(self, context: dict[str, Any], *, allow_ai: bool = False) -> dict[str, Any]:
        self.schemas.validate("region-context.schema.json", context)
        available = {profile["id"]: profile for profile in self.profiles.list()}
        selected = []
        if "global.base.v1" in available:
            selected.append("global.base.v1")
        biome = context["environment"].get("biome", "")
        density = context["urban"].get("density", "unknown")
        if "atlantic_forest" in biome or "coastal" in biome:
            for profile_id in available:
                if ".coastal." in profile_id:
                    selected.append(profile_id)
                    break
        elif density in {"high", "very_high"}:
            for profile_id in available:
                if "metropolitan" in profile_id or "dense" in profile_id:
                    selected.append(profile_id)
                    break
        if allow_ai and self.ai is not None:
            try:
                result = self.ai.request("style-classifier", {"region_context": context,
                                                               "allowed_profile_ids": sorted(available)})
                suggested = result.get("result", {}).get("profile_ids", [])
                selected.extend(pid for pid in suggested if pid in available)
                method = "local_ai+rules"
            except ApiError as error:
                if error.code != "MODEL_NOT_INSTALLED":
                    raise
                method = "rules_no_local_model"
        else:
            method = "rules"
        selected = list(dict.fromkeys(selected))
        if not selected and available:
            selected = [sorted(available)[0]]
        return {"profile_ids": selected, "method": method,
                "composed": self.profiles.compose(selected) if selected else None}

    @staticmethod
    def _first_metadata(manifests: list[dict[str, Any]], key: str) -> dict[str, Any] | None:
        for manifest in manifests:
            value = manifest.get("metadata", {}).get(key)
            if isinstance(value, dict):
                return value
        return None

    @staticmethod
    def _evidence(field: str, value: Any, source: str, source_ref: str, confidence: float) -> dict[str, Any]:
        return {"field": field, "value": value, "source": source, "source_ref": source_ref,
                "confidence": max(0.0, min(1.0, confidence)), "timestamp": now()}
