'use client'

import type { ModelingContext, ArczFloorplannerClient } from '@arcz/aedifex-bridge'
import { useThree } from '@react-three/fiber'
import { Group, Object3D } from 'three'
import { GLTFExporter } from 'three/examples/jsm/exporters/GLTFExporter.js'
import { useCallback, useEffect, useRef } from 'react'

export type ArczSceneExportRequest = {
  requestId: string
  reason: 'manual' | 'mode_exit' | 'host_request' | string
}

export type ArczSceneExportResult = {
  requestId: string
  export: Record<string, unknown>
  meshCount: number
  objectCount: number
}

export type ArczSceneExportHandler = (
  request: ArczSceneExportRequest,
) => Promise<ArczSceneExportResult>

type Props = {
  client: ArczFloorplannerClient
  context: ModelingContext
  revision: number
  sceneHash: string | null
  register: (handler: ArczSceneExportHandler | null) => void
  onStatus?: (status: 'idle' | 'exporting' | 'success' | 'error', message?: string) => void
}

function isEditorOnlyObject(object: Object3D): boolean {
  const type = object.type.toLowerCase()
  const name = object.name.toLowerCase()
  const data = object.userData || {}
  return Boolean(
    data.arczExportExclude ||
      data.arczContext ||
      object.visible === false ||
      (object as any).isCamera ||
      (object as any).isLight ||
      (object as any).isHelper ||
      type.includes('helper') ||
      name.includes('transformcontrols') ||
      name.includes('orbitcontrols') ||
      name.includes('selection-outline') ||
      name.includes('editor-grid') ||
      name === 'grid' ||
      name.startsWith('gizmo'),
  )
}

function cloneAuthoringGeometry(scene: Object3D): {
  root: Group
  meshCount: number
  objectCount: number
} {
  const cloned = scene.clone(true)
  const toRemove: Object3D[] = []
  cloned.traverse((object) => {
    if (object !== cloned && isEditorOnlyObject(object)) toRemove.push(object)
  })
  // Remove deepest objects first so parent/child removal stays deterministic.
  toRemove.sort((a, b) => {
    const depth = (object: Object3D) => {
      let value = 0
      for (let current = object.parent; current; current = current.parent) value++
      return value
    }
    return depth(b) - depth(a)
  })
  for (const object of toRemove) object.parent?.remove(object)

  const root = new Group()
  root.name = 'ARCZ_Aedifex_DerivedScene'
  for (const child of [...cloned.children]) root.add(child)
  let meshCount = 0
  let objectCount = 0
  root.traverse((object) => {
    objectCount++
    if ((object as any).isMesh) meshCount++
  })
  if (meshCount === 0) {
    throw new Error('A cena Aedifex não possui malha 3D exportável nesta revisão')
  }
  return { root, meshCount, objectCount }
}

async function exportBinary(root: Object3D): Promise<ArrayBuffer> {
  const exporter = new GLTFExporter()
  const result = await exporter.parseAsync(root, {
    binary: true,
    onlyVisible: true,
    trs: false,
    maxTextureSize: 8192,
  })
  if (!(result instanceof ArrayBuffer)) {
    throw new Error('GLTFExporter não devolveu GLB binário')
  }
  return result
}

/**
 * Vive dentro do Canvas real do Aedifex e, portanto, enxerga a cena Three.js
 * efetivamente renderizada. Não cria geometria simulada e não salva o GLB no
 * browser: o binário é validado/materializado pelo gateway local do ARCZ.
 */
export function ArczSceneExportBridge({
  client,
  context,
  revision,
  sceneHash,
  register,
  onStatus,
}: Props) {
  const scene = useThree((state) => state.scene)
  const inFlight = useRef<Promise<ArczSceneExportResult> | null>(null)

  const perform = useCallback<ArczSceneExportHandler>(
    async (request) => {
      if (inFlight.current) return inFlight.current
      const operation = (async () => {
        if (!Number.isInteger(revision) || revision <= 0) {
          throw new Error('Salve ao menos uma revisão antes de publicar no globo')
        }
        if (!sceneHash || !/^[a-f0-9]{64}$/.test(sceneHash)) {
          throw new Error('Hash da revisão ausente; recarregue o Floorplanner antes de exportar')
        }
        onStatus?.('exporting', 'Convertendo a cena real para GLB…')
        // Aguarda o frame atual terminar para evitar capturar transformações
        // intermediárias de ferramentas de arrasto.
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
        scene.updateMatrixWorld(true)
        const { root, meshCount, objectCount } = cloneAuthoringGeometry(scene)
        const bytes = await exportBinary(root)
        const semanticManifest = {
          schema_version: 1,
          source: 'aedifex_rendered_scene',
          project_id: client.projectId,
          revision,
          scene_hash: sceneHash,
          context_hash: context.context_hash,
          region_id: context.region_id,
          axis_policy: context.geo_anchor.axis_policy,
          geo_anchor: context.geo_anchor,
          mesh_count: meshCount,
          object_count: objectCount,
          reason: request.reason,
          generated_at: new Date().toISOString(),
          authority: {
            editable_scene: 'floorplanner_revision',
            glb_role: 'readonly_globe_derivative',
          },
        }
        const stored = await client.uploadGlbExport({
          revision,
          sceneHash,
          bytes,
          semanticManifest,
        })
        const result = { requestId: request.requestId, export: stored, meshCount, objectCount }
        onStatus?.('success', `Derivado publicado: ${meshCount} malhas`)
        return result
      })()
      inFlight.current = operation
      try {
        return await operation
      } catch (error) {
        onStatus?.('error', error instanceof Error ? error.message : String(error))
        throw error
      } finally {
        inFlight.current = null
      }
    },
    [client, context, onStatus, revision, scene, sceneHash],
  )

  useEffect(() => {
    register(perform)
    return () => register(null)
  }, [perform, register])

  return null
}
