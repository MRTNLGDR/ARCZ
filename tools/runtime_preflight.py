#!/usr/bin/env python3
"""Preflight real do runtime ARCZ Earth.

Não instala, não baixa e não mascara capacidade ausente. Cada perfil apenas
inspeciona os arquivos/binários efetivamente disponíveis nesta máquina.

Perfis:
- gateway: API Python local.
- interactive: gateway + Cesium vendor local + Aedifex compilado + Node 22+.
- full: interactive + workers Rust 1.97.1 + Blender vendor local + modelos locais.

A preparação com rede é uma etapa explícita e separada (`prepare_local_runtime.py`)
em ARCZ_NETWORK_MODE=import_assisted. Depois dela, o runtime volta a
`offline_strict` e não possui CDN/fallback remoto.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from typing import Any

ROOT_DEFAULT = Path(__file__).resolve().parents[1]
PYTHON_MODULES = ("jsonschema", "pyproj", "PIL")
CESIUM_COMMIT = "6d5d8b1f0725b6f831b336463f4b11c98023427b"
AEDIFEX_COMMIT = "5319368bae16500ca5267f6f8d68b36c9586d5bb"
RUST_REQUIRED = (1, 97, 1)
CESIUM_REQUIRED = (
    "vendor/cesium/Cesium/Cesium.js",
    "vendor/cesium/Cesium/Widgets/widgets.css",
    "vendor/cesium/Cesium/Assets/Textures/NaturalEarthII/tilemapresource.xml",
    "vendor/cesium/LICENSE.md",
    "vendor/cesium/resolved-package-lock.json",
    "vendor/cesium/manifest.json",
)
MODEL_TASKS = ("chat.global", "prompt.enhance", "prompt.translate", "render-diffusion", "upscale")


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _command_version(command: str, args: list[str], pattern: str) -> tuple[bool, str | None]:
    executable = shutil.which(command)
    if not executable:
        return False, None
    try:
        result = subprocess.run(
            [executable, *args], capture_output=True, text=True, timeout=10, check=False
        )
    except (OSError, subprocess.SubprocessError):
        return False, None
    text = (result.stdout or result.stderr or "").strip()
    match = re.search(pattern, text)
    return result.returncode == 0, match.group(1) if match else text[:120]


def _semver_tuple(value: str | None) -> tuple[int, int, int]:
    if not value:
        return (0, 0, 0)
    parts = value.lstrip("v").split(".")
    numbers: list[int] = []
    for part in parts[:3]:
        match = re.match(r"(\d+)", part)
        numbers.append(int(match.group(1)) if match else 0)
    while len(numbers) < 3:
        numbers.append(0)
    return tuple(numbers[:3])  # type: ignore[return-value]


def _check(name: str, ok: bool, *, required: bool = True, detail: Any = None, action: str | None = None) -> dict[str, Any]:
    return {
        "name": name,
        "status": "READY" if ok else ("BLOCKED" if required else "OPTIONAL_MISSING"),
        "required": required,
        "detail": detail,
        "action": action,
    }


def _read_json(path: Path) -> dict[str, Any] | None:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    return value if isinstance(value, dict) else None


def _python_checks() -> list[dict[str, Any]]:
    checks = [
        _check(
            "python_version",
            sys.version_info >= (3, 11),
            detail=f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}",
            action="Instale Python 3.11+ ou use o runtime Python empacotado do ARCZ.",
        )
    ]
    for module in PYTHON_MODULES:
        checks.append(
            _check(
                f"python_module:{module}",
                importlib.util.find_spec(module) is not None,
                detail=module,
                action="Instale requirements.txt a partir do ambiente local/import_assisted.",
            )
        )
    return checks


def _cesium_check(root: Path) -> dict[str, Any]:
    missing = [path for path in CESIUM_REQUIRED if not (root / path).is_file()]
    manifest = _read_json(root / "vendor/cesium/manifest.json")
    integrity_ok = bool(
        not missing
        and manifest
        and manifest.get("schema_version") == 2
        and manifest.get("dependency") == "CesiumJS"
        and manifest.get("version") == "1.144.0"
        and manifest.get("runtime_network_required") is False
        and manifest.get("upstream_commit") == CESIUM_COMMIT
        and isinstance(manifest.get("resolved_lockfile"), dict)
        and manifest["resolved_lockfile"].get("verified_frozen_offline") is True
    )
    return _check(
        "cesium_local_vendor",
        integrity_ok,
        detail={
            "missing": missing,
            "version": manifest.get("version") if manifest else None,
            "upstream_commit": manifest.get("upstream_commit") if manifest else None,
            "offline_lock_verified": (
                (manifest.get("resolved_lockfile") or {}).get("verified_frozen_offline")
                if manifest else None
            ),
        },
        action=(
            "ARCZ_NETWORK_MODE=import_assisted python tools/prepare_local_runtime.py --map; "
            "depois volte ARCZ_NETWORK_MODE=offline_strict."
        ),
    )


def _node_check() -> dict[str, Any]:
    ok, version = _command_version("node", ["--version"], r"v?(\d+(?:\.\d+){1,2})")
    return _check(
        "node_runtime",
        bool(ok and _semver_tuple(version) >= (22, 0, 0)),
        detail=version,
        action="Instale Node.js 22+ localmente (exigência do Cesium pinado).",
    )


def _aedifex_check(root: Path) -> dict[str, Any]:
    try:
        sys.path.insert(0, str(root))
        from arcz_server.aedifex_registry import AedifexRegistry

        status = AedifexRegistry(root).status(verify_tree=False)
        build = _read_json(root / "vendor/aedifex-floorplanner/arcz-aedifex-build.json")
        build_ok = bool(
            build
            and build.get("upstream_commit") == AEDIFEX_COMMIT
            and isinstance(build.get("resolved_lockfile"), dict)
            and build["resolved_lockfile"].get("verified_frozen_offline") is True
            and (build.get("integrity") or {}).get("file_count", 0) > 0
        )
        ready = bool(status.get("ready")) and build_ok
        return _check(
            "aedifex_vendor_and_build",
            ready,
            detail={
                "ready": status.get("ready"),
                "blockers": status.get("blockers", []),
                "dist": status.get("paths", {}).get("dist"),
                "upstream_commit": build.get("upstream_commit") if build else None,
                "offline_lock_verified": (
                    (build.get("resolved_lockfile") or {}).get("verified_frozen_offline")
                    if build else None
                ),
            },
            action=(
                "ARCZ_NETWORK_MODE=import_assisted python tools/prepare_local_runtime.py --modeler; "
                "depois volte ARCZ_NETWORK_MODE=offline_strict."
            ),
        )
    except Exception as error:
        return _check(
            "aedifex_vendor_and_build",
            False,
            detail={"error": error.__class__.__name__, "message": str(error)},
            action=(
                "ARCZ_NETWORK_MODE=import_assisted python tools/prepare_local_runtime.py --modeler"
            ),
        )
    finally:
        try:
            sys.path.remove(str(root))
        except ValueError:
            pass


def _rust_checks(root: Path) -> list[dict[str, Any]]:
    cargo_ok, cargo_version = _command_version("cargo", ["--version"], r"cargo\s+(\d+(?:\.\d+){1,2})")
    suffix = ".exe" if os.name == "nt" else ""
    binaries = [
        root / "target" / "release" / f"arcz-generation-cli{suffix}",
        root / "target" / "release" / f"arcz-osm-cli{suffix}",
    ]
    missing = [str(path.relative_to(root)) for path in binaries if not path.is_file()]
    return [
        _check(
            "cargo_runtime",
            bool(cargo_ok and _semver_tuple(cargo_version) >= RUST_REQUIRED),
            detail=cargo_version,
            action="Instale/use Rust/Cargo 1.97.1 conforme rust-toolchain.toml.",
        ),
        _check(
            "rust_workers_release",
            not missing,
            detail={"missing": missing},
            action="cargo +1.97.1 build --release --workspace --locked",
        ),
    ]


def _blender_check(root: Path) -> dict[str, Any]:
    vendor = (root / "vendor" / "blender").resolve()
    manifest_path = vendor / "manifest.json"
    manifest = _read_json(manifest_path)
    blockers: list[str] = []
    executable: Path | None = None

    if manifest is None:
        blockers.append("vendor/blender/manifest.json ausente ou inválido")
    else:
        if manifest.get("schema_version") != 1 or manifest.get("dependency") != "Blender":
            blockers.append("contrato do manifesto Blender inválido")
        if manifest.get("runtime_network_required") is not False:
            blockers.append("Blender vendor não declara runtime offline")
        relative = str(manifest.get("executable") or "").strip()
        if not relative:
            blockers.append("manifesto Blender sem executável")
        else:
            candidate = (vendor / relative).resolve()
            try:
                candidate.relative_to(vendor)
            except ValueError:
                blockers.append("executável Blender escapa de vendor/blender")
            else:
                if not candidate.is_file() or candidate.is_symlink():
                    blockers.append("executável Blender local ausente/inválido")
                else:
                    expected = str((manifest.get("integrity") or {}).get("executable_sha256") or "")
                    if len(expected) != 64:
                        blockers.append("manifesto Blender sem SHA-256 do executável")
                    elif _sha256(candidate) != expected:
                        blockers.append("SHA-256 do executável Blender diverge")
                    else:
                        executable = candidate

    configured = os.environ.get("ARCZ_BLENDER", "").strip()
    if configured:
        try:
            configured_path = Path(configured).expanduser().resolve()
            configured_path.relative_to(root.resolve())
        except (ValueError, OSError):
            blockers.append("ARCZ_BLENDER aponta para fora do repositório")
        else:
            if executable is None or configured_path != executable:
                blockers.append("ARCZ_BLENDER não corresponde ao executável auditado do vendor")

    ready = executable is not None and not blockers
    return _check(
        "blender_repo_vendor",
        ready,
        detail={
            "version": manifest.get("version") if manifest else None,
            "manifest": str(manifest_path),
            "executable": str(executable) if executable else None,
            "blockers": blockers,
        },
        action=(
            "Importe uma distribuição Blender portátil real para vendor/blender com: "
            "python tools/vendor_blender.py --source <ZIP_OU_DIRETORIO> "
            "--license-file <LICENCA> --force. PATH externo não é aceito."
        ),
    )


def _model_checks(root: Path) -> list[dict[str, Any]]:
    try:
        sys.path.insert(0, str(root))
        from arcz_server.ai_broker import ModelRegistry
        from arcz_server.schema_validation import SchemaRegistry

        registry = ModelRegistry(
            [root / "resources/models", root / "data/models"],
            SchemaRegistry(root / "schemas"),
        )
        models = registry.list(verify=True)
        valid_tasks = {
            str(model.get("task"))
            for model in models
            if isinstance(model.get("status"), dict) and model["status"].get("installed") is True
        }
    except Exception as error:
        return [
            _check(
                "local_ai_models",
                False,
                detail={"error": error.__class__.__name__, "message": str(error)},
                action="Instale pesos locais com manifesto, licença, tamanho e SHA-256 válidos.",
            )
        ]
    finally:
        try:
            sys.path.remove(str(root))
        except ValueError:
            pass
    return [
        _check(
            f"local_model_task:{task}",
            task in valid_tasks,
            detail={"task": task, "valid_tasks": sorted(valid_tasks)},
            action="Importe um modelo local auditado para esta tarefa.",
        )
        for task in MODEL_TASKS
    ]


def run_preflight(root: Path, profile: str) -> dict[str, Any]:
    root = root.resolve()
    checks = _python_checks()
    if profile in {"interactive", "full"}:
        checks.extend((_node_check(), _cesium_check(root), _aedifex_check(root)))
    if profile == "full":
        checks.extend(_rust_checks(root))
        checks.append(_blender_check(root))
        checks.extend(_model_checks(root))
    blocked = [item for item in checks if item["required"] and item["status"] != "READY"]
    return {
        "schema_version": 2,
        "product": "ARCZ Earth",
        "profile": profile,
        "root": str(root),
        "network_mode": os.environ.get("ARCZ_NETWORK_MODE", "offline_strict"),
        "ready": not blocked,
        "summary": {
            "ready": sum(item["status"] == "READY" for item in checks),
            "blocked": len(blocked),
            "optional_missing": sum(item["status"] == "OPTIONAL_MISSING" for item in checks),
        },
        "checks": checks,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT_DEFAULT)
    parser.add_argument("--profile", choices=("gateway", "interactive", "full"), default="interactive")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = run_preflight(args.root, args.profile)
    text = json.dumps(report, ensure_ascii=False, indent=2)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text + "\n", encoding="utf-8")
    print(text)
    return 0 if report["ready"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
