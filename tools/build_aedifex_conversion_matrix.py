#!/usr/bin/env python3
from __future__ import annotations

"""Generate the fail-closed Aedifex → ARCZ conversion matrix.

The generator reads the immutable upstream lock. Every required package, node
kind and MCP family must be classified here. A new upstream symbol without a
mapping raises instead of silently inheriting an unsafe default.
"""

import hashlib
import json
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
LOCK_PATH = ROOT / "integrations" / "aedifex" / "UPSTREAM_LOCK.json"
COMMUNITY_PATH = ROOT / "integrations" / "aedifex" / "COMMUNITY_SOURCES.json"
OUTPUT_PATH = ROOT / "integrations" / "aedifex" / "CONVERSION_MATRIX.json"

NODE_GROUPS = {
    "hierarchy_site": {"site", "building", "level", "spawn", "structural-grid"},
    "architecture_envelope": {"wall", "fence", "slab", "ceiling", "door", "window", "column", "zone"},
    "roof_system": {
        "roof", "roof-segment", "box-vent", "ridge-vent", "turbine-vent", "cupola",
        "eyebrow-vent", "chimney", "solar-panel", "skylight", "dormer", "gutter", "downspout",
    },
    "vertical_circulation": {"stair", "stair-segment", "elevator"},
    "catalog_furniture": {"item", "shelf", "cabinet", "cabinet-module"},
    "documentation_context": {"guide", "scan", "measurement", "construction-dimension"},
    "mep_hvac_plumbing": {
        "duct-segment", "duct-fitting", "duct-terminal", "hvac-equipment", "lineset",
        "liquid-line", "pipe-segment", "pipe-fitting", "pipe-trap",
    },
}

RUST_TARGETS = {
    "hierarchy_site": "arcz-cad-document/site-hierarchy",
    "architecture_envelope": "arcz-cad-document/architectural-elements",
    "roof_system": "arcz-roof + arcz-cad-document/roof-accessories",
    "vertical_circulation": "arcz-cad-document/circulation",
    "catalog_furniture": "arcz-bim-document/catalog-instances",
    "documentation_context": "arcz-cad-document/annotations",
    "mep_hvac_plumbing": "arcz-bim-document/mep",
    "nature_plugin": "arcz-vegetation/plugin-adapter",
}

PACKAGE_ROLES = {
    "@aedifex/core": ("AEDIFEX", "native scene schemas/store/history/registry remain authoritative"),
    "@aedifex/viewer": ("AEDIFEX", "native Three/R3F viewport mounted by the controlled host"),
    "@aedifex/editor": ("AEDIFEX", "single Floorplanner/inspector/catalog/tool surface"),
    "@aedifex/mcp": ("SHARED", "in-memory MCP catalog bridged to ARCZ revisions and approvals"),
    "@aedifex/nodes": ("AEDIFEX", "builtinPlugin loads every locked native node kind"),
    "@aedifex/plugin-trees": ("AEDIFEX_PLUGIN", "local Plugin API v2 admission for trees/flowers/grass"),
    "@aedifex/ifc-converter": ("AEDIFEX_IMPORT", "local web-ifc conversion in a transactional ARCZ panel"),
}

TOOL_POLICIES = {
    "scene-query": "AUTO_READ",
    "measurement": "AUTO_READ",
    "validation": "AUTO_READ",
    "collisions": "AUTO_READ",
    "export-json": "CONFIRM_EXPORT",
    "export-glb": "CONFIRM_EXPORT",
    "scene-lifecycle": "MIXED_FAIL_CLOSED",
}

MUTATING_TOOLS = {
    "construction", "rooms", "patch", "levels", "walls", "items", "openings", "zones",
    "duplicate", "delete", "undo-redo", "templates", "variants", "photo-to-scene",
}


def canonical_hash(value: dict[str, Any]) -> str:
    body = dict(value)
    body.pop("matrix_hash", None)
    encoded = json.dumps(body, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()
    return hashlib.sha256(encoded).hexdigest()


def group_for(kind: str) -> str:
    matches = [name for name, kinds in NODE_GROUPS.items() if kind in kinds]
    if len(matches) != 1:
        raise RuntimeError(f"node kind sem classificação única: {kind!r}; matches={matches}")
    return matches[0]


def node_entry(kind: str, *, category: str | None = None, source: str = "@aedifex/nodes") -> dict[str, Any]:
    category = category or group_for(kind)
    return {
        "id": kind,
        "category": category,
        "source": source,
        "authority": "AEDIFEX_BUILDING_AUTHORING_KERNEL",
        "document_authority": "AEDIFEX",
        "integration": "native registry → editor/viewport/tools → immutable ARCZ revision → validated export",
        "globe_policy": "READONLY_GLB_DERIVATIVE",
        "target": RUST_TARGETS[category],
        "rust_parity": "NOT_MIGRATED_UNTIL_GOLDEN_AND_ROUNDTRIP_GATES_PASS",
        "loss_policy": "FAIL_CLOSED_WITH_PER_FIELD_LOSS_REPORT",
        "feature_flag": f"arcz.aedifex.rust_parity.{kind}",
        "status": "PRESERVED_IN_PINNED_UPSTREAM_BUILD_BLOCKED",
        "tests": [
            "generated upstream inventory coverage",
            "native schema/serialization tests from pinned upstream",
            "Aedifex↔ARCZ golden round-trip before Rust activation",
            "GLB derivative hash/revision/GeoAnchor validation",
        ],
        "blockers": ["AEDIFEX_UPSTREAM_BUILD_MISSING", "RUST_KIND_PARITY_NOT_PROVEN"],
    }


def build() -> dict[str, Any]:
    lock = json.loads(LOCK_PATH.read_text(encoding="utf-8"))
    community = json.loads(COMMUNITY_PATH.read_text(encoding="utf-8"))

    unknown_packages = sorted(set(lock["packages"]) - set(PACKAGE_ROLES))
    if unknown_packages:
        raise RuntimeError(f"pacotes sem política: {unknown_packages}")

    nodes = [node_entry(kind) for kind in lock["required_node_kinds"]]
    tool_entries = []
    for family in lock["required_tool_families"]:
        policy = TOOL_POLICIES.get(family)
        if family in MUTATING_TOOLS:
            policy = "PREVIEW_APPROVE_MUTATION"
        if not policy:
            raise RuntimeError(f"família MCP sem política: {family}")
        tool_entries.append({
            "id": family,
            "authority": "AEDIFEX_MCP_WITH_ARCZ_TRANSACTION_GUARD",
            "integration": "dynamic MCP catalog; host injects project_id/expected_revision; result hash and audit trail",
            "source": "@aedifex/mcp",
            "target": "ARCZ Global Chat tool run",
            "status": "SOURCE_INTEGRATED_RUNTIME_BUILD_BLOCKED",
            "execution_policy": policy,
            "revision_guard": True,
            "audit": True,
            "loss_policy": "NO_TEXT_ONLY_SUCCESS; REAL_TOOL_RESULT_REQUIRED",
            "tests": ["tool catalog contract", "preview/approve/reject", "optimistic revision conflict", "request/result hash"],
            "blockers": ["AEDIFEX_UPSTREAM_BUILD_MISSING"],
        })

    packages = []
    for package_id, package in lock["packages"].items():
        authority, integration = PACKAGE_ROLES[package_id]
        packages.append({
            "id": package_id,
            "source": package["path"],
            "authority": authority,
            "integration": integration,
            "target": "controlled Aedifex fork + ARCZ overlay",
            "status": "LOCKED_AND_MAPPED_BUILD_BLOCKED",
            "loss_policy": "NO_SOURCE_REWRITE; PATCH_MANIFEST_ONLY",
            "tests": ["commit/version/license lock", "offline build", "upstream tests", "ARCZ overlay syntax/E2E"],
            "blockers": ["AEDIFEX_UPSTREAM_MISSING", "BUN_BUILD_NOT_EXECUTED"],
        })

    community_entries = []
    for item in community.get("audited_forks", []):
        community_entries.append({
            "id": item["repository"],
            "source": str(item.get("observed_commit") or "metadata-only"),
            "authority": "REFERENCE_OR_PATCH_CANDIDATE_ONLY",
            "integration": str(item.get("observed_change") or "no unique change established"),
            "status": item["decision"],
            "loss_policy": "NO_BLIND_MERGE",
            "tests": ["license", "commit/diff", "asset provenance", "offline", "plugin cleanup", "rollback"],
        })

    matrix: dict[str, Any] = {
        "schema_version": 1,
        "upstream": {
            "repository": lock["repository"],
            "commit": lock["commit"],
            "license": lock["license"],
            "plugin_api_version": lock["plugin_api_version"],
        },
        "decision": {
            "selected": "INTEGRATE_AEDIFEX_AS_BUILDING_AUTHORING_KERNEL",
            "world_core": "ARCZ_EARTH",
            "final_host": "SINGLE_TAURI_REACT_HOST",
            "transitional_host": "SANDBOXED_LOOPBACK_SIDECAR",
            "rejected": ["REPLACE_WORLD_CORE", "GLB_AS_EDITABLE_SOURCE", "REMOTE_RUNTIME", "BIG_BANG_RUST_REWRITE"],
        },
        "authorities": {
            "ARCZ": ["WGS84/ECEF/ENU", "region/lot", "terrain/context", "procedural world", "jobs/budget", "cinema", "render", "Street", "provenance"],
            "AEDIFEX": ["parametric building document", "2D/3D authoring", "all node kinds", "materials/catalog", "plugins", "IFC", "MCP building operations"],
            "DERIVED": ["readonly GLB", "semantic manifest", "GeoAnchor", "revision/hash", "render passes"],
        },
        "packages": packages,
        "apps": [
            {
                "id": "apps/editor", "authority": "REFERENCE_HOST_ONLY",
                "integration": "features folded into the single ARCZ floorplanner host; standalone app not shipped",
                "source": "apps/editor", "target": "@arcz/aedifex-floorplanner",
                "status": "MAPPED_BUILD_BLOCKED", "loss_policy": "NO_DUPLICATE_PROJECT_STORE",
                "tests": ["one Editor mount", "one chat", "one revision store"],
            },
            {
                "id": "apps/ifc-converter", "authority": "REFERENCE_UI_ONLY",
                "integration": "converter package mounted in the unified Import/Export panel",
                "source": "apps/ifc-converter", "target": "ArczImportExportPanel",
                "status": "SOURCE_INTEGRATED_BUILD_BLOCKED", "loss_policy": "TRANSACTIONAL_ROLLBACK",
                "tests": ["local WASM copy", "scene validation", "revision conflict", "rollback"],
            },
        ],
        "plugins": [
            {
                "id": "@aedifex/plugin-trees", "authority": "AEDIFEX_PLUGIN_API_V2",
                "integration": "treesPlugin plus treesHostPanel loaded once with duplicate guard and local assets",
                "source": "packages/plugin-trees", "target": "controlled floorplanner host",
                "status": "SOURCE_INTEGRATED_BUILD_BLOCKED", "loss_policy": "PLUGIN_PAYLOAD_PRESERVED",
                "tests": ["serialization", "clean unload", "SSR-safe admission", "asset license/provenance"],
                "blockers": ["AEDIFEX_UPSTREAM_BUILD_MISSING"],
            }
        ],
        "node_kinds": nodes,
        "extension_node_kinds": [
            node_entry("trees:tree", category="nature_plugin", source="@aedifex/plugin-trees"),
            node_entry("trees:flower", category="nature_plugin", source="@aedifex/plugin-trees"),
            node_entry("trees:grass", category="nature_plugin", source="@aedifex/plugin-trees"),
        ],
        "tool_families": tool_entries,
        "global_modules": [
            {"id": "region-to-floorplanner", "authority": "ARCZ", "integration": "ModelingContextPackage + GeoAnchor + readonly hash-verified context layers", "status": "VERIFIED_CONTRACT_BROWSER_BLOCKED"},
            {"id": "single-global-chat", "authority": "ARCZ", "integration": "one history; ARCZ tools plus real Aedifex MCP catalog", "status": "VERIFIED_BACKEND_AND_JS"},
            {"id": "prompt-library", "authority": "ARCZ", "integration": "SQLite versions, compile, import/export hash, enhancer and translator through local broker", "status": "VERIFIED_MODELS_OPTIONAL_BLOCKED"},
            {"id": "reference-media", "authority": "ARCZ", "integration": "content-addressed verified image/video/audio/document/BIM/CAD/geodata/point-cloud/HDR library", "status": "VERIFIED"},
            {"id": "photoreal-render", "authority": "ARCZ", "integration": "Aedifex GLB → Blender/Cycles → passes → local diffusion/upscale optional → geometry guard", "status": "SOURCE_VERIFIED_RUNTIME_BLOCKED"},
            {"id": "cinematic-earth", "authority": "ARCZ", "integration": "Cesium globe, atmosphere, local procedural clouds, sequential camera callbacks, cancel/restore", "status": "UNIT_VERIFIED_BROWSER_BLOCKED"},
            {"id": "global-panel-dock", "authority": "ARCZ", "integration": "collapsed default, hover/focus/touch, pin, resize, persistence, ARIA tabs", "status": "VERIFIED_JS"},
        ],
        "community_sources": community_entries,
        "gates": [
            {"id": "python-tests", "status": "PASSED", "proof": "pytest suite in validation report", "blocking": True},
            {"id": "javascript-tests", "status": "PASSED", "proof": "node:test suite in validation report", "blocking": True},
            {"id": "aedifex-vendor-build", "status": "BLOCKED", "proof": "pinned checkout/Bun build unavailable in this environment", "blocking": True},
            {"id": "cesium-browser-e2e", "status": "BLOCKED", "proof": "local Cesium vendor and browser run unavailable", "blocking": True},
            {"id": "rust-parity", "status": "BLOCKED", "proof": "cargo/rustc and per-kind parity corpus unavailable", "blocking": False},
            {"id": "blender-photoreal", "status": "BLOCKED", "proof": "Blender/Cycles and local model weights unavailable", "blocking": True},
            {"id": "windows-installer-soak", "status": "PENDING", "proof": "requires target Alienware/Windows", "blocking": True},
        ],
        "counts": {
            "packages": len(packages),
            "apps": 2,
            "plugins": 1,
            "native_node_kinds": len(nodes),
            "extension_node_kinds": 3,
            "tool_families": len(tool_entries),
            "global_modules": 7,
            "community_sources": len(community_entries),
        },
    }
    matrix["matrix_hash"] = canonical_hash(matrix)
    return matrix


def main() -> int:
    matrix = build()
    OUTPUT_PATH.write_text(json.dumps(matrix, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"path": str(OUTPUT_PATH), "hash": matrix["matrix_hash"], "counts": matrix["counts"]}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
