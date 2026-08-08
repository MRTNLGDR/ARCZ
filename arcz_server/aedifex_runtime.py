from __future__ import annotations

"""Gerencia e chama o sidecar Aedifex local com autenticação efêmera.

Aedifex só é iniciado a partir do build controlado, em loopback. Ferramentas
MCP usam um token aleatório mantido apenas em memória/ambiente do filho. Nenhum
endpoint de mutação é exposto sem esse token.
"""

import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import threading
import time
from typing import Any
import urllib.error
import urllib.request
from urllib.parse import urlsplit, urlunsplit

from .aedifex_registry import AedifexRegistry
from .errors import ApiError


class AedifexRuntimeManager:
    MAX_RESPONSE_BYTES = 16 * 1024 * 1024
    LOOPBACK_HOSTS = {"127.0.0.1", "localhost", "::1", "[::1]"}
    CHILD_ENV_ALLOWLIST = {
        "PATH", "PATHEXT", "SystemRoot", "SYSTEMROOT", "WINDIR",
        "TEMP", "TMP", "TMPDIR", "HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA",
        "LANG", "LANGUAGE", "LC_ALL", "TZ", "COMSPEC", "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH", "NUMBER_OF_PROCESSORS", "PROCESSOR_ARCHITECTURE",
    }

    @classmethod
    def _loopback_origin(cls, value: str) -> str:
        parsed = urlsplit(value)
        if parsed.scheme != "http" or parsed.hostname not in cls.LOOPBACK_HOSTS:
            raise ApiError("AEDIFEX_ARCZ_API_URL_DENIED", value, status=403)
        if parsed.username or parsed.password or parsed.query or parsed.fragment:
            raise ApiError("AEDIFEX_ARCZ_API_URL_INVALID", value, status=400)
        if parsed.path not in {"", "/"}:
            raise ApiError("AEDIFEX_ARCZ_API_URL_INVALID", value, status=400)
        try:
            port = parsed.port
        except ValueError as error:
            raise ApiError("AEDIFEX_ARCZ_API_URL_INVALID", value, status=400) from error
        if port is not None and not 1 <= port <= 65535:
            raise ApiError("AEDIFEX_ARCZ_API_URL_INVALID", value, status=400)
        # Reconstruct from validated components rather than returning the raw
        # netloc. This keeps the child contract free of surprising casing or
        # alternate user-info encodings.
        host = f"[{parsed.hostname}]" if ":" in parsed.hostname else parsed.hostname
        netloc = f"{host}:{port}" if port is not None else host
        return urlunsplit(("http", netloc, "", "", ""))

    @classmethod
    def _sanitized_child_env(cls) -> dict[str, str]:
        return {
            key: value for key, value in os.environ.items()
            if key in cls.CHILD_ENV_ALLOWLIST
        }

    def __init__(self, root: Path, registry: AedifexRegistry):
        self.root = root.resolve(); self.registry = registry
        self._process: subprocess.Popen[bytes] | None = None
        self._lock = threading.RLock(); self._bridge_token: str | None = None
        self.log_path = self.root / "logs" / "aedifex-sidecar.log"; self.log_path.parent.mkdir(parents=True, exist_ok=True)
        self.api_url = self._loopback_origin(os.environ.get("ARCZ_API_URL", "http://127.0.0.1:8123"))

    def _manifest(self) -> dict[str, Any]:
        path = self.registry.dist / "arcz-aedifex-build.json"
        if not path.is_file(): raise ApiError("AEDIFEX_BRIDGE_BUILD_MISSING", str(path), status=503)
        try: value = json.loads(path.read_text(encoding="utf-8"))
        except Exception as error: raise ApiError("AEDIFEX_BUILD_MANIFEST_INVALID", str(error), status=500) from error
        if not isinstance(value.get("runtime"), dict): raise ApiError("AEDIFEX_RUNTIME_MANIFEST_MISSING", "runtime ausente", status=500)
        return value

    @staticmethod
    def _port(runtime: dict[str, Any]) -> int:
        port = int(runtime.get("port", 8124))
        if port < 1024 or port > 65535: raise ApiError("AEDIFEX_PORT_INVALID", str(port), status=500)
        return port

    def _runtime_url(self, manifest: dict[str, Any] | None = None) -> str:
        if manifest is None:
            try: manifest = self._manifest()
            except ApiError: return "http://127.0.0.1:8124"
        return f"http://127.0.0.1:{self._port(manifest['runtime'])}"

    @staticmethod
    def _safe_path(path: str) -> str:
        if not path.startswith("/") or ".." in path or "\x00" in path:
            raise ApiError("AEDIFEX_RUNTIME_PATH_INVALID", path, status=400)
        return path

    def _health(self, manifest: dict[str, Any] | None = None, timeout: float = 0.6) -> dict[str, Any]:
        try: manifest = manifest or self._manifest()
        except ApiError as error: return {"healthy":False,"error":error.code}
        path = self._safe_path(str(manifest["runtime"].get("health_path", "/api/health")))
        url = self._runtime_url(manifest) + path
        try:
            with urllib.request.urlopen(url, timeout=timeout) as response:
                raw = response.read(1024 * 1024); payload = json.loads(raw.decode("utf-8")) if raw else {}
                expected = str(manifest.get("upstream_commit", "")); actual = str(payload.get("upstream_commit", expected))
                return {"healthy":response.status==200 and (not expected or actual==expected),"status":response.status,
                        "payload":payload,"url":url}
        except (OSError, urllib.error.URLError, ValueError, json.JSONDecodeError) as error:
            return {"healthy":False,"error":error.__class__.__name__,"detail":str(error),"url":url}

    def status(self) -> dict[str, Any]:
        registry_status = self.registry.status(); manifest = registry_status.get("build") if isinstance(registry_status.get("build"),dict) else None
        health = self._health(manifest) if manifest else {"healthy":False,"error":"BUILD_NOT_READY"}
        with self._lock:
            process=self._process; owned=bool(process is not None and process.poll() is None)
            exit_code=None if process is None or owned else process.returncode
            authenticated=bool(self._bridge_token and owned)
        return {**registry_status,"runtime":{"url":self._runtime_url(manifest),"healthy":bool(health.get("healthy")),"health":health,
            "owned_process_running":owned,"owned_pid":process.pid if owned and process else None,"owned_exit_code":exit_code,
            "authenticated_tool_bridge":authenticated,"log_path":str(self.log_path)},
            "ready":bool(registry_status.get("ready") and health.get("healthy"))}

    def start(self, *, wait_seconds: float = 20.0) -> dict[str, Any]:
        registry_status=self.registry.require_ready(); manifest=self._manifest(); health=self._health(manifest)
        if health.get("healthy"):
            # An independently started sidecar can serve the editor, but tools
            # stay unavailable unless the operator supplied the same token.
            with self._lock:
                self._bridge_token = os.environ.get("ARCZ_AEDIFEX_BRIDGE_TOKEN") or self._bridge_token
            return self.status()
        runtime=manifest["runtime"]; command=runtime.get("command")
        if not isinstance(command,list) or not command or not all(isinstance(i,str) and i for i in command):
            raise ApiError("AEDIFEX_RUNTIME_COMMAND_INVALID",repr(command),status=500)
        executable=command[0]
        if executable not in {"node","bun"}: raise ApiError("AEDIFEX_RUNTIME_EXECUTABLE_DENIED",executable,status=403)
        resolved=shutil.which(executable)
        if not resolved: raise ApiError("AEDIFEX_RUNTIME_EXECUTABLE_MISSING",executable,status=503)
        cwd_rel=str(runtime.get("cwd",".")); cwd=(self.registry.dist/cwd_rel).resolve()
        try: cwd.relative_to(self.registry.dist.resolve())
        except ValueError as error: raise ApiError("AEDIFEX_RUNTIME_CWD_ESCAPE",cwd_rel,status=403) from error
        if not cwd.is_dir(): raise ApiError("AEDIFEX_RUNTIME_CWD_MISSING",cwd_rel,status=503)
        safe=[resolved]
        for arg in command[1:]:
            if "\x00" in arg or len(arg)>4096: raise ApiError("AEDIFEX_RUNTIME_ARGUMENT_INVALID",repr(arg),status=400)
            safe.append(arg)
        token=secrets.token_urlsafe(48)
        # Do not inherit provider keys, cloud credentials, proxy variables or
        # telemetry secrets from the parent process. The child receives only
        # the OS variables required to execute Node plus the explicit ARCZ
        # loopback contract below.
        env=self._sanitized_child_env(); env.update({"HOSTNAME":"127.0.0.1","PORT":str(self._port(runtime)),"NODE_ENV":"production",
            "ARCZ_NETWORK_MODE":"offline_strict","NEXT_TELEMETRY_DISABLED":"1","ARCZ_API_URL":self.api_url,
            "ARCZ_AEDIFEX_BRIDGE_TOKEN":token})
        with self._lock:
            if self._process is not None and self._process.poll() is None: return self.status()
            log=self.log_path.open("ab",buffering=0)
            try:
                self._process=subprocess.Popen(safe,cwd=cwd,env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT,
                    start_new_session=(os.name!="nt")); self._bridge_token=token
            finally: log.close()
        deadline=time.monotonic()+max(.5,min(float(wait_seconds),60.0)); last=health
        while time.monotonic()<deadline:
            with self._lock:
                if self._process is not None and self._process.poll() is not None:
                    self._bridge_token=None
                    raise ApiError("AEDIFEX_RUNTIME_EXITED","Sidecar encerrou durante a inicialização",status=503,
                        details={"exit_code":self._process.returncode,"log_path":str(self.log_path)})
            last=self._health(manifest,timeout=1.0)
            if last.get("healthy"): return self.status()
            time.sleep(.25)
        raise ApiError("AEDIFEX_RUNTIME_HEALTH_TIMEOUT","Sidecar iniciou, mas não passou no health-check",status=503,
            details={"health":last,"log_path":str(self.log_path),"registry":registry_status})

    def stop(self, *, grace_seconds: float = 5.0) -> dict[str, Any]:
        with self._lock:
            process=self._process
            if process is None or process.poll() is not None:
                self._process=None; self._bridge_token=None; return self.status()
            process.terminate()
            try: process.wait(timeout=max(.1,min(float(grace_seconds),30.0)))
            except subprocess.TimeoutExpired: process.kill(); process.wait(timeout=5)
            self._process=None; self._bridge_token=None
        return self.status()

    def request_json(self, path: str, *, method: str = "GET", payload: dict[str, Any] | None = None,
                     timeout: float = 30.0, max_bytes: int | None = None) -> Any:
        self._safe_path(path); method=method.upper()
        if method not in {"GET","POST"}: raise ApiError("AEDIFEX_RUNTIME_METHOD_DENIED",method,status=405)
        with self._lock: token=self._bridge_token
        if not token:
            raise ApiError("AEDIFEX_TOOL_BRIDGE_UNAUTHENTICATED","Inicie o sidecar pelo ARCZ para habilitar ferramentas",status=503)
        body=None if payload is None else json.dumps(payload,ensure_ascii=False,separators=(",",":")).encode("utf-8")
        headers={"Accept":"application/json","Authorization":f"Bearer {token}"}
        if body is not None: headers["Content-Type"]="application/json"
        request=urllib.request.Request(self._runtime_url()+path,data=body,headers=headers,method=method)
        limit=max(1024,min(int(max_bytes or self.MAX_RESPONSE_BYTES),self.MAX_RESPONSE_BYTES))
        try:
            with urllib.request.urlopen(request,timeout=max(.1,min(float(timeout),120.0))) as response:
                raw=response.read(limit+1)
                if len(raw)>limit: raise ApiError("AEDIFEX_RUNTIME_RESPONSE_TOO_LARGE",str(len(raw)),status=502)
                value=json.loads(raw.decode("utf-8")) if raw else {}
                return value
        except urllib.error.HTTPError as error:
            raw=error.read(2*1024*1024)
            try: value=json.loads(raw.decode("utf-8")) if raw else {}
            except Exception: value={"error":{"message":raw.decode("utf-8",errors="replace")[:2048]}}
            details=value.get("error",{}) if isinstance(value,dict) else {}
            raise ApiError(str(details.get("code") or "AEDIFEX_RUNTIME_HTTP_ERROR"),
                str(details.get("message") or f"HTTP {error.code}"),status=error.code,details=details.get("details")) from error
        except urllib.error.URLError as error:
            raise ApiError("AEDIFEX_RUNTIME_UNREACHABLE",str(error),status=503,retryable=True) from error

    def list_tools(self) -> list[dict[str, Any]]:
        value=self.request_json("/api/arcz/tools/catalog")
        tools=value.get("tools") if isinstance(value,dict) else None
        if not isinstance(tools,list): raise ApiError("AEDIFEX_TOOL_CATALOG_INVALID","tools ausente",status=502)
        return [item for item in tools if isinstance(item,dict) and isinstance(item.get("name"),str)]

    def invoke_tool(self, name: str, arguments: dict[str, Any], *, project_id: str, expected_revision: int,
                    dry_run: bool = True, approval_id: str | None = None) -> dict[str, Any]:
        return self.request_json("/api/arcz/tools/invoke",method="POST",payload={"name":name,"arguments":arguments,
            "project_id":project_id,"expected_revision":expected_revision,"dry_run":bool(dry_run),"approval_id":approval_id})
