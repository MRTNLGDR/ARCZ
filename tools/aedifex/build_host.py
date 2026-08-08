#!/usr/bin/env python3
"""Compila o custom element Aedifex local e publica somente artefatos verificados."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
from datetime import datetime, timezone


def run(command: list[str], cwd: Path) -> None:
    result = subprocess.run(command, cwd=cwd, check=False, text=True)
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}): {' '.join(command)}")


def sha(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--vendor", type=Path)
    parser.add_argument("--runtime", choices=["bun", "pnpm"], default="bun")
    parser.add_argument("--allow-network-install", action="store_true",
                        help="permit dependency materialization during this explicit build step")
    args = parser.parse_args()
    root = args.root.resolve()
    vendor = (args.vendor or root / "integrations" / "aedifex" / "vendor").resolve()
    host = vendor / "apps" / "arcz-host"
    if not (vendor / "package.json").is_file() or not (host / "package.json").is_file():
        raise RuntimeError("verified Aedifex vendor/overlay is missing; run vendor_aedifex.py first")
    executable = shutil.which(args.runtime)
    if not executable:
        raise RuntimeError(f"required local runtime is not installed: {args.runtime}")
    if args.runtime == "bun":
        install = [executable, "install"]
        if not args.allow_network_install:
            install.append("--offline")
        run(install, vendor)
        run([executable, "run", "--cwd", "apps/arcz-host", "check-types"], vendor)
        run([executable, "run", "--cwd", "apps/arcz-host", "build"], vendor)
    else:
        install = [executable, "install"]
        install.extend(["--offline", "--frozen-lockfile"] if not args.allow_network_install else ["--no-frozen-lockfile"])
        run(install, vendor)
        run([executable, "--dir", "apps/arcz-host", "check-types"], vendor)
        run([executable, "--dir", "apps/arcz-host", "build"], vendor)
    source_dist = host / "dist"
    bundle = source_dist / "arcz-aedifex-host.js"
    if not bundle.is_file() or bundle.stat().st_size < 1024:
        raise RuntimeError("host build did not produce a usable arcz-aedifex-host.js")
    destination = root / "integrations" / "aedifex" / "dist"
    staging = destination.with_name("dist.staging")
    if staging.exists(): shutil.rmtree(staging)
    shutil.copytree(source_dist, staging)
    files = []
    for path in sorted(p for p in staging.rglob("*") if p.is_file()):
        files.append({"path": path.relative_to(staging).as_posix(), "bytes": path.stat().st_size, "sha256": sha(path)})
    manifest = {
        "schema_version": 1,
        "built_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "upstream_commit": json.loads((root / "integrations/aedifex/upstream.lock.json").read_text())["primary"]["commit"],
        "entry": "arcz-aedifex-host.js",
        "files": files,
    }
    (staging / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    if destination.exists(): shutil.rmtree(destination)
    staging.replace(destination)
    print(json.dumps({"ok": True, "dist": str(destination), "files": len(files)}))
    return 0


if __name__ == "__main__":
    try: raise SystemExit(main())
    except Exception as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        raise SystemExit(1)
