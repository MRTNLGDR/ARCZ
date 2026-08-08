#!/usr/bin/env python3
"""Compila e verifica o worker procedural Rust local.

Não baixa toolchain nem dependência silenciosamente. A máquina precisa possuir
Rust 1.82+ e Cargo. O script executa fmt, check, testes e release nesta ordem.
"""
from __future__ import annotations
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def run(command: list[str]) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=ROOT, check=True)


def main() -> int:
    cargo = shutil.which("cargo")
    rustc = shutil.which("rustc")
    if not cargo or not rustc:
        print("ERRO: cargo/rustc ausente. Instale Rust 1.82+ localmente e repita.", file=sys.stderr)
        return 2
    version = subprocess.check_output([rustc, "--version"], text=True).strip()
    print(version)
    run([cargo, "fmt", "--all", "--", "--check"])
    run([cargo, "check", "--workspace", "--all-targets"])
    run([cargo, "test", "--workspace", "--all-targets"])
    run([cargo, "build", "--release", "-p", "arcz-generation-cli"])
    binary = ROOT / "target" / "release" / ("arcz-generation-cli.exe" if os.name == "nt" else "arcz-generation-cli")
    if not binary.is_file() or binary.stat().st_size == 0:
        print(f"ERRO: binário não produzido: {binary}", file=sys.stderr)
        return 1
    print(f"OK: {binary}")
    print(f"Defina ARCZ_GENERATION_CLI={binary}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
