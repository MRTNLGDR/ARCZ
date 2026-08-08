from __future__ import annotations
import json
from pathlib import Path
from typing import Any
from .errors import ApiError

try:
    import jsonschema  # type: ignore
    from referencing import Registry, Resource  # type: ignore
except Exception:
    jsonschema = None
    Registry = None
    Resource = None

class SchemaRegistry:
    def __init__(self, root: Path):
        self.root = root.resolve()
        self._cache: dict[str, dict[str, Any]] = {}
        self._registry = None

    def load(self, name: str) -> dict[str, Any]:
        if name in self._cache: return self._cache[name]
        path = (self.root / name).resolve()
        try: path.relative_to(self.root)
        except ValueError: raise ApiError("SCHEMA_PATH_INVALID", "Schema fora da raiz", status=400)
        if not path.is_file(): raise ApiError("SCHEMA_NOT_FOUND", f"Schema não encontrado: {name}", status=500)
        schema = json.loads(path.read_text(encoding="utf-8")); self._cache[name] = schema; self._registry = None; return schema

    def _reference_registry(self):
        if Registry is None or Resource is None: return None
        if self._registry is not None: return self._registry
        registry = Registry()
        for path in sorted(self.root.glob("*.json")):
            try:
                schema = self.load(path.name)
                resource = Resource.from_contents(schema)
                # Register every schema under its canonical $id and under the
                # aliases emitted by relative $ref resolution. urllib does not
                # treat the custom ``arcz`` scheme as hierarchical on every
                # Python version, so registering the filename is deliberate.
                aliases = {
                    path.name,
                    f"arcz://schemas/{path.name}",
                    path.resolve().as_uri(),
                }
                uri = schema.get("$id")
                if isinstance(uri, str) and uri:
                    aliases.add(uri)
                for alias in aliases:
                    registry = registry.with_resource(alias, resource)
            except Exception:
                continue
        self._registry = registry
        return registry

    def validate(self, name: str, value: Any) -> Any:
        schema = self.load(name)
        if jsonschema is not None:
            try:
                jsonschema.Draft202012Validator(
                    schema, format_checker=jsonschema.FormatChecker(),
                    registry=self._reference_registry(),
                ).validate(value)
            except jsonschema.ValidationError as e:
                path = "$" + "".join(f"[{p}]" if isinstance(p, int) else f".{p}" for p in e.absolute_path)
                raise ApiError("SCHEMA_INVALID", e.message, status=400, details={"schema": name, "path": path}) from e
            except Exception as e:
                raise ApiError("SCHEMA_REFERENCE_INVALID", str(e), status=500, details={"schema": name}) from e
        else:
            self._minimum_validate(schema, value, "$", name)
        return value

    def _minimum_validate(self, schema: dict[str, Any], value: Any, path: str, name: str) -> None:
        expected = schema.get("type")
        if isinstance(expected, list):
            allowed = set(expected)
            if value is None and "null" in allowed: return
            checks = {
                "object": isinstance(value, dict), "array": isinstance(value, list), "string": isinstance(value, str),
                "number": isinstance(value, (int, float)) and not isinstance(value, bool),
                "integer": isinstance(value, int) and not isinstance(value, bool), "boolean": isinstance(value, bool),
            }
            if not any(checks.get(item, False) for item in allowed):
                raise ApiError("SCHEMA_INVALID", f"tipo esperado: {expected}", details={"schema":name,"path":path})
            return
        if expected == "object":
            if not isinstance(value, dict): raise ApiError("SCHEMA_INVALID", "objeto esperado", details={"schema":name,"path":path})
            for required in schema.get("required", []):
                if required not in value: raise ApiError("SCHEMA_INVALID", f"campo obrigatório: {required}", details={"schema":name,"path":path})
            props = schema.get("properties", {})
            for key, child in props.items():
                if key in value: self._minimum_validate(child, value[key], f"{path}.{key}", name)
        elif expected == "array" and not isinstance(value, list):
            raise ApiError("SCHEMA_INVALID", "lista esperada", details={"schema":name,"path":path})
        elif expected == "string" and not isinstance(value, str):
            raise ApiError("SCHEMA_INVALID", "texto esperado", details={"schema":name,"path":path})
        elif expected == "number" and (not isinstance(value,(int,float)) or isinstance(value,bool)):
            raise ApiError("SCHEMA_INVALID", "número esperado", details={"schema":name,"path":path})
        elif expected == "integer" and (not isinstance(value,int) or isinstance(value,bool)):
            raise ApiError("SCHEMA_INVALID", "inteiro esperado", details={"schema":name,"path":path})
        elif expected == "boolean" and not isinstance(value,bool):
            raise ApiError("SCHEMA_INVALID", "boolean esperado", details={"schema":name,"path":path})
        if "const" in schema and value != schema["const"]:
            raise ApiError("SCHEMA_INVALID", f"valor precisa ser {schema['const']!r}", details={"schema":name,"path":path})
        if "enum" in schema and value not in schema["enum"]:
            raise ApiError("SCHEMA_INVALID", "valor fora do enum", details={"schema":name,"path":path})
