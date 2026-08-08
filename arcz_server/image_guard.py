from __future__ import annotations

"""Validação conservadora da estrutura entre beauty base e enhancement.

Não tenta provar equivalência arquitetônica perfeita. Rejeita apenas alterações
claramente incompatíveis usando bordas dilatadas, e registra a métrica para
revisão humana. Usa apenas Pillow, já obrigatório no runtime local.
"""

from pathlib import Path
from typing import Any

from PIL import Image, ImageChops, ImageFilter, ImageOps


def _edge_mask(path: Path, *, threshold: int = 28) -> Image.Image:
    with Image.open(path) as source:
        gray = ImageOps.grayscale(source)
        edges = gray.filter(ImageFilter.FIND_EDGES)
        return edges.point(lambda value: 255 if value >= threshold else 0, mode="1").convert("L")


def compare_structure(base: Path, enhanced: Path, *, guard_px: int = 2,
                      max_mismatch_ratio: float = 0.28) -> dict[str, Any]:
    base = base.resolve(); enhanced = enhanced.resolve()
    base_edges = _edge_mask(base)
    enhanced_edges = _edge_mask(enhanced)
    if enhanced_edges.size != base_edges.size:
        enhanced_edges = enhanced_edges.resize(base_edges.size, Image.Resampling.LANCZOS)
    radius = max(0, min(int(guard_px), 32))
    filter_size = radius * 2 + 1
    base_dilated = base_edges.filter(ImageFilter.MaxFilter(filter_size)) if radius else base_edges
    enhanced_dilated = enhanced_edges.filter(ImageFilter.MaxFilter(filter_size)) if radius else enhanced_edges
    missing_from_enhanced = ImageChops.multiply(base_edges, ImageOps.invert(enhanced_dilated))
    new_in_enhanced = ImageChops.multiply(enhanced_edges, ImageOps.invert(base_dilated))
    base_count = sum(base_edges.histogram()[1:]) / 255.0
    enhanced_count = sum(enhanced_edges.histogram()[1:]) / 255.0
    mismatch = (sum(missing_from_enhanced.histogram()[1:]) + sum(new_in_enhanced.histogram()[1:])) / 255.0
    denominator = max(1.0, base_count + enhanced_count)
    ratio = float(mismatch / denominator)
    return {
        "ok": ratio <= float(max_mismatch_ratio),
        "mismatch_ratio": ratio,
        "limit": float(max_mismatch_ratio),
        "guard_px": radius,
        "base_edge_pixels": int(base_count),
        "enhanced_edge_pixels": int(enhanced_count),
    }
