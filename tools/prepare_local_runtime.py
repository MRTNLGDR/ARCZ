#!/usr/bin/env python3
"""Prepara vendors locais do ARCZ durante a fase explícita import_assisted.

Este script é instalador/setup, não runtime. Ele pode acessar os repositórios e
registries apenas quando ARCZ_NETWORK_MODE=import_assisted. O resultado aceito é
sempre materializado dentro do repo e imediatamente validado para uso
`offline_strict`.

Exemplos:
  ARCZ_NETWORK_MODE=import_assisted python tools/prepare_local_runtime.py --map
  ARCZ_NETWORK_MODE=import_assisted python tools/prepare_local_runtime.py --modeler
  ARCZ_NETWORK_MODE=import_assisted python tools/prepare_local_runtime.py --interactive
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def run(args: list[str]) -> None:
    print("+", " ".join(args), flush=True)
    completed = subprocess.run(args, cwd=ROOT, shell=False)
    if completed.returncode != 0:
        raise SystemExit(completed.returncode)


def require_import_assisted() -> None:
    mode = os.environ.get("ARCZ_NETWORK_MODE", "offline_strict").strip()
    if mode != "import_assisted":
        raise SystemExit(
            "Preparação recusada: defina ARCZ_NETWORK_MODE=import_assisted apenas durante o setup. "
            "O runtime normal deve permanecer offline_strict."
        )


def prepare_map() -> None:
    if not shutil.which("node") or not shutil.which("npm"):
        raise SystemExit("Node.js 22+ e npm são obrigatórios para compilar o Cesium pinado.")
    run([sys.executable, "tools/materialize_upstreams.py", "--only", "cesiumjs"])
    run([
        sys.executable,
        "tools/vendor_cesium.py",
        "--from-pinned-source",
        "--allow-network",
        "--force",
    ])


def prepare_modeler() -> None:
    if not shutil.which("node"):
        raise SystemExit("Node.js 22+ é obrigatório para o sidecar Aedifex.")
    if not shutil.which("bun"):
        raise SystemExit(
            "Bun é obrigatório apenas para preparar/construir o Aedifex. "
            "Instale Bun localmente e repita o setup; não há fallback remoto."
        )
    run([sys.executable, "tools/materialize_upstreams.py", "--only", "aedifex"])
    run([
        sys.executable,
        "tools/vendor_aedifex_controlled.py",
        "--source",
        "upstreams/sources/aedifex",
    ])
    run([sys.executable, "tools/build_aedifex_sidecar_controlled.py", "--allow-network"])
    run([sys.executable, "tools/smoke_aedifex_sidecar.py"])


def verify(profile: str) -> None:
    env = {**os.environ, "ARCZ_NETWORK_MODE": "offline_strict"}
    print(f"+ verify offline_strict profile={profile}", flush=True)
    completed = subprocess.run(
        [sys.executable, "tools/runtime_preflight.py", "--profile", profile],
        cwd=ROOT,
        env=env,
        shell=False,
    )
    if completed.returncode != 0:
        raise SystemExit(completed.returncode)


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--map", action="store_true", help="materializa e compila somente CesiumJS")
    group.add_argument("--modeler", action="store_true", help="materializa e compila somente Aedifex")
    group.add_argument("--interactive", action="store_true", help="prepara mapa + modelador e valida o perfil interativo")
    args = parser.parse_args()

    require_import_assisted()
    if args.map:
        prepare_map()
        env = {**os.environ, "ARCZ_NETWORK_MODE": "offline_strict"}
        run_check = subprocess.run(
            [sys.executable, "-c", (
                "from pathlib import Path; import json; "
                "p=Path('vendor/cesium/manifest.json'); assert p.is_file(); "
                "m=json.loads(p.read_text()); assert m['runtime_network_required'] is False; "
                "assert m['resolved_lockfile']['verified_frozen_offline'] is True"
            )],
            cwd=ROOT,
            env=env,
            shell=False,
        )
        return run_check.returncode
    if args.modeler:
        prepare_modeler()
        env = {**os.environ, "ARCZ_NETWORK_MODE": "offline_strict"}
        run_check = subprocess.run(
            [sys.executable, "-c", (
                "from pathlib import Path; import json; "
                "p=Path('vendor/aedifex-floorplanner/arcz-aedifex-build.json'); assert p.is_file(); "
                "m=json.loads(p.read_text()); assert m['resolved_lockfile']['verified_frozen_offline'] is True"
            )],
            cwd=ROOT,
            env=env,
            shell=False,
        )
        return run_check.returncode

    prepare_map()
    prepare_modeler()
    verify("interactive")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
