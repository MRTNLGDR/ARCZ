from __future__ import annotations

import math
from typing import Any

from .errors import ApiError
from .schema_validation import SchemaRegistry

RUST_GENERATOR_KINDS = frozenset({
    "terrain.generate", "parcels.generate", "roads.generate", "houses.generate",
    "buildings.generate", "vegetation.generate", "materials.generate", "tiles.generate",
})


class GeneratorContracts:
    """Pré-validação semântica antes de abrir subprocesso Rust.

    O schema impede campos silenciosamente ignorados. As regras abaixo garantem
    que um job sem geodado não vire uma cidade de demonstração. Fallbacks só são
    aceitos quando o usuário os habilitou explicitamente no request.
    """

    def __init__(self, schemas: SchemaRegistry):
        self.schemas = schemas

    def validate_job(self, kind: str, request: dict[str, Any]) -> dict[str, Any]:
        if kind == "region.context.generate":
            context = request.get("region", {}).get("context")
            if not isinstance(context, dict):
                raise ApiError("GENERATOR_INPUT_MISSING", "region.context obrigatório", status=400)
            return request
        if kind not in RUST_GENERATOR_KINDS:
            return request
        params = self.normalize_params(request)
        self.reject_non_finite(params)
        self.schemas.validate("generator-parameters.schema.json", params)
        self.validate_semantics(kind, params)
        return request

    @staticmethod
    def normalize_params(request: dict[str, Any]) -> dict[str, Any]:
        raw = request.get("params", {})
        if not isinstance(raw, dict):
            raise ApiError("GENERATOR_PARAMS_INVALID", "request.params precisa ser objeto", status=400)
        params = dict(raw)
        inputs = params.pop("inputs", None)
        if inputs is not None:
            if not isinstance(inputs, dict):
                raise ApiError("GENERATOR_INPUTS_INVALID", "params.inputs precisa ser objeto", status=400)
            duplicates = sorted(set(params).intersection(inputs))
            if duplicates:
                raise ApiError("GENERATOR_FIELD_DUPLICATED", "Campo duplicado entre params e params.inputs",
                               status=400, details={"fields": duplicates})
            params.update(inputs)
        params.pop("seed", None)
        return params

    @staticmethod
    def reject_non_finite(value: Any, path: str = "$") -> None:
        if isinstance(value, float) and not math.isfinite(value):
            raise ApiError("GENERATOR_NON_FINITE", f"Número não finito em {path}", status=400)
        if isinstance(value, list):
            for index, item in enumerate(value):
                GeneratorContracts.reject_non_finite(item, f"{path}[{index}]")
        elif isinstance(value, dict):
            for key, item in value.items():
                GeneratorContracts.reject_non_finite(item, f"{path}.{key}")

    @staticmethod
    def validate_semantics(kind: str, params: dict[str, Any]) -> None:
        def nonempty(name: str) -> bool:
            return isinstance(params.get(name), list) and bool(params[name])

        if kind == "terrain.generate":
            explicit = isinstance(params.get("terrain"), dict)
            fallback = params.get("allow_flat_terrain_fallback") is True and isinstance(params.get("flat_terrain"), dict)
            if not explicit and not fallback:
                raise ApiError("GENERATOR_INPUT_MISSING",
                               "terrain.generate exige DEM/grid explícito ou fallback plano explicitamente autorizado",
                               status=400)
            if explicit:
                terrain = params["terrain"]
                expected = int(terrain["columns"]) * int(terrain["rows"])
                actual = len(terrain["heights_m"])
                if expected != actual:
                    raise ApiError("TERRAIN_GRID_SIZE_MISMATCH", "rows*columns difere de heights_m",
                                   status=400, details={"expected": expected, "actual": actual})
        elif kind == "parcels.generate" and not nonempty("parcels"):
            raise ApiError("GENERATOR_INPUT_MISSING", "parcels.generate exige parcels", status=400)
        elif kind == "roads.generate" and not nonempty("roads"):
            raise ApiError("GENERATOR_INPUT_MISSING", "roads.generate exige roads", status=400)
        elif kind == "houses.generate":
            explicit_houses = any(item.get("category", "house") == "house" for item in params.get("buildings", []))
            estimated = params.get("allow_estimated_infill") is True and nonempty("parcels")
            if not explicit_houses and not estimated:
                raise ApiError("GENERATOR_INPUT_MISSING",
                               "houses.generate exige houses explícitas ou allow_estimated_infill com parcels",
                               status=400)
        elif kind == "buildings.generate":
            explicit = any(item.get("category", "house") != "house" for item in params.get("buildings", []))
            if not explicit:
                raise ApiError("GENERATOR_INPUT_MISSING", "buildings.generate exige edificação não residencial/casa",
                               status=400)
        elif kind == "vegetation.generate" and not nonempty("vegetation_zones"):
            raise ApiError("GENERATOR_INPUT_MISSING", "vegetation.generate exige vegetation_zones", status=400)
        elif kind == "tiles.generate" and not isinstance(params.get("tile_plan"), dict):
            raise ApiError("GENERATOR_INPUT_MISSING", "tiles.generate exige tile_plan", status=400)

        for variant in (v for zone in params.get("vegetation_zones", []) for v in zone.get("variants", [])):
            if variant["scale_min"] > variant["scale_max"]:
                raise ApiError("VEGETATION_SCALE_RANGE_INVALID", variant.get("id", "variant"), status=400)
        if isinstance(params.get("estimated_infill"), dict):
            heights = params["estimated_infill"].get("house_height_m")
            if heights and heights[0] > heights[1]:
                raise ApiError("INFILL_HEIGHT_RANGE_INVALID", "house_height_m mínimo > máximo", status=400)
        if isinstance(params.get("tile_plan"), dict):
            rings = params["tile_plan"].get("rings_m")
            if rings and any(a > b for a, b in zip(rings, rings[1:])):
                raise ApiError("TILE_RINGS_INVALID", "rings_m deve ser crescente", status=400)
