from __future__ import annotations

"""Conversão geográfica autoritativa ARCZ ↔ coordenadas locais do Aedifex.

O ARCZ mantém WGS84/ENU como autoridade territorial. O Aedifex trabalha em
metros, com X/Z no plano e Y para cima. Para que norte apareça no sentido
esperado pelo floorplanner, o contrato padrão usa X=leste e Z=sul (norte=-Z).
A reflexão faz parte do contrato e nunca deve ser escondida em código de UI.
"""

from dataclasses import dataclass
from math import cos, radians, sin
from pathlib import Path
import re
from typing import Any, Iterable

from .errors import ApiError
from .hashing import canonical_json_hash, sha256_file
from .schema_validation import SchemaRegistry

try:
    from pyproj import CRS, Transformer  # type: ignore
except Exception:  # pragma: no cover
    CRS = None
    Transformer = None


@dataclass(frozen=True, slots=True)
class GeoAnchor:
    origin_wgs84: tuple[float, float, float]
    north_rotation_deg: float = 0.0
    vertical_offset_m: float = 0.0
    axis_policy: str = "AEDIFEX_X_EAST_Y_UP_Z_SOUTH"

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> "GeoAnchor":
        origin = value.get("origin_wgs84")
        if not isinstance(origin, list) or len(origin) != 3:
            raise ApiError("GEO_ANCHOR_ORIGIN_INVALID", "origin_wgs84 exige [lon,lat,alt]", status=400)
        nums = [float(item) for item in origin]
        if not (-180 <= nums[0] <= 180 and -90 <= nums[1] <= 90):
            raise ApiError("GEO_ANCHOR_RANGE_INVALID", repr(origin), status=400)
        policy = str(value.get("axis_policy", "AEDIFEX_X_EAST_Y_UP_Z_SOUTH"))
        if policy != "AEDIFEX_X_EAST_Y_UP_Z_SOUTH":
            raise ApiError("GEO_ANCHOR_AXIS_POLICY_UNSUPPORTED", policy, status=422)
        return cls(
            origin_wgs84=(nums[0], nums[1], nums[2]),
            north_rotation_deg=float(value.get("north_rotation_deg", 0.0)),
            vertical_offset_m=float(value.get("vertical_offset_m", 0.0)),
            axis_policy=policy,
        )

    def to_dict(self) -> dict[str, Any]:
        return {
            "origin_wgs84": list(self.origin_wgs84),
            "north_rotation_deg": self.north_rotation_deg,
            "vertical_offset_m": self.vertical_offset_m,
            "axis_policy": self.axis_policy,
        }


class GeoModelTransform:
    """Transformações determinísticas e reversíveis em precisão dupla."""

    def __init__(self, anchor: GeoAnchor):
        self.anchor = anchor
        angle = radians(anchor.north_rotation_deg)
        self._cos = cos(angle)
        self._sin = sin(angle)
        self._forward = None
        self._inverse = None
        if CRS is not None and Transformer is not None:
            lon, lat, _ = anchor.origin_wgs84
            local = CRS.from_proj4(
                f"+proj=aeqd +lat_0={lat:.12f} +lon_0={lon:.12f} +datum=WGS84 +units=m +no_defs"
            )
            self._forward = Transformer.from_crs(CRS.from_epsg(4326), local, always_xy=True)
            self._inverse = Transformer.from_crs(local, CRS.from_epsg(4326), always_xy=True)

    def _require_projection(self) -> None:
        if self._forward is None or self._inverse is None:
            raise ApiError("PYPROJ_REQUIRED", "Transformação WGS84/ENU exige pyproj local", status=503)

    def wgs84_to_enu(self, point: Iterable[float]) -> list[float]:
        values = list(point)
        if len(values) not in {2, 3}:
            raise ApiError("WGS84_POINT_INVALID", repr(values), status=400)
        self._require_projection()
        e, n = self._forward.transform(float(values[0]), float(values[1]))
        alt = float(values[2]) if len(values) == 3 else self.anchor.origin_wgs84[2]
        return [float(e), float(n), alt - self.anchor.origin_wgs84[2]]

    def enu_to_wgs84(self, point: Iterable[float]) -> list[float]:
        values = list(point)
        if len(values) not in {2, 3}:
            raise ApiError("ENU_POINT_INVALID", repr(values), status=400)
        self._require_projection()
        lon, lat = self._inverse.transform(float(values[0]), float(values[1]))
        up = float(values[2]) if len(values) == 3 else 0.0
        return [float(lon), float(lat), self.anchor.origin_wgs84[2] + up]

    def enu_to_aedifex(self, point: Iterable[float]) -> list[float]:
        values = list(point)
        if len(values) not in {2, 3}:
            raise ApiError("ENU_POINT_INVALID", repr(values), status=400)
        east, north = float(values[0]), float(values[1])
        up = float(values[2]) if len(values) == 3 else 0.0
        # Matriz ortogonal auto-inversa: base X=leste/Z=sul e rotação declarada.
        x = self._cos * east + self._sin * north
        z = self._sin * east - self._cos * north
        y = up + self.anchor.vertical_offset_m
        return [x, y, z]

    def aedifex_to_enu(self, point: Iterable[float]) -> list[float]:
        values = list(point)
        if len(values) != 3:
            raise ApiError("AEDIFEX_POINT_INVALID", repr(values), status=400)
        x, y, z = (float(item) for item in values)
        east = self._cos * x + self._sin * z
        north = self._sin * x - self._cos * z
        up = y - self.anchor.vertical_offset_m
        return [east, north, up]

    def wgs84_to_aedifex(self, point: Iterable[float]) -> list[float]:
        return self.enu_to_aedifex(self.wgs84_to_enu(point))

    def aedifex_to_wgs84(self, point: Iterable[float]) -> list[float]:
        return self.enu_to_wgs84(self.aedifex_to_enu(point))

    def plan_enu_to_aedifex(self, points: list[list[float]], elevation_m: float = 0.0) -> list[list[float]]:
        return [self.enu_to_aedifex([point[0], point[1], elevation_m]) for point in points]

    def plan_wgs84_to_aedifex(self, points: list[list[float]]) -> list[list[float]]:
        return [self.wgs84_to_aedifex(point) for point in points]


class GeoModelBridge:
    """Constrói o pacote de contexto que entra no floorplanner.

    O pacote contém apenas dados locais/proveniência e não duplica a malha
    autoritativa do mundo. O Aedifex recebe um recorte de trabalho imutável e
    devolve revisões do scene graph do edifício.
    """

    def __init__(self, schemas: SchemaRegistry, root: Path | None = None):
        self.schemas = schemas
        self.root = root.resolve() if root is not None else None

    @staticmethod
    def _role(value: object) -> str:
        text = str(value or "").lower()
        for token, role in (
            ("terrain", "terrain"), ("relevo", "terrain"),
            ("road", "roads"), ("via", "roads"),
            ("veget", "vegetation"), ("tree", "vegetation"),
            ("building", "buildings"), ("house", "buildings"), ("edific", "buildings"),
            ("imagery", "imagery"), ("image", "imagery"),
            ("survey", "survey"), ("scan", "survey"),
        ):
            if token in text:
                return role
        return "surroundings"

    def _verify_local_asset(self, raw_path: object, expected_hash: object) -> tuple[str, str]:
        value = str(raw_path or "").strip().replace("\\", "/")
        if not value or re.match(r"^[a-z][a-z0-9+.-]*:", value, re.I) or value.startswith("//"):
            raise ApiError("CONTEXT_LAYER_PATH_INVALID", value or "<empty>", status=400)
        normalized = "/" + value.lstrip("/")
        if any(part in {"", ".", ".."} for part in normalized.split("/")[1:]):
            raise ApiError("CONTEXT_LAYER_PATH_INVALID", normalized, status=400)
        digest = str(expected_hash or "").lower()
        if not re.fullmatch(r"[a-f0-9]{64}", digest):
            raise ApiError("CONTEXT_LAYER_HASH_REQUIRED", normalized, status=400)
        if self.root is not None:
            candidate = (self.root / normalized.lstrip("/")).resolve()
            try:
                candidate.relative_to(self.root)
            except ValueError as error:
                raise ApiError("CONTEXT_LAYER_PATH_OUTSIDE_ROOT", normalized, status=400) from error
            if not candidate.is_file() or candidate.is_symlink():
                raise ApiError("CONTEXT_LAYER_ASSET_MISSING", normalized, status=409)
            actual = sha256_file(candidate)
            if actual != digest:
                raise ApiError(
                    "CONTEXT_LAYER_HASH_MISMATCH", normalized, status=409,
                    details={"expected": digest, "actual": actual},
                )
        return normalized, digest

    def _normalize_context_layers(
        self,
        values: object,
        transform: GeoModelTransform,
    ) -> list[dict[str, Any]]:
        if values is None:
            return []
        if not isinstance(values, list):
            raise ApiError("CONTEXT_LAYERS_INVALID", "context_layers deve ser array", status=400)
        result: list[dict[str, Any]] = []
        seen_ids: set[str] = set()
        seen_assets: set[tuple[str, str]] = set()
        for index, raw in enumerate(values):
            if not isinstance(raw, dict):
                raise ApiError("CONTEXT_LAYER_INVALID", f"index={index}", status=400)
            asset_path, digest = self._verify_local_asset(
                raw.get("asset_path") or raw.get("path") or raw.get("uri"),
                raw.get("sha256") or raw.get("content_hash"),
            )
            fmt = str(raw.get("format") or Path(asset_path).suffix.lstrip(".")).lower()
            if fmt not in {"glb", "geojson"}:
                raise ApiError("CONTEXT_LAYER_FORMAT_UNSUPPORTED", fmt, status=422)
            layer_id = str(raw.get("id") or f"context:{digest[:20]}:{index}")
            if layer_id in seen_ids:
                raise ApiError("CONTEXT_LAYER_ID_DUPLICATE", layer_id, status=409)
            role = self._role(raw.get("role") or raw.get("owner") or raw.get("generator"))
            if (asset_path, digest) in seen_assets:
                continue
            seen_ids.add(layer_id)
            seen_assets.add((asset_path, digest))

            explicit = raw.get("transform") if isinstance(raw.get("transform"), dict) else {}
            coordinate_space = str(raw.get("coordinate_space") or "AEDIFEX_LOCAL")
            if coordinate_space not in {"AEDIFEX_LOCAL", "ENU_LOCAL"}:
                raise ApiError("CONTEXT_LAYER_COORDINATE_SPACE_UNSUPPORTED", coordinate_space, status=422)
            position = explicit.get("position_m")
            rotation = explicit.get("rotation_euler_rad")
            scale = explicit.get("scale")

            placement = raw.get("geo_placement") or raw.get("placement")
            if isinstance(placement, dict) and placement.get("lon") is not None and placement.get("lat") is not None:
                position = transform.wgs84_to_aedifex([
                    float(placement["lon"]), float(placement["lat"]),
                    float(placement.get("alt", transform.anchor.origin_wgs84[2])),
                ])
                heading = float(placement.get("heading", placement.get("rumo", 0.0)))
                rotation = [0.0, radians(-heading), 0.0]
                scalar = float(placement.get("scale", 1.0))
                scale = [scalar, scalar, scalar]
                coordinate_space = "AEDIFEX_LOCAL"

            def vector(value: object, fallback: list[float], *, positive: bool = False) -> list[float]:
                if not isinstance(value, (list, tuple)) or len(value) != 3:
                    return fallback
                numbers = [float(item) for item in value]
                if positive and any(item <= 0 for item in numbers):
                    raise ApiError("CONTEXT_LAYER_SCALE_INVALID", repr(numbers), status=400)
                return numbers

            item = {
                "id": layer_id,
                "role": role,
                "format": fmt,
                "asset_path": asset_path,
                "sha256": digest,
                "readonly": True,
                "visible": bool(raw.get("visible", True)),
                "opacity": min(1.0, max(0.0, float(raw.get("opacity", 1.0)))),
                "coordinate_space": coordinate_space,
                "lod": str(raw.get("lod") or "reference"),
                "transform": {
                    "position_m": vector(position, [0.0, 0.0, 0.0]),
                    "rotation_euler_rad": vector(rotation, [0.0, 0.0, 0.0]),
                    "scale": vector(scale, [1.0, 1.0, 1.0], positive=True),
                },
                "provenance": raw.get("provenance") if isinstance(raw.get("provenance"), dict) else {
                    "owner": raw.get("owner"),
                    "generator": raw.get("generator"),
                    "manifest_hash": raw.get("manifest_hash"),
                },
                "metadata": raw.get("metadata") if isinstance(raw.get("metadata"), dict) else {},
            }
            self.schemas.validate("context-layer.schema.json", item)
            result.append(item)
        return sorted(result, key=lambda item: (item["role"], item["id"]))

    def build_context(self, payload: dict[str, Any]) -> dict[str, Any]:
        active = payload.get("active_region")
        if not isinstance(active, dict):
            raise ApiError("ACTIVE_REGION_REQUIRED", "Selecione uma Região Ativa antes do Floorplanner", status=409)
        request = active.get("request")
        context = active.get("context")
        if not isinstance(request, dict) or not isinstance(context, dict):
            raise ApiError("ACTIVE_REGION_INCOMPLETE", "request/context ausentes", status=400)
        origin = context.get("origin_wgs84")
        if not isinstance(origin, list) or len(origin) < 2:
            focus = request.get("focus", {})
            origin = [focus.get("lon"), focus.get("lat"), 0.0]
        if origin[0] is None or origin[1] is None:
            raise ApiError("ACTIVE_REGION_ORIGIN_MISSING", "Origem geográfica ausente", status=400)
        anchor_value = {
            "origin_wgs84": [float(origin[0]), float(origin[1]), float(origin[2] if len(origin) > 2 else 0.0)],
            "north_rotation_deg": float(payload.get("north_rotation_deg", 0.0)),
            "vertical_offset_m": float(payload.get("vertical_offset_m", 0.0)),
            "axis_policy": "AEDIFEX_X_EAST_Y_UP_Z_SOUTH",
        }
        self.schemas.validate("geo-anchor.schema.json", anchor_value)
        anchor = GeoAnchor.from_dict(anchor_value)
        transform = GeoModelTransform(anchor)

        selection = payload.get("selection") or {}
        parcel_enu = selection.get("parcel_polygon_enu_m") or []
        parcel_wgs84 = selection.get("parcel_polygon_wgs84") or request.get("polygon_wgs84") or []
        if parcel_enu and parcel_wgs84:
            raise ApiError("PARCEL_COORDINATE_CONFLICT", "Informe polígono ENU ou WGS84, não ambos", status=400)
        if parcel_wgs84:
            parcel_aed = transform.plan_wgs84_to_aedifex(parcel_wgs84)
        elif parcel_enu:
            parcel_aed = transform.plan_enu_to_aedifex(parcel_enu)
        else:
            parcel_aed = []

        package = {
            "schema_version": 1,
            "region_id": str(request.get("region_id") or context.get("region_id")),
            "generation_epoch": int(active.get("generation_epoch", request.get("generation_epoch", 0))),
            "scale": str(request.get("scale")),
            "geo_anchor": anchor.to_dict(),
            "selection": {
                "selection_id": str(selection.get("selection_id") or request.get("region_id")),
                "kind": str(selection.get("kind") or request.get("scale")),
                "bbox_wgs84": selection.get("bbox_wgs84") or request.get("bbox_wgs84"),
                "parcel_polygon_wgs84": parcel_wgs84,
                "parcel_polygon_enu_m": parcel_enu,
                "parcel_polygon_aedifex_xyz_m": parcel_aed,
                "source": selection.get("source") or {"kind": "active_region", "estimated": not bool(parcel_enu or parcel_wgs84)},
            },
            "terrain": context.get("terrain", {}),
            "urban": context.get("urban", {}),
            "environment": context.get("environment", {}),
            "regional_profiles": payload.get("regional_profiles", []),
            "constraints": payload.get("constraints", {}),
            "source_packages": context.get("source_packages", []),
            "reference_media": payload.get("reference_media", []),
            "context_layers": self._normalize_context_layers(payload.get("context_layers", []), transform),
            "warnings": list(context.get("warnings", [])),
        }
        package["context_hash"] = canonical_json_hash(package)
        self.schemas.validate("modeling-context-package.schema.json", package)
        return package
