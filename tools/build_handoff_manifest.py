#!/usr/bin/env python3
"""Gera manifesto SHA-256 reproduzível da árvore de handoff.

`MANIFESTO.json` exclui a si mesmo e `SHA256SUMS.txt` para evitar recursão.
`SHA256SUMS.txt` inclui o manifesto e exclui apenas a si próprio.
Artefatos voláteis/builds não entram na lista.
"""
from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sys
from typing import Iterable

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "MANIFESTO.json"
SUMS = ROOT / "SHA256SUMS.txt"

FORBIDDEN_PARTS = {".git", ".pytest_cache", "__pycache__", "target", "dist", "build"}
FORBIDDEN_SUFFIXES = {".pyc", ".pyo"}
SELF_EXCLUDED = {"MANIFESTO.json", "SHA256SUMS.txt"}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def is_release_file(path: Path) -> bool:
    relative = path.relative_to(ROOT)
    if any(part in FORBIDDEN_PARTS for part in relative.parts):
        return False
    if path.suffix.lower() in FORBIDDEN_SUFFIXES:
        return False
    if ".sqlite3" in path.name and (relative.parts[:1] == ("jobs",) or relative.parts[:1] == ("data",)):
        return False
    if relative.parts[:3] in {("data", "floorplanner", "exports"), ("data", "media", "content")} and path.name != ".gitkeep":
        return False
    if relative.parts[:1] == ("logs",) and path.name != ".gitkeep":
        return False
    return path.is_file()


def release_files(*, exclude: Iterable[str] = ()) -> list[Path]:
    excluded = set(exclude)
    return sorted(
        path for path in ROOT.rglob("*")
        if is_release_file(path) and path.relative_to(ROOT).as_posix() not in excluded
    )


def main() -> int:
    report_path = ROOT / "validation" / "verification-report.json"
    if not report_path.is_file():
        raise SystemExit("Execute tools/verify_handoff.py antes de gerar o manifesto")
    report = json.loads(report_path.read_text(encoding="utf-8"))

    files = release_files(exclude=SELF_EXCLUDED)
    entries = [
        {
            "path": path.relative_to(ROOT).as_posix(),
            "bytes": path.stat().st_size,
            "sha256": sha256_file(path),
        }
        for path in files
    ]
    blockers = [
        {"name": check["name"], "details": check.get("details", {})}
        for check in report.get("checks", []) if check.get("status") == "BLOCKED"
    ]
    manifest = {
        "schema_version": 1,
        "product": "ARCZ Earth + Aedifex Global",
        "handoff_version": "10.1.0",
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "source_export_manifest": "docs/audit/SOURCE_EXPORT_MANIFEST.json",
        "verification": {
            "overall": report.get("overall"),
            "summary": report.get("summary"),
            "generated_at": report.get("generated_at"),
            "authoritative_report": "docs/audit/VALIDATION_REPORT.md",
            "blockers": blockers,
        },
        "integrity": {
            "algorithm": "sha256",
            "manifest_self_excluded": True,
            "sha256sums_self_excluded": True,
        },
        "excluded": sorted(SELF_EXCLUDED),
        "file_count": len(entries),
        "total_bytes": sum(item["bytes"] for item in entries),
        "files": entries,
    }
    MANIFEST.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    sum_files = release_files(exclude={"SHA256SUMS.txt"})
    SUMS.write_text(
        "".join(f"{sha256_file(path)}  {path.relative_to(ROOT).as_posix()}\n" for path in sum_files),
        encoding="utf-8",
    )

    # Auto-verificação antes de publicar sucesso.
    for item in entries:
        path = ROOT / item["path"]
        if path.stat().st_size != item["bytes"] or sha256_file(path) != item["sha256"]:
            raise RuntimeError(f"manifesto divergente: {item['path']}")
    print(json.dumps({
        "ok": True,
        "files": len(entries),
        "bytes": manifest["total_bytes"],
        "manifest": str(MANIFEST),
        "sha256sums": str(SUMS),
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
