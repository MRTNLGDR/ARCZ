from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shutil

from arcz_server.aedifex_inventory import INVENTORY_CATEGORIES, inventory_upstream, validate_coverage
from arcz_server.aedifex_registry import AedifexRegistry
from arcz_server.hashing import sha256_file

ROOT = Path(__file__).resolve().parents[1]


def tree_integrity(root: Path) -> dict[str, object]:
    digest = hashlib.sha256()
    count = 0
    total = 0
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.is_symlink() or path.name == "arcz-aedifex-build.json":
            continue
        rel = path.relative_to(root).as_posix()
        size = path.stat().st_size
        value = sha256_file(path)
        digest.update(rel.encode()); digest.update(b"\0")
        digest.update(str(size).encode()); digest.update(b"\0")
        digest.update(value.encode()); digest.update(b"\n")
        count += 1; total += size
    return {"algorithm": "sha256", "file_count": count, "total_bytes": total, "tree_sha256": digest.hexdigest()}


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")


def materialize_test_installation(root: Path) -> tuple[AedifexRegistry, Path]:
    lock = json.loads((ROOT / "integrations/aedifex/UPSTREAM_LOCK.json").read_text(encoding="utf-8"))
    lock["required_node_kinds"] = ["wall"]
    shutil.copytree(ROOT / "schemas", root / "schemas")
    integration = root / "integrations/aedifex"
    integration.mkdir(parents=True)
    write_json(integration / "UPSTREAM_LOCK.json", lock)
    write_json(integration / "PATCH_MANIFEST.json", {
        "schema_version": 4,
        "upstream_commit": lock["commit"],
        "strategy": "AEDIFEX_BUILDING_AUTHORING_KERNEL_INSIDE_ARCZ_WORLD_CORE",
        "policy": "Test fixture keeps the upstream immutable and applies one reversible controlled overlay.",
        "patches": [{
            "id": "ARCAED-999",
            "kind": "test-overlay",
            "destination": "apps/arcz-floorplanner",
            "purpose": "Exercise strict patch manifest validation in the registry test fixture.",
            "authority": "BUILD_GOVERNANCE",
            "reversible": True,
            "test_gate": ["registry_fixture"],
        }],
    })

    for base in (root / "opensources/upstream/aedifex", root / "opensources/forks/aedifex-arcz"):
        for rel in set(AedifexRegistry.REQUIRED_PACKAGES) | set(lock["required_workspace_paths"]):
            path = base / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            if rel == "LICENSE":
                path.write_text("MIT License\n", encoding="utf-8")
            elif rel.endswith("package.json"):
                matched = next(((name, spec) for name, spec in lock["packages"].items() if spec["path"] == rel), None)
                if matched:
                    name, spec = matched
                    write_json(path, {"name": name, "version": spec["version"]})
                else:
                    write_json(path, {"name": rel.replace("/package.json", ""), "version": "1.0.0", "private": True})
            else:
                path.write_text("lock\n", encoding="utf-8")
        (base / "UPSTREAM_COMMIT").write_text(lock["commit"] + "\n", encoding="utf-8")
        node = base / "packages/nodes/src/wall/definition.ts"
        node.parent.mkdir(parents=True, exist_ok=True)
        node.write_text("export const definition = { kind: 'wall' }\n", encoding="utf-8")

    inventory = inventory_upstream(root / "opensources/upstream/aedifex", expected_commit=lock["commit"])
    policy = {
        "schema_version": 1,
        "upstream_commit": lock["commit"],
        "blocking_statuses": ["UNMAPPED", "REVIEW_REQUIRED", "BLOCKED"],
        "categories": {
            category: [{
                "pattern": "*", "status": "TEST_ADMITTED", "owner": "test",
                "integration": "test fixture", "rationale": "registry contract",
            }]
            for category in INVENTORY_CATEGORIES
        },
    }
    coverage = validate_coverage(inventory, policy)
    write_json(integration / "CONVERSION_COVERAGE.json", policy)
    write_json(integration / "generated/UPSTREAM_INVENTORY.json", inventory)
    write_json(integration / "generated/CONVERSION_COVERAGE_REPORT.json", coverage)

    dist = root / "vendor/aedifex-floorplanner"
    entry = dist / "app/server.js"
    entry.parent.mkdir(parents=True)
    entry.write_text("console.log('local')\n", encoding="utf-8")
    public = entry.parent / "public"
    public.mkdir()
    wasm_integrity = {}
    for name, fill in (("web-ifc.wasm", b"a"), ("web-ifc-mt.wasm", b"b")):
        path = public / name
        path.write_bytes(fill * 100_001)
        wasm_integrity[name] = {"sha256": sha256_file(path), "bytes": path.stat().st_size}
    integrity = tree_integrity(dist)
    integrity.update({"entry_path": "app/server.js", "entry_sha256": sha256_file(entry)})
    write_json(dist / "arcz-aedifex-build.json", {
        "schema_version": 3,
        "upstream_commit": lock["commit"],
        "inventory_hash": inventory["inventory_hash"],
        "coverage_report_hash": coverage["report_hash"],
        "built_at": "2026-08-06T12:00:00Z",
        "builder": "tools/build_aedifex_sidecar.py",
        "runtime": {
            "command": ["node", "server.js"], "cwd": "app", "port": 8124,
            "health_path": "/api/health", "tool_catalog_path": "/api/arcz/tools/catalog",
            "tool_invoke_path": "/api/arcz/tools/invoke", "requires_bridge_token": True,
            "loopback_only": True,
        },
        "quality_commands": [],
        "wasm_integrity": wasm_integrity,
        "integrity": integrity,
    })
    return AedifexRegistry(root), entry


def test_deep_aedifex_build_integrity_detects_tampering(tmp_path: Path) -> None:
    registry, entry = materialize_test_installation(tmp_path)
    assert registry.status(verify_tree=True)["ready"] is True
    entry.write_text("console.log('tampered')\n", encoding="utf-8")
    status = registry.status(verify_tree=True)
    assert status["ready"] is False
    codes = {item["code"] for item in status["blockers"]}
    assert "AEDIFEX_RUNTIME_ENTRY_HASH_MISMATCH" in codes
