#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "upstreams" / "manifest.toml"
EVIDENCE_ROOT = ROOT / "validation" / "upstreams"


def run(cmd: list[str], cwd: Path | None = None, dry: bool = False) -> str:
    printable = " ".join(map(str, cmd))
    print(f"+ {printable}")
    if dry:
        return ""
    proc = subprocess.run(cmd, cwd=cwd, text=True, capture_output=True, check=False)
    if proc.returncode:
        sys.stderr.write(proc.stdout + proc.stderr)
        raise SystemExit(proc.returncode)
    return proc.stdout.strip()


def run_bytes(cmd: list[str], cwd: Path) -> bytes:
    print("+ " + " ".join(map(str, cmd)))
    proc = subprocess.run(cmd, cwd=cwd, capture_output=True, check=False)
    if proc.returncode:
        sys.stderr.buffer.write(proc.stdout + proc.stderr)
        raise SystemExit(proc.returncode)
    return proc.stdout


def git_object_evidence(checkout: Path, paths: list[str], label: str) -> list[dict[str, object]]:
    if not paths:
        raise SystemExit(f"manifest must declare at least one {label} path")
    evidence: list[dict[str, object]] = []
    for raw_path in paths:
        path = str(raw_path).strip().replace("\\", "/")
        if not path or path.startswith("/") or ".." in Path(path).parts:
            raise SystemExit(f"unsafe {label} path in manifest: {raw_path!r}")
        payload = run_bytes(["git", "show", f"HEAD:{path}"], cwd=checkout)
        if not payload:
            raise SystemExit(f"empty {label} object at pinned HEAD: {path}")
        evidence.append(
            {
                "path": path,
                "bytes": len(payload),
                "sha256": hashlib.sha256(payload).hexdigest(),
            }
        )
    return evidence


def write_evidence(source: dict[str, object], checkout: Path, head: str) -> Path:
    licenses = git_object_evidence(
        checkout,
        [str(path) for path in source.get("legal_files", [])],
        "legal file",
    )
    boundaries = []
    if source.get("license_boundary_files"):
        boundaries = git_object_evidence(
            checkout,
            [str(path) for path in source.get("license_boundary_files", [])],
            "license-boundary file",
        )

    # Evidence belongs to ARCZ, never inside an immutable third-party checkout.
    EVIDENCE_ROOT.mkdir(parents=True, exist_ok=True)
    output = EVIDENCE_ROOT / f"{source['id']}.json"
    stamp = {
        "schema_version": 4,
        "id": source["id"],
        "repository": source["repository"],
        "commit": head,
        "declared_license": source["license"],
        "license_files": licenses,
        "license_boundary_files": boundaries,
        "checkout": str(checkout.relative_to(ROOT)).replace("\\", "/"),
        "immutable": True,
        "git_status_clean": True,
        "license_evidence_source": "declared_git_objects_at_pinned_head",
    }
    output.write_text(json.dumps(stamp, indent=2) + "\n", encoding="utf-8")
    return output


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Materialize exact ARCZ upstream snapshots without modifying them"
    )
    parser.add_argument("--only", action="append", default=[])
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument(
        "--reset", action="store_true", help="discard local changes in an upstream checkout"
    )
    args = parser.parse_args()

    data = tomllib.loads(MANIFEST.read_text(encoding="utf-8"))
    selected = set(args.only)
    known = {str(source["id"]) for source in data["source"]}
    unknown = selected - known
    if unknown:
        raise SystemExit("unknown upstream id(s): " + ", ".join(sorted(unknown)))

    for source in data["source"]:
        source_id = str(source["id"])
        if selected and source_id not in selected:
            continue
        checkout = ROOT / str(source["path"])
        if not checkout.exists():
            checkout.parent.mkdir(parents=True, exist_ok=True)
            run(
                [
                    "git",
                    "clone",
                    "--filter=blob:none",
                    "--no-checkout",
                    str(source["repository"]),
                    str(checkout),
                ],
                dry=args.dry_run,
            )
        if args.dry_run:
            print(
                f"  pin {source_id} -> {source['commit']} "
                f"legal={','.join(map(str, source.get('legal_files', [])))}"
            )
            continue

        if args.reset:
            run(["git", "reset", "--hard"], cwd=checkout)
            run(["git", "clean", "-fdx"], cwd=checkout)

        dirty = run(["git", "status", "--porcelain"], cwd=checkout)
        if dirty:
            raise SystemExit(f"refusing dirty immutable upstream: {source_id}\n{dirty}")

        run(["git", "remote", "set-url", "origin", str(source["repository"])], cwd=checkout)
        run(["git", "fetch", "--depth", "1", "origin", str(source["commit"])], cwd=checkout)
        run(["git", "checkout", "--detach", str(source["commit"])], cwd=checkout)
        head = run(["git", "rev-parse", "HEAD"], cwd=checkout)
        if head != source["commit"]:
            raise SystemExit(f"pin mismatch for {source_id}: {head}")

        if run(["git", "status", "--porcelain"], cwd=checkout):
            raise SystemExit(f"checkout became dirty before evidence: {source_id}")
        output = write_evidence(source, checkout, head)
        if run(["git", "status", "--porcelain"], cwd=checkout):
            raise SystemExit(f"evidence modified immutable upstream: {source_id}")
        print(f"OK {source_id} {head} evidence={output.relative_to(ROOT)}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
