from __future__ import annotations

import hashlib
import json
from pathlib import Path

from tools.runtime_preflight import _blender_check
from workers.blender.launch_blender import _resolve_blender


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def materialize_fake_vendor(root: Path) -> tuple[Path, Path]:
    vendor = root / "vendor" / "blender"
    runtime = vendor / "runtime"
    runtime.mkdir(parents=True)
    executable = runtime / "blender.exe"
    executable.write_bytes(b"real-local-blender-fixture\n")
    manifest = {
        "schema_version": 1,
        "dependency": "Blender",
        "version": "4.3.0",
        "runtime_network_required": False,
        "executable": "runtime/blender.exe",
        "integrity": {
            "executable_sha256": sha256(executable),
            "executable_bytes": executable.stat().st_size,
            "license_sha256": "0" * 64,
            "license_bytes": 1,
        },
    }
    manifest_path = vendor / "manifest.json"
    manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
    return executable, manifest_path


def test_full_preflight_accepts_only_hashed_repo_vendor(tmp_path: Path, monkeypatch) -> None:
    executable, _manifest = materialize_fake_vendor(tmp_path)
    monkeypatch.delenv("ARCZ_BLENDER", raising=False)

    result = _blender_check(tmp_path)
    assert result["status"] == "READY"
    assert result["detail"]["executable"] == str(executable.resolve())

    executable.write_bytes(b"tampered")
    rejected = _blender_check(tmp_path)
    assert rejected["status"] == "BLOCKED"
    assert any("SHA-256" in item for item in rejected["detail"]["blockers"])


def test_full_preflight_rejects_external_arcz_blender_override(tmp_path: Path, monkeypatch) -> None:
    materialize_fake_vendor(tmp_path)
    external = tmp_path.parent / "external-blender.exe"
    external.write_bytes(b"external")
    monkeypatch.setenv("ARCZ_BLENDER", str(external))

    result = _blender_check(tmp_path)
    assert result["status"] == "BLOCKED"
    assert any("fora do repositório" in item for item in result["detail"]["blockers"])


def test_worker_revalidates_frozen_blender_manifest_and_hash(tmp_path: Path) -> None:
    executable, manifest = materialize_fake_vendor(tmp_path)
    wrapper = {
        "request": {
            "resolved_blender": {
                "verified_repo_local": True,
                "executable": str(executable),
                "manifest": str(manifest),
            }
        }
    }

    resolved = _resolve_blender(wrapper, tmp_path)
    assert resolved == executable.resolve()

    executable.write_bytes(b"changed-after-preflight")
    try:
        _resolve_blender(wrapper, tmp_path)
    except RuntimeError as error:
        assert "BLENDER_HASH_MISMATCH" in str(error)
    else:
        raise AssertionError("worker accepted a Blender binary changed after preflight")


def test_worker_rejects_frozen_external_executable(tmp_path: Path) -> None:
    _executable, manifest = materialize_fake_vendor(tmp_path)
    external = tmp_path.parent / "blender.exe"
    external.write_bytes(b"external")
    wrapper = {
        "request": {
            "resolved_blender": {
                "verified_repo_local": True,
                "executable": str(external),
                "manifest": str(manifest),
            }
        }
    }
    try:
        _resolve_blender(wrapper, tmp_path)
    except RuntimeError as error:
        assert "escaped ARCZ repository" in str(error)
    else:
        raise AssertionError("worker accepted Blender outside the repository")
