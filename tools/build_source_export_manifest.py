#!/usr/bin/env python3
from __future__ import annotations

"""Generate a current, non-recursive source export inventory for V10.1."""

import json
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "docs" / "audit" / "SOURCE_EXPORT_MANIFEST.json"
FORBIDDEN_PARTS = {".git", ".pytest_cache", "__pycache__", "target", "dist", "build"}
FORBIDDEN_SUFFIXES = {".pyc", ".pyo"}
EXCLUDED_FILES = {
    "MANIFESTO.json", "SHA256SUMS.txt", "docs/audit/SOURCE_EXPORT_MANIFEST.json"
}
TEXT_SUFFIXES = {
    ".py", ".js", ".mjs", ".ts", ".tsx", ".rs", ".json", ".md", ".txt",
    ".css", ".html", ".toml", ".yml", ".yaml", ".sh", ".ps1", ".bat", ".sql",
}


def eligible(path: Path) -> bool:
    rel = path.relative_to(ROOT)
    if rel.as_posix() in EXCLUDED_FILES:
        return False
    if any(part in FORBIDDEN_PARTS for part in rel.parts):
        return False
    if path.suffix.lower() in FORBIDDEN_SUFFIXES:
        return False
    if ".sqlite3" in path.name and rel.parts[:1] in {("jobs",), ("data",)}:
        return False
    if rel.parts[:3] in {("data", "floorplanner", "exports"), ("data", "media", "content")} and path.name != ".gitkeep":
        return False
    if rel.parts[:1] == ("logs",) and path.name != ".gitkeep":
        return False
    return path.is_file()


def main() -> int:
    files = sorted(path for path in ROOT.rglob("*") if eligible(path))
    suffixes: Counter[str] = Counter()
    top_dirs: Counter[str] = Counter()
    total_bytes = 0
    text_lines = 0
    text_files = 0
    for path in files:
        rel = path.relative_to(ROOT)
        total_bytes += path.stat().st_size
        suffixes[path.suffix.lower() or "<none>"] += 1
        top_dirs[rel.parts[0]] += 1
        if path.suffix.lower() in TEXT_SUFFIXES:
            try:
                text_lines += sum(1 for _ in path.open("r", encoding="utf-8", errors="strict"))
                text_files += 1
            except UnicodeDecodeError:
                pass

    matrix = json.loads((ROOT / "integrations/aedifex/CONVERSION_MATRIX.json").read_text(encoding="utf-8"))
    verification = json.loads((ROOT / "validation/verification-report.json").read_text(encoding="utf-8"))
    aedifex = json.loads((ROOT / "validation/aedifex-integration-status.json").read_text(encoding="utf-8"))
    result = {
        "schema_version": 2,
        "product": "ARCZ Earth + Aedifex Global",
        "handoff_version": "10.1.0",
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "scope": "release source tree excluding generated/runtime/build artifacts",
        "counts": {
            "files": len(files),
            "bytes": total_bytes,
            "text_files_counted": text_files,
            "text_lines": text_lines,
            "by_extension": dict(sorted(suffixes.items())),
            "by_top_level": dict(sorted(top_dirs.items())),
        },
        "aedifex_conversion": {
            "repository": matrix["upstream"]["repository"],
            "commit": matrix["upstream"]["commit"],
            "matrix_hash": matrix["matrix_hash"],
            "counts": matrix["counts"],
            "upstream_materialized": bool(aedifex.get("commit_verified")),
            "runtime_ready": bool(aedifex.get("ready")),
        },
        "verification": {
            "overall": verification["overall"],
            "summary": verification["summary"],
            "report": "docs/audit/VALIDATION_REPORT.md",
        },
        "critical_paths": [
            "README.md", "LEIA-PRIMEIRO.md", "AGENTS.md", "TASKS.json",
            "IMPLEMENTATION_STATUS.json", "integrations/aedifex/CONVERSION_MATRIX.json",
            "integrations/aedifex/AUTHOR_REPOSITORY_AUDIT.json",
            "docs/integration/USER_REQUIREMENT_TRACEABILITY_V10.md",
            "validation/aedifex-integration-status.json",
        ],
        "excluded": sorted(EXCLUDED_FILES),
        "warnings": [
            "Aedifex upstream source bytes are not embedded because the pinned checkout could not be materialized in this environment.",
            "CesiumJS, Blender, Rust toolchain and local AI model weights are not embedded.",
            "File/line counts are inventory evidence, not proof that blocked runtimes work.",
        ],
    }
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result["counts"], ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
