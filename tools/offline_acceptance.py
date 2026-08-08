#!/usr/bin/env python3
"""Executa os gates automatizáveis do aceite local-first do ARCZ Earth.

Este script é deliberadamente honesto:

* não tenta acessar a internet;
* não converte ausência do worker/Cesium em sucesso;
* não substitui o smoke test de navegador, GPU ou firewall;
* emite ``FAILED`` para contrato violado e ``BLOCKED`` para pré-requisito
  externo ainda não materializado.

Códigos de saída:
    0  todos os checks automatizáveis passaram (ainda há checks manuais);
    1  pelo menos um check falhou;
    2  nenhum check falhou, porém existe bloqueio objetivo.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from arcz_server.network_policy import NetworkMode, NetworkPolicy
from arcz_server.schema_validation import SchemaRegistry
from arcz_server.source_registry import SourceRegistry

CESIUM_REQUIRED_FILES = (
    "vendor/cesium/Cesium/Cesium.js",
    "vendor/cesium/Cesium/Widgets/widgets.css",
    "vendor/cesium/Cesium/Assets/Textures/NaturalEarthII/tilemapresource.xml",
    "vendor/cesium/LICENSE.md",
    "vendor/cesium/manifest.json",
)


def _check_network_policy(policy: NetworkPolicy) -> list[dict[str, Any]]:
    checks: list[dict[str, Any]] = []
    for host in ("example.com", "8.8.8.8"):
        allowed = policy.allows_host(host)
        checks.append({
            "name": f"remote_denied:{host}",
            "status": "FAILED" if allowed else "PASSED",
            "details": {"allowed": allowed, "mode": policy.mode.value},
        })
    for host in ("127.0.0.1", "localhost", "::1"):
        allowed = policy.allows_host(host)
        checks.append({
            "name": f"loopback_allowed:{host}",
            "status": "PASSED" if allowed else "FAILED",
            "details": {"allowed": allowed, "mode": policy.mode.value},
        })
    return checks


def _check_cesium_vendor(root: Path) -> dict[str, Any]:
    missing = [relative for relative in CESIUM_REQUIRED_FILES if not (root / relative).is_file()]
    return {
        "name": "cesium_local_vendor",
        "status": "BLOCKED" if missing else "PASSED",
        "missing": missing,
        "remediation": (
            "python tools/vendor_cesium.py --source <Build/Cesium-local> "
            "--license-file <LICENSE-local> --version 1.143.0"
        ) if missing else None,
    }


def _check_generation_worker(root: Path) -> dict[str, Any]:
    candidates = [
        root / "target" / "release" / "arcz-generation-cli",
        root / "target" / "release" / "arcz-generation-cli.exe",
        root / "target" / "debug" / "arcz-generation-cli",
        root / "target" / "debug" / "arcz-generation-cli.exe",
    ]
    found = next((path for path in candidates if path.is_file()), None)
    return {
        "name": "generation_worker_built",
        "status": "PASSED" if found else "BLOCKED",
        "worker": str(found) if found else None,
        "searched": [str(path) for path in candidates],
        "remediation": None if found else "python tools/build_generation_worker.py",
    }


def build_report(*, root: Path, required_source_kinds: list[str]) -> dict[str, Any]:
    """Monta o relatório sem executar rede nem alterar o projeto."""
    checks: list[dict[str, Any]] = []
    policy = NetworkPolicy(mode=NetworkMode.OFFLINE_STRICT)
    checks.extend(_check_network_policy(policy))

    schemas = SchemaRegistry(root / "schemas")
    registry = SourceRegistry(root / "data", schemas)
    installed = registry.list()
    for kind in required_source_kinds:
        matches = [row for row in installed if row["kind"] == kind]
        checks.append({
            "name": f"source_installed:{kind}",
            "status": "PASSED" if matches else "BLOCKED",
            "count": len(matches),
            "reason": None if matches else "nenhum pacote local materializado para o tipo exigido",
        })

    checks.append(_check_generation_worker(root))
    checks.append(_check_cesium_vendor(root))

    manual = [
        "navegador/Cesium real abre com DNS e egress bloqueados",
        "busca de região funciona exclusivamente sobre índice local",
        "geração e render concluem usando somente pacotes locais verificados",
        "cancelamento remove primitivas, listeners, timers e staging",
        "save/crash/reopen perde no máximo 5 segundos e mantém JSON parseável",
        "auditoria de sockets/firewall confirma zero egress em offline_strict",
        "captura de frame confirma Natural Earth local e ausência de requests CDN",
    ]
    failed = any(check["status"] == "FAILED" for check in checks)
    blocked = any(check["status"] == "BLOCKED" for check in checks)
    overall = "FAILED" if failed else "BLOCKED" if blocked else "PARTIAL_PASS"
    return {
        "schema_version": 2,
        "mode": NetworkMode.OFFLINE_STRICT.value,
        "automated_checks": checks,
        "manual_checks_required": manual,
        "overall": overall,
        "notice": (
            "PARTIAL_PASS significa que somente os checks automatizáveis deste script passaram; "
            "os checks manuais continuam obrigatórios."
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--require-source-kind",
        action="append",
        default=[],
        help="tipo de pacote local que precisa estar instalado; pode ser repetido",
    )
    parser.add_argument(
        "--report",
        type=Path,
        default=ROOT / "validation" / "offline-acceptance.json",
    )
    args = parser.parse_args()

    report = build_report(root=ROOT, required_source_kinds=args.require_source_kind)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(args.report)
    return 1 if report["overall"] == "FAILED" else 2 if report["overall"] == "BLOCKED" else 0


if __name__ == "__main__":
    raise SystemExit(main())
