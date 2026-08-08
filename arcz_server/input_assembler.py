from __future__ import annotations

from copy import deepcopy
import json
from pathlib import Path
from typing import Any

from .errors import ApiError
from .generator_contracts import GeneratorContracts
from .hashing import canonical_json_hash
from .schema_validation import SchemaRegistry
from .source_registry import SourceRegistry

try:
    from pyproj import CRS, Transformer  # type: ignore
except Exception:  # pragma: no cover - gate explicitamente testado
    CRS = None
    Transformer = None

ARRAY_KEYS = ("parcels", "roads", "buildings", "vegetation_zones", "materials")
KIND_KEYS = {
    "terrain.generate": ("terrain", "materials", "flat_terrain"),
    "parcels.generate": ("parcels", "materials"),
    "roads.generate": ("roads", "materials"),
    "houses.generate": ("buildings", "parcels", "materials"),
    "buildings.generate": ("buildings", "materials"),
    "vegetation.generate": ("vegetation_zones", "materials"),
    "materials.generate": ("materials",),
    "tiles.generate": (),
}
PACKAGE_KINDS = {
    "terrain.generate": ("dem", "other"),
    "parcels.generate": ("osm", "overture", "other"),
    "roads.generate": ("osm", "overture", "other"),
    "houses.generate": ("osm", "overture", "assets", "other"),
    "buildings.generate": ("osm", "overture", "assets", "other"),
    "vegetation.generate": ("osm", "overture", "imagery", "other"),
    "materials.generate": ("assets", "other"),
    "tiles.generate": (),
}


class LocalInputAssembler:
    """Monta o request do worker somente a partir de dados locais verificáveis.

    Convenção de pacote: qualquer pacote materializado pode conter um arquivo
    `arcz-generator-inputs.json`, ou indicar seu caminho em
    `package.json.metadata.arcz_generator_inputs`. O arquivo declara WGS84 ou
    ENU_LOCAL e é reprojetado para a origem ENU da Região Ativa. Nenhum acesso
    remoto ocorre aqui.
    """

    def __init__(self, schemas: SchemaRegistry, sources: SourceRegistry,
                 contracts: GeneratorContracts):
        self.schemas = schemas
        self.sources = sources
        self.contracts = contracts

    def resolve(self, kind: str, region: dict[str, Any], user_params: dict[str, Any]) -> dict[str, Any]:
        if kind not in KIND_KEYS:
            raise ApiError("GENERATOR_KIND_UNSUPPORTED", kind, status=422)
        request = region.get("request") if isinstance(region, dict) else None
        context = region.get("context") if isinstance(region, dict) else None
        if not isinstance(request, dict) or not isinstance(context, dict):
            raise ApiError("ACTIVE_REGION_INCOMPLETE", "Região Ativa precisa conter request e context", status=400)
        bbox = request.get("bbox_wgs84")
        origin = context.get("origin_wgs84")
        if not (isinstance(bbox, list) and len(bbox) == 4 and isinstance(origin, list) and len(origin) >= 2):
            raise ApiError("ACTIVE_REGION_GEOMETRY_INVALID", "bbox/origin ausente", status=400)

        explicit = self.contracts.normalize_params({"params": user_params})
        imported: dict[str, Any] = {}
        source_versions: dict[str, str] = {}
        package_reports: list[dict[str, Any]] = []
        seen_hashes: set[str] = set()

        for package_kind in PACKAGE_KINDS[kind]:
            for row in self.sources.resolve_bbox(package_kind, bbox):
                content_hash = str(row["content_hash"])
                if content_hash in seen_hashes:
                    continue
                seen_hashes.add(content_hash)
                manifest = self.sources.manifest(content_hash)
                input_path = self._input_file(Path(row["manifest_path"]), manifest)
                if input_path is None:
                    continue
                envelope = json.loads(input_path.read_text(encoding="utf-8"))
                self.schemas.validate("generator-input-package.schema.json", envelope)
                transformed = self._transform(envelope, [float(origin[0]), float(origin[1]), float(origin[2] if len(origin) > 2 else 0.0)])
                filtered = {key: transformed[key] for key in KIND_KEYS[kind] if key in transformed}
                self._inject_provenance(filtered, manifest, content_hash)
                self._merge(imported, filtered, explicit_precedence=False)
                source_versions[manifest["package_id"]] = f"{manifest['version']}@{content_hash}"
                package_reports.append({
                    "package_id": manifest["package_id"], "version": manifest["version"],
                    "content_hash": content_hash, "input_file": input_path.name,
                    "license": manifest["license"],
                })

        merged = deepcopy(imported)
        self._merge(merged, explicit, explicit_precedence=True)
        # Controles de execução permanecem exatamente os escolhidos pelo usuário.
        for key, value in explicit.items():
            if key not in ARRAY_KEYS and key not in {"terrain", "flat_terrain"}:
                merged[key] = deepcopy(value)

        self.contracts.reject_non_finite(merged)
        self.schemas.validate("generator-parameters.schema.json", merged)
        self.contracts.validate_semantics(kind, merged)
        return {
            "params": merged,
            "source_versions": source_versions,
            "packages": package_reports,
            "inputs_hash": canonical_json_hash(merged),
            "offline": True,
        }

    @staticmethod
    def _input_file(manifest_path: Path, manifest: dict[str, Any]) -> Path | None:
        package_dir = manifest_path.resolve().parent
        declared = manifest.get("metadata", {}).get("arcz_generator_inputs")
        candidates: list[str] = []
        if isinstance(declared, str):
            candidates.append(declared)
        candidates.extend(item["path"] for item in manifest.get("files", [])
                          if Path(item["path"]).name == "arcz-generator-inputs.json")
        for relative in dict.fromkeys(candidates):
            candidate = (package_dir / relative).resolve()
            try:
                candidate.relative_to(package_dir)
            except ValueError as error:
                raise ApiError("PACKAGE_INPUT_PATH_ESCAPE", relative, status=500) from error
            if candidate.is_file():
                return candidate
        return None

    def _transform(self, envelope: dict[str, Any], target_origin: list[float]) -> dict[str, Any]:
        data = deepcopy(envelope["data"])
        coordinate_system = envelope["coordinate_system"]
        source_origin = envelope.get("origin_wgs84")
        transform = _CoordinateTransform(coordinate_system, source_origin, target_origin)

        for parcel in data.get("parcels", []):
            parcel["polygon_enu_m"] = transform.points(parcel["polygon_enu_m"])
        for road in data.get("roads", []):
            road["centerline_enu_m"] = transform.points(road["centerline_enu_m"])
        for building in data.get("buildings", []):
            building["footprint_enu_m"] = transform.points(building["footprint_enu_m"])
        for zone in data.get("vegetation_zones", []):
            zone["polygon_enu_m"] = transform.points(zone["polygon_enu_m"])
            for exclusion in zone.get("exclusions", []):
                exclusion["center"] = transform.point(exclusion["center"])

        if "terrain" in data:
            transform.assert_grid_compatible("terrain")
        if "flat_terrain" in data:
            bounds = data["flat_terrain"]["bounds_enu_m"]
            corners = transform.points([[bounds[0], bounds[1]], [bounds[2], bounds[1]],
                                        [bounds[2], bounds[3]], [bounds[0], bounds[3]]])
            data["flat_terrain"]["bounds_enu_m"] = [
                min(p[0] for p in corners), min(p[1] for p in corners),
                max(p[0] for p in corners), max(p[1] for p in corners),
            ]
        return data

    @staticmethod
    def _inject_provenance(data: dict[str, Any], manifest: dict[str, Any], content_hash: str) -> None:
        source = f"package:{manifest['package_id']}@{manifest['version']}"
        for key in ("parcels", "roads", "buildings", "vegetation_zones"):
            for item in data.get(key, []):
                evidence = item.setdefault("source", {})
                evidence.setdefault("source", source)
                evidence.setdefault("source_ref", content_hash)
                evidence.setdefault("confidence", 1.0)
                evidence.setdefault("estimated", False)

    @staticmethod
    def _merge(target: dict[str, Any], incoming: dict[str, Any], *, explicit_precedence: bool) -> None:
        for key, value in incoming.items():
            if key in ARRAY_KEYS:
                existing = target.setdefault(key, [])
                if not isinstance(existing, list) or not isinstance(value, list):
                    raise ApiError("GENERATOR_INPUT_TYPE_CONFLICT", key, status=409)
                by_id = {str(item.get("id")): (index, item) for index, item in enumerate(existing)
                         if isinstance(item, dict) and item.get("id") is not None}
                for item in value:
                    if not isinstance(item, dict) or not item.get("id"):
                        raise ApiError("GENERATOR_ENTITY_ID_REQUIRED", key, status=400)
                    entity_id = str(item["id"])
                    if entity_id not in by_id:
                        existing.append(deepcopy(item))
                        by_id[entity_id] = (len(existing) - 1, item)
                    else:
                        index, previous = by_id[entity_id]
                        if canonical_json_hash(previous) == canonical_json_hash(item):
                            continue
                        if explicit_precedence:
                            existing[index] = deepcopy(item)
                        else:
                            raise ApiError("GENERATOR_ENTITY_CONFLICT",
                                           f"{key}/{entity_id} difere entre pacotes locais", status=409)
            elif key in {"terrain", "flat_terrain"}:
                if key not in target or explicit_precedence:
                    target[key] = deepcopy(value)
                elif canonical_json_hash(target[key]) != canonical_json_hash(value):
                    raise ApiError("GENERATOR_SINGLETON_CONFLICT", key, status=409)
            elif key not in target or explicit_precedence:
                target[key] = deepcopy(value)


class _CoordinateTransform:
    def __init__(self, coordinate_system: str, source_origin: Any, target_origin: list[float]):
        self.coordinate_system = coordinate_system
        self.source_origin = source_origin
        self.target_origin = target_origin
        self.identity = coordinate_system == "ENU_LOCAL" and self._same_origin(source_origin, target_origin)
        self._source_inverse = None
        self._target_forward = None
        if self.identity:
            return
        if CRS is None or Transformer is None:
            raise ApiError("PYPROJ_REQUIRED", "Conversão WGS84/ENU exige pyproj local", status=503)
        target_crs = self._local_crs(target_origin)
        self._target_forward = Transformer.from_crs(CRS.from_epsg(4326), target_crs, always_xy=True)
        if coordinate_system == "ENU_LOCAL":
            if not (isinstance(source_origin, list) and len(source_origin) >= 2):
                raise ApiError("PACKAGE_ORIGIN_REQUIRED", "ENU_LOCAL exige origin_wgs84", status=400)
            source_crs = self._local_crs(source_origin)
            self._source_inverse = Transformer.from_crs(source_crs, CRS.from_epsg(4326), always_xy=True)
        elif coordinate_system != "WGS84":
            raise ApiError("PACKAGE_COORDINATE_SYSTEM_INVALID", coordinate_system, status=400)

    @staticmethod
    def _local_crs(origin: list[float]):
        return CRS.from_proj4(
            f"+proj=aeqd +lat_0={float(origin[1])} +lon_0={float(origin[0])} "
            "+datum=WGS84 +units=m +no_defs"
        )

    @staticmethod
    def _same_origin(a: Any, b: list[float]) -> bool:
        return isinstance(a, list) and len(a) >= 2 and abs(float(a[0]) - b[0]) <= 1e-10 and abs(float(a[1]) - b[1]) <= 1e-10

    def point(self, point: list[float]) -> list[float]:
        if not (isinstance(point, list) and len(point) == 2):
            raise ApiError("GENERATOR_POINT_INVALID", repr(point), status=400)
        if self.identity:
            return [float(point[0]), float(point[1])]
        if self.coordinate_system == "WGS84":
            lon, lat = float(point[0]), float(point[1])
        else:
            lon, lat = self._source_inverse.transform(float(point[0]), float(point[1]))
        east, north = self._target_forward.transform(lon, lat)
        return [float(east), float(north)]

    def points(self, values: list[list[float]]) -> list[list[float]]:
        return [self.point(point) for point in values]

    def assert_grid_compatible(self, field: str) -> None:
        if not self.identity:
            raise ApiError("GRID_REPROJECTION_REQUIRED",
                           f"{field} regular precisa ser pré-reprojetado para a origem ENU ativa",
                           status=422, details={"target_origin_wgs84": self.target_origin})
