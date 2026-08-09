#!/usr/bin/env python3
from __future__ import annotations

"""ARCZ IFC worker backed by real IfcOpenShell.

Commands:
  validate <file.ifc>
  inspect <file.ifc>
  export-arcz <scene.json> <file.ifc>

No IFC is synthesized without passing through IfcOpenShell. Exported files are
reopened and geometry is checked before success is reported.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path
import sys
from typing import Any

import ifcopenshell
import ifcopenshell.api.aggregate
import ifcopenshell.api.context
import ifcopenshell.api.geometry
import ifcopenshell.api.project
import ifcopenshell.api.root
import ifcopenshell.api.spatial
import ifcopenshell.api.unit
import ifcopenshell.geom
import ifcopenshell.util.placement
import numpy as np

SUPPORTED_SCHEMA = {"IFC4", "IFC4X3"}
SUPPORTED_TYPES = {
    "wall": "IfcWall",
    "slab": "IfcSlab",
    "column": "IfcColumn",
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def open_ifc(path: Path):
    path = path.expanduser().resolve()
    if not path.is_file() or path.suffix.lower() not in {".ifc", ".ifczip"}:
        raise FileNotFoundError(f"IFC file missing/unsupported: {path}")
    model = ifcopenshell.open(str(path))
    if str(model.schema).upper() not in SUPPORTED_SCHEMA:
        raise RuntimeError(f"unsupported IFC schema for ARCZ worker: {model.schema}")
    return path, model


def validate_model(path: Path, model) -> dict[str, Any]:
    projects = model.by_type("IfcProject")
    if len(projects) != 1:
        raise RuntimeError(f"IFC must contain exactly one IfcProject, got {len(projects)}")

    global_ids: list[str] = []
    for entity in model.by_type("IfcRoot"):
        value = getattr(entity, "GlobalId", None)
        if value:
            global_ids.append(str(value))
    if len(global_ids) != len(set(global_ids)):
        raise RuntimeError("IFC contains duplicate GlobalId values")

    products = model.by_type("IfcProduct")
    spatial = model.by_type("IfcSpatialElement")
    geometries = 0
    geometry_errors: list[str] = []
    settings = ifcopenshell.geom.settings()
    for product in products:
        representation = getattr(product, "Representation", None)
        if representation is None:
            continue
        try:
            shape = ifcopenshell.geom.create_shape(settings, product)
            vertices = list(shape.geometry.verts)
            if len(vertices) < 9:
                raise RuntimeError("fewer than 3 geometric vertices")
            if not all(math.isfinite(float(value)) for value in vertices):
                raise RuntimeError("non-finite geometry")
            geometries += 1
        except Exception as error:
            geometry_errors.append(f"#{product.id()} {product.is_a()}: {error}")
    if geometry_errors:
        raise RuntimeError("IFC geometry validation failed: " + " | ".join(geometry_errors[:20]))

    return {
        "ok": True,
        "schema": str(model.schema),
        "ifcopenshell_version": str(ifcopenshell.version),
        "path": str(path),
        "sha256": sha256(path),
        "bytes": path.stat().st_size,
        "entities": len(list(model)),
        "products": len(products),
        "spatial_elements": len(spatial),
        "geometric_products": geometries,
        "classes": {
            name: len(model.by_type(name))
            for name in ("IfcWall", "IfcSlab", "IfcColumn", "IfcDoor", "IfcWindow", "IfcSpace")
        },
    }


def semantic_inspect(path: Path, model) -> dict[str, Any]:
    result = validate_model(path, model)
    elements: list[dict[str, Any]] = []
    for product in model.by_type("IfcProduct"):
        if product.is_a("IfcSpatialElement"):
            continue
        placement = getattr(product, "ObjectPlacement", None)
        matrix = None
        if placement is not None:
            try:
                raw = ifcopenshell.util.placement.get_local_placement(placement)
                matrix = [[float(raw[row, col]) for col in range(4)] for row in range(4)]
            except Exception:
                matrix = None
        elements.append(
            {
                "step_id": product.id(),
                "global_id": getattr(product, "GlobalId", None),
                "ifc_class": product.is_a(),
                "name": getattr(product, "Name", None),
                "tag": getattr(product, "Tag", None),
                "placement": matrix,
                "has_representation": getattr(product, "Representation", None) is not None,
            }
        )
    result["elements"] = elements
    return result


def node_props(node: dict[str, Any]) -> dict[str, Any]:
    merged: dict[str, Any] = {}
    props = node.get("properties")
    if isinstance(props, dict):
        merged.update(props)
    merged.update(node)
    return merged


def finite_number(value: Any, label: str) -> float:
    result = float(value)
    if not math.isfinite(result):
        raise ValueError(f"{label} is not finite")
    return result


def point2(value: Any, label: str) -> tuple[float, float]:
    if not isinstance(value, (list, tuple)) or len(value) < 2:
        raise ValueError(f"{label} must be [x,y]")
    return finite_number(value[0], f"{label}.x"), finite_number(value[1], f"{label}.y")


def positive(value: Any, label: str) -> float:
    result = finite_number(value, label)
    if result <= 0.0:
        raise ValueError(f"{label} must be > 0")
    return result


def create_spatial_tree(model, project_name: str, levels: list[dict[str, Any]]):
    project = ifcopenshell.api.root.create_entity(model, ifc_class="IfcProject", name=project_name)
    length = ifcopenshell.api.unit.add_si_unit(model, unit_type="LENGTHUNIT")
    area = ifcopenshell.api.unit.add_si_unit(model, unit_type="AREAUNIT")
    volume = ifcopenshell.api.unit.add_si_unit(model, unit_type="VOLUMEUNIT")
    ifcopenshell.api.unit.assign_unit(model, units=[length, area, volume])

    model_context = ifcopenshell.api.context.add_context(model, context_type="Model")
    body = ifcopenshell.api.context.add_context(
        model,
        context_type="Model",
        context_identifier="Body",
        target_view="MODEL_VIEW",
        parent=model_context,
    )
    site = ifcopenshell.api.root.create_entity(model, ifc_class="IfcSite", name="ARCZ Site")
    building = ifcopenshell.api.root.create_entity(model, ifc_class="IfcBuilding", name=project_name)
    ifcopenshell.api.aggregate.assign_object(model, relating_object=project, products=[site])
    ifcopenshell.api.aggregate.assign_object(model, relating_object=site, products=[building])

    storeys: dict[str, Any] = {}
    if not levels:
        levels = [{"id": "level-0", "name": "Ground Floor", "elevation": 0.0}]
    for index, level in enumerate(levels):
        props = node_props(level)
        node_id = str(level.get("id") or f"level-{index}")
        elevation = finite_number(props.get("elevation", 0.0), f"{node_id}.elevation")
        storey = ifcopenshell.api.root.create_entity(
            model,
            ifc_class="IfcBuildingStorey",
            name=str(level.get("name") or props.get("name") or node_id),
        )
        storey.Tag = node_id
        storey.Elevation = elevation
        placement = np.identity(4)
        placement[2, 3] = elevation
        ifcopenshell.api.geometry.edit_object_placement(model, product=storey, matrix=placement, is_si=True)
        ifcopenshell.api.aggregate.assign_object(model, relating_object=building, products=[storey])
        storeys[node_id] = storey
    return project, body, storeys


def nearest_storey(node: dict[str, Any], storeys: dict[str, Any]):
    parent = str(node.get("parentId") or node.get("parent_id") or "")
    if parent in storeys:
        return storeys[parent]
    return next(iter(storeys.values()))


def export_scene(scene_path: Path, output_path: Path) -> dict[str, Any]:
    scene_path = scene_path.expanduser().resolve()
    output_path = output_path.expanduser().resolve()
    data = json.loads(scene_path.read_text(encoding="utf-8"))
    scene = data.get("scene_document") if isinstance(data.get("scene_document"), dict) else data
    raw_nodes = scene.get("nodes")
    if isinstance(raw_nodes, dict):
        nodes = [dict(value, id=value.get("id", key)) for key, value in raw_nodes.items() if isinstance(value, dict)]
    elif isinstance(raw_nodes, list):
        nodes = [value for value in raw_nodes if isinstance(value, dict)]
    else:
        raise ValueError("ARCZ scene_document.nodes must be object/list")

    levels = [node for node in nodes if str(node.get("type", "")).lower() in {"level", "storey"}]
    model = ifcopenshell.api.project.create_file(version="IFC4")
    project_name = str(scene.get("name") or data.get("project_name") or "ARCZ Project")
    _, body, storeys = create_spatial_tree(model, project_name, levels)

    exported: list[dict[str, Any]] = []
    for index, node in enumerate(nodes):
        kind = str(node.get("type") or "").lower()
        if kind not in SUPPORTED_TYPES:
            continue
        props = node_props(node)
        node_id = str(node.get("id") or f"{kind}-{index}")
        name = str(node.get("name") or props.get("name") or node_id)
        storey = nearest_storey(node, storeys)
        elevation = float(getattr(storey, "Elevation", 0.0) or 0.0)

        if kind == "wall":
            start = point2(props.get("start"), f"{node_id}.start")
            end = point2(props.get("end"), f"{node_id}.end")
            height = positive(props.get("height", 2.8), f"{node_id}.height")
            thickness = positive(props.get("thickness", 0.15), f"{node_id}.thickness")
            element = ifcopenshell.api.root.create_entity(model, ifc_class="IfcWall", name=name)
            element.Tag = node_id
            representation = ifcopenshell.api.geometry.create_2pt_wall(
                model,
                element=element,
                context=body,
                p1=start,
                p2=end,
                elevation=elevation,
                height=height,
                thickness=thickness,
                is_si=True,
            )
            if getattr(element, "Representation", None) is None and representation is not None:
                ifcopenshell.api.geometry.assign_representation(
                    model, product=element, representation=representation
                )

        elif kind == "slab":
            polygon = props.get("polygon") or props.get("points")
            if not isinstance(polygon, list) or len(polygon) < 3:
                raise ValueError(f"{node_id}.polygon requires at least 3 points")
            points = [point2(value, f"{node_id}.polygon") for value in polygon]
            origin = points[0]
            local = [(x - origin[0], y - origin[1]) for x, y in points]
            depth = positive(props.get("thickness", props.get("depth", 0.15)), f"{node_id}.thickness")
            element = ifcopenshell.api.root.create_entity(model, ifc_class="IfcSlab", name=name)
            element.Tag = node_id
            representation = ifcopenshell.api.geometry.add_slab_representation(
                model, context=body, depth=depth, polyline=local
            )
            ifcopenshell.api.geometry.assign_representation(model, product=element, representation=representation)
            matrix = np.identity(4)
            matrix[0, 3], matrix[1, 3], matrix[2, 3] = origin[0], origin[1], elevation
            ifcopenshell.api.geometry.edit_object_placement(model, product=element, matrix=matrix, is_si=True)

        else:  # column
            position = point2(props.get("position", [0.0, 0.0]), f"{node_id}.position")
            height = positive(props.get("height", 2.8), f"{node_id}.height")
            width = positive(props.get("width", 0.3), f"{node_id}.width")
            depth = positive(props.get("depth", width), f"{node_id}.depth")
            element = ifcopenshell.api.root.create_entity(model, ifc_class="IfcColumn", name=name)
            element.Tag = node_id
            representation = ifcopenshell.api.geometry.add_wall_representation(
                model, context=body, length=width, height=height, thickness=depth
            )
            ifcopenshell.api.geometry.assign_representation(model, product=element, representation=representation)
            matrix = np.identity(4)
            matrix[0, 3] = position[0] - width * 0.5
            matrix[1, 3] = position[1] - depth * 0.5
            matrix[2, 3] = elevation
            ifcopenshell.api.geometry.edit_object_placement(model, product=element, matrix=matrix, is_si=True)

        ifcopenshell.api.spatial.assign_container(model, relating_structure=storey, products=[element])
        exported.append({"arcz_id": node_id, "ifc_class": element.is_a(), "global_id": element.GlobalId})

    if not exported:
        raise RuntimeError("ARCZ scene contains no IFC-exportable wall/slab/column nodes")
    output_path.parent.mkdir(parents=True, exist_ok=True)
    model.write(str(output_path))
    reopened_path, reopened = open_ifc(output_path)
    validation = validate_model(reopened_path, reopened)
    validation.update({"exported": exported, "source_scene": str(scene_path)})
    return validation


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("validate", "inspect"):
        item = sub.add_parser(name)
        item.add_argument("ifc", type=Path)
    export = sub.add_parser("export-arcz")
    export.add_argument("scene", type=Path)
    export.add_argument("ifc", type=Path)
    args = parser.parse_args()

    if str(ifcopenshell.version) != "0.8.5":
        raise RuntimeError(f"ARCZ IFC worker requires IfcOpenShell 0.8.5, found {ifcopenshell.version}")

    if args.command == "export-arcz":
        result = export_scene(args.scene, args.ifc)
    else:
        path, model = open_ifc(args.ifc)
        result = validate_model(path, model) if args.command == "validate" else semantic_inspect(path, model)
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
