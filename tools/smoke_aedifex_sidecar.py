#!/usr/bin/env python3
from __future__ import annotations

import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "vendor/aedifex-floorplanner"
MANIFEST = DIST / "arcz-aedifex-build.json"
PIN = "5319368bae16500ca5267f6f8d68b36c9586d5bb"
EVIDENCE_DIR = ROOT / "validation/aedifex"
SMOKE_LOG = EVIDENCE_DIR / "standalone-smoke.log"
SMOKE_RESULT = EVIDENCE_DIR / "standalone-smoke-result.json"


def request_json(url: str, *, token: str | None = None, timeout: float = 5.0) -> tuple[int, dict]:
    headers = {"Accept": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = Request(url, headers=headers)
    try:
        with urlopen(request, timeout=timeout) as response:  # noqa: S310 - loopback only below
            payload = json.loads(response.read().decode("utf-8"))
            return response.status, payload
    except HTTPError as error:
        payload = json.loads(error.read().decode("utf-8") or "{}")
        return error.code, payload


def log_tail(limit: int = 12000) -> str:
    try:
        text = SMOKE_LOG.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return "<smoke log indisponível>"
    return text[-limit:]


def write_result(payload: dict) -> None:
    EVIDENCE_DIR.mkdir(parents=True, exist_ok=True)
    SMOKE_RESULT.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )


def main() -> int:
    EVIDENCE_DIR.mkdir(parents=True, exist_ok=True)
    SMOKE_LOG.write_text("", encoding="utf-8")
    if SMOKE_RESULT.exists():
        SMOKE_RESULT.unlink()

    if not MANIFEST.is_file():
        raise SystemExit(f"build manifest ausente: {MANIFEST}")
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    runtime = manifest.get("runtime") or {}
    if runtime.get("loopback_only") is not True:
        raise SystemExit("manifest não declara sidecar loopback_only")
    if manifest.get("upstream_commit") != PIN:
        raise SystemExit("build pertence a outro commit Aedifex")

    port = int(runtime.get("port") or 8124)
    if port != 8124:
        raise SystemExit(f"porta inesperada para smoke: {port}")
    base = f"http://127.0.0.1:{port}"
    command = [str(part) for part in runtime.get("command") or []]
    if not command or command[0] != "node":
        raise SystemExit(f"comando runtime inesperado: {command}")
    cwd_rel = str(runtime.get("cwd") or ".")
    cwd = (DIST / cwd_rel).resolve()
    if DIST.resolve() not in {cwd, *cwd.parents}:
        raise SystemExit(f"cwd runtime escapa do vendor: {cwd}")
    if len(command) < 2:
        raise SystemExit(f"comando runtime incompleto: {command}")
    entry = cwd / command[1]
    if not entry.is_file():
        raise SystemExit(f"entrypoint standalone ausente: {entry}")

    token = secrets.token_urlsafe(32)
    env = os.environ.copy()
    env.update(
        {
            "HOSTNAME": "127.0.0.1",
            "PORT": str(port),
            "NODE_ENV": "production",
            "ARCZ_AEDIFEX_BRIDGE_TOKEN": token,
            "ARCZ_API_URL": "http://127.0.0.1:8123",
        }
    )

    process: subprocess.Popen[str] | None = None
    with SMOKE_LOG.open("w", encoding="utf-8", buffering=1) as log_handle:
        log_handle.write(
            json.dumps(
                {
                    "event": "start",
                    "cwd": str(cwd.relative_to(ROOT)),
                    "command": command,
                    "port": port,
                    "upstream_commit": PIN,
                },
                ensure_ascii=False,
            )
            + "\n"
        )
        process = subprocess.Popen(
            command,
            cwd=cwd,
            env=env,
            stdout=log_handle,
            stderr=subprocess.STDOUT,
            text=True,
        )
        try:
            deadline = time.monotonic() + 60.0
            health: dict | None = None
            last_error: Exception | None = None
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    log_handle.flush()
                    failure = {
                        "ok": False,
                        "phase": "startup",
                        "returncode": process.returncode,
                        "log": str(SMOKE_LOG.relative_to(ROOT)),
                    }
                    write_result(failure)
                    raise RuntimeError(
                        f"sidecar encerrou antes do health rc={process.returncode}\n"
                        f"LOG TAIL:\n{log_tail()}"
                    )
                try:
                    status, payload = request_json(
                        base + str(runtime.get("health_path") or "/api/health")
                    )
                    if status == 200:
                        health = payload
                        break
                except (URLError, TimeoutError, json.JSONDecodeError) as error:
                    last_error = error
                time.sleep(0.25)
            if health is None:
                log_handle.flush()
                write_result(
                    {
                        "ok": False,
                        "phase": "health_timeout",
                        "last_error": str(last_error) if last_error else None,
                        "log": str(SMOKE_LOG.relative_to(ROOT)),
                    }
                )
                raise RuntimeError(
                    f"sidecar não ficou saudável em 60s: {last_error}\nLOG TAIL:\n{log_tail()}"
                )
            if health.get("ok") is not True or health.get("service") != "arcz-aedifex-floorplanner":
                raise RuntimeError(f"health inválido: {health}")
            if health.get("upstream_commit") != PIN:
                raise RuntimeError(f"health reportou pin incorreto: {health}")

            catalog_path = str(runtime.get("tool_catalog_path") or "/api/arcz/tools/catalog")
            unauth_status, _ = request_json(base + catalog_path)
            if unauth_status != 401:
                raise RuntimeError(
                    f"catálogo sem token deveria retornar 401, retornou {unauth_status}"
                )
            auth_status, catalog = request_json(base + catalog_path, token=token, timeout=20.0)
            if auth_status != 200:
                raise RuntimeError(f"catálogo autenticado retornou {auth_status}: {catalog}")
            tools = catalog.get("tools")
            if catalog.get("schema_version") != 1 or not isinstance(tools, list) or len(tools) < 20:
                raise RuntimeError(f"catálogo MCP incompleto: {catalog}")
            names = {str(item.get("name")) for item in tools if isinstance(item, dict)}
            required = {"aedifex.get_scene", "aedifex.create_wall", "aedifex.export_glb"}
            missing = sorted(required - names)
            if missing:
                raise RuntimeError(f"ferramentas Aedifex obrigatórias ausentes: {missing}")
            if not any(
                item.get("requiresApproval") is True
                for item in tools
                if isinstance(item, dict)
            ):
                raise RuntimeError(
                    "catálogo MCP não marcou nenhuma mutação como requiresApproval"
                )
            result = {
                "ok": True,
                "health": health,
                "tool_count": len(tools),
                "required_tools": sorted(required),
                "unauthorized_catalog_status": unauth_status,
                "log": str(SMOKE_LOG.relative_to(ROOT)),
            }
            write_result(result)
            print(json.dumps(result, ensure_ascii=False))
            return 0
        except Exception as error:
            if not SMOKE_RESULT.exists():
                write_result(
                    {
                        "ok": False,
                        "phase": "verification",
                        "error": error.__class__.__name__,
                        "message": str(error),
                        "log": str(SMOKE_LOG.relative_to(ROOT)),
                    }
                )
            raise
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)


if __name__ == "__main__":
    raise SystemExit(main())
