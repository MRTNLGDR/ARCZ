#!/usr/bin/env python3
"""Build and verify the controlled local Aedifex sidecar.

Default behavior is air-gapped: Bun must satisfy the lockfile from its local
cache. Network is permitted only with both --allow-network and
ARCZ_NETWORK_MODE=import_assisted. The build is refused unless the generated
upstream inventory and conversion coverage report match the pinned commit and
contain zero blockers.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from arcz_server.atomic_io import atomic_write_json
from arcz_server.hashing import sha256_file

FORK = ROOT / "opensources/forks/aedifex-arcz"
DIST = ROOT / "vendor/aedifex-floorplanner"
INTEGRATION = ROOT / "integrations/aedifex"
LOCK = json.loads((INTEGRATION / "UPSTREAM_LOCK.json").read_text(encoding="utf-8"))


def tree_integrity(root: Path, *, excluded: set[str] | None = None) -> dict[str, object]:
    excluded = excluded or set()
    digest = hashlib.sha256()
    count = 0
    total = 0
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.is_symlink():
            continue
        rel = path.relative_to(root).as_posix()
        if rel in excluded:
            continue
        size = path.stat().st_size
        file_hash = sha256_file(path)
        digest.update(rel.encode("utf-8")); digest.update(b"\0")
        digest.update(str(size).encode("ascii")); digest.update(b"\0")
        digest.update(file_hash.encode("ascii")); digest.update(b"\n")
        count += 1
        total += size
    return {
        "algorithm": "sha256",
        "file_count": count,
        "total_bytes": total,
        "tree_sha256": digest.hexdigest(),
    }


def run(args: list[str], cwd: Path, *, timeout: int = 1800) -> dict[str, object]:
    completed = subprocess.run(
        args,
        cwd=cwd,
        text=True,
        capture_output=True,
        timeout=timeout,
        check=False,
    )
    record = {
        "command": args,
        "cwd": str(cwd),
        "returncode": completed.returncode,
        "stdout_tail": completed.stdout[-12000:],
        "stderr_tail": completed.stderr[-12000:],
    }
    if completed.returncode:
        raise RuntimeError(
            f"{' '.join(args)}\nSTDOUT:\n{completed.stdout}\nSTDERR:\n{completed.stderr}"
        )
    return record


def require_coverage() -> tuple[dict, dict]:
    inventory_path = INTEGRATION / "generated/UPSTREAM_INVENTORY.json"
    coverage_path = INTEGRATION / "generated/CONVERSION_COVERAGE_REPORT.json"
    if not inventory_path.is_file() or not coverage_path.is_file():
        raise RuntimeError("Inventário/cobertura ausentes; execute tools/vendor_aedifex.py")
    inventory = json.loads(inventory_path.read_text(encoding="utf-8"))
    coverage = json.loads(coverage_path.read_text(encoding="utf-8"))
    if inventory.get("commit") != LOCK["commit"] or coverage.get("upstream_commit") != LOCK["commit"]:
        raise RuntimeError("Inventário/cobertura pertencem a outro commit Aedifex")
    if coverage.get("inventory_hash") != inventory.get("inventory_hash"):
        raise RuntimeError("Cobertura não corresponde ao inventário materializado")
    if coverage.get("ready") is not True or coverage.get("blockers"):
        raise RuntimeError("Cobertura Aedifex contém itens bloqueados")
    return inventory, coverage


def find_server(standalone: Path) -> Path:
    candidates = sorted(standalone.rglob("server.js"), key=lambda path: (len(path.parts), str(path)))
    for candidate in candidates:
        if "arcz-floorplanner" in candidate.as_posix():
            return candidate
    if len(candidates) == 1:
        return candidates[0]
    raise RuntimeError(f"Não foi possível identificar server.js: {candidates}")


def package_quality_commands(bun: str) -> list[tuple[list[str], Path]]:
    commands: list[tuple[list[str], Path]] = []
    required_paths = [str(spec["path"]) for spec in LOCK.get("packages", {}).values()]
    required_paths.append("apps/arcz-floorplanner/package.json")
    seen: set[tuple[str, ...]] = set()
    for relative in required_paths:
        package_path = FORK / relative
        if not package_path.is_file():
            raise RuntimeError(f"Manifesto obrigatório ausente no fork: {relative}")
        package = json.loads(package_path.read_text(encoding="utf-8"))
        scripts = package.get("scripts") if isinstance(package.get("scripts"), dict) else {}
        directory = package_path.parent
        for script_name in ("check-types", "typecheck", "test"):
            if script_name not in scripts:
                continue
            key = (str(directory), script_name)
            if key in seen:
                continue
            seen.add(key)
            commands.append(([bun, "run", script_name], directory))
    return commands


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--allow-network", action="store_true")
    parser.add_argument("--skip-install", action="store_true")
    parser.add_argument("--skip-package-tests", action="store_true")
    args = parser.parse_args()
    if args.allow_network and os.environ.get("ARCZ_NETWORK_MODE") != "import_assisted":
        raise SystemExit("--allow-network exige ARCZ_NETWORK_MODE=import_assisted")
    if not FORK.is_dir():
        raise SystemExit("Fork ausente. Execute tools/vendor_aedifex.py primeiro.")
    inventory, coverage = require_coverage()
    bun = shutil.which("bun")
    node = shutil.which("node")
    if not bun:
        raise SystemExit("Bun não instalado; build Aedifex não executado.")
    if not node:
        raise SystemExit("Node não instalado; runtime standalone não pode ser validado.")

    evidence: list[dict[str, object]] = []
    if not args.skip_install:
        install = [bun, "install", "--frozen-lockfile"]
        if not args.allow_network:
            install.append("--offline")
        evidence.append(run(install, FORK))

    if not args.skip_package_tests:
        for command, cwd in package_quality_commands(bun):
            evidence.append(run(command, cwd))

    evidence.append(run([bun, "run", "build"], FORK / "apps/arcz-floorplanner"))

    app = FORK / "apps/arcz-floorplanner"
    for name in ("web-ifc.wasm", "web-ifc-mt.wasm"):
        wasm = app / "public" / name
        if not wasm.is_file() or wasm.stat().st_size < 100_000:
            raise RuntimeError(f"Parser IFC WASM ausente/inválido após prebuild: {wasm}")

    standalone = app / ".next/standalone"
    if not standalone.is_dir():
        raise RuntimeError(f"Next standalone ausente: {standalone}")
    entry = find_server(standalone)
    entry_rel = entry.relative_to(standalone)
    if DIST.exists():
        shutil.rmtree(DIST)
    shutil.copytree(standalone, DIST, symlinks=False)
    entry_out = DIST / entry_rel

    static_source = app / ".next/static"
    static_destination = entry_out.parent / ".next/static"
    if static_source.is_dir():
        static_destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(static_source, static_destination, dirs_exist_ok=True)
    public_source = app / "public"
    if public_source.is_dir():
        shutil.copytree(public_source, entry_out.parent / "public", dirs_exist_ok=True)

    entry_rel_dist = entry_out.relative_to(DIST).as_posix()
    integrity = tree_integrity(DIST, excluded={"arcz-aedifex-build.json"})
    integrity.update({"entry_path": entry_rel_dist, "entry_sha256": sha256_file(entry_out)})
    wasm_integrity = {
        name: {
            "sha256": sha256_file(entry_out.parent / "public" / name),
            "bytes": (entry_out.parent / "public" / name).stat().st_size,
        }
        for name in ("web-ifc.wasm", "web-ifc-mt.wasm")
    }
    manifest = {
        "schema_version": 3,
        "upstream_commit": LOCK["commit"],
        "inventory_hash": inventory["inventory_hash"],
        "coverage_report_hash": coverage["report_hash"],
        "built_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "builder": "tools/build_aedifex_sidecar.py",
        "runtime": {
            "command": ["node", entry_out.name],
            "cwd": entry_out.parent.relative_to(DIST).as_posix() or ".",
            "port": 8124,
            "health_path": "/api/health",
            "tool_catalog_path": "/api/arcz/tools/catalog",
            "tool_invoke_path": "/api/arcz/tools/invoke",
            "requires_bridge_token": True,
            "loopback_only": True,
        },
        "quality_commands": evidence,
        "wasm_integrity": wasm_integrity,
        "integrity": integrity,
    }
    atomic_write_json(DIST / "arcz-aedifex-build.json", manifest)
    print(DIST)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
