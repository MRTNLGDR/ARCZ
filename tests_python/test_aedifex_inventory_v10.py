from __future__ import annotations

import json
from pathlib import Path

import pytest

from arcz_server.aedifex_inventory import INVENTORY_CATEGORIES, inventory_upstream, validate_coverage
from arcz_server.errors import ApiError
from arcz_server.schema_validation import SchemaRegistry

ROOT = Path(__file__).resolve().parents[1]
SCHEMAS = SchemaRegistry(ROOT / "schemas")
COMMIT = "1" * 40


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")


def source_tree(root: Path) -> Path:
    source = root / "aedifex"
    source.mkdir()
    (source / "LICENSE").write_text("MIT License\n", encoding="utf-8")
    (source / "UPSTREAM_COMMIT").write_text(COMMIT + "\n", encoding="utf-8")
    write_json(source / "package.json", {"name": "aedifex", "version": "1.0.0", "private": True})
    write_json(source / "packages/core/package.json", {"name": "@aedifex/core", "version": "0.10.0"})
    write_json(source / "packages/nodes/package.json", {"name": "@aedifex/nodes", "version": "0.2.0"})
    write_json(source / "packages/mcp/package.json", {"name": "@aedifex/mcp", "version": "0.3.3"})
    write_json(source / "apps/editor/package.json", {"name": "@aedifex/editor-app", "version": "0.1.0", "private": True})
    node = source / "packages/nodes/src/wall/definition.ts"
    node.parent.mkdir(parents=True)
    node.write_text("export const wall = { kind: 'wall' }\n", encoding="utf-8")
    tool = source / "packages/mcp/src/tools/walls/create-wall.ts"
    tool.parent.mkdir(parents=True)
    tool.write_text("export const tool = { name: 'create_wall' }\n", encoding="utf-8")
    route = source / "apps/editor/app/api/ai/chat/route.ts"
    route.parent.mkdir(parents=True)
    route.write_text(
        "const key = process.env.AI_API_KEY; export async function POST(){ return fetch('https://api.example.invalid/v1') }\n",
        encoding="utf-8",
    )
    test = source / "packages/core/src/network.test.ts"
    test.parent.mkdir(parents=True)
    test.write_text("test('x',()=>fetch('https://example.invalid/test'))\n", encoding="utf-8")
    return source


def permissive_policy() -> dict:
    return {
        "schema_version": 1,
        "upstream_commit": COMMIT,
        "blocking_statuses": ["UNMAPPED", "REVIEW_REQUIRED", "BLOCKED"],
        "categories": {
            category: [{
                "pattern": "*",
                "status": "TEST_ADMITTED",
                "owner": "test",
                "integration": "fixture admission",
                "rationale": "contract test",
            }]
            for category in INVENTORY_CATEGORIES
        },
    }


def test_inventory_enumerates_source_surfaces_and_is_schema_valid(tmp_path: Path) -> None:
    inventory = inventory_upstream(source_tree(tmp_path), expected_commit=COMMIT)
    assert inventory["commit"] == COMMIT
    assert inventory["root_license"] == "MIT"
    assert {item["id"] for item in inventory["packages"]} == {
        "@aedifex/core", "@aedifex/mcp", "@aedifex/nodes",
    }
    assert {item["id"] for item in inventory["apps"]} == {"@aedifex/editor-app"}
    assert "wall" in {item["id"] for item in inventory["node_kinds"]}
    assert "create_wall" in {item["id"] for item in inventory["mcp_tools"]}
    assert "/api/ai/chat" in {item["id"] for item in inventory["api_routes"]}
    assert "AI_API_KEY" in {item["id"] for item in inventory["environment_variables"]}
    assert "https://api.example.invalid/v1" in {item["id"] for item in inventory["external_urls"]}
    assert any(item["source"].endswith("route.ts") and item["test_only"] is False for item in inventory["network_call_sites"])
    assert any(item["source"].endswith("network.test.ts") and item["test_only"] is True for item in inventory["network_call_sites"])
    SCHEMAS.validate("aedifex-upstream-inventory.schema.json", inventory)


def test_coverage_fails_closed_for_unmapped_item(tmp_path: Path) -> None:
    inventory = inventory_upstream(source_tree(tmp_path), expected_commit=COMMIT)
    policy = permissive_policy()
    policy["categories"]["external_urls"] = [{
        "pattern": "https://github.com/*",
        "status": "REFERENCE_ONLY",
        "owner": "test",
        "integration": "reference",
        "rationale": "only GitHub admitted",
    }]
    report = validate_coverage(inventory, policy)
    assert report["ready"] is False
    assert any(item["category"] == "external_urls" and item["status"] == "UNMAPPED" for item in report["blockers"])
    SCHEMAS.validate("aedifex-conversion-coverage.schema.json", report)


def test_source_scope_rule_does_not_hide_runtime_network_call(tmp_path: Path) -> None:
    inventory = inventory_upstream(source_tree(tmp_path), expected_commit=COMMIT)
    policy = permissive_policy()
    policy["categories"]["network_call_sites"] = [
        {
            "pattern": "*",
            "source_patterns": ["*.test.ts"],
            "status": "TEST_ONLY_NETWORK_STUB",
            "owner": "test",
            "integration": "test",
            "rationale": "test only",
        }
    ]
    report = validate_coverage(inventory, policy)
    blocked_sources = {str(item.get("source")) for item in report["blockers"]}
    assert any(value.endswith("route.ts") for value in blocked_sources)
    assert not any(value.endswith("network.test.ts") for value in blocked_sources)


def test_inventory_rejects_commit_mismatch(tmp_path: Path) -> None:
    source = source_tree(tmp_path)
    with pytest.raises(ApiError) as caught:
        inventory_upstream(source, expected_commit="2" * 40)
    assert caught.value.code == "AEDIFEX_COMMIT_MISMATCH"


def test_generated_conversion_matrix_covers_lock_exactly_and_hashes_canonically() -> None:
    lock = json.loads((ROOT / "integrations/aedifex/UPSTREAM_LOCK.json").read_text(encoding="utf-8"))
    matrix = json.loads((ROOT / "integrations/aedifex/CONVERSION_MATRIX.json").read_text(encoding="utf-8"))
    SCHEMAS.validate("aedifex-conversion-matrix.schema.json", matrix)
    assert matrix["upstream"]["commit"] == lock["commit"]
    assert {item["id"] for item in matrix["packages"]} == set(lock["packages"])
    assert [item["id"] for item in matrix["node_kinds"]] == lock["required_node_kinds"]
    assert [item["id"] for item in matrix["tool_families"]] == lock["required_tool_families"]
    assert all(item["document_authority"] == "AEDIFEX" for item in matrix["node_kinds"])
    assert all(item["globe_policy"] == "READONLY_GLB_DERIVATIVE" for item in matrix["node_kinds"])
    body = dict(matrix); expected = body.pop("matrix_hash")
    import hashlib
    actual = hashlib.sha256(json.dumps(
        body, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False,
    ).encode()).hexdigest()
    assert actual == expected
