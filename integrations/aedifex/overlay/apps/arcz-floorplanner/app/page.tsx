'use client'

import { Editor, ItemsPanel, SettingsPanel } from '@aedifex/editor'
import type { SceneSnapshot } from '@aedifex/core'
import {
  ArczFloorplannerClient,
  ModelingContextSchema,
  normalizeLoopbackApiBase,
  type ModelingContext,
} from '@arcz/aedifex-bridge'
import { Bot, FileUp, Hammer, Layers, Package, Settings, UploadCloud } from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ensureAedifexBootstrapped } from '../lib/bootstrap'
import { ArczBuildPanel } from './ui/arcz-build-panel'
import { CombinedAiPanel } from './ui/combined-ai-panel'
import { ArczPanelBehavior } from './ui/arcz-panel-behavior'
import { ArczImportExportPanel } from './ui/arcz-import-export-panel'
import { ArczRegionPanel } from './ui/arcz-region-panel'
import { RegionContextFloorplan, RegionContextWorld } from './ui/region-context'
import {
  ArczSceneExportBridge,
  type ArczSceneExportHandler,
} from './ui/arcz-scene-export-bridge'

type LocationConfig = { projectId: string; apiBaseUrl: string; channel: string }

function configFromLocation(): LocationConfig {
  const params = new URLSearchParams(window.location.search)
  const projectId = params.get('project')?.trim()
  if (!projectId) throw new Error('URL sem ?project=<floorplanner_project_id>')
  const channel = params.get('channel')?.trim() || ''
  if (!/^[a-f0-9-]{32,64}$/i.test(channel)) throw new Error('URL sem canal de sessão ARCZ válido')
  return { projectId, apiBaseUrl: normalizeLoopbackApiBase(params.get('api')), channel }
}

function Items() {
  // Preserve the complete upstream library surface; no filters are hidden.
  return <ItemsPanel showSourceFilter showTagFilters />
}

function reportToHost(config: LocationConfig | null, payload: Record<string, unknown>): void {
  if (!config || window.parent === window) return
  window.parent.postMessage({ ...payload, channel: config.channel }, config.apiBaseUrl)
}

export default function Home() {
  const [config, setConfig] = useState<LocationConfig | null>(null)
  const [bootState, setBootState] = useState<'loading'|'ready'|'error'>('loading')
  const [bootError, setBootError] = useState('')
  const [context, setContext] = useState<ModelingContext | null>(null)
  const [revision, setRevision] = useState(0)
  const [remoteRevision, setRemoteRevision] = useState<number | null>(null)
  const [sceneHash, setSceneHash] = useState<string | null>(null)
  const [exportState, setExportState] = useState<{status: 'idle' | 'exporting' | 'success' | 'error'; message?: string}>({status: 'idle'})
  const [error, setError] = useState<string | null>(null)
  const [contextLayerErrors, setContextLayerErrors] = useState<Record<string, string>>({})
  const revisionRef = useRef(0)
  const contextRef = useRef<ModelingContext | null>(null)
  const saveChainRef = useRef<Promise<void>>(Promise.resolve())
  const readySentRef = useRef(false)
  const exportHandlerRef = useRef<ArczSceneExportHandler | null>(null)

  useEffect(() => {
    void ensureAedifexBootstrapped().then(() => setBootState('ready')).catch((caught) => { setBootError(caught instanceof Error ? caught.message : String(caught)); setBootState('error') })
    try {
      setConfig(configFromLocation())
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    }
  }, [])

  useEffect(() => {
    if (!config) return
    // Makes the actual ARCZ loopback origin available to the server-side AI
    // proxy even when the ARCZ gateway selected a fallback port. The API route
    // validates loopback again before every request.
    document.cookie = `arcz_api_base=${encodeURIComponent(config.apiBaseUrl)}; Path=/; SameSite=Strict`
  }, [config])

  const client = useMemo(
    () => (config ? new ArczFloorplannerClient(config.projectId, { apiBaseUrl: config.apiBaseUrl }) : null),
    [config],
  )

  const setRevisionSafe = useCallback((value: number) => {
    revisionRef.current = value
    setRevision(value)
  }, [])

  const load = useCallback(async () => {
    if (!client || !config) return null
    try {
      const project = await client.getProject(true)
      const parsedContext = ModelingContextSchema.parse(project.context)
      contextRef.current = parsedContext
      setContext(parsedContext)
      setRevisionSafe(Number(project.current_revision || 0))
      setSceneHash(project.scene_revision?.scene_hash || null)
      setRemoteRevision(null)
      if (!readySentRef.current) {
        readySentRef.current = true
        reportToHost(config, {
          type: 'arcz:aedifex-ready',
          project_id: config.projectId,
          revision: Number(project.current_revision || 0),
          scene_hash: project.scene_revision?.scene_hash || null,
          context_hash: parsedContext.context_hash,
        })
      }
      return (project.scene_revision?.scene || null) as SceneSnapshot | null
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught)
      reportToHost(config, {
        type: 'arcz:aedifex-error',
        project_id: config.projectId,
        message,
        details: (caught as any)?.details,
      })
      throw caught
    }
  }, [client, config, setRevisionSafe])

  const persistScene = useCallback(
    async (
      scene: SceneSnapshot,
      origin = 'editor',
      metadata: Record<string, unknown> = {},
    ) => {
      if (!client || !config) return
      const operation = async () => {
        try {
          const result = await client.saveScene(scene, revisionRef.current, origin, metadata)
          const nextRevision = Number(result.current_revision)
          setRevisionSafe(nextRevision)
          setSceneHash(result.scene_hash || null)
          setRemoteRevision(null)
          reportToHost(config, {
            type: 'arcz:aedifex-saved',
            project_id: config.projectId,
            revision: nextRevision,
            scene_hash: result.scene_hash,
            changed: result.changed,
            origin,
          })
        } catch (caught: any) {
          if (caught?.code === 'FLOORPLANNER_VERSION_CONFLICT') {
            const latest = await client.getProject(false)
            const latestRevision = Number(latest.current_revision || 0)
            setRemoteRevision(latestRevision)
          }
          reportToHost(config, {
            type: 'arcz:aedifex-error',
            project_id: config.projectId,
            message: caught?.message || String(caught),
            details: caught?.details,
          })
          throw caught
        }
      }
      const queued = saveChainRef.current.then(operation, operation)
      saveChainRef.current = queued.catch(() => undefined)
      return queued
    },
    [client, config, setRevisionSafe],
  )

  const save = useCallback(
    (scene: SceneSnapshot) => persistScene(scene, 'editor'),
    [persistScene],
  )

  const commitImportedScene = useCallback(
    (scene: SceneSnapshot, metadata: Record<string, unknown>) =>
      persistScene(scene, 'import', { import: metadata }),
    [persistScene],
  )

  const onContextLayerError = useCallback((layer: { id: string }, layerError: Error) => {
    setContextLayerErrors((current) => ({ ...current, [layer.id]: layerError.message }))
  }, [])

  const registerExportHandler = useCallback((handler: ArczSceneExportHandler | null) => {
    exportHandlerRef.current = handler
  }, [])

  const requestSceneExport = useCallback(async (reason: string, requestedId?: string) => {
    if (!config) throw new Error('Configuração ARCZ ausente')
    const handler = exportHandlerRef.current
    if (!handler) throw new Error('Canvas Aedifex ainda não está pronto para exportação')
    const requestId = requestedId || globalThis.crypto?.randomUUID?.() || `export-${Date.now()}`
    try {
      const result = await handler({ requestId, reason })
      reportToHost(config, {
        type: 'arcz:aedifex-exported',
        project_id: config.projectId,
        revision,
        scene_hash: sceneHash,
        request_id: requestId,
        result,
      })
      return result
    } catch (caught: any) {
      reportToHost(config, {
        type: 'arcz:aedifex-export-error',
        project_id: config.projectId,
        revision,
        request_id: requestId,
        message: caught?.message || String(caught),
        details: caught?.details,
      })
      throw caught
    }
  }, [config, revision, sceneHash])

  useEffect(() => {
    if (!config) return
    const onHostMessage = (event: MessageEvent) => {
      if (event.origin !== config.apiBaseUrl || event.source !== window.parent) return
      const data = event.data || {}
      if (data.channel !== config.channel || data.project_id !== config.projectId) return
      if (data.type !== 'arcz:request-scene-export') return
      void requestSceneExport(String(data.reason || 'host_request'), String(data.request_id || ''))
    }
    window.addEventListener('message', onHostMessage)
    return () => window.removeEventListener('message', onHostMessage)
  }, [config, requestSceneExport])

  useEffect(() => {
    if (!client || !config) return
    const source = client.events(0)
    source.onmessage = (event) => {
      try {
        const update = JSON.parse(event.data)
        if (update.event_type !== 'scene.committed') return
        const incoming = Number(update.revision || update.payload?.revision || 0)
        if (!Number.isInteger(incoming) || incoming <= revisionRef.current) return
        // Never overwrite unsaved editor state. An explicit reload preserves
        // the versioned/transactional contract and avoids silent last-write-wins.
        setRemoteRevision(incoming)
        reportToHost(config, {
          type: 'arcz:aedifex-remote-revision',
          project_id: config.projectId,
          revision: incoming,
          origin: update.payload?.origin,
        })
      } catch (caught) {
        console.error('[ARCZ/Aedifex] evento inválido', caught)
      }
    }
    source.onerror = () => {
      // Native EventSource reconnects. A dropped stream is not treated as a
      // successful sync and does not reset the editor.
    }
    return () => source.close()
  }, [client, config])


  const tabs = useMemo(
    () => [
      {
        id: 'site',
        label: 'Região',
        component: () => <ArczRegionPanel context={context} />,
        mobileDefaultSnap: 0.5,
        mobileIcon: <Layers />,
        icon: <Layers />,
      },
      {
        id: 'build',
        label: 'Construir',
        component: ArczBuildPanel,
        mobileDefaultSnap: 0.65,
        mobileIcon: <Hammer />,
        icon: <Hammer />,
      },
      {
        id: 'exchange',
        label: 'Importar',
        component: () => (
          <ArczImportExportPanel
            currentRevision={revision}
            onCommitImportedScene={commitImportedScene}
          />
        ),
        mobileDefaultSnap: 0.62,
        mobileIcon: <FileUp />,
        icon: <FileUp />,
      },
      {
        id: 'ai',
        label: 'ARCZ AI',
        component: () => (client ? (
          <CombinedAiPanel
            client={client}
            context={context}
            onCommittedRevision={(next) => setRemoteRevision(next)}
            revision={revision}
          />
        ) : null),
        mobileDefaultSnap: 0.7,
        mobileIcon: <Bot />,
        icon: <Bot />,
      },
      {
        id: 'items',
        label: 'Biblioteca',
        component: Items,
        mobileDefaultSnap: 0.55,
        mobileIcon: <Package />,
        icon: <Package />,
      },
      {
        id: 'settings',
        label: 'Configurações',
        component: SettingsPanel,
        mobileDefaultSnap: 0.55,
        mobileIcon: <Settings />,
        icon: <Settings />,
      },
    ],
    [client, commitImportedScene, context, revision],
  )

  if (bootState === 'error') return <main className="arcz-fatal"><div><h1>Aedifex incompatível</h1><pre>{bootError}</pre></div></main>
  if (bootState !== 'ready') return <main className="arcz-fatal">Validando plugins e schemas Aedifex…</main>

  if (error) {
    return (
      <main className="arcz-fatal">
        <div><h1>Floorplanner indisponível</h1><pre>{error}</pre></div>
      </main>
    )
  }
  if (!client || !config) return <main className="arcz-fatal">Abrindo projeto…</main>

  return (
    <div className="relative h-screen w-screen">
      {Object.keys(contextLayerErrors).length > 0 && (
        <div className="arcz-revision-banner is-error" role="alert">
          <span>{Object.keys(contextLayerErrors).length} camada(s) de contexto não puderam ser verificadas.</span>
          <button onClick={() => setContextLayerErrors({})} type="button">Dispensar aviso</button>
        </div>
      )}
      {remoteRevision !== null && remoteRevision > revision && (
        <div className="arcz-revision-banner" role="alert">
          <span>Existe uma revisão externa mais nova ({remoteRevision}).</span>
          <button onClick={() => window.location.reload()} type="button">Recarregar com segurança</button>
        </div>
      )}
      <Editor
        floorplanSceneSlot={context ? <RegionContextFloorplan client={client} context={context} onLayerError={onContextLayerError} /> : null}
        layoutVersion="v2"
        navbarSlot={(
          <div className="arcz-aedifex-navbar">
            <div>
              <strong>ARCZ Floorplanner</strong>
              <span>revisão {revision} · {context?.scale || 'carregando'} · norte −Z</span>
            </div>
            <div className="arcz-navbar-actions">
              <button
                className="arcz-publish-button"
                disabled={revision <= 0 || exportState.status === 'exporting'}
                onClick={() => void requestSceneExport('manual')}
                title={exportState.message || 'Publicar derivado validado no globo'}
                type="button"
              >
                <UploadCloud size={15} />
                {exportState.status === 'exporting' ? 'Publicando…' : 'Atualizar no globo'}
              </button>
              <ArczPanelBehavior />
            </div>
          </div>
        )}
        onLoad={load}
        onSave={save}
        projectId={config.projectId}
        sidebarTabs={tabs}
        viewerSceneSlot={context ? (
          <>
            <RegionContextWorld client={client} context={context} onLayerError={onContextLayerError} />
            <ArczSceneExportBridge
              client={client}
              context={context}
              onStatus={(status, message) => setExportState({ status, message })}
              register={registerExportHandler}
              revision={revision}
              sceneHash={sceneHash}
            />
          </>
        ) : null}
      />
    </div>
  )
}
