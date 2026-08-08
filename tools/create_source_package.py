#!/usr/bin/env python3
"""Cria pacote local imutável aceito por `/api/v2/sources/import`.

Este utilitário NÃO baixa dados e NÃO inventa geometrias. Ele materializa bytes
já existentes, calcula SHA-256/tamanho e produz `package.json` validável.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import mimetypes
from pathlib import Path
import shutil
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def safe_relative(path: Path, base: Path) -> str:
    relative = path.resolve().relative_to(base.resolve())
    if any(part in {"", ".", ".."} for part in relative.parts):
        raise ValueError(f"caminho inválido: {path}")
    return relative.as_posix()


def parse_bbox(raw: str) -> list[float]:
    values = [float(part.strip()) for part in raw.split(",")]
    if len(values) != 4:
        raise argparse.ArgumentTypeError("bbox deve ser west,south,east,north")
    west, south, east, north = values
    if west >= east or south >= north:
        raise argparse.ArgumentTypeError("bbox inválido")
    return values


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path, help="diretório com bytes já obtidos legalmente")
    parser.add_argument("--output", required=True, type=Path, help="diretório final dentro de data/imports/inbox")
    parser.add_argument("--package-id", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--kind", required=True, choices=["osm", "overture", "dem", "imagery", "geocoder", "panorama", "assets", "models", "profiles", "other"])
    parser.add_argument("--license-id", required=True)
    parser.add_argument("--license-name", required=True)
    parser.add_argument("--attribution-required", action="store_true")
    parser.add_argument("--redistribution-allowed", action="store_true")
    parser.add_argument("--attribution-text", default="")
    parser.add_argument("--source", required=True)
    parser.add_argument("--import-method", required=True)
    parser.add_argument("--bbox", required=True, type=parse_bbox)
    parser.add_argument("--generator-inputs", type=Path,
                        help="JSON conforme generator-input-package.schema.json; será copiado como arcz-generator-inputs.json")
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args()

    source = args.input.resolve()
    destination = args.output.resolve()
    if not source.is_dir():
        parser.error(f"input não é diretório: {source}")
    if destination.exists() and not args.force:
        parser.error(f"output já existe: {destination}; use --force conscientemente")
    if not args.package_id or not all(c.islower() or c.isdigit() or c in "._-" for c in args.package_id):
        parser.error("package-id deve usar somente a-z, 0-9, ponto, underscore ou hífen")

    with tempfile.TemporaryDirectory(prefix="arcz-package-") as temporary_raw:
        temporary = Path(temporary_raw) / destination.name
        shutil.copytree(source, temporary, symlinks=False)
        for symlink in temporary.rglob("*"):
            if symlink.is_symlink():
                raise SystemExit(f"symlink recusado: {symlink}")
        if args.generator_inputs:
            value = json.loads(args.generator_inputs.read_text(encoding="utf-8"))
            try:
                from jsonschema import Draft202012Validator
                schema = json.loads((ROOT / "schemas" / "generator-input-package.schema.json").read_text(encoding="utf-8"))
                errors = list(Draft202012Validator(schema).iter_errors(value))
                if errors:
                    raise SystemExit("generator-inputs inválido: " + "; ".join(e.message for e in errors[:10]))
            except ImportError as error:
                raise SystemExit("jsonschema é obrigatório para validar generator-inputs; instale requirements-dev.txt") from error
            (temporary / "arcz-generator-inputs.json").write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

        files = []
        for path in sorted(temporary.rglob("*")):
            if not path.is_file() or path.name == "package.json":
                continue
            relative = safe_relative(path, temporary)
            files.append({
                "path": relative,
                "sha256": sha256(path),
                "bytes": path.stat().st_size,
                "mime": mimetypes.guess_type(path.name)[0] or "application/octet-stream",
            })
        if not files:
            raise SystemExit("pacote vazio recusado")
        manifest = {
            "schema_version": 1,
            "package_id": args.package_id,
            "version": args.version,
            "kind": args.kind,
            "license": {
                "id": args.license_id,
                "name": args.license_name,
                "attribution_required": args.attribution_required,
                "redistribution_allowed": args.redistribution_allowed,
                **({"attribution_text": args.attribution_text} if args.attribution_text else {}),
            },
            "provenance": {"source": args.source, "import_method": args.import_method},
            "created_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
            "bbox_wgs84": args.bbox,
            "files": files,
            "immutable": True,
            "metadata": {"arcz_generator_inputs": "arcz-generator-inputs.json"} if args.generator_inputs else {},
        }
        try:
            from jsonschema import Draft202012Validator
            schema = json.loads((ROOT / "schemas" / "source-package.schema.json").read_text(encoding="utf-8"))
            errors = list(Draft202012Validator(schema).iter_errors(manifest))
            if errors:
                raise SystemExit("manifesto inválido: " + "; ".join(e.message for e in errors[:10]))
        except ImportError as error:
            raise SystemExit("jsonschema é obrigatório para criar source packages validados; instale requirements-dev.txt") from error
        (temporary / "package.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        destination.parent.mkdir(parents=True, exist_ok=True)
        if destination.exists():
            shutil.rmtree(destination)
        shutil.move(str(temporary), destination)
    print(destination)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
