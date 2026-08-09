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


def main() -> int:
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
    entry = cwd / command[1]
    if len(command) < 2 or not entry.is_file():
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
    process = subprocess.Popen(
        command,
        cwd=cwd,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        deadline = time.monotonic() + 60.0
        health: dict | None = None
        last_error: Exception | None = None
        while time.monotonic() < deadline:
            if process.poll() is not None:
                stdout, stderr = process.communicate(timeout=5)
                raise RuntimeError(
                    f"sidecar encerrou antes do health rc={process.returncode}\n"
                    f"STDOUT:\n{stdout[-8000:]}\nSTDERR:\n{stderr[-8000:]}"
                )
            try:
                status, payload = request_json(base + str(runtime.get("health_path") or "/api/health"))
                if status == 200:
                    health = payload
                    break
            except (URLError, TimeoutError, json.JSONDecodeError) as error:
                last_error = error
            time.sleep(0.25)
        if health is None:
            raise RuntimeError(f"sidecar não ficou saudável em 60s: {last_error}")
        if health.get("ok") is not True or health.get("service") != "arcz-aedifex-floorplanner":
            raise RuntimeError(f"health inválido: {health}")
        if health.get("upstream_commit") != PIN:
            raise RuntimeError(f"health reportou pin incorreto: {health}")

        catalog_path = str(runtime.get("tool_catalog_path") or "/api/arcz/tools/catalog")
        unauth_status, _ = request_json(base + catalog_path)
        if unauth_status != 401:
            raise RuntimeError(f"catálogo sem token deveria retornar 401, retornou {unauth_status}")
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
        if not any(item.get("requiresApproval") is True for item in tools if isinstance(item, dict)):
            raise RuntimeError("catálogo MCP não marcou nenhuma mutação como requiresApproval")
        print(
            json.dumps(
                {
                    "ok": True,
                    "health": health,
                    "tool_count": len(tools),
                    "required_tools": sorted(required),
                    "unauthorized_catalog_status": unauth_status,
                },
                ensure_ascii=False,
            )
        )
        return 0
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
