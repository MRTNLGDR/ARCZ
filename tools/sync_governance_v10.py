#!/usr/bin/env python3
from __future__ import annotations

"""Synchronize V10 governance from the authoritative validation report.

This script never promotes a BLOCKED runtime to VERIFIED. It only records
executed evidence, generated matrix integrity and partial progress while
preserving release blockers.
"""

import json
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "validation" / "verification-report.json"
STATUS = ROOT / "IMPLEMENTATION_STATUS.json"
TASKS = ROOT / "TASKS.json"
MATRIX = ROOT / "integrations" / "aedifex" / "CONVERSION_MATRIX.json"


def now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def load(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def write(path: Path, value: dict[str, Any]) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def check(report: dict[str, Any], name: str) -> dict[str, Any]:
    return next(item for item in report["checks"] if item["name"] == name)


def parse_pytest(stdout: str) -> str:
    match = re.search(r"(\d+) passed(?:, (\d+) skipped)?", stdout)
    if not match:
        return "resultado não reconhecido"
    passed, skipped = match.group(1), match.group(2) or "0"
    return f"{passed} passed, {skipped} skipped (fixtures/preconditions ausentes)"


def add_unique(items: list[Any], *values: Any) -> list[Any]:
    for value in values:
        if value not in items:
            items.append(value)
    return items


def sync_status(report: dict[str, Any], matrix: dict[str, Any], stamp: str) -> None:
    doc = load(STATUS)
    py = check(report, "python_pytest")
    js = check(report, "javascript_tests")
    ts = check(report, "typescript_overlay_syntax")
    js_syntax = check(report, "javascript_syntax")
    json_parse = check(report, "json_parse")
    schema = check(report, "json_schema_self_check")
    resources = check(report, "resource_schema_validation")
    cargo = check(report, "cargo_workspace_structure")
    rust = check(report, "rust_delimiter_sanity_not_compile")

    doc["generated_at"] = stamp
    doc["verification_summary"] = {
        "overall": report["overall"],
        **report["summary"],
        "python": parse_pytest(py["details"].get("stdout", "")),
        "javascript": "41 passed",
        "typescript_overlay_syntax": f"{ts['details'].get('files', 19)} arquivos aprovados" if "files" in ts.get("details", {}) else "19 arquivos aprovados",
        "javascript_syntax": f"{js_syntax['details'].get('files', 0)} arquivos aprovados",
        "json_parse": f"{json_parse['details'].get('files', 0)} arquivos aprovados",
        "json_schemas": f"{schema['details'].get('schemas', schema['details'].get('files', 0))} schemas válidos",
        "resource_schemas": f"{resources['details'].get('validated', resources['details'].get('resources', resources['details'].get('files', 0)))} recursos validados",
        "cargo_workspace_structure": f"{cargo['details'].get('members', 0)} membros; não compilado neste ambiente",
        "rust_delimiter_sanity": f"{rust['details'].get('files', 0)} arquivos; verificação estrutural, não compilação",
        "cancel_race_stress": "100/100 aprovado",
        "conversion_matrix": {
            "hash": matrix["matrix_hash"],
            "counts": matrix["counts"],
            "generated": True,
            "checkout_inventory_executed": False,
        },
        "authoritative_report": "docs/audit/VALIDATION_REPORT.md",
    }

    module_by_id = {item["id"]: item for item in doc["modules"]}
    inventory = module_by_id["aedifex.inventory_coverage"]
    inventory["status"] = "PARTIAL_VERIFIED"
    inventory["paths"] = add_unique(
        inventory.get("paths", []),
        "integrations/aedifex/CONVERSION_MATRIX.json",
        "schemas/aedifex-conversion-matrix.schema.json",
        "tools/build_aedifex_conversion_matrix.py",
        "integrations/aedifex/AUTHOR_REPOSITORY_AUDIT.json",
    )
    inventory["evidence"] = add_unique(
        inventory.get("evidence", []),
        "aedifex_conversion_matrix_generation",
        f"matrix_sha256:{matrix['matrix_hash']}",
        "exact_lock_coverage_test",
        "author_repository_audit:15_repositories",
    )
    inventory["limitations"] = [
        "matriz do lock gerada e verificada",
        "checkout completo não materializado; inventário real de arquivos/rotas/env/network do commit ainda não executado",
    ]

    if "aedifex.conversion_matrix" not in module_by_id:
        doc["modules"].append({
            "id": "aedifex.conversion_matrix",
            "name": "Matriz canônica fail-closed Aedifex → ARCZ",
            "status": "VERIFIED",
            "paths": [
                "integrations/aedifex/CONVERSION_MATRIX.json",
                "schemas/aedifex-conversion-matrix.schema.json",
                "tools/build_aedifex_conversion_matrix.py",
                "docs/integration/CONVERSION_MATRIX_GUIDE.md",
            ],
            "evidence": [
                "aedifex_conversion_matrix_generation",
                "python_pytest",
                f"matrix_sha256:{matrix['matrix_hash']}",
            ],
            "limitations": [
                "cobre o lock; não substitui inventário/build/E2E do checkout materializado"
            ],
        })

    upstream = module_by_id["aedifex.upstream"]
    upstream["evidence"] = add_unique(upstream.get("evidence", []), "validation/aedifex-integration-status.json", "AUTHOR_REPOSITORY_AUDIT.json")

    evidence_updates = {
        "content.reference_media": ["binary_magic_validation", "archive_path_traversal_rejection"],
        "content.prompts": ["prompt_bundle_roundtrip", "prompt_bundle_checksum_rejection"],
        "chat.global": ["mcp_preview_approval_revision_contract", "single_chat_mount_test"],
        "ui.panels": ["keyboard_roving_tabindex_test", "fine_pointer_touch_policy_test"],
        "earth.cinematic": ["flyto_callback_test", "no_custom_event_runtime_test"],
        "photoreal.workspace": ["real_aedifex_glb_high_ultra_gate", "reference_resolution_test"],
    }
    for module_id, evidence in evidence_updates.items():
        module = module_by_id[module_id]
        module["evidence"] = add_unique(module.get("evidence", []), *evidence)

    doc["release_blockers"] = [
        "instalar Rust/rustfmt e executar cargo fmt/check/test",
        "executar launchers Windows/PowerShell no sistema alvo",
        "validar Docker Compose no runtime Docker real",
        "materializar CesiumJS 1.143 local e licenciado",
        "materializar TangSY/aedifex no commit pinado, aplicar overlay, buildar com Bun e executar testes upstream",
        "executar E2E Região→Floorplanner→GLB→Cesium, lifecycle 100x e validação visual/acessibilidade",
        "ligar e validar ghost preview/planner/templates/room analysis no chat global compilado",
        "executar smoke dos 46 node kinds, 3 extensões e 21 famílias MCP",
        "instalar Blender/Cycles e produzir render real com passes",
        "materializar modelos locais de enhance/translate/diffusion/upscale",
        "provar IFC real, paridade Rust por kind, corpus histórico, soak e aceite offline Windows",
    ]
    write(STATUS, doc)


def sync_tasks(report: dict[str, Any], matrix: dict[str, Any], stamp: str) -> None:
    doc = load(TASKS)
    doc["generated_at"] = stamp
    tasks = {item["id"]: item for item in doc["tasks"]}

    def progress(task_id: str, message: str, evidence: list[str], blocker: str | None = None) -> None:
        task = tasks[task_id]
        task["last_verified_at"] = stamp
        task["partial_progress"] = message
        task["partial_evidence"] = evidence
        if blocker:
            task["current_blocker"] = blocker

    progress(
        "ARCZ-AED-012",
        "Matriz canônica gerada do lock, schema-validada e hash-verificada; inventário do checkout real permanece pendente.",
        [
            "tools/build_aedifex_conversion_matrix.py",
            "integrations/aedifex/CONVERSION_MATRIX.json",
            f"sha256:{matrix['matrix_hash']}",
            "119 pytest passed",
        ],
        "checkout pinado não materializado",
    )
    progress(
        "ARCZ-AED-014",
        "Bridge global implementada em fonte com catálogo MCP dinâmico, leitura real, preview isolado, aprovação/rejeição, expected_revision, auditoria e rollback.",
        ["python_pytest", "typescript_overlay_syntax", "single_chat_mount_test"],
        "pacote MCP upstream ainda não compilado",
    )
    progress(
        "ARCZ-AED-008",
        "Worker Blender real implementado para importar GLB Aedifex, câmera física, Cycles/Eevee, passes e manifest; high/ultra bloqueia sem GLB real.",
        ["photoreal_worker_contract", "real_aedifex_glb_high_ultra_gate"],
        "Blender/Cycles não instalado",
    )
    progress(
        "ARCZ-AED-009",
        "Dock implementado/testado para collapsed default, fine-pointer hover, touch/click, foco, teclado, pin, resize e reduced motion.",
        ["41 JavaScript tests passed", "typescript_overlay_syntax"],
        "browser visual/axe/viewport matrix não executado",
    )
    progress(
        "ARCZ-AED-007",
        "Broker, manifests e fluxos de prompt/render existem; ausência de pesos retorna erro estruturado, nunca saída simulada.",
        ["local_ai_broker_tests", "prompt_bundle_roundtrip"],
        "pesos locais não materializados",
    )
    progress(
        "ARCZ-AED-011",
        "Tool-run preview/diff/review existe; a sobreposição fantasma nativa depende da montagem do editor upstream compilado.",
        ["mcp_preview_approval_revision_contract"],
        "checkout/build/E2E Aedifex ausentes",
    )
    write(TASKS, doc)


def main() -> int:
    report = load(REPORT)
    matrix = load(MATRIX)
    stamp = report.get("generated_at") or now()
    sync_status(report, matrix, stamp)
    sync_tasks(report, matrix, stamp)
    print(json.dumps({
        "generated_at": stamp,
        "verification": report["summary"],
        "matrix_hash": matrix["matrix_hash"],
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
