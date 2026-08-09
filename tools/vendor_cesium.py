#!/usr/bin/env python3
"""Publica CesiumJS como vendor local auditável do ARCZ.

Há dois modos reais:
1. ``--from-pinned-source --allow-network``: usa o checkout imutável auditado em
   ``upstreams/sources/cesium``, compila uma CÓPIA controlada com Node/npm e
   publica ``Build/Cesium`` dentro de ``vendor/cesium``;
2. ``--source``: aceita uma build/ZIP local já fornecida e valida antes de publicar.

Nenhum modo contém CDN ou fallback remoto de runtime. O navegador sempre usa
``/vendor/cesium/Cesium`` dentro do próprio repositório ARCZ.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
DEST_ROOT = ROOT / "vendor" / "cesium"
DEST_CESIUM = DEST_ROOT / "Cesium"
UPSTREAM_MANIFEST = ROOT / "upstreams" / "manifest.toml"
PINNED_SOURCE = ROOT / "upstreams" / "sources" / "cesium"
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


def run(args: list[str], cwd: Path, *, env: dict[str, str] | None = None) -> str:
    completed = subprocess.run(
        args,
        cwd=cwd,
        env=env,
        shell=False,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if completed.returncode != 0:
        tail = "\n".join(completed.stdout.splitlines()[-120:])
        raise RuntimeError(f"{' '.join(args)} falhou ({completed.returncode})\n{tail}")
    return completed.stdout


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
        source / "Build" / "CesiumUnminified",
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
        "não encontrei build Cesium válida; esperados: " + ", ".join(REQUIRED_FILES)
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


def pinned_config() -> dict:
    data = tomllib.loads(UPSTREAM_MANIFEST.read_text(encoding="utf-8"))
    for source in data.get("source", []):
        if source.get("id") == "cesiumjs":
            return source
    raise RuntimeError("pin cesiumjs ausente em upstreams/manifest.toml")


def assert_pinned_checkout(source: Path, expected_sha: str) -> None:
    if not (source / ".git").exists():
        raise RuntimeError(
            f"checkout Cesium ausente em {source}; rode: "
            "python tools/materialize_upstreams.py --only cesiumjs"
        )
    head = run(["git", "rev-parse", "HEAD"], source).strip()
    if head != expected_sha:
        raise RuntimeError(f"Cesium SHA divergente: esperado {expected_sha}, encontrado {head}")
    dirty = run(["git", "status", "--porcelain"], source).strip()
    if dirty:
        raise RuntimeError(f"checkout Cesium imutável está sujo:\n{dirty}")


def build_from_pinned_source(source: Path, *, allow_network: bool) -> tuple[Path, Path, str, str, tempfile.TemporaryDirectory]:
    pin = pinned_config()
    expected_sha = str(pin["commit"])
    assert_pinned_checkout(source, expected_sha)
    if not allow_network:
        raise RuntimeError(
            "dependências npm só podem ser instaladas na fase import_assisted; "
            "use --allow-network durante setup. O runtime gerado continua offline."
        )
    npm = shutil.which("npm")
    node = shutil.which("node")
    if not npm or not node:
        raise RuntimeError("Node 22+ e npm são obrigatórios para compilar CesiumJS")
    node_version = run([node, "--version"], ROOT).strip()

    holder = tempfile.TemporaryDirectory(prefix="arcz-cesium-build-")
    work = Path(holder.name) / "cesium"
    shutil.copytree(
        source,
        work,
        ignore=shutil.ignore_patterns(".git", "node_modules", "Build"),
    )
    env = {
        **os.environ,
        "CI": "1",
        "HUSKY": "0",
        "PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD": "1",
        "npm_config_audit": "false",
        "npm_config_fund": "false",
    }
    run([npm, "ci"], work, env=env)
    run([npm, "run", "build-release"], work, env=env)
    built = find_cesium_root(work / "Build")
    package = json.loads((work / "package.json").read_text(encoding="utf-8"))
    version = str(package.get("version") or "unknown")
    license_file = source / "LICENSE.md"
    if not license_file.is_file():
        raise RuntimeError("LICENSE.md ausente no checkout Cesium auditado")
    assert_pinned_checkout(source, expected_sha)
    return built, license_file, version, node_version, holder


def build_manifest(
    root: Path,
    *,
    version: str,
    license_path: Path,
    upstream_commit: str | None = None,
    node_version: str | None = None,
) -> dict:
    files = []
    for path in sorted(p for p in root.rglob("*") if p.is_file()):
        files.append({
            "path": path.relative_to(root).as_posix(),
            "bytes": path.stat().st_size,
            "sha256": sha256_file(path),
        })
    manifest = {
        "schema_version": 2,
        "dependency": "CesiumJS",
        "version": version,
        "runtime_scope": "local_browser_vendor",
        "runtime_network_required": False,
        "license": {
            "id": "Apache-2.0",
            "file": "LICENSE.md",
            "sha256": sha256_file(license_path),
        },
        "installed_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "files": files,
    }
    if upstream_commit:
        manifest["upstream_commit"] = upstream_commit
    if node_version:
        manifest["toolchain"] = {"node": node_version}
    return manifest


def publish(source_root: Path, license_file: Path, *, version: str, force: bool,
            upstream_commit: str | None = None, node_version: str | None = None) -> dict:
    validate_tree(source_root)
    if DEST_CESIUM.exists() and not force:
        raise FileExistsError(f"{DEST_CESIUM} já existe; use --force para substituição atômica")

    DEST_ROOT.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="arcz-cesium-stage-", dir=DEST_ROOT.parent) as stage_tmp_name:
        stage_tmp = Path(stage_tmp_name)
        staged_root = stage_tmp / "cesium"
        staged_cesium = staged_root / "Cesium"
        shutil.copytree(source_root, staged_cesium)
        staged_license = staged_root / "LICENSE.md"
        shutil.copy2(license_file, staged_license)
        validate_tree(staged_cesium)
        manifest = build_manifest(
            staged_cesium,
            version=version,
            license_path=staged_license,
            upstream_commit=upstream_commit,
            node_version=node_version,
        )
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
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, help="diretório/ZIP local já construído")
    parser.add_argument("--license-file", type=Path, help="licença para --source manual")
    parser.add_argument("--version", help="versão para --source manual")
    parser.add_argument("--from-pinned-source", action="store_true", help="compila o checkout Cesium auditado")
    parser.add_argument("--allow-network", action="store_true", help="permite npm ci apenas durante setup")
    parser.add_argument("--force", action="store_true", help="substitui vendor existente após validação")
    args = parser.parse_args()

    holder = None
    try:
        if args.from_pinned_source:
            if args.source or args.license_file or args.version:
                parser.error("--from-pinned-source não aceita --source/--license-file/--version")
            source_root, license_file, version, node_version, holder = build_from_pinned_source(
                PINNED_SOURCE, allow_network=args.allow_network
            )
            pin = pinned_config()
            manifest = publish(
                source_root,
                license_file,
                version=version,
                force=args.force,
                upstream_commit=str(pin["commit"]),
                node_version=node_version,
            )
        else:
            if not args.source or not args.license_file or not args.version:
                parser.error("use --from-pinned-source ou forneça --source, --license-file e --version")
            source = args.source.expanduser().resolve()
            license_file = args.license_file.expanduser().resolve()
            if not source.exists():
                raise FileNotFoundError(source)
            if not license_file.is_file() or license_file.stat().st_size == 0:
                raise FileNotFoundError(f"licença ausente/vazia: {license_file}")
            with tempfile.TemporaryDirectory(prefix="arcz-cesium-source-") as source_tmp_name:
                source_tmp = Path(source_tmp_name)
                if source.is_file():
                    if not zipfile.is_zipfile(source):
                        raise ValueError("arquivo source precisa ser ZIP")
                    safe_extract_zip(source, source_tmp)
                    source_root = find_cesium_root(source_tmp)
                else:
                    source_root = find_cesium_root(source)
                manifest = publish(source_root, license_file, version=args.version, force=args.force)

        print(json.dumps({
            "ok": True,
            "destination": str(DEST_CESIUM),
            "version": manifest["version"],
            "files": len(manifest["files"]),
            "manifest": str(DEST_ROOT / "manifest.json"),
            "upstream_commit": manifest.get("upstream_commit"),
        }, ensure_ascii=False))
        return 0
    finally:
        if holder is not None:
            holder.cleanup()


if __name__ == "__main__":
    raise SystemExit(main())
