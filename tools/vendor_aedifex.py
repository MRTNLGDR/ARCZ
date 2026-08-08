#!/usr/bin/env python3
"""Materialize the pinned Aedifex source and apply removable ARCZ overlays.

This command is the only supported path for bringing Aedifex into the ARCZ
workspace. It preserves an immutable upstream copy, inventories the complete
source tree, fails closed on unclassified surfaces, then creates a controlled
fork and copies the ARCZ host/bridge packages into it.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from arcz_server.aedifex_inventory import inventory_upstream, validate_coverage
from arcz_server.atomic_io import atomic_write_json
from arcz_server.hashing import sha256_file
from arcz_server.schema_validation import SchemaRegistry

LOCK_PATH = ROOT / "integrations/aedifex/UPSTREAM_LOCK.json"
POLICY_PATH = ROOT / "integrations/aedifex/CONVERSION_COVERAGE.json"
PATCH_PATH = ROOT / "integrations/aedifex/PATCH_MANIFEST.json"
LOCK = json.loads(LOCK_PATH.read_text(encoding="utf-8"))
SCHEMAS = SchemaRegistry(ROOT / "schemas")


def tree_fingerprint(root: Path) -> dict[str, object]:
    import hashlib

    digest = hashlib.sha256()
    count = 0
    total = 0
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.is_symlink():
            continue
        rel = path.relative_to(root).as_posix()
        size = path.stat().st_size
        file_hash = sha256_file(path)
        digest.update(rel.encode("utf-8")); digest.update(b"\0")
        digest.update(str(size).encode("ascii")); digest.update(b"\0")
        digest.update(file_hash.encode("ascii")); digest.update(b"\n")
        count += 1
        total += size
    return {"file_count": count, "total_bytes": total, "tree_sha256": digest.hexdigest()}


def run(args: list[str], cwd: Path | None = None) -> str:
    completed = subprocess.run(args, cwd=cwd, text=True, capture_output=True, check=False)
    if completed.returncode:
        raise RuntimeError(
            f"{' '.join(map(str, args))}\nSTDOUT:\n{completed.stdout}\nSTDERR:\n{completed.stderr}"
        )
    return completed.stdout.strip()


def copytree(source: Path, destination: Path) -> None:
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(
        source,
        destination,
        symlinks=False,
        ignore=shutil.ignore_patterns("node_modules", ".next", "dist", "coverage", ".turbo", ".git"),
    )


def verify_source(source: Path) -> str:
    license_path = source / "LICENSE"
    if not license_path.is_file() or "MIT License" not in license_path.read_text(errors="ignore"):
        raise RuntimeError("Licença MIT do upstream não foi verificada")
    if (source / ".git").exists():
        head = run(["git", "-C", str(source), "rev-parse", "HEAD"])
    else:
        marker = source / "UPSTREAM_COMMIT"
        if not marker.is_file():
            raise RuntimeError("Checkout sem .git e sem UPSTREAM_COMMIT")
        head = marker.read_text(encoding="utf-8").strip()
    if head != LOCK["commit"]:
        raise RuntimeError(f"Commit incorreto: {head}; esperado {LOCK['commit']}")
    for rel in LOCK.get("required_workspace_paths", []):
        if not (source / rel).is_file():
            raise RuntimeError(f"Arquivo obrigatório ausente: {rel}")
    for name, spec in LOCK.get("packages", {}).items():
        path = source / str(spec["path"])
        package = json.loads(path.read_text(encoding="utf-8"))
        if package.get("name") != name or package.get("version") != spec["version"]:
            raise RuntimeError(
                f"Pacote incompatível {name}: {package.get('name')}@{package.get('version')}; "
                f"esperado {spec['version']}"
            )
    return head


def audit_source(source: Path) -> tuple[dict, dict]:
    inventory = inventory_upstream(source, expected_commit=LOCK["commit"])
    policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
    SCHEMAS.validate("aedifex-conversion-policy.schema.json", policy)
    SCHEMAS.validate("aedifex-upstream-inventory.schema.json", inventory)
    report = validate_coverage(inventory, policy)
    SCHEMAS.validate("aedifex-conversion-coverage.schema.json", report)

    evidence_dir = ROOT / "validation/aedifex"
    atomic_write_json(evidence_dir / "UPSTREAM_INVENTORY.json", inventory)
    atomic_write_json(evidence_dir / "CONVERSION_COVERAGE_REPORT.json", report)
    if not report["ready"]:
        summary = ", ".join(
            f"{item['category']}:{item['id']}={item['status']}" for item in report["blockers"][:25]
        )
        raise RuntimeError(
            "Conversão Aedifex bloqueada por superfícies não admitidas. "
            f"Atualize CONVERSION_COVERAGE.json após auditoria: {summary}"
        )
    return inventory, report


def merge_workspace(fork: Path) -> None:
    package_path = fork / "package.json"
    document = json.loads(package_path.read_text(encoding="utf-8"))
    workspaces = document.setdefault("workspaces", [])
    if not isinstance(workspaces, list):
        raise RuntimeError("package.json upstream possui workspaces em formato não suportado")
    for item in (
        "apps/arcz-floorplanner",
        "packages/arcz-bridge",
        "packages/arcz-aedifex-tools",
    ):
        if item not in workspaces:
            workspaces.append(item)
    package_path.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path)
    parser.add_argument("--clone", action="store_true")
    args = parser.parse_args()
    source = args.source
    if args.clone:
        if os.environ.get("ARCZ_NETWORK_MODE") != "import_assisted":
            raise SystemExit("--clone exige ARCZ_NETWORK_MODE=import_assisted")
        source = ROOT / "opensources/.incoming/aedifex"
        source.parent.mkdir(parents=True, exist_ok=True)
        if source.exists():
            shutil.rmtree(source)
        run(["git", "clone", "--filter=blob:none", "https://github.com/TangSY/aedifex.git", str(source)])
        run(["git", "-C", str(source), "checkout", "--detach", LOCK["commit"]])
    if not source or not source.resolve().is_dir():
        raise SystemExit("Forneça --source <checkout-local> ou --clone explicitamente")

    source = source.resolve()
    verify_source(source)
    inventory, coverage = audit_source(source)

    upstream = ROOT / "opensources/upstream/aedifex"
    fork = ROOT / "opensources/forks/aedifex-arcz"
    copytree(source, upstream)
    (upstream / "UPSTREAM_COMMIT").write_text(LOCK["commit"] + "\n", encoding="utf-8")
    copytree(upstream, fork)

    overlay = ROOT / "integrations/aedifex/overlay"
    for rel in (
        "apps/arcz-floorplanner",
        "packages/arcz-bridge",
        "packages/arcz-aedifex-tools",
    ):
        source_overlay = overlay / rel
        if not source_overlay.is_dir():
            raise RuntimeError(f"Overlay obrigatório ausente: {source_overlay}")
        copytree(source_overlay, fork / rel)
    merge_workspace(fork)

    generated = ROOT / "integrations/aedifex/generated"
    atomic_write_json(generated / "UPSTREAM_INVENTORY.json", inventory)
    atomic_write_json(generated / "CONVERSION_COVERAGE_REPORT.json", coverage)

    output = ROOT / "opensources/integrations/aedifex-materialization.json"
    atomic_write_json(output, {
        "schema_version": 3,
        "commit": LOCK["commit"],
        "upstream": "opensources/upstream/aedifex",
        "fork": "opensources/forks/aedifex-arcz",
        "license_sha256": sha256_file(upstream / "LICENSE"),
        "upstream_integrity": tree_fingerprint(upstream),
        "fork_integrity": tree_fingerprint(fork),
        "overlay_manifest": "integrations/aedifex/PATCH_MANIFEST.json",
        "overlay_manifest_sha256": sha256_file(PATCH_PATH),
        "inventory_hash": inventory["inventory_hash"],
        "coverage_report_hash": coverage["report_hash"],
        "coverage_ready": True,
    })
    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
