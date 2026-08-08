#!/usr/bin/env python3
"""Preflight de runtime do ARCZ Earth + Aedifex Global.

Não instala nada, não baixa dependências e não mascara ausência de runtime.
Perfis:
- gateway: gateway Python/API local.
- interactive: gateway + Cesium local + sidecar Aedifex compilado + Node.
- full: interactive + workers Rust + Blender + modelos locais de render/prompts.

Saída JSON é adequada a instaladores e CI. Exit 0 apenas quando todos os checks
obrigatórios do perfil selecionado estiverem prontos.
"""
from __future__ import annotations

import argparse
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
CESIUM_REQUIRED = (
    "vendor/cesium/Cesium/Cesium.js",
    "vendor/cesium/Cesium/Widgets/widgets.css",
    "vendor/cesium/Cesium/Assets/Textures/NaturalEarthII/tilemapresource.xml",
    "vendor/cesium/LICENSE.md",
    "vendor/cesium/manifest.json",
)
MODEL_TASKS = ("chat.global", "prompt.enhance", "prompt.translate", "render-diffusion", "upscale")


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


def _check(name: str, ok: bool, *, required: bool = True, detail: Any = None, action: str | None = None) -> dict[str, Any]:
    return {
        "name": name,
        "status": "READY" if ok else ("BLOCKED" if required else "OPTIONAL_MISSING"),
        "required": required,
        "detail": detail,
        "action": action,
    }


def _python_checks() -> list[dict[str, Any]]:
    checks = [
        _check(
            "python_version",
            sys.version_info >= (3, 11),
            detail=f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}",
            action="Instale Python 3.11+ ou use o runtime empacotado.",
        )
    ]
    for module in PYTHON_MODULES:
        checks.append(
            _check(
                f"python_module:{module}",
                importlib.util.find_spec(module) is not None,
                detail=module,
                action="Execute install.ps1/install.sh com wheelhouse local ou import_assisted.",
            )
        )
    return checks


def _cesium_check(root: Path) -> dict[str, Any]:
    missing = [path for path in CESIUM_REQUIRED if not (root / path).is_file()]
    return _check(
        "cesium_local_vendor",
        not missing,
        detail={"missing": missing},
        action=(
            "python tools/vendor_cesium.py --source <Cesium-local-ou-ZIP> "
            "--license-file <LICENSE-local> --version 1.143.0"
        ),
    )


def _node_check() -> dict[str, Any]:
    ok, version = _command_version("node", ["--version"], r"v?(\d+(?:\.\d+){1,2})")
    major = int(version.split(".", 1)[0]) if ok and version and version.split(".", 1)[0].isdigit() else 0
    return _check(
        "node_runtime",
        bool(ok and major >= 20),
        detail=version,
        action="Instale Node.js 20+ localmente.",
    )


def _aedifex_check(root: Path) -> dict[str, Any]:
    try:
        sys.path.insert(0, str(root))
        from arcz_server.aedifex_registry import AedifexRegistry

        status = AedifexRegistry(root).status(verify_tree=False)
        return _check(
            "aedifex_vendor_and_build",
            bool(status.get("ready")),
            detail={
                "ready": status.get("ready"),
                "blockers": status.get("blockers", []),
                "dist": status.get("paths", {}).get("dist"),
            },
            action=(
                "python tools/vendor_aedifex.py --source <checkout-local-no-commit-fixado> && "
                "python tools/build_aedifex_sidecar.py"
            ),
        )
    except Exception as error:  # surfaced as a failed check, never ignored
        return _check(
            "aedifex_vendor_and_build",
            False,
            detail={"error": error.__class__.__name__, "message": str(error)},
            action="Valide integrations/aedifex/UPSTREAM_LOCK.json e materialize o checkout fixado.",
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
            cargo_ok,
            detail=cargo_version,
            action="Instale Rust/Cargo 1.82+ localmente.",
        ),
        _check(
            "rust_workers_release",
            not missing,
            detail={"missing": missing},
            action="cargo build --release --workspace",
        ),
    ]


def _blender_check() -> dict[str, Any]:
    configured = os.environ.get("ARCZ_BLENDER", "").strip()
    candidate = Path(configured).expanduser() if configured else None
    executable = str(candidate) if candidate and candidate.is_file() else shutil.which("blender")
    return _check(
        "blender_cycles",
        bool(executable),
        detail=executable,
        action="Defina ARCZ_BLENDER para um Blender local com Cycles.",
    )


def _model_checks(root: Path) -> list[dict[str, Any]]:
    try:
        sys.path.insert(0, str(root))
        from arcz_server.ai_broker import ModelRegistry
        from arcz_server.schema_validation import SchemaRegistry

        registry = ModelRegistry([root / "resources/models", root / "data/models"], SchemaRegistry(root / "schemas"))
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
                action="Instale manifestos e pesos locais validados em resources/models ou data/models.",
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
            action="Importe um modelo local com manifesto, licença, tamanho e SHA-256 válidos.",
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
        checks.append(_blender_check())
        checks.extend(_model_checks(root))
    blocked = [item for item in checks if item["required"] and item["status"] != "READY"]
    return {
        "schema_version": 1,
        "product": "ARCZ Earth + Aedifex Global",
        "profile": profile,
        "root": str(root),
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
