#!/usr/bin/env python3
from __future__ import annotations

"""Canonical ARCZ entrypoint for the controlled Aedifex fork.

It reuses the audited vendor pipeline while applying provenance-safe catalog
localization and ARCZ-local runtime rewrites only to the controlled fork. The
immutable upstream checkout is never modified.
"""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import tools.vendor_aedifex as vendor
from arcz_server.aedifex_catalog_localizer import localize_catalog_assets

_ORIGINAL_RUNTIME_LOCALIZER = vendor.localize_runtime_sources
_OPENAI_TYPE_IMPORT = "import type { ChatCompletionTool } from 'openai/resources/chat/completions'\n"
_LOCAL_TOOL_TYPE = """type ChatCompletionTool = {
  type: 'function'
  function: {
    name: string
    description?: string
    parameters?: Record<string, unknown>
    strict?: boolean
  }
}
"""


def _localize_catalog(fork: Path):
    return localize_catalog_assets(fork, vendor._local_catalog_value)


def _localize_runtime(fork: Path) -> dict[str, object]:
    report = _ORIGINAL_RUNTIME_LOCALIZER(fork)
    tools = fork / "packages/editor/src/components/ai/prompt/openai-tools.ts"
    text = tools.read_text(encoding="utf-8")
    count = text.count(_OPENAI_TYPE_IMPORT)
    if count != 1:
        raise RuntimeError(
            f"import de tipo OpenAI inesperado no fork Aedifex: esperado 1, encontrado {count}"
        )
    tools.write_text(text.replace(_OPENAI_TYPE_IMPORT, _LOCAL_TOOL_TYPE, 1), encoding="utf-8")
    rewrites = report.setdefault("source_rewrites", {})
    if not isinstance(rewrites, dict):
        raise RuntimeError("LOCALIZATION_REPORT source_rewrites inválido")
    rewrites["openai_sdk_type_coupling_removed"] = 1
    return report


def main() -> int:
    vendor.localize_catalog_assets = _localize_catalog
    vendor.localize_runtime_sources = _localize_runtime
    return vendor.main()


if __name__ == "__main__":
    raise SystemExit(main())
