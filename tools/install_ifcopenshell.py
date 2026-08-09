#!/usr/bin/env python3
from __future__ import annotations

"""Install the pinned IfcOpenShell runtime with wheel integrity evidence.

Network is allowed only when ARCZ_NETWORK_MODE=import_assisted. The selected
official wheel is downloaded into vendor/ifcopenshell/wheelhouse, verified
against the platform/Python SHA-256 allowlist, then installed into the active
repo-local .venv. Runtime use itself is offline.
"""

import hashlib
import importlib
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[1]
VENDOR = ROOT / "vendor/ifcopenshell"
WHEELHOUSE = VENDOR / "wheelhouse"
VERSION = "0.8.5"

# Official PyPI file hashes for IfcOpenShell 0.8.5. ARCZ bootstrap guarantees
# Python >= 3.11; x86-64 Windows is the primary desktop target. Linux entries
# keep CI/developer installation equally reproducible.
WHEEL_SHA256 = {
    ("Windows", "3.11", "x86_64"): "a994cb398aa822b0153ace11fc19deea108706d3a10339de9ac9dc13da21076d",
    ("Windows", "3.12", "x86_64"): "7927921dbfd18024f44780880c37e48460bbca7476da3b0c2607e044b31521d5",
    ("Windows", "3.13", "x86_64"): "6dd40d29b21d1c92104a4585a1ee6a31811305b68f5e3f7ee991f89f4d386366",
    ("Windows", "3.14", "x86_64"): "13a5992dc07e69c0c78df5479e1ff9635e6ce9fa70c841d9ce2931e8481eabb9",
    ("Linux", "3.11", "x86_64"): "612e955ab0975bbc4134fe26c4c36d71a12525fd5c4b391573e6df3cd81713ec",
    ("Linux", "3.12", "x86_64"): "dd154b121815e25bc4890f030bf2bdd8d99b059448e0d1205ac81388a3d321f4",
    ("Linux", "3.13", "x86_64"): "c03c47abbfd0afddcbc91b510d984d7c017a222877460da5e0a6496d89f185e4",
    ("Linux", "3.14", "x86_64"): "15b488347efce0d7f5ab5b6d5561cf30428353b6ee8c7c8a53724f4b10ad4ac6",
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def normalized_machine() -> str:
    value = platform.machine().lower()
    if value in {"amd64", "x86_64", "x64"}:
        return "x86_64"
    return value


def runtime_key() -> tuple[str, str, str]:
    return (
        platform.system(),
        f"{sys.version_info.major}.{sys.version_info.minor}",
        normalized_machine(),
    )


def run(args: list[str], *, capture: bool = False) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        args,
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=capture,
        check=False,
        shell=False,
    )
    if completed.returncode:
        detail = (completed.stderr or completed.stdout or "")[-6000:]
        raise RuntimeError(f"command failed ({completed.returncode}): {' '.join(args)}\n{detail}")
    return completed


def current_version() -> str | None:
    try:
        module = importlib.import_module("ifcopenshell")
    except Exception:
        return None
    value = str(getattr(module, "version", "") or "")
    return value or None


def copy_license() -> tuple[Path, str]:
    distribution = importlib.metadata.distribution("ifcopenshell")
    candidates = []
    for item in distribution.files or ():
        name = str(item).replace("\\", "/").lower()
        if any(token in name for token in ("license", "copying", "lgpl")):
            path = Path(distribution.locate_file(item)).resolve()
            if path.is_file() and path.stat().st_size:
                candidates.append(path)
    if not candidates:
        raise RuntimeError("installed IfcOpenShell distribution exposes no license file")
    source = sorted(candidates, key=lambda path: (len(path.parts), str(path)))[0]
    destination = VENDOR / "LICENSE"
    shutil.copy2(source, destination)
    return destination, sha256(destination)


def validate_import() -> dict:
    module = importlib.import_module("ifcopenshell")
    if str(getattr(module, "version", "")) != VERSION:
        raise RuntimeError(f"IfcOpenShell version mismatch: {getattr(module, 'version', None)}")
    model = module.file(schema="IFC4")
    if str(model.schema).upper() != "IFC4":
        raise RuntimeError(f"IfcOpenShell could not create IFC4 file: {model.schema}")
    module_path = Path(module.__file__).resolve()
    if ".venv" not in {part.lower() for part in module_path.parts}:
        raise RuntimeError(f"IfcOpenShell escaped repo-local .venv: {module_path}")
    try:
        module_path.relative_to(ROOT.resolve())
    except ValueError as error:
        raise RuntimeError(f"IfcOpenShell is outside ARCZ repository: {module_path}") from error
    return {"module": str(module_path), "version": VERSION}


def main() -> int:
    if os.environ.get("ARCZ_NETWORK_MODE") != "import_assisted":
        raise RuntimeError("IfcOpenShell installation requires ARCZ_NETWORK_MODE=import_assisted")
    key = runtime_key()
    expected = WHEEL_SHA256.get(key)
    if not expected:
        raise RuntimeError(f"unsupported IfcOpenShell wheel target: {key}")

    VENDOR.mkdir(parents=True, exist_ok=True)
    WHEELHOUSE.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="arcz-ifcopenshell-") as temp_name:
        temp = Path(temp_name)
        run([
            sys.executable,
            "-m",
            "pip",
            "download",
            "--disable-pip-version-check",
            "--no-deps",
            "--only-binary=:all:",
            "--dest",
            str(temp),
            f"ifcopenshell=={VERSION}",
        ])
        wheels = list(temp.glob("ifcopenshell-*.whl"))
        if len(wheels) != 1:
            raise RuntimeError(f"expected one IfcOpenShell wheel, got {len(wheels)}")
        wheel = wheels[0]
        actual = sha256(wheel)
        if actual != expected:
            raise RuntimeError(
                f"IfcOpenShell wheel SHA-256 mismatch for {key}: expected {expected}, got {actual}"
            )
        published = WHEELHOUSE / wheel.name
        shutil.copy2(wheel, published)

    run([
        sys.executable,
        "-m",
        "pip",
        "install",
        "--disable-pip-version-check",
        "--upgrade",
        str(published),
    ])
    importlib.invalidate_caches()
    evidence = validate_import()
    license_path, license_hash = copy_license()

    manifest = {
        "schema_version": 1,
        "dependency": "IfcOpenShell",
        "version": VERSION,
        "license": "LGPL-3.0-or-later",
        "runtime_network_required": False,
        "python": f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}",
        "platform": platform.platform(),
        "wheel": {
            "path": str(published.relative_to(ROOT)),
            "sha256": actual,
            "bytes": published.stat().st_size,
        },
        "license_file": {
            "path": str(license_path.relative_to(ROOT)),
            "sha256": license_hash,
            "bytes": license_path.stat().st_size,
        },
        "module": evidence["module"],
        "installed_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    }
    (VENDOR / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps({"ok": True, **manifest}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
