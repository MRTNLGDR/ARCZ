from __future__ import annotations

from html import escape
import json
from pathlib import Path
from typing import Any

from .atomic_io import atomic_write_bytes
from .errors import ApiError
from .hashing import sha256_file


class SheetComposer:
    """Compositor vetorial real para pranchas a partir de passes existentes.

    Não inventa plantas/cortes. Cada viewport precisa apontar para uma imagem
    efetivamente exportada pelo ARCZ. O resultado SVG é editável e determinístico.
    """

    def __init__(self, root: Path):
        self.root = root.resolve()

    def compose_svg(self, specification: dict[str, Any], destination: Path) -> dict[str, Any]:
        width = float(specification.get("width_mm", 841))
        height = float(specification.get("height_mm", 594))
        if width <= 0 or height <= 0:
            raise ApiError("SHEET_SIZE_INVALID", "Dimensão da prancha precisa ser positiva", status=400)
        title = str(specification.get("title", "ARCZ"))
        elements = []
        for index, viewport in enumerate(specification.get("viewports", [])):
            source = self._source(viewport.get("source", ""))
            x, y = float(viewport.get("x_mm", 0)), float(viewport.get("y_mm", 0))
            w, h = float(viewport.get("width_mm", 100)), float(viewport.get("height_mm", 100))
            if w <= 0 or h <= 0:
                raise ApiError("SHEET_VIEWPORT_INVALID", f"Viewport {index} sem dimensão válida", status=400)
            mime = "image/png" if source.suffix.lower() == ".png" else "image/jpeg"
            import base64
            encoded = base64.b64encode(source.read_bytes()).decode("ascii")
            label = escape(str(viewport.get("label", source.stem)))
            elements.append(
                f'<image x="{x}" y="{y}" width="{w}" height="{h}" preserveAspectRatio="xMidYMid meet" '
                f'href="data:{mime};base64,{encoded}"/>'
                f'<text x="{x}" y="{y+h+4}" font-size="3.2" font-family="sans-serif">{label}</text>'
            )
        svg = f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}mm" height="{height}mm" viewBox="0 0 {width} {height}">
<rect width="100%" height="100%" fill="white"/>
<rect x="5" y="5" width="{width-10}" height="{height-10}" fill="none" stroke="black" stroke-width="0.25"/>
<text x="10" y="12" font-size="5" font-family="sans-serif" font-weight="700">{escape(title)}</text>
{''.join(elements)}
</svg>"""
        destination = destination.resolve()
        try: destination.relative_to(self.root)
        except ValueError as error:
            raise ApiError("SHEET_OUTPUT_ESCAPE", str(destination), status=400) from error
        atomic_write_bytes(destination, svg.encode("utf-8"))
        return {"path": str(destination.relative_to(self.root).as_posix()),
                "sha256": sha256_file(destination), "bytes": destination.stat().st_size, "kind": "svg"}

    def _source(self, relative: str) -> Path:
        source = (self.root / str(relative).lstrip("/\\")).resolve()
        try: source.relative_to(self.root)
        except ValueError as error:
            raise ApiError("SHEET_SOURCE_ESCAPE", relative, status=400) from error
        if not source.is_file() or source.suffix.lower() not in {".png", ".jpg", ".jpeg"}:
            raise ApiError("SHEET_SOURCE_MISSING", relative, status=404)
        return source
