#!/usr/bin/env python3
"""Materializa uma distribuição Blender LOCAL dentro do repositório ARCZ.

Não baixa Blender. A entrada deve ser um diretório portátil ou ZIP local que o
usuário já possua, acompanhado da licença correspondente. O importador valida o
executável executando ``--version``, rejeita symlinks/escape de ZIP ou diretório,
copia para ``vendor/blender/runtime`` e registra SHA-256/licença/versão.

Isso transforma Blender em dependência local explícita do projeto, em vez de um
binário arbitrário encontrado no PATH.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import os
import re
import shutil
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
DEST = ROOT / "vendor" / "blender"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def safe_extract(archive: Path, destination: Path) -> None:
    with zipfile.ZipFile(archive) as zf:
        for info in zf.infolist():
            name = info.filename.replace("\\", "/")
            if name.startswith("/") or ".." in Path(name).parts:
                raise ValueError(f"entrada ZIP insegura: {info.filename}")
            mode = (info.external_attr >> 16) & 0o170000
            if mode == 0o120000:
                raise ValueError(f"symlink não é aceito no ZIP Blender: {info.filename}")
            target = (destination / name).resolve()
            target.relative_to(destination.resolve())
        zf.extractall(destination)


def reject_source_symlinks(root: Path) -> None:
    links = [path for path in root.rglob("*") if path.is_symlink()]
    if links:
        preview = ", ".join(str(path.relative_to(root)) for path in links[:12])
        raise ValueError(
            "distribuição Blender contém symlink; forneça uma distribuição portátil "
            f"autocontida sem links externos. Exemplos: {preview}"
        )


def executable_candidates(root: Path) -> list[Path]:
    names = {"blender", "blender.exe"}
    values = [path for path in root.rglob("*") if path.is_file() and path.name.lower() in names]
    return sorted(values, key=lambda path: (len(path.relative_to(root).parts), str(path).lower()))


def find_blender(root: Path) -> Path:
    for candidate in executable_candidates(root):
        if candidate.is_symlink():
            continue
        return candidate
    raise FileNotFoundError("executável blender/blender.exe não encontrado na distribuição fornecida")


def probe(executable: Path) -> dict:
    completed = subprocess.run(
        [str(executable), "--version"],
        cwd=executable.parent,
        shell=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=30,
        env={**os.environ, "BLENDER_USER_CONFIG": str(executable.parent / ".arcz-probe-config")},
    )
    output = ((completed.stdout or "") + "\n" + (completed.stderr or "")).strip()
    if completed.returncode != 0:
        raise RuntimeError(f"Blender --version falhou ({completed.returncode}): {output[-2000:]}")
    match = re.search(r"Blender\s+(\d+(?:\.\d+){1,2})", output, flags=re.I)
    if not match:
        raise RuntimeError(f"saída de Blender --version não reconhecida: {output[:1000]}")
    return {"version": match.group(1), "output": output[:2000]}


def copy_runtime(source: Path, destination: Path) -> None:
    reject_source_symlinks(source)
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(source, destination, symlinks=False)
    for path in destination.rglob("*"):
        if path.is_symlink():
            raise RuntimeError(f"vendor Blender contém symlink após cópia: {path}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True, help="diretório portátil ou ZIP Blender local")
    parser.add_argument("--license-file", type=Path, required=True, help="arquivo de licença da distribuição")
    parser.add_argument("--version", help="versão esperada; se informada precisa coincidir com blender --version")
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args()

    source = args.source.expanduser().resolve()
    license_file = args.license_file.expanduser().resolve()
    if not source.exists():
        raise FileNotFoundError(source)
    if not license_file.is_file() or license_file.stat().st_size == 0:
        raise FileNotFoundError(f"licença Blender ausente/vazia: {license_file}")
    if DEST.exists() and not args.force:
        raise FileExistsError(f"{DEST} já existe; use --force para substituir após validação")

    with tempfile.TemporaryDirectory(prefix="arcz-blender-") as temp_name:
        temp = Path(temp_name)
        if source.is_file():
            if not zipfile.is_zipfile(source):
                raise ValueError("--source arquivo precisa ser ZIP")
            extracted = temp / "source"
            extracted.mkdir()
            safe_extract(source, extracted)
            source_root = extracted
        elif source.is_dir():
            source_root = source
            reject_source_symlinks(source_root)
        else:
            raise ValueError("--source precisa ser diretório ou ZIP")

        executable = find_blender(source_root)
        probe_result = probe(executable)
        actual_version = probe_result["version"]
        if args.version and actual_version != args.version:
            raise RuntimeError(f"versão Blender divergente: esperado {args.version}, encontrado {actual_version}")

        # Use the directory containing the shallowest executable as the runtime
        # root. Portable distributions usually expose blender at their root.
        runtime_root = executable.parent
        stage = temp / "publish"
        runtime_dest = stage / "runtime"
        copy_runtime(runtime_root, runtime_dest)
        copied_executable = runtime_dest / executable.name
        if not copied_executable.is_file():
            raise RuntimeError("executável Blender não sobreviveu à cópia")
        copied_probe = probe(copied_executable)
        if copied_probe["version"] != actual_version:
            raise RuntimeError("Blender copiado responde com versão diferente da origem")

        shutil.copy2(license_file, stage / "LICENSE")
        manifest = {
            "schema_version": 1,
            "dependency": "Blender",
            "version": actual_version,
            "runtime_network_required": False,
            "executable": f"runtime/{copied_executable.name}",
            "integrity": {
                "executable_sha256": sha256_file(copied_executable),
                "executable_bytes": copied_executable.stat().st_size,
                "license_sha256": sha256_file(stage / "LICENSE"),
                "license_bytes": (stage / "LICENSE").stat().st_size,
            },
            "probe": {"version": copied_probe["version"]},
            "installed_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        }
        (stage / "manifest.json").write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )

        previous = DEST.with_name("blender.previous")
        if previous.exists():
            shutil.rmtree(previous)
        DEST.parent.mkdir(parents=True, exist_ok=True)
        if DEST.exists():
            DEST.replace(previous)
        try:
            stage.replace(DEST)
        except Exception:
            if previous.exists() and not DEST.exists():
                previous.replace(DEST)
            raise
        else:
            if previous.exists():
                shutil.rmtree(previous)

    print(json.dumps({
        "ok": True,
        "version": actual_version,
        "manifest": str(DEST / "manifest.json"),
        "executable": str(DEST / manifest["executable"]),
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
