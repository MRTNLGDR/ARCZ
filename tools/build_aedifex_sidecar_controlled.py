#!/usr/bin/env python3
from __future__ import annotations

"""ARCZ packaging guard for the Aedifex standalone build.

Next.js standalone generated from a Bun workspace may contain symlinks under
``.next/standalone/node_modules`` whose targets live in the controlled fork's
Bun store but were not copied by Next's file tracer. The base builder correctly
refuses to silently ignore them; this wrapper materializes those links from the
already-installed, frozen/offline-proven Bun store before the final vendor copy.

Nothing is fetched here. A missing target is fatal. The final vendor tree cannot
contain dangling symlinks or dependencies that resolve outside the standalone
source tree.
"""

from pathlib import Path
import os
import shutil
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import tools.build_aedifex_sidecar as builder

_ORIGINAL_COPYTREE = shutil.copytree


def _inside(path: Path, root: Path) -> bool:
    try:
        path.resolve(strict=False).relative_to(root.resolve())
        return True
    except ValueError:
        return False


def _fallback_targets(link: Path, standalone: Path) -> list[Path]:
    rel = link.relative_to(standalone)
    values = [
        builder.FORK / rel,
        builder.FORK / "apps/arcz-floorplanner" / rel,
    ]
    # Bun's hoisted links under node_modules/.bun/node_modules/<name> always
    # have an equivalent link in the controlled fork root store.
    parts = rel.parts
    marker = ("node_modules", ".bun", "node_modules")
    for index in range(max(0, len(parts) - len(marker) + 1)):
        if tuple(parts[index:index + len(marker)]) == marker:
            suffix = Path(*parts[index:])
            values.insert(0, builder.FORK / suffix)
            break
    result: list[Path] = []
    for value in values:
        if value not in result:
            result.append(value)
    return result


def materialize_dangling_links(standalone: Path) -> list[dict[str, str]]:
    repaired: list[dict[str, str]] = []
    # Snapshot first because replacements change traversal semantics.
    links = sorted(
        (path for path in standalone.rglob("*") if path.is_symlink()),
        key=lambda path: len(path.parts),
    )
    for link in links:
        raw_target = os.readlink(link)
        resolved = (link.parent / raw_target).resolve(strict=False)
        if resolved.exists() and _inside(resolved, standalone):
            continue
        # Even a currently-existing target outside standalone is forbidden in
        # the artifact: the vendor must be self-contained after the worktree is
        # deleted.
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
        if not link.is_symlink():
            continue
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
        print(f"[aedifex-standalone] materialized {len(repaired)} Bun link(s)")
        for item in repaired:
            print(
                "[aedifex-standalone] "
                f"{item['path']} <= {item['materialized_from']}"
            )
        kwargs["symlinks"] = False
    return _ORIGINAL_COPYTREE(src, dst, *args, **kwargs)


def main() -> int:
    # The imported builder references the shared shutil module. Replace only for
    # this process and restore in finally so helper calls remain deterministic.
    original = builder.shutil.copytree
    builder.shutil.copytree = _guarded_copytree
    try:
        return builder.main()
    finally:
        builder.shutil.copytree = original


if __name__ == "__main__":
    raise SystemExit(main())
