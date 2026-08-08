#!/usr/bin/env python3
"""Instala uma distribuição CesiumJS local sem fazer download.

A ferramenta recebe um diretório/ZIP já fornecido pelo implementador, valida os
arquivos mínimos usados pelo ARCZ, exige uma licença local e publica o vendor por
rename atômico. Não contém URL de download nem fallback CDN.

Exemplos:
    python tools/vendor_cesium.py --source D:/deps/cesium/Build/Cesium \
      --license-file D:/deps/cesium/LICENSE.md --version 1.143.0

    python tools/vendor_cesium.py --source D:/deps/cesium-1.143.0.zip \
      --license-file D:/deps/LICENSE-CESIUM.md --version 1.143.0
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
DEST_ROOT = ROOT / "vendor" / "cesium"
DEST_CESIUM = DEST_ROOT / "Cesium"
REQUIRED_FILES = (
    "Cesium.js",
    "Widgets/widgets.css",
    "Assets/Textures/NaturalEarthII/tilemapresource.xml",
)
REQUIRED_DIRS = ("Assets", "Widgets", "Workers", "ThirdParty")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def safe_extract_zip(archive: Path, destination: Path) -> None:
    with zipfile.ZipFile(archive) as zf:
        for info in zf.infolist():
            target = (destination / info.filename).resolve()
            try:
                target.relative_to(destination.resolve())
            except ValueError as exc:
                raise ValueError(f"entrada ZIP escapa do destino: {info.filename}") from exc
        zf.extractall(destination)


def find_cesium_root(source: Path) -> Path:
    candidates = [
        source,
        source / "Build" / "Cesium",
        source / "Cesium",
        source / "package" / "Build" / "Cesium",
    ]
    candidates.extend(path.parent for path in source.rglob("Cesium.js"))
    seen: set[Path] = set()
    for candidate in candidates:
        candidate = candidate.resolve()
        if candidate in seen:
            continue
        seen.add(candidate)
        if all((candidate / rel).is_file() for rel in REQUIRED_FILES):
            return candidate
    raise FileNotFoundError(
        "não encontrei uma build Cesium válida; esperados: " + ", ".join(REQUIRED_FILES)
    )


def validate_tree(root: Path) -> None:
    missing = [rel for rel in REQUIRED_FILES if not (root / rel).is_file()]
    missing.extend(rel + "/" for rel in REQUIRED_DIRS if not (root / rel).is_dir())
    if missing:
        raise FileNotFoundError("vendor Cesium incompleto: " + ", ".join(missing))
    empty = [rel for rel in REQUIRED_FILES if (root / rel).stat().st_size == 0]
    if empty:
        raise ValueError("arquivos Cesium vazios: " + ", ".join(empty))
    if not any((root / "Workers").rglob("*.js")):
        raise ValueError("diretório Workers não contém JavaScript")


def build_manifest(root: Path, *, version: str, license_path: Path) -> dict:
    files = []
    for path in sorted(p for p in root.rglob("*") if p.is_file()):
        files.append({
            "path": path.relative_to(root).as_posix(),
            "bytes": path.stat().st_size,
            "sha256": sha256_file(path),
        })
    return {
        "schema_version": 1,
        "dependency": "CesiumJS",
        "version": version,
        "runtime_scope": "local_browser_vendor",
        "license": {
            "id": "Apache-2.0",
            "file": "LICENSE.md",
            "sha256": sha256_file(license_path),
        },
        "installed_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "files": files,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True, type=Path, help="diretório ou ZIP local")
    parser.add_argument("--license-file", required=True, type=Path, help="licença Cesium local")
    parser.add_argument("--version", required=True, help="versão auditada, por exemplo 1.143.0")
    parser.add_argument("--force", action="store_true", help="substitui vendor existente após validação")
    args = parser.parse_args()

    source = args.source.expanduser().resolve()
    license_file = args.license_file.expanduser().resolve()
    if not source.exists():
        raise FileNotFoundError(source)
    if not license_file.is_file() or license_file.stat().st_size == 0:
        raise FileNotFoundError(f"licença ausente/vazia: {license_file}")
    if DEST_CESIUM.exists() and not args.force:
        raise FileExistsError(f"{DEST_CESIUM} já existe; use --force para substituição atômica")

    DEST_ROOT.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="arcz-cesium-source-") as source_tmp_name, \
         tempfile.TemporaryDirectory(prefix="arcz-cesium-stage-", dir=DEST_ROOT.parent) as stage_tmp_name:
        source_tmp = Path(source_tmp_name)
        stage_tmp = Path(stage_tmp_name)
        if source.is_file():
            if not zipfile.is_zipfile(source):
                raise ValueError("arquivo source precisa ser ZIP")
            safe_extract_zip(source, source_tmp)
            source_root = find_cesium_root(source_tmp)
        else:
            source_root = find_cesium_root(source)

        validate_tree(source_root)
        staged_root = stage_tmp / "cesium"
        staged_cesium = staged_root / "Cesium"
        shutil.copytree(source_root, staged_cesium)
        staged_license = staged_root / "LICENSE.md"
        shutil.copy2(license_file, staged_license)
        validate_tree(staged_cesium)
        manifest = build_manifest(staged_cesium, version=args.version, license_path=staged_license)
        (staged_root / "manifest.json").write_text(
            json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )

        old = DEST_ROOT.with_name("cesium.previous")
        if old.exists():
            shutil.rmtree(old)
        if DEST_ROOT.exists():
            DEST_ROOT.replace(old)
        try:
            staged_root.replace(DEST_ROOT)
        except Exception:
            if old.exists() and not DEST_ROOT.exists():
                old.replace(DEST_ROOT)
            raise
        else:
            if old.exists():
                shutil.rmtree(old)

    print(json.dumps({
        "ok": True,
        "destination": str(DEST_CESIUM),
        "version": args.version,
        "files": len(manifest["files"]),
        "manifest": str(DEST_ROOT / "manifest.json"),
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
