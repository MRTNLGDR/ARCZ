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


def run(cmd: list[str], cwd: Path | None = None, dry: bool = False) -> str:
    printable = " ".join(map(str, cmd))
    print(f"+ {printable}")
    if dry:
        return ""
    proc = subprocess.run(cmd, cwd=cwd, text=True, capture_output=True)
    if proc.returncode:
        sys.stderr.write(proc.stdout + proc.stderr)
        raise SystemExit(proc.returncode)
    return proc.stdout.strip()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Materialize exact ARCZ upstream snapshots without modifying them"
    )
    parser.add_argument("--only", action="append", default=[])
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument(
        "--reset",
        action="store_true",
        help="discard local changes in an upstream checkout",
    )
    args = parser.parse_args()

    data = tomllib.loads(MANIFEST.read_text(encoding="utf-8"))
    selected = set(args.only)

    for source in data["source"]:
        if selected and source["id"] not in selected:
            continue

        path = ROOT / source["path"]
        if not path.exists():
            path.parent.mkdir(parents=True, exist_ok=True)
            run(
                [
                    "git",
                    "clone",
                    "--filter=blob:none",
                    "--no-checkout",
                    source["repository"],
                    str(path),
                ],
                dry=args.dry_run,
            )

        if args.dry_run:
            print(f"  pin {source['id']} -> {source['commit']}")
            continue

        if args.reset:
            run(["git", "reset", "--hard"], cwd=path)
            run(["git", "clean", "-fdx"], cwd=path)

        dirty = run(["git", "status", "--porcelain"], cwd=path)
        if dirty:
            raise SystemExit(
                f"refusing dirty immutable upstream: {source['id']}\n{dirty}"
            )

        run(["git", "remote", "set-url", "origin", source["repository"]], cwd=path)
        run(["git", "fetch", "--depth", "1", "origin", source["commit"]], cwd=path)
        run(["git", "checkout", "--detach", source["commit"]], cwd=path)

        head = run(["git", "rev-parse", "HEAD"], cwd=path)
        if head != source["commit"]:
            raise SystemExit(f"pin mismatch for {source['id']}: {head}")

        license_files = []
        for name in ["LICENSE", "LICENSE.md", "COPYING", "COPYING.md"]:
            candidate = path / name
            if candidate.exists():
                license_files.append({"path": name, "sha256": sha256(candidate)})

        stamp = {
            "schema_version": 1,
            "id": source["id"],
            "repository": source["repository"],
            "commit": head,
            "declared_license": source["license"],
            "license_files": license_files,
            "immutable": True,
        }
        (path / ".arcz-upstream.json").write_text(
            json.dumps(stamp, indent=2) + "\n", encoding="utf-8"
        )
        print(f"OK {source['id']} {head}")


if __name__ == "__main__":
    main()
