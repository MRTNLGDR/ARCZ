#!/usr/bin/env python3
from __future__ import annotations

"""Canonical ARCZ entrypoint for the controlled Aedifex fork.

Only the controlled fork is rewritten. The pinned upstream checkout remains
immutable and is still the conformance oracle.
"""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import tools.vendor_aedifex as vendor
from arcz_server.aedifex_catalog_localizer import localize_catalog_assets

_ORIGINAL_RUNTIME_LOCALIZER = vendor.localize_runtime_sources
_ORIGINAL_MERGE_WORKSPACE = vendor.merge_workspace
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


def _replace_exact(text: str, old: str, new: str, expected: int, label: str) -> str:
    count = text.count(old)
    if count != expected:
        raise RuntimeError(
            f"rewrite {label} esperava {expected} ocorrência(s), encontrou {count}"
        )
    return text.replace(old, new)


def _replace_once(text: str, old: str, new: str, label: str) -> str:
    return _replace_exact(text, old, new, 1, label)


def _patch_autosave_performance(fork: Path) -> None:
    """Remove scene-wide JSON serialization from the continuous drag hot path.

    Zustand replaces the scene containers when they change. Reference comparison
    is therefore enough to schedule the existing debounced save. Serialization
    still occurs when persistence actually writes the snapshot; it no longer
    happens for every pointer movement.
    """

    path = fork / "packages/editor/src/hooks/use-auto-save.ts"
    text = path.read_text(encoding="utf-8")
    text = _replace_once(
        text,
        """    let lastNodesSnapshot = JSON.stringify(useScene.getState().nodes)
    let lastNodeCount = Object.keys(useScene.getState().nodes).length
""",
        """    let lastNodesRef = useScene.getState().nodes
    let lastRootNodeIdsRef = useScene.getState().rootNodeIds
    let lastNodeCount = Object.keys(useScene.getState().nodes).length
""",
        "autosave initial node references",
    )

    refresh_old = """        lastNodesSnapshot = JSON.stringify(state.nodes)
        lastCollectionsRef = state.collections
        lastMaterialsRef = state.materials
        lastInstalledPluginsRef = state.installedPlugins
"""
    refresh_new = """        lastNodesRef = state.nodes
        lastRootNodeIdsRef = state.rootNodeIds
        lastCollectionsRef = state.collections
        lastMaterialsRef = state.materials
        lastInstalledPluginsRef = state.installedPlugins
"""
    # The pinned upstream has this exact refresh block twice: while loading and
    # while version-preview mode is active. Requiring exactly two occurrences
    # keeps the rewrite fail-closed if upstream behavior changes.
    text = _replace_exact(
        text,
        refresh_old,
        refresh_new,
        2,
        "autosave loading/preview reference refresh",
    )

    text = _replace_once(
        text,
        """      const currentNodesSnapshot = JSON.stringify(state.nodes)
      const changed =
        currentNodesSnapshot !== lastNodesSnapshot ||
        state.collections !== lastCollectionsRef ||
        state.materials !== lastMaterialsRef ||
        state.installedPlugins !== lastInstalledPluginsRef
""",
        """      const changed =
        state.nodes !== lastNodesRef ||
        state.rootNodeIds !== lastRootNodeIdsRef ||
        state.collections !== lastCollectionsRef ||
        state.materials !== lastMaterialsRef ||
        state.installedPlugins !== lastInstalledPluginsRef
""",
        "autosave O(1) dirty detection",
    )
    text = _replace_once(
        text,
        """      lastNodesSnapshot = currentNodesSnapshot
      lastCollectionsRef = state.collections
      lastMaterialsRef = state.materials
      lastInstalledPluginsRef = state.installedPlugins
""",
        """      lastNodesRef = state.nodes
      lastRootNodeIdsRef = state.rootNodeIds
      lastCollectionsRef = state.collections
      lastMaterialsRef = state.materials
      lastInstalledPluginsRef = state.installedPlugins
""",
        "autosave dirty reference commit",
    )
    if "JSON.stringify(state.nodes)" in text or "lastNodesSnapshot" in text:
        raise RuntimeError("autosave Aedifex ainda contém serialização scene-wide no hot path")
    path.write_text(text, encoding="utf-8")


def _patch_host_contracts(fork: Path) -> None:
    page = fork / "apps/arcz-floorplanner/app/page.tsx"
    text = page.read_text(encoding="utf-8")
    text = _replace_once(
        text,
        "import { Editor, ItemsPanel, SettingsPanel } from '@aedifex/editor'",
        "import { Editor, ItemsPanel, SettingsPanel, useScene, type EditorProps } from '@aedifex/editor'",
        "editor imports",
    )
    text = _replace_once(
        text,
        "type LocationConfig = { projectId: string; apiBaseUrl: string; channel: string }\n",
        """type LocationConfig = { projectId: string; apiBaseUrl: string; channel: string }
type EditorSaveHandler = NonNullable<EditorProps['onSave']>

function currentSceneSnapshot(): SceneSnapshot {
  const state = useScene.getState()
  return {
    nodes: state.nodes,
    rootNodeIds: state.rootNodeIds,
    collections: state.collections,
    materials: state.materials,
    installedPlugins: state.installedPlugins,
  }
}
""",
        "typed scene snapshot helper",
    )
    text = _replace_once(
        text,
        """      scene: SceneSnapshot,
      origin = 'editor',
      metadata: Record<string, unknown> = {},
    ) => {
""",
        """      scene: SceneSnapshot,
      origin = 'editor',
      metadata: Record<string, unknown> = {},
      requestOptions: { keepalive?: boolean } = {},
    ) => {
""",
        "persistScene keepalive signature",
    )
    text = _replace_once(
        text,
        "const result = await client.saveScene(scene, revisionRef.current, origin, metadata)",
        "const result = await client.saveScene(scene, revisionRef.current, origin, metadata, requestOptions)",
        "persistScene bridge call",
    )
    text = _replace_once(
        text,
        """  const save = useCallback(
    (scene: SceneSnapshot) => persistScene(scene, 'editor'),
    [persistScene],
  )
""",
        """  const save: EditorSaveHandler = useCallback(
    async (_scene, options) => {
      await persistScene(currentSceneSnapshot(), 'editor', {}, options)
    },
    [persistScene],
  )
""",
        "Editor onSave contract",
    )
    page.write_text(text, encoding="utf-8")

    bridge = fork / "packages/arcz-bridge/src/index.ts"
    text = bridge.read_text(encoding="utf-8")
    text = _replace_once(
        text,
        """    origin = 'editor',
    metadata: Record<string, unknown> = {},
  ) {
""",
        """    origin = 'editor',
    metadata: Record<string, unknown> = {},
    requestOptions: { keepalive?: boolean } = {},
  ) {
""",
        "bridge saveScene keepalive signature",
    )
    text = _replace_once(
        text,
        """      {
        method: 'POST',
        body: JSON.stringify({
""",
        """      {
        method: 'POST',
        keepalive: requestOptions.keepalive === true,
        body: JSON.stringify({
""",
        "bridge saveScene keepalive request",
    )
    bridge.write_text(text, encoding="utf-8")

    _patch_autosave_performance(fork)


def _merge_workspace(fork: Path) -> None:
    _ORIGINAL_MERGE_WORKSPACE(fork)
    _patch_host_contracts(fork)


def main() -> int:
    vendor.localize_catalog_assets = _localize_catalog
    vendor.localize_runtime_sources = _localize_runtime
    vendor.merge_workspace = _merge_workspace
    return vendor.main()


if __name__ == "__main__":
    raise SystemExit(main())
