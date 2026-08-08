"""Worker Blender local para o ARCZ Floorplanner/Aedifex.

Estratégia de fidelidade:
1. importa o GLB real exportado pelo viewport Aedifex quando presente;
2. usa o scene graph apenas como fonte semântica e fallback paramétrico;
3. nunca baixa asset, textura, HDRI ou modelo;
4. produz beauty e passes técnicos reais, mais manifestos de decodificação.

O fallback não tenta fingir paridade integral com o renderer Aedifex. Tipos que
não podem ser reconstruídos fielmente sem o GLB são omitidos e registrados em
``warnings``/``unsupported_node_types``.
"""
from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
import hashlib
import json
import math
import pathlib
import shutil
import sys
import time
from typing import Any, Iterable

import bpy
from mathutils import Vector


QUALITY_SAMPLES = {
    "draft": 16,
    "preview": 64,
    "balanced": 128,
    "high": 256,
    "ultra": 512,
}

SEMANTIC_IDS = {
    "unknown": 1,
    "terrain": 2,
    "site": 3,
    "building": 4,
    "wall": 10,
    "fence": 11,
    "door": 12,
    "window": 13,
    "slab": 14,
    "ceiling": 15,
    "roof": 16,
    "stair": 17,
    "column": 18,
    "railing": 19,
    "cabinet": 20,
    "shelf": 21,
    "item": 22,
    "tree": 30,
    "flower": 31,
    "grass": 32,
    "pipe": 40,
    "duct": 41,
    "hvac": 42,
    "electrical": 43,
}


@dataclass
class BuildState:
    nodes: dict[str, dict[str, Any]]
    objects_by_node: dict[str, bpy.types.Object]
    warnings: list[str]
    unsupported: Counter
    object_index_map: dict[str, dict[str, Any]]
    material_index_map: dict[str, dict[str, Any]]
    next_object_index: int = 1
    next_material_index: int = 1


def args() -> tuple[pathlib.Path, pathlib.Path]:
    try:
        marker = sys.argv.index("--")
    except ValueError as error:
        raise RuntimeError("Blender worker requires -- request.json output_dir") from error
    if len(sys.argv) <= marker + 2:
        raise RuntimeError("request/output arguments missing")
    return pathlib.Path(sys.argv[marker + 1]).resolve(), pathlib.Path(sys.argv[marker + 2]).resolve()


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def finite(value: Any, fallback: float = 0.0) -> float:
    try:
        number = float(value)
    except (TypeError, ValueError):
        return fallback
    return number if math.isfinite(number) else fallback


def vector(value: Any, length: int, fallback: Iterable[float]) -> list[float]:
    fallback_values = list(fallback)
    if not isinstance(value, (list, tuple)) or len(value) < length:
        return fallback_values[:length]
    result = [finite(value[index], fallback_values[index]) for index in range(length)]
    return result


def props(node: dict[str, Any]) -> dict[str, Any]:
    """Merges known Aedifex storage styles without mutating the document."""
    result: dict[str, Any] = {}
    for key in ("metadata", "properties", "data", "params"):
        value = node.get(key)
        if isinstance(value, dict):
            result.update(value)
    for key, value in node.items():
        if key not in {"metadata", "properties", "data", "params", "children"}:
            result.setdefault(key, value)
    return result


def node_type(node: dict[str, Any]) -> str:
    return str(node.get("type") or node.get("kind") or "unknown").lower()


def node_name(node: dict[str, Any]) -> str:
    return str(node.get("name") or node.get("id") or node_type(node))


def clear_scene() -> None:
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    for datablocks in (bpy.data.meshes, bpy.data.curves, bpy.data.cameras, bpy.data.lights):
        for datablock in list(datablocks):
            if datablock.users == 0:
                datablocks.remove(datablock)


def make_material(name: str, color: tuple[float, float, float], roughness: float = 0.65,
                  metallic: float = 0.0, alpha: float = 1.0) -> bpy.types.Material:
    material = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    material.use_nodes = True
    material.diffuse_color = (*color, alpha)
    nodes = material.node_tree.nodes
    bsdf = nodes.get("Principled BSDF")
    if bsdf:
        bsdf.inputs["Base Color"].default_value = (*color, alpha)
        bsdf.inputs["Roughness"].default_value = roughness
        bsdf.inputs["Metallic"].default_value = metallic
        if "Alpha" in bsdf.inputs:
            bsdf.inputs["Alpha"].default_value = alpha
        if alpha < 1:
            if hasattr(material, "surface_render_method"):
                material.surface_render_method = "DITHERED"
            elif hasattr(material, "blend_method"):
                material.blend_method = "BLEND"
    return material


def ensure_material_index(state: BuildState, material: bpy.types.Material) -> int:
    key = material.name
    existing = state.material_index_map.get(key)
    if existing:
        return int(existing["index"])
    index = state.next_material_index
    state.next_material_index += 1
    material.pass_index = index
    state.material_index_map[key] = {"index": index, "material": key}
    return index


def assign_object_semantics(state: BuildState, obj: bpy.types.Object, node: dict[str, Any], semantic: str | None = None) -> None:
    identifier = str(node.get("id") or obj.name)
    semantic_name = semantic or node_type(node)
    semantic_id = SEMANTIC_IDS.get(semantic_name, SEMANTIC_IDS["unknown"])
    object_index = state.next_object_index
    state.next_object_index += 1
    obj.pass_index = object_index
    obj["arcz_node_id"] = identifier
    obj["arcz_node_type"] = node_type(node)
    obj["arcz_semantic"] = semantic_name
    obj["arcz_semantic_id"] = semantic_id
    state.object_index_map[str(object_index)] = {
        "node_id": identifier,
        "node_type": node_type(node),
        "semantic": semantic_name,
        "semantic_id": semantic_id,
        "object_name": obj.name,
    }
    state.objects_by_node[identifier] = obj
    for material in obj.data.materials if getattr(obj, "data", None) and hasattr(obj.data, "materials") else []:
        if material:
            ensure_material_index(state, material)


def level_elevations(nodes: dict[str, dict[str, Any]]) -> dict[str, float]:
    elevations: dict[str, float] = {}
    for identifier, node in nodes.items():
        if node_type(node) != "level":
            continue
        p = props(node)
        elevations[identifier] = finite(
            p.get("elevation", p.get("elevation_m", p.get("height", 0.0))),
            0.0,
        )
    return elevations


def parent_level_id(node: dict[str, Any], nodes: dict[str, dict[str, Any]]) -> str | None:
    p = props(node)
    direct = p.get("levelId") or p.get("level_id")
    if direct and str(direct) in nodes:
        return str(direct)
    current = str(node.get("parentId") or node.get("parent_id") or "")
    visited: set[str] = set()
    while current and current not in visited:
        visited.add(current)
        parent = nodes.get(current)
        if not parent:
            break
        if node_type(parent) == "level":
            return current
        current = str(parent.get("parentId") or parent.get("parent_id") or "")
    return None


def node_base_elevation(node: dict[str, Any], nodes: dict[str, dict[str, Any]], elevations: dict[str, float]) -> float:
    p = props(node)
    explicit = p.get("elevation", p.get("base_elevation", p.get("y")))
    level_id = parent_level_id(node, nodes)
    level = elevations.get(level_id or "", 0.0)
    return level + finite(explicit, 0.0)


def add_cube(name: str, location: tuple[float, float, float], dimensions: tuple[float, float, float],
             material: bpy.types.Material | None = None) -> bpy.types.Object:
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=location)
    obj = bpy.context.object
    obj.name = name
    obj.dimensions = tuple(max(0.001, finite(value, 0.001)) for value in dimensions)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    if material:
        obj.data.materials.append(material)
    return obj


def wall_endpoints(p: dict[str, Any]) -> tuple[list[float], list[float]]:
    start = p.get("start") or p.get("startPoint") or p.get("from") or [0.0, 0.0]
    end = p.get("end") or p.get("endPoint") or p.get("to") or [1.0, 0.0]
    start = vector(start, 2, [0.0, 0.0])
    end = vector(end, 2, [1.0, 0.0])
    return start, end


def add_wall(state: BuildState, node: dict[str, Any], material: bpy.types.Material,
             elevations: dict[str, float]) -> bpy.types.Object | None:
    p = props(node)
    start, end = wall_endpoints(p)
    dx, dz = end[0] - start[0], end[1] - start[1]
    length = math.hypot(dx, dz)
    if length <= 0.001:
        state.warnings.append(f"WALL_ZERO_LENGTH:{node.get('id')}")
        return None
    thickness = max(0.03, finite(p.get("thickness", p.get("width", 0.15)), 0.15))
    height = max(0.1, finite(p.get("height", p.get("wallHeight", 3.0)), 3.0))
    base = node_base_elevation(node, state.nodes, elevations)
    obj = add_cube(
        node_name(node),
        ((start[0] + end[0]) / 2, base + height / 2, (start[1] + end[1]) / 2),
        (length, height, thickness),
        material,
    )
    obj.rotation_euler[2] = math.atan2(dz, dx)
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=False)
    obj.select_set(False)
    assign_object_semantics(state, obj, node, "fence" if node_type(node) == "fence" else "wall")
    return obj


def polygon_points(p: dict[str, Any]) -> list[list[float]]:
    raw = p.get("polygon") or p.get("points") or p.get("outline") or p.get("vertices") or []
    result: list[list[float]] = []
    if isinstance(raw, list):
        for item in raw:
            if isinstance(item, dict):
                result.append([finite(item.get("x")), finite(item.get("z", item.get("y")))])
            elif isinstance(item, (list, tuple)) and len(item) >= 2:
                result.append([finite(item[0]), finite(item[1])])
    if len(result) > 2 and result[0] == result[-1]:
        result.pop()
    return result


def add_polygon_prism(state: BuildState, node: dict[str, Any], material: bpy.types.Material,
                      elevations: dict[str, float], *, semantic: str, default_thickness: float,
                      default_elevation_offset: float = 0.0) -> bpy.types.Object | None:
    p = props(node)
    poly = polygon_points(p)
    if len(poly) < 3:
        state.warnings.append(f"POLYGON_MISSING:{node.get('id')}:{node_type(node)}")
        return None
    base = node_base_elevation(node, state.nodes, elevations) + finite(
        p.get("offset", p.get("elevationOffset", default_elevation_offset)), default_elevation_offset
    )
    thickness = max(0.005, finite(p.get("thickness", default_thickness), default_thickness))
    vertices = [(x, base, z) for x, z in poly]
    mesh = bpy.data.meshes.new(f"{node.get('id', semantic)}_mesh")
    mesh.from_pydata(vertices, [], [list(range(len(vertices)))])
    mesh.update(calc_edges=True)
    obj = bpy.data.objects.new(node_name(node), mesh)
    bpy.context.collection.objects.link(obj)
    obj.data.materials.append(material)
    solid = obj.modifiers.new("ARCZ thickness", "SOLIDIFY")
    solid.thickness = thickness
    solid.offset = 0.0
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    try:
        bpy.ops.object.modifier_apply(modifier=solid.name)
    except RuntimeError as error:
        state.warnings.append(f"SOLIDIFY_FAILED:{node.get('id')}:{error}")
    obj.select_set(False)
    assign_object_semantics(state, obj, node, semantic)
    return obj


def wall_local_position(opening: dict[str, Any], wall: dict[str, Any]) -> tuple[Vector, float, float]:
    op = props(opening)
    wp = props(wall)
    start, end = wall_endpoints(wp)
    direction = Vector((end[0] - start[0], 0.0, end[1] - start[1]))
    length = max(direction.length, 0.001)
    direction.normalize()
    normal = Vector((-direction.z, 0.0, direction.x))
    raw_t = op.get("wallT", op.get("t", op.get("positionAlongWall", op.get("offset", 0.5))))
    t = finite(raw_t, 0.5)
    if abs(t) > 1.0:
        t = t / length
    t = min(1.0, max(0.0, t))
    center = Vector((start[0], 0.0, start[1])) + direction * (length * t)
    angle = math.atan2(direction.z, direction.x)
    return center, angle, max(0.03, finite(wp.get("thickness", 0.15), 0.15))


def add_opening(state: BuildState, node: dict[str, Any], elevations: dict[str, float],
                wall_material: bpy.types.Material, frame_material: bpy.types.Material,
                glass_material: bpy.types.Material) -> None:
    p = props(node)
    wall_id = str(p.get("wallId") or p.get("wall_id") or node.get("parentId") or "")
    wall_node = state.nodes.get(wall_id)
    wall_obj = state.objects_by_node.get(wall_id)
    if not wall_node or not wall_obj:
        state.warnings.append(f"OPENING_WALL_MISSING:{node.get('id')}:{wall_id}")
        return
    center, angle, wall_thickness = wall_local_position(node, wall_node)
    kind = node_type(node)
    width = max(0.2, finite(p.get("width", 0.9 if kind == "door" else 1.2), 1.0))
    height = max(0.2, finite(p.get("height", 2.1 if kind == "door" else 1.2), 1.2))
    sill = 0.0 if kind == "door" else max(0.0, finite(p.get("sillHeight", p.get("sill_height", 0.9)), 0.9))
    base = node_base_elevation(wall_node, state.nodes, elevations)
    cutter = add_cube(
        f"__cut_{node.get('id')}",
        (center.x, base + sill + height / 2, center.z),
        (width, height, wall_thickness * 3.0),
    )
    cutter.rotation_euler[2] = angle
    boolean = wall_obj.modifiers.new(f"Opening {node.get('id')}", "BOOLEAN")
    boolean.operation = "DIFFERENCE"
    boolean.solver = "EXACT"
    boolean.object = cutter
    bpy.ops.object.select_all(action="DESELECT")
    wall_obj.select_set(True)
    bpy.context.view_layer.objects.active = wall_obj
    try:
        bpy.ops.object.modifier_apply(modifier=boolean.name)
    except RuntimeError as error:
        state.warnings.append(f"OPENING_BOOLEAN_FAILED:{node.get('id')}:{error}")
    wall_obj.select_set(False)
    bpy.data.objects.remove(cutter, do_unlink=True)

    frame_depth = wall_thickness * 1.05
    frame_width = max(0.03, finite(p.get("frameThickness", 0.06), 0.06))
    components = [
        ((center.x, base + sill + height - frame_width / 2, center.z), (width, frame_width, frame_depth)),
        ((center.x - math.cos(angle) * (width / 2 - frame_width / 2), base + sill + height / 2, center.z - math.sin(angle) * (width / 2 - frame_width / 2)), (frame_width, height, frame_depth)),
        ((center.x + math.cos(angle) * (width / 2 - frame_width / 2), base + sill + height / 2, center.z + math.sin(angle) * (width / 2 - frame_width / 2)), (frame_width, height, frame_depth)),
    ]
    if kind == "window":
        components.append(((center.x, base + sill + frame_width / 2, center.z), (width, frame_width, frame_depth)))
    parent = bpy.data.objects.new(node_name(node), None)
    bpy.context.collection.objects.link(parent)
    parent["arcz_node_id"] = str(node.get("id"))
    parent["arcz_node_type"] = kind
    for index, (location, dimensions) in enumerate(components):
        part = add_cube(f"{node_name(node)} frame {index + 1}", location, dimensions, frame_material)
        part.rotation_euler[2] = angle
        part.parent = parent
        assign_object_semantics(state, part, {**node, "id": f"{node.get('id')}:frame:{index}"}, kind)
    if kind == "window":
        glass_obj = add_cube(
            f"{node_name(node)} glass",
            (center.x, base + sill + height / 2, center.z),
            (max(0.05, width - frame_width * 2), max(0.05, height - frame_width * 2), max(0.005, frame_depth * 0.08)),
            glass_material,
        )
        glass_obj.rotation_euler[2] = angle
        glass_obj.parent = parent
        assign_object_semantics(state, glass_obj, {**node, "id": f"{node.get('id')}:glass"}, kind)
    elif kind == "door":
        leaf = add_cube(
            f"{node_name(node)} leaf",
            (center.x, base + height / 2, center.z),
            (max(0.05, width - frame_width * 2), max(0.05, height - frame_width), max(0.02, frame_depth * 0.25)),
            wall_material,
        )
        leaf.rotation_euler[2] = angle
        leaf.parent = parent
        assign_object_semantics(state, leaf, {**node, "id": f"{node.get('id')}:leaf"}, kind)


def add_column(state: BuildState, node: dict[str, Any], material: bpy.types.Material,
               elevations: dict[str, float]) -> None:
    p = props(node)
    position = vector(p.get("position", [p.get("x", 0), p.get("z", 0)]), 2, [0, 0])
    base = node_base_elevation(node, state.nodes, elevations)
    height = max(0.1, finite(p.get("height", 3.0), 3.0))
    radius = finite(p.get("radius"), 0.0)
    if radius > 0:
        bpy.ops.mesh.primitive_cylinder_add(vertices=32, radius=radius, depth=height,
                                            location=(position[0], base + height / 2, position[1]))
        obj = bpy.context.object
        obj.name = node_name(node)
        obj.data.materials.append(material)
    else:
        width = max(0.05, finite(p.get("width", p.get("size", 0.3)), 0.3))
        depth = max(0.05, finite(p.get("depth", width), width))
        obj = add_cube(node_name(node), (position[0], base + height / 2, position[1]), (width, height, depth), material)
    assign_object_semantics(state, obj, node, "column")


def add_stair(state: BuildState, node: dict[str, Any], material: bpy.types.Material,
              elevations: dict[str, float]) -> None:
    p = props(node)
    position = vector(p.get("position", [p.get("x", 0), p.get("z", 0)]), 2, [0, 0])
    width = max(0.3, finite(p.get("width", 1.0), 1.0))
    run = max(0.3, finite(p.get("run", p.get("length", 3.0)), 3.0))
    total_height = max(0.1, finite(p.get("height", p.get("rise", 3.0)), 3.0))
    count = max(1, min(200, int(finite(p.get("steps", p.get("stepCount", round(total_height / 0.175))), 17))))
    angle = finite(p.get("rotation", p.get("rotationY", 0.0)), 0.0)
    if abs(angle) > math.tau:
        angle = math.radians(angle)
    base = node_base_elevation(node, state.nodes, elevations)
    parent = bpy.data.objects.new(node_name(node), None)
    bpy.context.collection.objects.link(parent)
    direction = Vector((math.cos(angle), 0.0, math.sin(angle)))
    for index in range(count):
        tread = run / count
        rise = total_height / count
        center = Vector((position[0], base, position[1])) + direction * (tread * (index + 0.5))
        obj = add_cube(
            f"{node_name(node)} step {index + 1}",
            (center.x, base + rise * (index + 0.5), center.z),
            (tread, rise * (index + 1), width),
            material,
        )
        obj.rotation_euler[2] = angle
        obj.parent = parent
        assign_object_semantics(state, obj, {**node, "id": f"{node.get('id')}:step:{index}"}, "stair")


def import_aedifex_glb(state: BuildState, path: pathlib.Path, scene_hash: str | None) -> bool:
    if not path.is_file() or path.suffix.lower() != ".glb":
        return False
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=str(path), import_pack_images=True)
    imported = [obj for obj in bpy.data.objects if obj not in before]
    if not imported:
        raise RuntimeError("GLB import returned no objects")
    for obj in imported:
        if obj.type not in {"MESH", "CURVE", "SURFACE", "META", "FONT"}:
            continue
        guessed = str(obj.get("arcz_node_type") or obj.get("aedifex_type") or "unknown").lower()
        synthetic_node = {
            "id": str(obj.get("arcz_node_id") or obj.name),
            "type": guessed,
            "name": obj.name,
        }
        assign_object_semantics(state, obj, synthetic_node, guessed)
    root = bpy.data.objects.new("ARCZ Aedifex GLB root", None)
    bpy.context.collection.objects.link(root)
    root["arcz_source"] = "aedifex_glb"
    root["arcz_scene_hash"] = scene_hash or ""
    for obj in imported:
        if obj.parent is None:
            obj.parent = root
    return True


def build_fallback(state: BuildState) -> None:
    elevations = level_elevations(state.nodes)
    materials = {
        "wall": make_material("ARCZ plaster", (0.74, 0.71, 0.66), 0.78),
        "concrete": make_material("ARCZ concrete", (0.30, 0.32, 0.34), 0.72),
        "roof": make_material("ARCZ roof", (0.22, 0.12, 0.08), 0.72),
        "frame": make_material("ARCZ frame", (0.06, 0.07, 0.08), 0.35, 0.2),
        "glass": make_material("ARCZ glass", (0.18, 0.32, 0.38), 0.08, 0.0, 0.28),
        "wood": make_material("ARCZ wood", (0.28, 0.12, 0.055), 0.52),
    }
    for value in materials.values():
        ensure_material_index(state, value)

    # Host geometry first; openings require wall objects.
    for node in state.nodes.values():
        kind = node_type(node)
        if kind in {"wall", "fence", "railing"}:
            add_wall(state, node, materials["wall" if kind == "wall" else "frame"], elevations)
    for node in state.nodes.values():
        kind = node_type(node)
        if kind in {"slab", "floor", "deck"}:
            add_polygon_prism(state, node, materials["concrete"], elevations, semantic="slab", default_thickness=0.15)
        elif kind == "ceiling":
            add_polygon_prism(state, node, materials["wall"], elevations, semantic="ceiling", default_thickness=0.06,
                              default_elevation_offset=finite(props(node).get("height", 3.0), 3.0))
        elif kind in {"roof", "roof-segment", "roof_segment"}:
            add_polygon_prism(state, node, materials["roof"], elevations, semantic="roof", default_thickness=0.12,
                              default_elevation_offset=finite(props(node).get("height", 3.0), 3.0))
        elif kind == "column":
            add_column(state, node, materials["concrete"], elevations)
        elif kind == "stair":
            add_stair(state, node, materials["concrete"], elevations)
    for node in state.nodes.values():
        kind = node_type(node)
        if kind in {"door", "window"}:
            add_opening(state, node, elevations, materials["wall"], materials["frame"], materials["glass"])

    supported = {
        "site", "building", "level", "collection", "wall", "fence", "railing", "door", "window",
        "slab", "floor", "deck", "ceiling", "roof", "roof-segment", "roof_segment", "column", "stair",
        "zone", "room", "dimension", "structural-grid", "structural_grid",
    }
    for node in state.nodes.values():
        kind = node_type(node)
        if kind not in supported:
            state.unsupported[kind] += 1


def scene_bounds() -> tuple[Vector, Vector]:
    points: list[Vector] = []
    for obj in bpy.context.scene.objects:
        if obj.type not in {"MESH", "CURVE", "SURFACE", "META", "FONT"} or not obj.visible_get():
            continue
        try:
            points.extend(obj.matrix_world @ Vector(corner) for corner in obj.bound_box)
        except Exception:
            points.append(obj.matrix_world.translation.copy())
    if not points:
        return Vector((-5, 0, -5)), Vector((5, 5, 5))
    return (
        Vector((min(point.x for point in points), min(point.y for point in points), min(point.z for point in points))),
        Vector((max(point.x for point in points), max(point.y for point in points), max(point.z for point in points))),
    )


def setup_ground(state: BuildState, color: tuple[float, float, float]) -> None:
    minimum, maximum = scene_bounds()
    span = max(maximum.x - minimum.x, maximum.z - minimum.z, 10.0)
    center = (minimum + maximum) * 0.5
    ground = add_cube("ARCZ ground", (center.x, minimum.y - 0.075, center.z), (span * 3.0, 0.15, span * 3.0),
                      make_material("ARCZ ground material", color, 0.9))
    assign_object_semantics(state, ground, {"id": "arcz:ground", "type": "terrain", "name": "Ground"}, "terrain")


def setup_camera(request: dict[str, Any]) -> bpy.types.Object:
    data = request.get("camera") if isinstance(request.get("camera"), dict) else {}
    position = vector(data.get("position"), 3, [12, 8, 12])
    target = vector(data.get("target"), 3, [0, 2, 0])
    bpy.ops.object.camera_add(location=tuple(position))
    camera = bpy.context.object
    camera.name = "ARCZ cinematic camera"
    bpy.context.scene.camera = camera
    direction = Vector(target) - camera.location
    if direction.length < 0.001:
        direction = Vector((0, 0, -1))
    camera.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()
    camera.data.lens = min(800.0, max(8.0, finite(data.get("focal_length_mm"), 35.0)))
    camera.data.sensor_width = min(100.0, max(4.0, finite(data.get("sensor_width_mm"), 36.0)))
    camera.data.shift_x = min(2.0, max(-2.0, finite(data.get("shift_x"), 0.0)))
    camera.data.shift_y = min(2.0, max(-2.0, finite(data.get("shift_y"), 0.0)))
    camera.data.clip_start = max(0.001, finite(data.get("clip_start_m"), 0.05))
    camera.data.clip_end = max(camera.data.clip_start + 1, finite(data.get("clip_end_m"), 100000.0))
    aperture = max(0.7, finite(data.get("aperture"), 5.6))
    focus_distance = max(0.01, finite(data.get("focus_distance_m"), direction.length))
    camera.data.dof.use_dof = aperture < 64
    camera.data.dof.aperture_fstop = aperture
    focus = bpy.data.objects.new("ARCZ focus target", None)
    focus.location = tuple(target)
    bpy.context.collection.objects.link(focus)
    camera.data.dof.focus_object = focus
    camera.data.dof.focus_distance = focus_distance
    return camera


def setup_world(request: dict[str, Any]) -> None:
    environment = request.get("environment") if isinstance(request.get("environment"), dict) else {}
    mode = str(environment.get("world_mode") or "nishita")
    world = bpy.context.scene.world or bpy.data.worlds.new("ARCZ World")
    bpy.context.scene.world = world
    world.use_nodes = True
    nodes = world.node_tree.nodes
    links = world.node_tree.links
    nodes.clear()
    output = nodes.new("ShaderNodeOutputWorld")
    background = nodes.new("ShaderNodeBackground")
    background.inputs["Strength"].default_value = max(0.0, finite(environment.get("strength"), 0.8))
    if mode == "nishita":
        sky = nodes.new("ShaderNodeTexSky")
        sky.sky_type = "NISHITA"
        sky.sun_elevation = math.radians(min(90.0, max(-10.0, finite(environment.get("sun_elevation_deg"), 25.0))))
        sky.sun_rotation = math.radians(finite(environment.get("sun_rotation_deg"), -35.0))
        sky.air_density = 1.0
        sky.dust_density = min(10.0, max(0.0, finite(environment.get("haze"), 1.0)))
        sky.ozone_density = 1.0
        links.new(sky.outputs["Color"], background.inputs["Color"])
    else:
        background.inputs["Color"].default_value = (0.055, 0.065, 0.08, 1.0)
    links.new(background.outputs["Background"], output.inputs["Surface"])

    bpy.ops.object.light_add(type="SUN", location=(0, 10, 0))
    sun = bpy.context.object
    sun.name = "ARCZ physical sun"
    sun.data.energy = max(0.0, finite(environment.get("sun_energy"), 3.0))
    elevation = math.radians(finite(environment.get("sun_elevation_deg"), 25.0))
    azimuth = math.radians(finite(environment.get("sun_rotation_deg"), -35.0))
    direction = Vector((math.cos(elevation) * math.cos(azimuth), math.sin(elevation), math.cos(elevation) * math.sin(azimuth)))
    sun.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()


def configure_cycles(scene: bpy.types.Scene, request: dict[str, Any], warnings: list[str]) -> None:
    quality = str(request.get("quality") or "balanced")
    settings = request.get("render_settings") if isinstance(request.get("render_settings"), dict) else {}
    samples = int(settings.get("samples") or QUALITY_SAMPLES.get(quality, 128))
    scene.cycles.samples = max(1, min(8192, samples))
    scene.cycles.use_denoising = bool(settings.get("denoise", True))
    if hasattr(scene.cycles, "use_adaptive_sampling"):
        scene.cycles.use_adaptive_sampling = True
    if hasattr(scene.cycles, "tile_size"):
        scene.cycles.tile_size = int(settings.get("tile_size") or 256)
    device = str(settings.get("device") or "auto")
    if device == "cpu":
        scene.cycles.device = "CPU"
        return
    try:
        addon = bpy.context.preferences.addons.get("cycles")
        preferences = addon.preferences if addon else None
        if preferences:
            preferences.get_devices()
            preferred = []
            for entry in preferences.devices:
                entry.use = entry.type in {"OPTIX", "CUDA", "HIP", "METAL", "ONEAPI"}
                if entry.use:
                    preferred.append(entry.type)
            if preferred:
                scene.cycles.device = "GPU"
            elif device == "gpu":
                raise RuntimeError("nenhum dispositivo GPU Cycles disponível")
            else:
                warnings.append("CYCLES_GPU_UNAVAILABLE:using_cpu")
    except Exception as error:
        if device == "gpu":
            raise
        warnings.append(f"CYCLES_DEVICE_FALLBACK:{error}")


def configure_render(request: dict[str, Any], warnings: list[str]) -> tuple[bpy.types.Scene, str]:
    scene = bpy.context.scene
    resolution = request["resolution"]
    scene.render.resolution_x = int(resolution["width"])
    scene.render.resolution_y = int(resolution["height"])
    scene.render.resolution_percentage = 100
    scene.render.image_settings.color_mode = "RGBA"
    engine = str(request.get("engine") or "cycles")
    if engine == "eevee":
        try:
            scene.render.engine = "BLENDER_EEVEE_NEXT"
        except TypeError:
            scene.render.engine = "BLENDER_EEVEE"
    else:
        scene.render.engine = "CYCLES"
        configure_cycles(scene, request, warnings)
    settings = request.get("render_settings") if isinstance(request.get("render_settings"), dict) else {}
    scene.render.film_transparent = bool(settings.get("transparent_background")) or (
        request.get("environment", {}).get("world_mode") == "transparent"
    )
    if hasattr(scene.render, "use_motion_blur"):
        scene.render.use_motion_blur = bool(settings.get("motion_blur", False))
    if hasattr(scene.render, "motion_blur_shutter"):
        scene.render.motion_blur_shutter = finite(settings.get("motion_blur_shutter"), 0.5)
    view = scene.view_settings
    requested_transform = str(settings.get("color_management") or "AgX")
    try:
        view.view_transform = requested_transform
    except TypeError:
        warnings.append(f"COLOR_TRANSFORM_UNAVAILABLE:{requested_transform}")
    look = str(settings.get("look") or "AgX - Medium High Contrast")
    try:
        view.look = look
    except TypeError:
        warnings.append(f"COLOR_LOOK_UNAVAILABLE:{look}")
    view.exposure = 0.0
    view.gamma = 1.0
    return scene, engine


def configure_passes(scene: bpy.types.Scene, requested: list[str], tech_dir: pathlib.Path,
                     camera: bpy.types.Object) -> tuple[dict[str, pathlib.Path], bpy.types.Node | None]:
    layer = scene.view_layers[0]
    layer.use_pass_z = "depth" in requested or "sky_mask" in requested
    layer.use_pass_normal = "normals" in requested
    layer.use_pass_object_index = bool({"object_ids", "semantic_masks"} & set(requested))
    layer.use_pass_material_index = "material_masks" in requested
    scene.use_nodes = True
    nodes = scene.node_tree.nodes
    links = scene.node_tree.links
    nodes.clear()
    render_layers = nodes.new("CompositorNodeRLayers")
    composite = nodes.new("CompositorNodeComposite")
    links.new(render_layers.outputs["Image"], composite.inputs["Image"])
    outputs: dict[str, pathlib.Path] = {}
    file_node = None
    if any(name != "beauty" for name in requested):
        file_node = nodes.new("CompositorNodeOutputFile")
        file_node.base_path = str(tech_dir)
        file_node.format.file_format = "OPEN_EXR"
        file_node.format.color_depth = "32"
        file_node.format.exr_codec = "ZIP"
        file_node.file_slots.clear()

        def slot(name: str, socket) -> None:
            if socket is None:
                return
            output_slot = file_node.file_slots.new(name)
            output_slot.path = name
            links.new(socket, file_node.inputs[output_slot.name])
            outputs[name] = tech_dir / f"{name}.exr"

        if "depth" in requested:
            slot("depth", render_layers.outputs.get("Depth"))
        if "normals" in requested:
            slot("normals", render_layers.outputs.get("Normal"))
        if "object_ids" in requested:
            slot("object_ids", render_layers.outputs.get("IndexOB"))
        if "semantic_masks" in requested:
            # Decoded by semantic mapping in the manifest. The scalar pass is
            # intentionally the object index so no scene mutation is required.
            slot("semantic_masks", render_layers.outputs.get("IndexOB"))
        if "material_masks" in requested:
            slot("material_masks", render_layers.outputs.get("IndexMA"))
        if "sky_mask" in requested and render_layers.outputs.get("Depth"):
            threshold = nodes.new("CompositorNodeMath")
            threshold.operation = "GREATER_THAN"
            threshold.inputs[1].default_value = float(camera.data.clip_end) * 0.99
            links.new(render_layers.outputs["Depth"], threshold.inputs[0])
            slot("sky_mask", threshold.outputs[0])
    return outputs, file_node


def normalize_compositor_outputs(tech_dir: pathlib.Path, expected: dict[str, pathlib.Path]) -> dict[str, pathlib.Path]:
    result: dict[str, pathlib.Path] = {}
    for name, destination in expected.items():
        candidates = sorted(tech_dir.glob(f"{name}*.exr"), key=lambda path: path.stat().st_mtime_ns, reverse=True)
        if not candidates:
            continue
        source = candidates[0]
        if source != destination:
            destination.unlink(missing_ok=True)
            source.replace(destination)
        result[name] = destination
    return result


def output_settings(scene: bpy.types.Scene, request: dict[str, Any]) -> tuple[str, str]:
    fmt = str(request.get("format") or "png")
    if fmt == "exr":
        scene.render.image_settings.file_format = "OPEN_EXR"
        scene.render.image_settings.color_depth = "32"
        scene.render.image_settings.exr_codec = "ZIP"
        return ".exr", "OPEN_EXR"
    if fmt == "jpg":
        scene.render.image_settings.file_format = "JPEG"
        scene.render.image_settings.color_mode = "RGB"
        scene.render.image_settings.quality = 100
        return ".jpg", "JPEG"
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_depth = "16"
    scene.render.image_settings.compression = 15
    return ".png", "PNG"


def output_record(path: pathlib.Path, kind: str, **extra: Any) -> dict[str, Any]:
    return {
        "path": str(path),
        "sha256": sha256(path),
        "bytes": path.stat().st_size,
        "kind": kind,
        **extra,
    }


def main() -> None:
    request_path, output_dir = args()
    output_dir.mkdir(parents=True, exist_ok=True)
    wrapper = json.loads(request_path.read_text(encoding="utf-8"))
    request = wrapper["request"]
    document = request.get("scene_document") or {}
    nodes = document.get("nodes") if isinstance(document, dict) else None
    if not isinstance(nodes, dict):
        raise RuntimeError("scene_document.nodes missing")
    clear_scene()
    state = BuildState(
        nodes={str(key): value for key, value in nodes.items() if isinstance(value, dict)},
        objects_by_node={}, warnings=[], unsupported=Counter(), object_index_map={}, material_index_map={},
    )

    source_export = request.get("resolved_scene_export") if isinstance(request.get("resolved_scene_export"), dict) else None
    imported_glb = False
    if source_export and source_export.get("absolute_path"):
        source_path = pathlib.Path(str(source_export["absolute_path"])).resolve()
        if sha256(source_path) != str(source_export.get("sha256")):
            raise RuntimeError("AEDIFEX_GLB_HASH_MISMATCH")
        imported_glb = import_aedifex_glb(state, source_path, request.get("scene_hash"))
    if not imported_glb:
        build_fallback(state)
        state.warnings.append("PARAMETRIC_FALLBACK_USED:no_aedifex_glb")

    environment = request.get("environment") if isinstance(request.get("environment"), dict) else {}
    ground_color = tuple(vector(environment.get("ground_color"), 3, [0.16, 0.18, 0.14]))
    setup_ground(state, ground_color)
    camera = setup_camera(request)
    setup_world(request)
    scene, engine = configure_render(request, state.warnings)

    render_dir = output_dir / "render"
    tech_dir = render_dir / "passes"
    render_dir.mkdir(parents=True, exist_ok=True)
    tech_dir.mkdir(parents=True, exist_ok=True)
    requested_passes = [str(value) for value in request.get("passes", ["beauty"])]
    expected_technical, _ = configure_passes(scene, requested_passes, tech_dir, camera)
    suffix, _ = output_settings(scene, request)
    beauty = render_dir / f"{request.get('output_name', 'beauty')}{suffix}"
    scene.render.filepath = str(beauty)

    blend = output_dir / "scene.blend"
    bpy.ops.wm.save_as_mainfile(filepath=str(blend), check_existing=False)
    started = time.monotonic()
    bpy.ops.render.render(write_still=True)
    elapsed = time.monotonic() - started
    if not beauty.is_file():
        raise RuntimeError(f"beauty output missing: {beauty}")
    technical = normalize_compositor_outputs(tech_dir, expected_technical)

    mappings_dir = output_dir / "mappings"
    mappings_dir.mkdir(exist_ok=True)
    object_map = mappings_dir / "object-index.json"
    semantic_map = mappings_dir / "semantic-index.json"
    material_map = mappings_dir / "material-index.json"
    object_map.write_text(json.dumps(state.object_index_map, ensure_ascii=False, indent=2), encoding="utf-8")
    semantic_payload: dict[str, list[dict[str, Any]]] = {}
    for entry in state.object_index_map.values():
        semantic_payload.setdefault(str(entry["semantic_id"]), []).append(entry)
    semantic_map.write_text(json.dumps(semantic_payload, ensure_ascii=False, indent=2), encoding="utf-8")
    material_map.write_text(json.dumps(state.material_index_map, ensure_ascii=False, indent=2), encoding="utf-8")

    outputs = [output_record(beauty, "beauty"), output_record(blend, "scene")]
    for name, path in technical.items():
        outputs.append(output_record(path, name, encoding="float32_exr"))
    outputs.extend([
        output_record(object_map, "object-index-map"),
        output_record(semantic_map, "semantic-index-map"),
        output_record(material_map, "material-index-map"),
    ])
    manifest = {
        "schema_version": 1,
        "job_id": wrapper["job_id"],
        "generator": "arcz.render.blender@2.0.0",
        "inputs_hash": request.get("scene_hash", "0" * 64),
        "profile_hash": "0" * 64,
        "seed": int(request.get("enhancement", {}).get("seed", 0)),
        "source_versions": {"blender": bpy.app.version_string},
        "outputs": outputs,
        "technical_passes": {name: str(path) for name, path in technical.items()},
        "warnings": state.warnings,
        "metrics": {
            "objects": len(bpy.context.scene.objects),
            "meshes": sum(1 for obj in bpy.context.scene.objects if obj.type == "MESH"),
            "materials": len(bpy.data.materials),
            "engine": engine,
            "samples": int(scene.cycles.samples) if engine == "cycles" else None,
            "render_seconds": round(elapsed, 3),
            "resolution": [scene.render.resolution_x, scene.render.resolution_y],
            "source": "aedifex_glb" if imported_glb else "parametric_fallback",
            "unsupported_node_types": dict(state.unsupported),
        },
        "scene_source": None if not source_export else {
            "export_id": source_export.get("id"),
            "path": source_export.get("path"),
            "sha256": source_export.get("sha256"),
        },
        "created_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "deterministic": False,
        "generation_epoch": wrapper.get("generation_epoch", 0),
    }
    (output_dir / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8"
    )


if __name__ == "__main__":
    main()
