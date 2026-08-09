from __future__ import annotations

import json
from pathlib import Path

from arcz_server.aedifex_inventory import inventory_upstream


def _write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def test_node_kind_inventory_ignores_handle_and_renderer_kinds(tmp_path: Path) -> None:
    _write(
        tmp_path / "package.json",
        json.dumps({"name": "aedifex", "version": "0.0.0", "private": True}),
    )
    _write(
        tmp_path / "packages/nodes/package.json",
        json.dumps({"name": "@aedifex/nodes", "version": "0.0.0"}),
    )
    _write(
        tmp_path / "packages/nodes/src/wall/definition.ts",
        """
export const wallDefinition: NodeDefinition<typeof WallNode> = {
  kind: 'wall',
  renderer: { kind: 'parametric' },
  handles: () => [
    { kind: 'linear-resize', decoration: { kind: 'ring' } },
  ],
}
""",
    )
    _write(
        tmp_path / "packages/nodes/src/cabinet/definition.ts",
        """
export const cabinetDefinition: NodeDefinition<typeof CabinetNode> = {
  kind: 'cabinet',
}
export const cabinetModuleDefinition: NodeDefinition<typeof CabinetModuleNode> = {
  kind: 'cabinet-module',
}
""",
    )

    inventory = inventory_upstream(tmp_path)
    kinds = {item["id"] for item in inventory["node_kinds"]}

    assert {"wall", "cabinet", "cabinet-module"}.issubset(kinds)
    assert "linear-resize" not in kinds
    assert "parametric" not in kinds
    assert "ring" not in kinds
