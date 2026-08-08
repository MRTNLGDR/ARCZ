'use client'

import {
  ArczFloorplannerClient,
  ContextLayerSchema,
  enuToAedifex,
  type ContextLayer,
  type ModelingContext,
} from '@arcz/aedifex-bridge'
import { useEffect, useMemo, useState } from 'react'
import {
  Euler,
  Group,
  Material,
  Matrix4,
  Mesh,
  Object3D,
  Quaternion,
  Texture,
  Vector3,
} from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'

/**
 * IMPORTANT FOR IMPLEMENTERS
 * --------------------------
 * Context layers are immutable evidence from the ARCZ world domain. They are
 * never copied into the editable Aedifex SceneSnapshot and never exported back
 * to the globe. This prevents recursive duplication in globe -> floorplanner ->
 * globe round-trips.
 */

function disposeObject(root: Object3D) {
  const disposedTextures = new Set<Texture>()
  const disposedMaterials = new Set<Material>()
  root.traverse((object: any) => {
    object.geometry?.dispose?.()
    const materials: Material[] = Array.isArray(object.material)
      ? object.material
      : object.material
        ? [object.material]
        : []
    for (const material of materials) {
      if (disposedMaterials.has(material)) continue
      disposedMaterials.add(material)
      for (const value of Object.values(material as unknown as Record<string, unknown>)) {
        if (value instanceof Texture && !disposedTextures.has(value)) {
          disposedTextures.add(value)
          value.dispose()
        }
      }
      material.dispose()
    }
  })
}

/** Matrix mapping the layer's native coordinates to Aedifex local metres. */
function contextLayerMatrix(layer: ContextLayer, context: ModelingContext): Matrix4 {
  const [px, py, pz] = layer.transform.position_m
  const [rx, ry, rz] = layer.transform.rotation_euler_rad
  const [sx, sy, sz] = layer.transform.scale
  const position = new Vector3(px, py, pz)
  const rotation = new Quaternion().setFromEuler(new Euler(rx, ry, rz, 'XYZ'))
  const scale = new Vector3(sx, sy, sz)
  const local = new Matrix4().compose(new Vector3(), rotation, scale)

  if (layer.coordinate_space === 'AEDIFEX_LOCAL') {
    return new Matrix4().compose(position, rotation, scale)
  }

  // Source ENU basis: X=east, Y=north, Z=up.
  // Aedifex basis: X=east, Y=up, Z=south, plus the Region's declared north
  // rotation. This exact matrix mirrors GeoModelTransform.enu_to_aedifex().
  const angle = (context.geo_anchor.north_rotation_deg * Math.PI) / 180
  const c = Math.cos(angle)
  const s = Math.sin(angle)
  const basis = new Matrix4().set(
    c, s, 0, 0,
    0, 0, 1, 0,
    s, -c, 0, 0,
    0, 0, 0, 1,
  )
  const translated = new Matrix4().makeTranslation(
    ...enuToAedifex(context.geo_anchor, [position.x, position.y, position.z]),
  )
  return translated.multiply(basis).multiply(local)
}

function configureReadonly(root: Object3D, layer: ContextLayer, context: ModelingContext) {
  root.name = `ARCZ context · ${layer.role} · ${layer.id}`
  root.userData = {
    ...root.userData,
    arczContext: true,
    arczContextLayerId: layer.id,
    arczExportExclude: true,
    nonEditable: true,
    readonly: true,
    provenance: layer.provenance,
  }
  root.traverse((object: any) => {
    object.userData = { ...object.userData, ...root.userData }
    // Context objects must never win a selection raycast in Aedifex.
    object.raycast = () => null
    if (object instanceof Mesh && object.material) {
      const clone = (material: Material) => {
        const next: any = material.clone()
        if (typeof next.opacity === 'number') next.opacity *= layer.opacity
        if (layer.opacity < 1) {
          next.transparent = true
          next.depthWrite = false
        }
        return next
      }
      object.material = Array.isArray(object.material)
        ? object.material.map(clone)
        : clone(object.material)
    }
  })
  root.matrixAutoUpdate = false
  root.matrix.copy(contextLayerMatrix(layer, context))
  root.matrixWorldNeedsUpdate = true
  root.visible = layer.visible
}

function GlbContextLayer({
  client,
  context,
  layer,
  onError,
}: {
  client: ArczFloorplannerClient
  context: ModelingContext
  layer: ContextLayer
  onError?: (layer: ContextLayer, error: Error) => void
}) {
  const [object, setObject] = useState<Group | null>(null)

  useEffect(() => {
    const controller = new AbortController()
    let owned: Group | null = null
    setObject(null)
    void (async () => {
      try {
        const bytes = await client.fetchVerifiedAsset(layer, { signal: controller.signal })
        if (controller.signal.aborted) return
        const loader = new GLTFLoader()
        const gltf = await new Promise<any>((resolve, reject) => {
          loader.parse(bytes, '', resolve, reject)
        })
        if (controller.signal.aborted) {
          disposeObject(gltf.scene)
          return
        }
        // Keep the GLTF's own root transform intact. ARCZ's coordinate transform
        // lives on a wrapper group and can therefore be removed atomically.
        owned = new Group()
        owned.add(gltf.scene)
        configureReadonly(owned, layer, context)
        setObject(owned)
      } catch (caught) {
        if (controller.signal.aborted) return
        const error = caught instanceof Error ? caught : new Error(String(caught))
        onError?.(layer, error)
      }
    })()
    return () => {
      controller.abort()
      if (owned) disposeObject(owned)
    }
  }, [client, context, layer, onError])

  return object ? <primitive object={object} /> : null
}

export function ArczContextWorldLayers({
  client,
  context,
  onError,
}: {
  client: ArczFloorplannerClient
  context: ModelingContext
  onError?: (layer: ContextLayer, error: Error) => void
}) {
  const layers = useMemo(
    () => context.context_layers
      .map((value) => ContextLayerSchema.parse(value))
      .filter((layer) => layer.format === 'glb'),
    [context],
  )
  return (
    <group name="ARCZ immutable regional context" userData={{ arczContext: true, arczExportExclude: true }}>
      {layers.map((layer) => (
        <GlbContextLayer client={client} context={context} key={layer.id} layer={layer} onError={onError} />
      ))}
    </group>
  )
}

type PlanPoint = [number, number]

type PlanShape = {
  id: string
  kind: 'line' | 'polygon'
  points: PlanPoint[]
}

function sourcePoint(layer: ContextLayer, point: number[]): Vector3 {
  if (layer.coordinate_space === 'ENU_LOCAL') {
    // GeoJSON/ENU convention: [east, north, up].
    return new Vector3(Number(point[0] || 0), Number(point[1] || 0), Number(point[2] || 0))
  }
  if (point.length >= 3) {
    // Aedifex convention: [x, y, z].
    return new Vector3(Number(point[0] || 0), Number(point[1] || 0), Number(point[2] || 0))
  }
  // Two-dimensional floorplan convention: [x, z].
  return new Vector3(Number(point[0] || 0), 0, Number(point[1] || 0))
}

function applyPlanTransform(layer: ContextLayer, context: ModelingContext, point: number[]): PlanPoint {
  const transformed = sourcePoint(layer, point).applyMatrix4(contextLayerMatrix(layer, context))
  return [transformed.x, transformed.z]
}

function collectGeometry(
  geometry: any,
  layer: ContextLayer,
  context: ModelingContext,
  id: string,
  result: PlanShape[],
) {
  if (!geometry || typeof geometry.type !== 'string') return
  const map = (points: number[][]) => points.map((point) => applyPlanTransform(layer, context, point))
  if (geometry.type === 'LineString') result.push({ id, kind: 'line', points: map(geometry.coordinates || []) })
  else if (geometry.type === 'MultiLineString') {
    for (const [index, line] of (geometry.coordinates || []).entries()) {
      result.push({ id: `${id}:${index}`, kind: 'line', points: map(line) })
    }
  } else if (geometry.type === 'Polygon') {
    for (const [index, ring] of (geometry.coordinates || []).entries()) {
      result.push({ id: `${id}:${index}`, kind: 'polygon', points: map(ring) })
    }
  } else if (geometry.type === 'MultiPolygon') {
    for (const [polygonIndex, polygon] of (geometry.coordinates || []).entries()) {
      for (const [ringIndex, ring] of polygon.entries()) {
        result.push({ id: `${id}:${polygonIndex}:${ringIndex}`, kind: 'polygon', points: map(ring) })
      }
    }
  } else if (geometry.type === 'GeometryCollection') {
    for (const [index, child] of (geometry.geometries || []).entries()) {
      collectGeometry(child, layer, context, `${id}:${index}`, result)
    }
  }
}

function GeoJsonFloorplanLayer({
  client,
  context,
  layer,
  onError,
}: {
  client: ArczFloorplannerClient
  context: ModelingContext
  layer: ContextLayer
  onError?: (layer: ContextLayer, error: Error) => void
}) {
  const [shapes, setShapes] = useState<PlanShape[]>([])
  useEffect(() => {
    const controller = new AbortController()
    setShapes([])
    void (async () => {
      try {
        const bytes = await client.fetchVerifiedAsset(layer, { signal: controller.signal })
        if (controller.signal.aborted) return
        const value = JSON.parse(new TextDecoder().decode(bytes))
        const result: PlanShape[] = []
        if (value?.type === 'FeatureCollection') {
          for (const [index, feature] of (value.features || []).entries()) {
            collectGeometry(feature?.geometry, layer, context, String(feature?.id || `${layer.id}:${index}`), result)
          }
        } else if (value?.type === 'Feature') {
          collectGeometry(value.geometry, layer, context, String(value.id || layer.id), result)
        } else collectGeometry(value, layer, context, layer.id, result)
        if (!controller.signal.aborted) {
          setShapes(result.filter((shape) => shape.points.length >= (shape.kind === 'polygon' ? 3 : 2)))
        }
      } catch (caught) {
        if (controller.signal.aborted) return
        const error = caught instanceof Error ? caught : new Error(String(caught))
        onError?.(layer, error)
      }
    })()
    return () => controller.abort()
  }, [client, context, layer, onError])
  if (!layer.visible) return null
  return (
    <g data-arcz-context-layer={layer.id} opacity={layer.opacity} pointerEvents="none">
      {shapes.map((shape) => {
        const points = shape.points.map((point) => `${point[0]},${point[1]}`).join(' ')
        return shape.kind === 'polygon'
          ? <polygon fill="rgba(99,115,129,.035)" key={shape.id} points={points} stroke="rgba(99,115,129,.42)" strokeWidth={0.06} vectorEffect="non-scaling-stroke" />
          : <polyline fill="none" key={shape.id} points={points} stroke="rgba(99,115,129,.5)" strokeWidth={0.08} vectorEffect="non-scaling-stroke" />
      })}
    </g>
  )
}

export function ArczContextFloorplanLayers({
  client,
  context,
  onError,
}: {
  client: ArczFloorplannerClient
  context: ModelingContext
  onError?: (layer: ContextLayer, error: Error) => void
}) {
  return (
    <g aria-label="Contexto regional ARCZ somente leitura" pointerEvents="none">
      {context.context_layers.filter((layer) => layer.format === 'geojson').map((layer) => (
        <GeoJsonFloorplanLayer client={client} context={context} key={layer.id} layer={layer} onError={onError} />
      ))}
    </g>
  )
}
