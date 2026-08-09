from __future__ import annotations

import re
from pathlib import Path
from typing import Callable

SUPABASE_ITEM_URL_RE = re.compile(
    r"https://[a-z0-9-]+\.supabase\.co/storage/v1/object/public/items/[^'\"`\s]+",
    re.I,
)
ID_RE = re.compile(r"\bid\s*:\s*['\"]([^'\"]+)['\"]")

LocalResolver = Callable[[Path, str], tuple[str | None, str, str]]


def _top_level_object_ranges(text: str, marker: str = "export const CATALOG_ITEMS") -> list[tuple[int, int]]:
    marker_index = text.find(marker)
    if marker_index < 0:
        raise RuntimeError("CATALOG_ITEMS não encontrado")
    array_start = text.find("[", marker_index)
    if array_start < 0:
        raise RuntimeError("array CATALOG_ITEMS não encontrado")

    ranges: list[tuple[int, int]] = []
    depth = 0
    start: int | None = None
    i = array_start + 1
    state = "code"
    quote = ""
    while i < len(text):
        c = text[i]
        n = text[i + 1] if i + 1 < len(text) else ""
        if state == "line_comment":
            if c == "\n":
                state = "code"
            i += 1
            continue
        if state == "block_comment":
            if c == "*" and n == "/":
                state = "code"
                i += 2
            else:
                i += 1
            continue
        if state == "string":
            if c == "\\" and i + 1 < len(text):
                i += 2
                continue
            if c == quote:
                state = "code"
            i += 1
            continue
        if c == "/" and n == "/":
            state = "line_comment"
            i += 2
            continue
        if c == "/" and n == "*":
            state = "block_comment"
            i += 2
            continue
        if c in {"'", '"', "`"}:
            state = "string"
            quote = c
            i += 1
            continue
        if c == "{":
            if depth == 0:
                start = i
            depth += 1
            i += 1
            continue
        if c == "}":
            if depth <= 0:
                raise RuntimeError("chave inesperada em CATALOG_ITEMS")
            depth -= 1
            if depth == 0 and start is not None:
                end = i + 1
                j = end
                while j < len(text) and text[j] in " \t\r\n":
                    j += 1
                if j < len(text) and text[j] == ",":
                    end = j + 1
                ranges.append((start, end))
                start = None
            i += 1
            continue
        if c == "]" and depth == 0:
            break
        i += 1

    if depth != 0 or start is not None:
        raise RuntimeError("CATALOG_ITEMS possui objeto não fechado")
    if not ranges:
        raise RuntimeError("CATALOG_ITEMS não possui entradas")
    return ranges


def localize_catalog_assets(
    fork: Path,
    resolve_local: LocalResolver,
) -> dict[str, object]:
    """Rewrite locally shipped catalog assets and remove remote-only entries.

    Models/thumbnails not present in the immutable upstream checkout are not
    copied from a remote bucket because their independent asset licensing is
    not proven by the source repository. Their complete catalog entry is
    removed from the controlled offline fork and recorded in the report.
    Missing floor-plan artwork is optional and becomes ``undefined``.
    """

    catalog = fork / "packages/editor/src/components/ui/item-catalog/catalog-items.tsx"
    text = catalog.read_text(encoding="utf-8")
    ranges = _top_level_object_ranges(text)
    output: list[str] = []
    cursor = 0
    rewritten = 0
    omitted_floorplans = 0
    excluded_entries: list[dict[str, object]] = []
    remote_urls = 0
    roles: dict[str, int] = {"model": 0, "thumbnail": 0, "floorplan": 0}

    for start, end in ranges:
        block = text[start:end]
        urls = sorted(set(SUPABASE_ITEM_URL_RE.findall(block)))
        if not urls:
            output.append(text[cursor:end])
            cursor = end
            continue

        resolved: list[tuple[str, str | None, str, str]] = []
        missing_required: list[dict[str, str]] = []
        for url in urls:
            local, slug, role = resolve_local(fork, url)
            remote_urls += 1
            roles[role] = roles.get(role, 0) + 1
            resolved.append((url, local, slug, role))
            if local is None and role in {"model", "thumbnail"}:
                missing_required.append({"slug": slug, "role": role, "url": url})

        if missing_required:
            match = ID_RE.search(block)
            item_id = match.group(1) if match else str(missing_required[0]["slug"])
            output.append(text[cursor:start])
            cursor = end
            excluded_entries.append(
                {
                    "id": item_id,
                    "reason": "remote_asset_not_in_upstream_source",
                    "missing": missing_required,
                }
            )
            continue

        localized = block
        for url, local, _slug, role in resolved:
            quoted = re.compile(r"(['\"])" + re.escape(url) + r"\1")
            if local is not None:
                localized, count = quoted.subn(
                    lambda match: f"{match.group(1)}{local}{match.group(1)}", localized
                )
                if count == 0:
                    raise RuntimeError(f"URL catalogada não pôde ser substituída: {url}")
                rewritten += count
            elif role == "floorplan":
                localized, count = quoted.subn("undefined", localized)
                if count == 0:
                    raise RuntimeError(f"floorPlan remoto não pôde ser neutralizado: {url}")
                omitted_floorplans += count
            else:
                raise RuntimeError(f"asset obrigatório sem resolução local: {url}")

        output.append(text[cursor:start])
        output.append(localized)
        cursor = end

    output.append(text[cursor:])
    localized_text = "".join(output)
    if "supabase.co/storage/v1/object/public/items/" in localized_text:
        raise RuntimeError("catálogo Aedifex ainda contém URL Supabase após localização")
    catalog.write_text(localized_text, encoding="utf-8")
    return {
        "remote_urls": remote_urls,
        "rewritten": rewritten,
        "omitted_floorplans": omitted_floorplans,
        "excluded_remote_only_entries": excluded_entries,
        "excluded_remote_only_count": len(excluded_entries),
        "roles": roles,
    }
