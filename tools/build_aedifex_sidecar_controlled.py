#!/usr/bin/env python3
from __future__ import annotations

"""ARCZ packaging guard for the Aedifex standalone build.

Next.js standalone generated from this Bun workspace may omit a small number of
runtime packages even though they are present in the frozen Bun store. We only
materialize packages that are either declared runtime dependencies of the ARCZ
host or were proven missing by a real standalone smoke. We never install/fetch
anything at smoke time.

The controlled fork lock has already been verified by a second frozen/offline
install. A missing or ambiguous package in that store is fatal. The final vendor
tree cannot contain dangling/external symlinks.
"""

from pathlib import Path
import json
import os
import re
import shutil
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import tools.build_aedifex_sidecar as builder

_ORIGINAL_COPYTREE = shutil.copytree
# @swc/helpers and @next/env were observed missing by actual standalone smokes.
# react/react-dom are explicit dependencies of the ARCZ floorplanner host and
# Next's production server imports them before serving /api/health. scheduler is
# a runtime dependency of react-dom. Every package is resolved only from the
# already-frozen Bun store; nothing is fetched by this packager.
_REQUIRED_RUNTIME_PACKAGES = (
    "@swc/helpers",
    "@next/env",
    "react",
    "react-dom",
    "scheduler",
)


def _inside(path: Path, root: Path) -> bool:
    try:
        path.resolve(strict=False).relative_to(root.resolve())
        return True
    except ValueError:
        return False


def _read_package(path: Path) -> dict[str, object] | None:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    return value if isinstance(value, dict) else None


def _fallback_targets(link: Path, standalone: Path) -> list[Path]:
    rel = link.relative_to(standalone)
    values = [
        builder.FORK / rel,
        builder.FORK / "apps/arcz-floorplanner" / rel,
    ]
    parts = rel.parts
    marker = ("node_modules", ".bun", "node_modules")
    for index in range(max(0, len(parts) - len(marker) + 1)):
        if tuple(parts[index:index + len(marker)]) == marker:
            values.insert(0, builder.FORK / Path(*parts[index:]))
            break
    result: list[Path] = []
    for value in values:
        if value not in result:
            result.append(value)
    return result


def _declared_runtime_requirement(standalone: Path, package_name: str) -> str | None:
    manifests = [
        builder.FORK / "apps/arcz-floorplanner/package.json",
        standalone / "node_modules/next/package.json",
        builder.FORK / "node_modules/next/package.json",
        builder.FORK / "apps/arcz-floorplanner/node_modules/next/package.json",
        builder.FORK / "node_modules/react-dom/package.json",
        builder.FORK / "apps/arcz-floorplanner/node_modules/react-dom/package.json",
    ]
    for manifest_path in manifests:
        package = _read_package(manifest_path)
        if not package:
            continue
        for field in ("dependencies", "optionalDependencies", "peerDependencies"):
            values = package.get(field)
            if isinstance(values, dict) and package_name in values:
                requirement = values.get(package_name)
                if isinstance(requirement, str) and requirement.strip():
                    return requirement.strip()
    return None


def _bun_store_candidates(package_name: str) -> list[Path]:
    store = builder.FORK / "node_modules/.bun"
    if not store.is_dir():
        return []
    package_parts = Path(*package_name.split("/"))
    pattern = str(Path("*") / "node_modules" / package_parts / "package.json")
    candidates: list[Path] = []
    seen: set[Path] = set()
    for manifest_path in store.glob(pattern):
        package = _read_package(manifest_path)
        if not package or package.get("name") != package_name:
            continue
        try:
            resolved = manifest_path.parent.resolve(strict=True)
        except (FileNotFoundError, RuntimeError, OSError):
            continue
        if not _inside(resolved, builder.FORK):
            raise RuntimeError(
                f"pacote Bun escapou do fork controlado: {package_name} -> {resolved}"
            )
        if resolved not in seen:
            seen.add(resolved)
            candidates.append(resolved)
    return sorted(candidates, key=lambda path: path.as_posix())


def _version_of(package_dir: Path) -> str | None:
    package = _read_package(package_dir / "package.json")
    value = package.get("version") if package else None
    return value.strip() if isinstance(value, str) and value.strip() else None


def _exact_requirement_version(requirement: str | None) -> str | None:
    if not requirement:
        return None
    match = re.fullmatch(
        r"(?:npm:)?(?:[~^])?(\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?)",
        requirement.strip(),
    )
    return match.group(1) if match else None


def _resolve_fork_package(package_name: str, standalone: Path) -> Path:
    package_path = Path(*package_name.split("/"))
    direct = [
        builder.FORK / "node_modules" / package_path,
        builder.FORK / "apps/arcz-floorplanner/node_modules" / package_path,
    ]
    direct_matches: list[Path] = []
    for candidate in direct:
        try:
            resolved = candidate.resolve(strict=True)
        except (FileNotFoundError, RuntimeError, OSError):
            continue
        package = _read_package(resolved / "package.json")
        if package and package.get("name") == package_name:
            if not _inside(resolved, builder.FORK):
                raise RuntimeError(
                    f"pacote runtime escapou do fork controlado: {package_name} -> {resolved}"
                )
            if resolved not in direct_matches:
                direct_matches.append(resolved)
    if len(direct_matches) == 1:
        return direct_matches[0]
    if len(direct_matches) > 1 and len({_version_of(path) for path in direct_matches}) == 1:
        return sorted(direct_matches, key=lambda path: path.as_posix())[0]

    candidates = _bun_store_candidates(package_name)
    if not candidates:
        raise RuntimeError(
            f"pacote runtime obrigatório não existe no Bun store congelado: {package_name}"
        )
    if len(candidates) == 1:
        return candidates[0]

    requirement = _declared_runtime_requirement(standalone, package_name)
    exact = _exact_requirement_version(requirement)
    if exact:
        exact_matches = [path for path in candidates if _version_of(path) == exact]
        unique = sorted({path.resolve() for path in exact_matches}, key=lambda p: p.as_posix())
        if len(unique) == 1:
            return unique[0]

    versions = {_version_of(candidate) for candidate in candidates}
    if len(versions) == 1 and None not in versions:
        return candidates[0]

    inventory = ", ".join(
        f"{_version_of(candidate) or '?'}:{candidate.relative_to(builder.FORK).as_posix()}"
        for candidate in candidates
    )
    raise RuntimeError(
        "pacote runtime possui múltiplas versões no Bun store; seleção por "
        f"aproximação proibida: {package_name}; requirement={requirement!r}; candidates={inventory}"
    )


def materialize_required_packages(standalone: Path) -> list[dict[str, str]]:
    materialized: list[dict[str, str]] = []
    for package_name in _REQUIRED_RUNTIME_PACKAGES:
        destination = standalone / "node_modules" / Path(*package_name.split("/"))
        existing = _read_package(destination / "package.json")
        if existing and existing.get("name") == package_name:
            continue
        source = _resolve_fork_package(package_name, standalone)
        if destination.exists() or destination.is_symlink():
            if destination.is_dir() and not destination.is_symlink():
                shutil.rmtree(destination)
            else:
                destination.unlink()
        destination.parent.mkdir(parents=True, exist_ok=True)
        _ORIGINAL_COPYTREE(source, destination, symlinks=False)
        copied = _read_package(destination / "package.json")
        if not copied or copied.get("name") != package_name:
            raise RuntimeError(f"materialização do pacote runtime falhou: {package_name}")
        materialized.append({
            "package": package_name,
            "version": str(copied.get("version") or "unknown"),
            "requirement": _declared_runtime_requirement(standalone, package_name) or "transitive",
            "source": source.relative_to(builder.FORK).as_posix(),
            "destination": destination.relative_to(standalone).as_posix(),
        })
    return materialized


def materialize_dangling_links(standalone: Path) -> list[dict[str, str]]:
    repaired: list[dict[str, str]] = []
    links = sorted(
        (path for path in standalone.rglob("*") if path.is_symlink()),
        key=lambda path: len(path.parts),
    )
    for link in links:
        raw_target = os.readlink(link)
        resolved = (link.parent / raw_target).resolve(strict=False)
        if resolved.exists() and _inside(resolved, standalone):
            continue
        source = None
        for candidate in _fallback_targets(link, standalone):
            try:
                candidate_resolved = candidate.resolve(strict=True)
            except (FileNotFoundError, RuntimeError, OSError):
                continue
            if candidate_resolved.is_file() or candidate_resolved.is_dir():
                source = candidate_resolved
                break
        if source is None:
            raise RuntimeError(
                "Aedifex standalone contém symlink sem alvo materializável: "
                f"{link.relative_to(standalone)} -> {raw_target}"
            )
        link.unlink()
        if source.is_dir():
            _ORIGINAL_COPYTREE(source, link, symlinks=False)
        else:
            link.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, link)
        repaired.append({
            "path": link.relative_to(standalone).as_posix(),
            "original_target": raw_target,
            "materialized_from": source.relative_to(builder.FORK).as_posix()
                if _inside(source, builder.FORK) else str(source),
        })

    remaining = []
    for link in standalone.rglob("*"):
        if link.is_symlink():
            resolved = link.resolve(strict=False)
            if not resolved.exists() or not _inside(resolved, standalone):
                remaining.append(f"{link.relative_to(standalone)} -> {os.readlink(link)}")
    if remaining:
        raise RuntimeError(
            "standalone ainda possui symlink dangling/externo após materialização:\n"
            + "\n".join(remaining[:50])
        )
    return repaired


def _guarded_copytree(src, dst, *args, **kwargs):
    source = Path(src)
    try:
        is_standalone = source.resolve() == (
            builder.FORK / "apps/arcz-floorplanner/.next/standalone"
        ).resolve()
    except OSError:
        is_standalone = False
    if is_standalone:
        repaired = materialize_dangling_links(source)
        packages = materialize_required_packages(source)
        print(f"[aedifex-standalone] materialized {len(repaired)} Bun link(s)")
        for item in repaired:
            print(f"[aedifex-standalone] {item['path']} <= {item['materialized_from']}")
        for item in packages:
            print(
                "[aedifex-standalone] runtime package "
                f"{item['package']}@{item['version']} ({item['requirement']}) <= {item['source']}"
            )
        kwargs["symlinks"] = False
    return _ORIGINAL_COPYTREE(src, dst, *args, **kwargs)


def main() -> int:
    original = builder.shutil.copytree
    builder.shutil.copytree = _guarded_copytree
    try:
        return builder.main()
    finally:
        builder.shutil.copytree = original


if __name__ == "__main__":
    raise SystemExit(main())
