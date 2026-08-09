#!/usr/bin/env python3
from __future__ import annotations

"""Install the pinned IfcOpenShell runtime with wheel and license evidence.

Network is allowed only when ARCZ_NETWORK_MODE=import_assisted. The selected
official wheel is downloaded into vendor/ifcopenshell/wheelhouse, verified
against the platform/Python SHA-256 allowlist, then installed into the active
repo-local .venv. License texts come from the exact immutable IfcOpenShell
checkout already verified by ``tools/materialize_upstreams.py``. Runtime use
itself is offline.
"""

import hashlib
import importlib
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
UPSTREAM = ROOT / "upstreams/sources/ifcopenshell"
UPSTREAM_EVIDENCE = ROOT / "validation/upstreams/ifcopenshell-bonsai.json"
VERSION = "0.8.5"
UPSTREAM_COMMIT = "7ed8584edc6609654cea608d699348c9cca7ce5d"

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


def validate_upstream_license_evidence() -> dict[str, dict[str, object]]:
    if not (UPSTREAM / ".git").is_dir() or not UPSTREAM_EVIDENCE.is_file():
        raise RuntimeError(
            "pinned IfcOpenShell checkout/evidence missing; run materialize_upstreams.py --only ifcopenshell-bonsai"
        )
    head = run(["git", "-C", str(UPSTREAM), "rev-parse", "HEAD"], capture=True).stdout.strip()
    if head != UPSTREAM_COMMIT:
        raise RuntimeError(f"IfcOpenShell upstream pin mismatch: {head}")
    dirty = run(["git", "-C", str(UPSTREAM), "status", "--porcelain"], capture=True).stdout.strip()
    if dirty:
        raise RuntimeError("immutable IfcOpenShell upstream checkout is dirty")

    evidence = json.loads(UPSTREAM_EVIDENCE.read_text(encoding="utf-8"))
    if evidence.get("commit") != UPSTREAM_COMMIT or evidence.get("git_status_clean") is not True:
        raise RuntimeError("IfcOpenShell upstream evidence does not match the pinned clean commit")
    records = {
        str(record.get("path")): record
        for record in evidence.get("license_files") or []
        if isinstance(record, dict)
    }
    required = {"COPYING", "COPYING.LESSER"}
    if not required.issubset(records):
        raise RuntimeError(f"IfcOpenShell upstream evidence lacks legal files: {sorted(required - records.keys())}")
    for name in required:
        source = UPSTREAM / name
        record = records[name]
        if not source.is_file():
            raise RuntimeError(f"pinned legal file missing: {source}")
        actual = sha256(source)
        if actual != record.get("sha256") or source.stat().st_size != int(record.get("bytes") or -1):
            raise RuntimeError(f"pinned legal file evidence mismatch: {name}")
    return records


def copy_license_evidence() -> list[dict[str, object]]:
    records = validate_upstream_license_evidence()
    outputs: list[dict[str, object]] = []
    mapping = {
        "COPYING": "LICENSE.GPL-3.0",
        "COPYING.LESSER": "LICENSE.LGPL-3.0",
    }
    for source_name, output_name in mapping.items():
        source = UPSTREAM / source_name
        destination = VENDOR / output_name
        shutil.copy2(source, destination)
        digest = sha256(destination)
        upstream_record = records[source_name]
        if digest != upstream_record.get("sha256"):
            raise RuntimeError(f"copied IfcOpenShell legal file hash mismatch: {source_name}")
        outputs.append(
            {
                "path": str(destination.relative_to(ROOT)),
                "sha256": digest,
                "bytes": destination.stat().st_size,
                "upstream_path": source_name,
                "upstream_commit": UPSTREAM_COMMIT,
            }
        )
    return outputs


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

    # Legal provenance must be present and pinned before the binary wheel is
    # accepted into the runtime vendor.
    validate_upstream_license_evidence()
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
    runtime_evidence = validate_import()
    legal_files = copy_license_evidence()
    lgpl = next(item for item in legal_files if item["upstream_path"] == "COPYING.LESSER")

    manifest = {
        "schema_version": 2,
        "dependency": "IfcOpenShell",
        "version": VERSION,
        "license": "LGPL-3.0-or-later",
        "license_boundary": "IfcOpenShell Python/C++ engine only; Bonsai GPL subtree is not imported into ARCZ core",
        "upstream_commit": UPSTREAM_COMMIT,
        "runtime_network_required": False,
        "python": f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}",
        "platform": platform.platform(),
        "wheel": {
            "path": str(published.relative_to(ROOT)),
            "sha256": actual,
            "bytes": published.stat().st_size,
        },
        # Backward-compatible primary LGPL evidence for the fast preflight.
        "license_file": lgpl,
        "license_files": legal_files,
        "module": runtime_evidence["module"],
        "installed_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    }
    (VENDOR / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps({"ok": True, **manifest}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
