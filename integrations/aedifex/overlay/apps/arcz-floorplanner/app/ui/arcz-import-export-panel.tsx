'use client'

import { type SceneSnapshot } from '@aedifex/core'
import {
  applySceneGraphToEditor,
  useScene,
} from '@aedifex/editor'
import {
  type AedifexSceneGraph,
  convertIfcToAedifex,
} from '@aedifex/ifc-converter'
import { Download, FileBox, FileJson, RotateCcw, Upload } from 'lucide-react'
import { useCallback, useMemo, useRef, useState } from 'react'

const MAX_IMPORT_BYTES = 1024 * 1024 * 1024

type ImportMetadata = {
  format: 'ifc' | 'aedifex-json'
  filename: string
  sha256: string
  bytes: number
  stats?: unknown
}

type Props = {
  currentRevision: number
  onCommitImportedScene: (scene: SceneSnapshot, metadata: ImportMetadata) => Promise<void>
}

type Status =
  | { state: 'idle' }
  | { state: 'reading' | 'converting' | 'applying' | 'saving'; message: string; progress?: number }
  | { state: 'success'; message: string }
  | { state: 'error'; message: string; code?: string }

function cloneValue<T>(value: T): T {
  if (typeof structuredClone === 'function') return structuredClone(value)
  return JSON.parse(JSON.stringify(value)) as T
}

function snapshotFromStore(): SceneSnapshot {
  const state = useScene.getState() as any
  return cloneValue({
    nodes: state.nodes || {},
    rootNodeIds: state.rootNodeIds || [],
    collections: state.collections || {},
    materials: state.materials || {},
    installedPlugins: state.installedPlugins || [],
  }) as SceneSnapshot
}

async function sha256(bytes: ArrayBuffer): Promise<string> {
  const value = await crypto.subtle.digest('SHA-256', bytes)
  return Array.from(new Uint8Array(value), (item) => item.toString(16).padStart(2, '0')).join('')
}

function verifyFile(file: File) {
  if (!file.name.trim()) throw new Error('Arquivo sem nome')
  if (file.size <= 0) throw new Error('Arquivo vazio')
  if (file.size > MAX_IMPORT_BYTES) throw new Error('Arquivo excede o limite local de 1 GiB')
  const extension = file.name.toLowerCase().split('.').pop()
  if (!['ifc', 'json', 'aedifex'].includes(extension || '')) {
    throw new Error('Use IFC ou um scene graph Aedifex JSON')
  }
  return extension === 'ifc' ? 'ifc' : 'aedifex-json'
}

function validateIfcHeader(bytes: ArrayBuffer) {
  const prefix = new TextDecoder('ascii').decode(bytes.slice(0, Math.min(bytes.byteLength, 4096)))
  if (!prefix.includes('ISO-10303-21')) {
    const error = new Error('O arquivo não possui o cabeçalho STEP/IFC ISO-10303-21') as Error & { code?: string }
    error.code = 'IFC_HEADER_INVALID'
    throw error
  }
}

function parseAedifexJson(bytes: ArrayBuffer): AedifexSceneGraph {
  const value = JSON.parse(new TextDecoder().decode(bytes))
  if (!value || typeof value !== 'object' || !value.nodes || !Array.isArray(value.rootNodeIds)) {
    const error = new Error('JSON não contém nodes e rootNodeIds do Aedifex') as Error & { code?: string }
    error.code = 'AEDIFEX_SCENE_JSON_INVALID'
    throw error
  }
  return value as AedifexSceneGraph
}

function download(name: string, type: string, content: BlobPart) {
  const url = URL.createObjectURL(new Blob([content], { type }))
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = name
  anchor.click()
  setTimeout(() => URL.revokeObjectURL(url), 0)
}

export function ArczImportExportPanel({ currentRevision, onCommitImportedScene }: Props) {
  const inputRef = useRef<HTMLInputElement | null>(null)
  const [status, setStatus] = useState<Status>({ state: 'idle' })
  const [allowReplace, setAllowReplace] = useState(currentRevision === 0)
  const busy = ['reading', 'converting', 'applying', 'saving'].includes(status.state)
  const sceneNodeCount = useScene((state: any) => Object.keys(state.nodes || {}).length)

  const canImport = useMemo(
    () => !busy && (currentRevision === 0 || allowReplace),
    [allowReplace, busy, currentRevision],
  )

  const importFile = useCallback(async (file: File) => {
    if (!canImport) return
    const before = snapshotFromStore()
    let applied = false
    try {
      const format = verifyFile(file)
      setStatus({ state: 'reading', message: `Lendo ${file.name}…`, progress: 2 })
      const bytes = await file.arrayBuffer()
      const digest = await sha256(bytes)
      let graph: AedifexSceneGraph
      let stats: unknown
      if (format === 'ifc') {
        validateIfcHeader(bytes)
        setStatus({ state: 'converting', message: 'Convertendo IFC para o scene graph nativo…', progress: 5 })
        const converted = await convertIfcToAedifex(new Uint8Array(bytes), (message, percent) => {
          setStatus({ state: 'converting', message, progress: Math.max(5, Math.min(90, percent)) })
        })
        graph = converted
        stats = (converted as any).stats
      } else {
        graph = parseAedifexJson(bytes)
      }

      setStatus({ state: 'applying', message: 'Validando e aplicando a cena de forma transacional…', progress: 92 })
      applySceneGraphToEditor(graph)
      applied = true
      const current = snapshotFromStore()
      if (Object.keys(current.nodes).length === 0 || current.rootNodeIds.length === 0) {
        throw new Error('A conversão não produziu uma cena utilizável')
      }
      setStatus({ state: 'saving', message: 'Persistindo nova revisão ARCZ…', progress: 97 })
      await onCommitImportedScene(current, {
        format,
        filename: file.name,
        sha256: digest,
        bytes: bytes.byteLength,
        stats,
      })
      setStatus({
        state: 'success',
        message: `${file.name} importado: ${Object.keys(current.nodes).length} nós persistidos.`,
      })
    } catch (caught: any) {
      if (applied) {
        try { applySceneGraphToEditor(before as any) }
        catch (rollbackError) { console.error('[ARCZ/Aedifex] rollback visual falhou', rollbackError) }
      }
      setStatus({
        state: 'error',
        message: caught?.message || String(caught),
        code: caught?.code,
      })
    } finally {
      if (inputRef.current) inputRef.current.value = ''
    }
  }, [canImport, onCommitImportedScene])

  const exportJson = useCallback(() => {
    const snapshot = snapshotFromStore()
    download(
      `arcz-aedifex-scene-r${currentRevision}.json`,
      'application/json',
      JSON.stringify(snapshot, null, 2),
    )
  }, [currentRevision])

  return (
    <div className="arcz-tool-panel">
      <header>
        <FileBox size={18} />
        <div>
          <strong>IFC e scene graph</strong>
          <span>Conversão local, sem upload externo</span>
        </div>
      </header>

      <section className="arcz-tool-section">
        <h3>Importar</h3>
        <p>
          IFC é convertido pelo pacote oficial <code>@aedifex/ifc-converter</code>. A cena atual só é
          substituída depois da conversão e volta ao estado anterior se a revisão não persistir.
        </p>
        {currentRevision > 0 ? (
          <label className="arcz-confirm-row">
            <input
              checked={allowReplace}
              disabled={busy}
              onChange={(event) => setAllowReplace(event.target.checked)}
              type="checkbox"
            />
            Confirmo substituir a cena autoral atual. A revisão anterior permanece no histórico ARCZ.
          </label>
        ) : null}
        <input
          accept=".ifc,.json,.aedifex,application/json,application/x-step"
          hidden
          onChange={(event) => {
            const file = event.currentTarget.files?.[0]
            if (file) void importFile(file)
          }}
          ref={inputRef}
          type="file"
        />
        <button disabled={!canImport} onClick={() => inputRef.current?.click()} type="button">
          <Upload size={15} /> Selecionar IFC ou JSON
        </button>
      </section>

      <section className="arcz-tool-section">
        <h3>Exportar fonte editável</h3>
        <p>{sceneNodeCount} nós na cena atual. GLB, OBJ e STL continuam disponíveis nas ferramentas nativas.</p>
        <button disabled={busy || sceneNodeCount === 0} onClick={exportJson} type="button">
          <FileJson size={15} /> Exportar Aedifex JSON
        </button>
      </section>

      {status.state !== 'idle' ? (
        <section className={`arcz-import-status is-${status.state}`} role={status.state === 'error' ? 'alert' : 'status'}>
          {busy ? <RotateCcw className="arcz-spin" size={15} /> : status.state === 'success' ? <Download size={15} /> : null}
          <div>
            <strong>{status.state === 'error' ? status.code || 'IMPORT_FAILED' : status.state.toUpperCase()}</strong>
            <span>{status.message}</span>
            {'progress' in status && typeof status.progress === 'number' ? (
              <progress max={100} value={status.progress} />
            ) : null}
          </div>
        </section>
      ) : null}
    </div>
  )
}
