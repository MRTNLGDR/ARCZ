'use client'

import { nodeRegistry } from '@aedifex/core'
import {
  getFloorplanNodeExtension,
  isFloorplanToolAvailableInMode,
  MaterialPaintPanel,
  TerrainSculptPanel,
  useEditor,
  useFloorplanMode,
} from '@aedifex/editor'
import { useMemo } from 'react'

type ToolEntry = {
  kind: string
  label: string
  section: string
  order: number
}

const SECTION_LABELS: Record<string, string> = {
  structure: 'Estrutura e arquitetura',
  openings: 'Aberturas',
  mep: 'MEP e instalações',
  site: 'Terreno e sítio',
  furnish: 'Elementos e mobiliário',
  technical: 'Técnico',
  other: 'Outros',
}

function sectionOf(definition: any): string {
  const declared = String(definition?.presentation?.paletteSection || '').toLowerCase()
  if (declared) return declared
  const capabilities = definition?.capabilities || {}
  if (capabilities.wallOpeningPlacement) return 'openings'
  if (capabilities.roofAccessory) return 'structure'
  if (capabilities.floorPlacement || capabilities.wallPlacement || capabilities.ceilingPlacement) return 'furnish'
  return 'other'
}

function activateTool(kind: string): void {
  const definition = nodeRegistry.get(kind)
  if (!definition) throw new Error(`Ferramenta Aedifex não registrada: ${kind}`)
  const extension = getFloorplanNodeExtension(definition)
  const floorplan = useFloorplanMode.getState()
  if (!extension?.tool || !isFloorplanToolAvailableInMode(extension.availableModes, floorplan.mode)) {
    floorplan.showExpertModeNotice(definition.presentation?.label ?? kind)
    return
  }
  const editor = useEditor.getState()
  if (extension.preferredView) editor.setViewMode(extension.preferredView)
  editor.setPhase('structure')
  editor.setStructureLayer('elements')
  editor.setCatalogCategory(null)
  editor.setToolDefaults(kind, null)
  editor.setMode('build')
  editor.setTool(kind)
}

export function ArczBuildPanel() {
  const activeTool = useEditor((state) => state.tool)
  const mode = useEditor((state) => state.mode)
  const floorplanMode = useFloorplanMode((state) => state.mode)

  const groups = useMemo(() => {
    const entries: ToolEntry[] = []
    for (const [kind, definition] of nodeRegistry.entries()) {
      const extension = getFloorplanNodeExtension(definition)
      const presentation = definition.presentation
      if (!extension?.tool || presentation?.hidden) continue
      if (!isFloorplanToolAvailableInMode(extension.availableModes, floorplanMode)) continue
      entries.push({
        kind,
        label: presentation?.label || kind,
        section: sectionOf(definition),
        order: Number(presentation?.paletteOrder ?? Number.MAX_SAFE_INTEGER),
      })
    }
    entries.sort((a, b) => a.section.localeCompare(b.section) || a.order - b.order || a.label.localeCompare(b.label))
    const map = new Map<string, ToolEntry[]>()
    for (const entry of entries) map.set(entry.section, [...(map.get(entry.section) || []), entry])
    return [...map.entries()]
  }, [floorplanMode])

  return (
    <div className="arcz-aedifex-panel arcz-build-panel">
      <header className="arcz-panel-copy">
        <strong>Construção integral</strong>
        <span>Todos os tipos registrados no kernel local. A lista cresce automaticamente com plugins vendorizados.</span>
      </header>

      <div className="arcz-tool-grid arcz-tool-grid--special">
        <button
          className={mode === 'material-paint' ? 'is-active' : ''}
          onClick={() => useEditor.getState().setMode('material-paint')}
          type="button"
        >
          Materiais
        </button>
        <button
          className={mode === 'terrain-sculpt' ? 'is-active' : ''}
          onClick={() => useEditor.getState().setMode('terrain-sculpt')}
          type="button"
        >
          Esculpir terreno
        </button>
      </div>

      {mode === 'material-paint' ? (
        <MaterialPaintPanel />
      ) : mode === 'terrain-sculpt' ? (
        <TerrainSculptPanel />
      ) : groups.length ? (
        groups.map(([section, tools]) => (
          <section className="arcz-tool-section" key={section}>
            <h3>{SECTION_LABELS[section] || section}</h3>
            <div className="arcz-tool-grid">
              {tools.map((tool) => (
                <button
                  className={mode === 'build' && activeTool === tool.kind ? 'is-active' : ''}
                  key={tool.kind}
                  onClick={() => activateTool(tool.kind)}
                  title={tool.kind}
                  type="button"
                >
                  {tool.label}
                </button>
              ))}
            </div>
          </section>
        ))
      ) : (
        <div className="arcz-empty-state">Nenhuma ferramenta registrada. O build Aedifex está incompleto.</div>
      )}
    </div>
  )
}
