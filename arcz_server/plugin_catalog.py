from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from .errors import ApiError
from .schema_validation import SchemaRegistry


class PluginCatalog:
    def __init__(self, roots: list[Path], schemas: SchemaRegistry):
        self.roots = [root.resolve() for root in roots]
        self.schemas = schemas

    def list(self) -> list[dict[str, Any]]:
        result: dict[str, dict[str, Any]] = {}
        for root in self.roots:
            if not root.is_dir():
                continue
            for path in sorted(root.rglob("*.plugin.json")):
                value = json.loads(path.read_text(encoding="utf-8"))
                self.schemas.validate("plugin-manifest-v2.schema.json", value)
                value = {**value, "manifest_path": str(path), "available": self._entrypoint_exists(value)}
                result[value["id"]] = value
        return [result[key] for key in sorted(result)]

    def get(self, plugin_id: str) -> dict[str, Any]:
        for plugin in self.list():
            if plugin["id"] == plugin_id:
                return plugin
        raise ApiError("PLUGIN_NOT_FOUND", plugin_id, status=404)

    def _entrypoint_exists(self, manifest: dict[str, Any]) -> bool:
        # entrypoint começa em / no front, mas é relativo à raiz do projeto.
        for root in self.roots:
            project_root = root
            while project_root.parent != project_root and not (project_root / "app").is_dir():
                project_root = project_root.parent
            path = (project_root / manifest["entrypoint"].lstrip("/")).resolve()
            if path.is_file():
                return True
        return False
