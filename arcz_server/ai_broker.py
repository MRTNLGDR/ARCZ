from __future__ import annotations

from dataclasses import dataclass
import importlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
from typing import Any
import urllib.request

from .atomic_io import atomic_write_bytes, atomic_write_json
from .content_store import ContentStore
from .errors import ApiError
from .hashing import canonical_json_hash, sha256_file
from .network_policy import NetworkPolicy
from .schema_validation import SchemaRegistry


@dataclass(slots=True)
class InstalledModel:
    manifest_path: Path
    manifest: dict[str, Any]

    @property
    def directory(self) -> Path:
        return self.manifest_path.parent


class ModelRegistry:
    def __init__(self, roots: list[Path], schemas: SchemaRegistry):
        self.roots = [root.resolve() for root in roots]
        self.schemas = schemas

    def list(self, *, verify: bool = True) -> list[dict[str, Any]]:
        result = []
        for model in self._iter():
            status = self.verify(model) if verify else {"installed": True, "errors": []}
            result.append({**model.manifest, "manifest_path": str(model.manifest_path), "status": status})
        return result

    def _iter(self):
        seen: set[tuple[str, str]] = set()
        for root in self.roots:
            if not root.is_dir():
                continue
            for path in sorted(root.rglob("*.json")):
                try:
                    value = json.loads(path.read_text(encoding="utf-8"))
                    self.schemas.validate("ai-model-manifest.schema.json", value)
                except Exception:
                    continue
                key = (value["id"], value["version"])
                if key in seen:
                    continue
                seen.add(key)
                yield InstalledModel(path.resolve(), value)

    def find(self, *, task: str, model_id: str | None = None) -> InstalledModel:
        candidates = [model for model in self._iter()
                      if model.manifest["task"] == task and (model_id is None or model.manifest["id"] == model_id)]
        valid = []
        invalid = []
        for model in candidates:
            status = self.verify(model)
            if status["installed"]:
                valid.append(model)
            else:
                invalid.append({"id": model.manifest["id"], "errors": status["errors"]})
        if not valid:
            raise ApiError("MODEL_NOT_INSTALLED", f"Nenhum modelo local válido para {task}", status=503,
                           details={"task": task, "requested_model": model_id, "invalid": invalid})
        valid.sort(key=lambda item: (item.manifest["id"], item.manifest["version"]))
        return valid[-1]

    def verify(self, model: InstalledModel) -> dict[str, Any]:
        errors = []
        for entry in model.manifest["files"]:
            path = (model.directory / entry["path"]).resolve()
            try:
                path.relative_to(model.directory)
            except ValueError:
                errors.append(f"path_escape:{entry['path']}")
                continue
            if not path.is_file():
                errors.append(f"missing:{entry['path']}")
                continue
            if path.stat().st_size != entry["bytes"]:
                errors.append(f"size:{entry['path']}")
                continue
            if sha256_file(path) != entry["sha256"]:
                errors.append(f"hash:{entry['path']}")
        return {"installed": not errors, "errors": errors}


class LocalAIBroker:
    """Broker local auditável. Não contém respostas sintéticas de demonstração."""

    def __init__(self, root: Path, registry: ModelRegistry, schemas: SchemaRegistry,
                 network_policy: NetworkPolicy):
        self.root = root.resolve()
        self.registry = registry
        self.schemas = schemas
        self.network_policy = network_policy
        self.cache = ContentStore(self.root / "cache" / "v2" / "ai")

    def request(self, task: str, payload: dict[str, Any], *, model_id: str | None = None,
                timeout_seconds: int | None = None) -> dict[str, Any]:
        model = self.registry.find(task=task, model_id=model_id)
        manifest = model.manifest
        key = canonical_json_hash({
            "model": f"{manifest['id']}@{manifest['version']}",
            "files": [(f["sha256"], f["bytes"]) for f in manifest["files"]],
            "payload": payload,
        })
        cached_path = self.cache.path_for(key)
        if cached_path.is_file():
            return json.loads(cached_path.read_text(encoding="utf-8"))
        timeout = int(timeout_seconds or manifest.get("timeout_seconds", 300))
        backend = manifest["backend"]
        if backend == "command":
            result = self._run_command(model, payload, timeout)
        elif backend in {"ollama_local", "comfyui_local"}:
            result = self._run_loopback(model, payload, timeout)
        elif backend == "python_adapter":
            result = self._run_python_adapter(model, payload)
        elif backend in {"onnxruntime", "llama_cpp"}:
            # Pré/pós-processamento é específico da tarefa. O manifesto deve
            # apontar um adapter explícito para não adivinhar tensores/tokens.
            if not manifest.get("adapter"):
                raise ApiError("MODEL_ADAPTER_REQUIRED",
                               f"{backend} requer `adapter` no manifesto para a tarefa {task}", status=422)
            result = self._run_python_adapter(model, payload)
        else:
            raise ApiError("MODEL_BACKEND_UNSUPPORTED", backend, status=422)
        envelope = {
            "schema_version": 1,
            "task": task,
            "model": {"id": manifest["id"], "version": manifest["version"]},
            "cache_key": key,
            "result": result,
        }
        cached_path.parent.mkdir(parents=True, exist_ok=True)
        atomic_write_bytes(
            cached_path,
            json.dumps(envelope, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode("utf-8"),
        )
        return envelope

    def _run_command(self, model: InstalledModel, payload: dict[str, Any], timeout: int) -> Any:
        with tempfile.TemporaryDirectory(prefix="arcz-ai-", dir=self.root / "jobs") as temp_raw:
            temp = Path(temp_raw)
            input_path = temp / "input.json"
            output_path = temp / "output.json"
            atomic_write_json(input_path, payload)
            replacements = {
                "{input}": str(input_path), "{output}": str(output_path),
                "{model_dir}": str(model.directory), "{root}": str(self.root),
            }
            command = [replacements.get(token, token) for token in model.manifest["command"]]
            if not command:
                raise ApiError("MODEL_COMMAND_EMPTY", model.manifest["id"], status=422)
            try:
                completed = subprocess.run(command, cwd=model.directory, shell=False, timeout=timeout,
                                           capture_output=True, text=True, encoding="utf-8",
                                           env={**os.environ, "ARCZ_NETWORK_MODE": "offline_strict",
                                                "NO_PROXY": "*", "no_proxy": "*"})
            except FileNotFoundError as error:
                raise ApiError("MODEL_EXECUTABLE_NOT_FOUND", command[0], status=503) from error
            except subprocess.TimeoutExpired as error:
                raise ApiError("MODEL_TIMEOUT", model.manifest["id"], status=504, retryable=True) from error
            if completed.returncode != 0:
                raise ApiError("MODEL_FAILED", f"Modelo retornou {completed.returncode}", status=500,
                               details={"stderr": completed.stderr[-12000:], "stdout": completed.stdout[-12000:]})
            if output_path.is_file():
                return json.loads(output_path.read_text(encoding="utf-8"))
            stdout = completed.stdout.strip()
            if not stdout:
                raise ApiError("MODEL_OUTPUT_MISSING", "Modelo não produziu JSON", status=500)
            return json.loads(stdout)

    def _run_loopback(self, model: InstalledModel, payload: dict[str, Any], timeout: int) -> Any:
        endpoint = model.manifest["endpoint"]
        self.network_policy.assert_url(endpoint)
        request = urllib.request.Request(endpoint,
                                         data=json.dumps(payload, ensure_ascii=False).encode("utf-8"),
                                         headers={"Content-Type": "application/json"}, method="POST")
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return json.loads(response.read().decode("utf-8"))
        except Exception as error:
            raise ApiError("LOCAL_MODEL_ENDPOINT_FAILED", str(error), status=503, retryable=True,
                           details={"endpoint": endpoint}) from error

    def _run_python_adapter(self, model: InstalledModel, payload: dict[str, Any]) -> Any:
        adapter = model.manifest.get("adapter")
        if not adapter or not adapter.startswith("arcz_ai_adapters."):
            raise ApiError("MODEL_ADAPTER_DENIED", "Adapter precisa estar em arcz_ai_adapters.*", status=403)
        module_name, separator, callable_name = adapter.partition(":")
        callable_name = callable_name or "run"
        try:
            module = importlib.import_module(module_name)
            function = getattr(module, callable_name)
        except Exception as error:
            raise ApiError("MODEL_ADAPTER_LOAD_FAILED", str(error), status=503) from error
        return function(payload=payload, manifest=model.manifest, model_dir=model.directory, root=self.root)
