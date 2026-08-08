import { loadPlugin, nodeRegistry } from '@aedifex/core'
import { registerEditorHostPanel } from '@aedifex/editor'
import { builtinPlugin } from '@aedifex/nodes'
import { treesHostPanel, treesPlugin } from '@aedifex/plugin-trees'

/** Offline deterministic plugin bootstrap. Do not bypass `loadPlugin`: it is
 * the API-v2/version/ownership gate used by the pinned upstream. */
let promise: Promise<{ kinds: string[] }> | null = null
let panelRegistered = false

export function ensureAedifexBootstrapped(): Promise<{ kinds: string[] }> {
  promise ??= (async () => {
    for (const plugin of [builtinPlugin, treesPlugin]) {
      try { await loadPlugin(plugin) }
      catch (caught) {
        const message = caught instanceof Error ? caught.message : String(caught)
        if (!message.includes('duplicate node kind')) throw caught
      }
    }
    if (!panelRegistered) {
      registerEditorHostPanel(treesHostPanel)
      panelRegistered = true
    }
    const kinds = Array.from(nodeRegistry.entries(), ([kind]) => kind).sort()
    console.info(`[ARCZ/Aedifex] ${kinds.length} tipos locais registrados via Plugin API v2`)
    return { kinds }
  })()
  return promise
}
