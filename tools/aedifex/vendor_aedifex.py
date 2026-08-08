#!/usr/bin/env python3
"""Vendoriza Aedifex de uma cópia local verificada, sem baixar nada em runtime.

Fluxo intencional:
  clone/local checkout -> backup upstream imutável -> fork controlado -> overlay ARCZ

O script NÃO cria fonte falsa quando o repositório não está disponível. Por
padrão exige um checkout Git no commit fixado em integrations/aedifex/upstream.lock.json.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
from typing import Iterable

EXCLUDED_COPY_NAMES = {"node_modules", ".next", ".turbo", "dist", "coverage"}


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def run(command: list[str], *, cwd: Path | None = None) -> str:
    result = subprocess.run(command, cwd=cwd, check=False, text=True, capture_output=True)
    if result.returncode:
        detail = (result.stderr or result.stdout).strip()
        raise RuntimeError(f"command failed ({result.returncode}): {' '.join(command)}\n{detail}")
    return result.stdout.strip()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_lock(root: Path) -> dict:
    path = root / "integrations" / "aedifex" / "upstream.lock.json"
    value = json.loads(path.read_text(encoding="utf-8"))
    commit = str(value.get("primary", {}).get("commit", ""))
    if len(commit) != 40 or any(ch not in "0123456789abcdef" for ch in commit):
        raise RuntimeError("upstream.lock.json has an invalid pinned commit")
    return value


def package_version(source: Path, package_path: str) -> str:
    value = json.loads((source / package_path / "package.json").read_text(encoding="utf-8"))
    return str(value.get("version", ""))


def verify_source(source: Path, lock: dict, *, expected_commit: str | None = None) -> dict:
    source = source.resolve()
    if not source.is_dir():
        raise RuntimeError(f"source directory not found: {source}")
    required = ["LICENSE", "package.json", "packages/core/package.json", "packages/editor/package.json"]
    missing = [item for item in required if not (source / item).is_file()]
    if missing:
        raise RuntimeError(f"Aedifex source is incomplete; missing: {', '.join(missing)}")
    if not (source / ".git").is_dir():
        raise RuntimeError("strict vendoring requires a Git checkout; archive snapshots are not accepted")
    wanted = expected_commit or lock["primary"]["commit"]
    actual = run(["git", "rev-parse", "HEAD"], cwd=source)
    if actual != wanted:
        raise RuntimeError(f"Aedifex commit mismatch: expected {wanted}, found {actual}")
    status = run(["git", "status", "--porcelain"], cwd=source)
    if status:
        raise RuntimeError("Aedifex source checkout is dirty; commit or discard changes before vendoring")
    license_text = (source / "LICENSE").read_text(encoding="utf-8", errors="replace")
    if "MIT License" not in license_text:
        raise RuntimeError("Aedifex license is not the expected MIT text")
    expected_packages = lock["primary"].get("packages", {})
    versions = {
        "@aedifex/core": package_version(source, "packages/core"),
        "@aedifex/editor": package_version(source, "packages/editor"),
    }
    for name, actual_version in versions.items():
        wanted_version = expected_packages.get(name)
        if wanted_version and wanted_version != "source-at-pinned-commit" and actual_version != wanted_version:
            raise RuntimeError(f"package version mismatch for {name}: expected {wanted_version}, found {actual_version}")
    return {
        "source": str(source),
        "commit": actual,
        "license_sha256": sha256_file(source / "LICENSE"),
        "package_versions": versions,
    }


def copy_overlay(root: Path, fork: Path) -> None:
    source = root / "integrations" / "aedifex" / "overlay"
    destination = fork / "apps" / "arcz-host"
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(source, destination)


def inventory_tree(root: Path, paths: Iterable[Path]) -> list[dict]:
    rows: list[dict] = []
    for base in paths:
        for path in sorted(p for p in base.rglob("*") if p.is_file() and ".git" not in p.parts):
            relative = path.relative_to(root).as_posix()
            rows.append({"path": relative, "bytes": path.stat().st_size, "sha256": sha256_file(path)})
    return rows


def clone_local(source: Path, destination: Path, commit: str) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    run(["git", "clone", "--local", "--no-hardlinks", str(source), str(destination)])
    run(["git", "checkout", "--detach", commit], cwd=destination)
    run(["git", "submodule", "update", "--init", "--recursive"], cwd=destination)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True, type=Path, help="Aedifex Git checkout already available locally")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--expected-commit", help="test-only override; normal execution uses upstream.lock.json")
    parser.add_argument("--replace", action="store_true", help="replace an existing controlled copy")
    args = parser.parse_args()

    root = args.root.resolve()
    lock = load_lock(root)
    receipt = verify_source(args.source, lock, expected_commit=args.expected_commit)
    commit = receipt["commit"]
    upstream = root / "Avangard One" / "opensources" / "upstream" / "aedifex" / commit
    fork = root / "Avangard One" / "opensources" / "forks" / "aedifex-arcz" / commit
    integration_link = root / "integrations" / "aedifex" / "vendor"

    for path in (upstream, fork, integration_link):
        if path.exists() or path.is_symlink():
            if not args.replace:
                raise RuntimeError(f"destination already exists: {path}; pass --replace only after auditing")
            if path.is_symlink() or path.is_file():
                path.unlink()
            else:
                shutil.rmtree(path)

    clone_local(args.source.resolve(), upstream, commit)
    clone_local(upstream, fork, commit)
    copy_overlay(root, fork)

    # Prefer a relative symlink. Windows without Developer Mode can replace it
    # with a junction; the build tool also accepts --vendor explicitly.
    integration_link.parent.mkdir(parents=True, exist_ok=True)
    try:
        integration_link.symlink_to(fork, target_is_directory=True)
        link_mode = "symlink"
    except OSError:
        shutil.copytree(fork, integration_link)
        link_mode = "copy"

    manifest = {
        "schema_version": 1,
        "created_at": utc_now(),
        "repository": lock["primary"]["repository"],
        "commit": commit,
        "backup": upstream.relative_to(root).as_posix(),
        "controlled_fork": fork.relative_to(root).as_posix(),
        "integration_path": integration_link.relative_to(root).as_posix(),
        "integration_link_mode": link_mode,
        "source_receipt": receipt,
        "overlay": "apps/arcz-host",
        "files": inventory_tree(root, [root / "integrations" / "aedifex" / "overlay"]),
    }
    manifest_path = root / "Avangard One" / "opensources" / "integrations" / "aedifex.json"
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    checksum_path = root / "Avangard One" / "opensources" / "checksums" / f"aedifex-{commit}.sha256"
    checksum_path.write_text(f"{sha256_file(manifest_path)}  {manifest_path.relative_to(root).as_posix()}\n", encoding="utf-8")
    print(json.dumps({"ok": True, "manifest": str(manifest_path), "commit": commit}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(json.dumps({"ok": False, "error": str(error)}, ensure_ascii=False), file=sys.stderr)
        raise SystemExit(1)
