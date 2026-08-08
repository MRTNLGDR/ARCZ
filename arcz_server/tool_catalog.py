from __future__ import annotations

"""Catálogo global e executável de ferramentas ARCZ + Aedifex.

Regras de segurança:
- o catálogo entregue ao modelo contém apenas ferramentas realmente disponíveis;
- ferramentas Aedifex são descobertas no sidecar autenticado, nunca inventadas;
- projeto/revisão são contexto confiável do host, não argumentos controlados pelo modelo;
- mutações e exclusões são sempre pré-visualizadas e exigem approval_id para commit;
- nenhuma ferramenta recebe acesso direto ao viewer ou ao sistema de arquivos.
"""

import json
from pathlib import Path
from typing import Any, Callable

from .aedifex_registry import AedifexRegistry
from .aedifex_runtime import AedifexRuntimeManager
from .errors import ApiError


ToolService = Callable[..., Any]


class GlobalToolCatalog:
    def __init__(self, root: Path, aedifex: AedifexRegistry, runtime: AedifexRuntimeManager):
        self.root = root.resolve()
        self.aedifex = aedifex
        self.runtime = runtime
        self._core = self._core_descriptors()

    @staticmethod
    def _descriptor(
        name: str,
        description: str,
        input_schema: dict[str, Any],
        *,
        side_effect: str = "read",
        requires_approval: bool = False,
        service: str | None = None,
    ) -> dict[str, Any]:
        return {
            "name": name,
            "description": description,
            "inputSchema": input_schema,
            "side_effect": side_effect,
            "requires_approval": requires_approval,
            "namespace": name.split(".", 1)[0],
            "available": True,
            "service": service,
        }

    def _core_descriptors(self) -> list[dict[str, Any]]:
        object_schema = {"type": "object", "additionalProperties": False}
        return [
            self._descriptor(
                "arcz.floorplanner.list_projects",
                "Lista projetos locais do Floorplanner, opcionalmente filtrados por região.",
                {
                    "type": "object",
                    "properties": {
                        "region_id": {"type": ["string", "null"]},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 500},
                    },
                    "additionalProperties": False,
                },
                service="floorplanner_list",
            ),
            self._descriptor(
                "arcz.floorplanner.get_project",
                "Lê metadados, contexto territorial e uma revisão real do Floorplanner.",
                {
                    "type": "object",
                    "required": ["project_id"],
                    "properties": {
                        "project_id": {"type": "string", "minLength": 1},
                        "include_scene": {"type": "boolean"},
                        "revision": {"type": ["integer", "null"], "minimum": 1},
                    },
                    "additionalProperties": False,
                },
                service="floorplanner_get",
            ),
            self._descriptor(
                "arcz.prompts.search",
                "Pesquisa a biblioteca local, versionada e multilíngue de prompts.",
                {
                    "type": "object",
                    "properties": {
                        "query": {"type": ["string", "null"]},
                        "category": {"type": ["string", "null"]},
                        "language": {"type": ["string", "null"]},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 500},
                    },
                    "additionalProperties": False,
                },
                service="prompt_list",
            ),
            self._descriptor(
                "arcz.prompts.compile",
                "Compila um template local com variáveis e contexto, sem inferência.",
                {
                    "type": "object",
                    "required": ["identifier"],
                    "properties": {
                        "identifier": {"type": "string", "minLength": 1},
                        "variables": {"type": "object"},
                        "context": {"type": ["object", "null"]},
                    },
                    "additionalProperties": False,
                },
                service="prompt_compile",
            ),
            self._descriptor(
                "arcz.prompts.enhance",
                "Aprimora um prompt por modelo local registrado; falha explicitamente se o modelo não estiver instalado.",
                {
                    "type": "object",
                    "required": ["prompt"],
                    "properties": {
                        "prompt": {"type": "string", "minLength": 1},
                        "category": {"type": ["string", "null"]},
                        "language": {"type": ["string", "null"]},
                        "model_id": {"type": ["string", "null"]},
                        "context": {"type": ["object", "null"]},
                    },
                    "additionalProperties": True,
                },
                service="prompt_enhance",
            ),
            self._descriptor(
                "arcz.prompts.translate",
                "Traduz um prompt usando somente modelo local registrado e preserva termos técnicos.",
                {
                    "type": "object",
                    "required": ["text", "target_language"],
                    "properties": {
                        "text": {"type": "string", "minLength": 1},
                        "source_language": {"type": ["string", "null"]},
                        "target_language": {"type": "string", "minLength": 2},
                        "model_id": {"type": ["string", "null"]},
                        "glossary": {"type": ["object", "null"]},
                    },
                    "additionalProperties": True,
                },
                service="prompt_translate",
            ),
            self._descriptor(
                "arcz.media.list",
                "Lista mídias de referência locais por hash, categoria, licença e proveniência.",
                {
                    "type": "object",
                    "properties": {
                        "category": {"type": ["string", "null"]},
                        "limit": {"type": "integer", "minimum": 1, "maximum": 500},
                    },
                    "additionalProperties": False,
                },
                service="media_list",
            ),
            self._descriptor(
                "arcz.photoreal.preflight",
                "Valida cena, referências, modelos locais, Blender, VRAM e passes antes de renderizar.",
                {"type": "object", "additionalProperties": True},
                service="photoreal_preflight",
            ),
            self._descriptor(
                "arcz.photoreal.submit",
                "Cria um job local de render fotorreal após preflight real.",
                {"type": "object", "additionalProperties": True},
                side_effect="mutate",
                requires_approval=True,
                service="photoreal_submit",
            ),
            self._descriptor(
                "arcz.aedifex.status",
                "Verifica commit, integridade, build e runtime Aedifex controlado.",
                object_schema,
                service="aedifex_status",
            ),
        ]

    @staticmethod
    def _normalize_aedifex_tool(tool: dict[str, Any], *, available: bool, reason: str | None = None) -> dict[str, Any]:
        raw_name = str(tool.get("name") or "").strip()
        name = raw_name if raw_name.startswith("aedifex.") else f"aedifex.{raw_name}"
        effect = str(tool.get("sideEffect") or tool.get("side_effect") or "mutate").lower()
        if effect not in {"read", "export", "mutate", "destructive"}:
            effect = "mutate"
        requires = bool(tool.get("requiresApproval", tool.get("requires_approval", effect in {"export", "mutate", "destructive"})))
        return {
            "name": name,
            "description": str(tool.get("description") or "Ferramenta do kernel Aedifex"),
            "inputSchema": tool.get("inputSchema") if isinstance(tool.get("inputSchema"), dict) else {"type": "object"},
            "side_effect": effect,
            "requires_approval": requires,
            "namespace": "aedifex",
            "available": bool(available),
            "unavailable_reason": reason,
            "service": "aedifex_runtime",
        }

    def _static_aedifex_catalog(self) -> list[dict[str, Any]]:
        path = self.root / "integrations" / "aedifex" / "runtime" / "tool-catalog.json"
        if not path.is_file():
            return []
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            return []
        tools = value.get("tools") if isinstance(value, dict) else None
        if not isinstance(tools, list):
            return []
        return [item for item in tools if isinstance(item, dict) and isinstance(item.get("name"), str)]

    def list(self, *, include_unavailable: bool = False) -> list[dict[str, Any]]:
        result = [dict(item) for item in self._core]
        status = self.runtime.status()
        runtime = status.get("runtime", {}) if isinstance(status, dict) else {}
        authenticated = bool(runtime.get("authenticated_tool_bridge"))
        ready = bool(status.get("ready"))
        dynamic: list[dict[str, Any]] = []
        reason: str | None = None
        if ready and authenticated:
            try:
                dynamic = [self._normalize_aedifex_tool(item, available=True) for item in self.runtime.list_tools()]
            except ApiError as error:
                reason = error.code
        else:
            reason = "AEDIFEX_TOOL_RUNTIME_NOT_READY" if not ready else "AEDIFEX_TOOL_BRIDGE_UNAUTHENTICATED"

        if not dynamic and include_unavailable:
            dynamic = [self._normalize_aedifex_tool(item, available=False, reason=reason) for item in self._static_aedifex_catalog()]
        result.extend(dynamic)
        # Defesas contra catálogo corrompido/duplicado. O primeiro registro vence,
        # mas uma ferramenta disponível sempre substitui a mesma entrada indisponível.
        unique: dict[str, dict[str, Any]] = {}
        for item in result:
            name = str(item.get("name") or "")
            if not name:
                continue
            previous = unique.get(name)
            if previous is None or (not previous.get("available") and item.get("available")):
                unique[name] = item
        return [unique[name] for name in sorted(unique)]

    def describe(self, name: str, *, include_unavailable: bool = True) -> dict[str, Any]:
        for item in self.list(include_unavailable=include_unavailable):
            if item["name"] == name:
                return item
        raise ApiError("CHAT_TOOL_NOT_FOUND", name, status=404)

    @staticmethod
    def _required_string(value: Any, field: str) -> str:
        text = str(value or "").strip()
        if not text:
            raise ApiError("CHAT_TOOL_CONTEXT_REQUIRED", field, status=400)
        return text

    @staticmethod
    def _required_revision(value: Any) -> int:
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            raise ApiError("CHAT_TOOL_EXPECTED_REVISION_REQUIRED", "expected_revision", status=400)
        return value

    def invoke(
        self,
        name: str,
        arguments: dict[str, Any],
        *,
        services: dict[str, ToolService],
        context: dict[str, Any] | None = None,
    ) -> Any:
        if not isinstance(arguments, dict):
            raise ApiError("CHAT_TOOL_ARGUMENTS_INVALID", name, status=400)
        descriptor = self.describe(name, include_unavailable=True)
        if not descriptor.get("available"):
            raise ApiError(
                "CHAT_TOOL_UNAVAILABLE",
                name,
                status=503,
                details={"reason": descriptor.get("unavailable_reason")},
            )
        context = context or {}
        if name.startswith("aedifex."):
            project_id = self._required_string(context.get("project_id"), "floorplanner_project_id")
            expected_revision = self._required_revision(context.get("expected_revision"))
            dry_run = bool(context.get("dry_run", True))
            approval_id = context.get("approval_id")
            if not dry_run and descriptor["requires_approval"] and not str(approval_id or "").strip():
                raise ApiError("CHAT_TOOL_APPROVAL_REQUIRED", name, status=409)
            return self.runtime.invoke_tool(
                name,
                arguments,
                project_id=project_id,
                expected_revision=expected_revision,
                dry_run=dry_run,
                approval_id=str(approval_id) if approval_id else None,
            )

        service_name = descriptor.get("service")
        function = services.get(str(service_name))
        if function is None:
            raise ApiError("CHAT_TOOL_SERVICE_MISSING", str(service_name), status=500)
        try:
            if name == "arcz.floorplanner.get_project":
                return function(
                    str(arguments["project_id"]),
                    include_scene=bool(arguments.get("include_scene", False)),
                    revision=arguments.get("revision"),
                )
            if name == "arcz.prompts.compile":
                return function(
                    str(arguments["identifier"]),
                    arguments.get("variables", {}),
                    context=arguments.get("context"),
                )
            if name in {"arcz.prompts.enhance", "arcz.prompts.translate", "arcz.photoreal.preflight"}:
                return function(arguments)
            if name == "arcz.photoreal.submit":
                if bool(context.get("dry_run", True)):
                    preflight = services["photoreal_preflight"](arguments)
                    return {"schema_version": 1, "dry_run": True, "changed": bool(preflight.get("ready")),
                            "preflight": preflight}
                approval_id = str(context.get("approval_id") or "").strip()
                if not approval_id:
                    raise ApiError("CHAT_TOOL_APPROVAL_REQUIRED", name, status=409)
                return function(arguments)
            if name in {"arcz.aedifex.status"}:
                return function()
            return function(**arguments)
        except KeyError as error:
            raise ApiError("CHAT_TOOL_ARGUMENT_REQUIRED", str(error), status=400) from error
