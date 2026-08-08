'use client'

import type {
  ArczFloorplannerClient,
  ContextLayer,
  ModelingContext,
} from '@arcz/aedifex-bridge'
import {
  BufferGeometry,
  Float32BufferAttribute,
  LineBasicMaterial,
  LineLoop,
} from 'three'
import { useEffect, useMemo } from 'react'
import {
  ArczContextFloorplanLayers,
  ArczContextWorldLayers,
} from './arcz-context-layers'

type ContextProps = {
  client: ArczFloorplannerClient
  context: ModelingContext
  onLayerError?: (layer: ContextLayer, error: Error) => void
}

export function RegionContextWorld({ client, context, onLayerError }: ContextProps) {
  const boundary = useMemo(() => {
    const points = context.selection.parcel_polygon_aedifex_xyz_m
    if (points.length < 3) return null
    const geometry = new BufferGeometry()
    geometry.setAttribute(
      'position',
      new Float32BufferAttribute(points.flatMap((point) => [point[0], point[1] + 0.02, point[2]]), 3),
    )
    const line = new LineLoop(
      geometry,
      new LineBasicMaterial({ color: 0x2f6fed, depthTest: false, transparent: true, opacity: 0.9 }),
    )
    line.renderOrder = 999
    line.userData = { arczContext: true, arczExportExclude: true, nonEditable: true, readonly: true }
    line.raycast = () => null as never
    return line
  }, [context])

  useEffect(
    () => () => {
      boundary?.geometry.dispose()
      ;(boundary?.material as LineBasicMaterial | undefined)?.dispose()
    },
    [boundary],
  )

  return (
    <group name="ARCZ regional modeling context" userData={{ arczContext: true, arczExportExclude: true }}>
      {boundary ? <primitive object={boundary} /> : null}
      <ArczContextWorldLayers client={client} context={context} onError={onLayerError} />
    </group>
  )
}

export function RegionContextFloorplan({ client, context, onLayerError }: ContextProps) {
  const points = context.selection.parcel_polygon_aedifex_xyz_m
    .map((point) => `${point[0]},${point[2]}`)
    .join(' ')
  return (
    <g aria-label="Região Ativa ARCZ" pointerEvents="none">
      <ArczContextFloorplanLayers client={client} context={context} onError={onLayerError} />
      {points ? (
        <polygon
          fill="rgba(47,111,237,.06)"
          points={points}
          stroke="#2f6fed"
          strokeWidth={0.08}
          vectorEffect="non-scaling-stroke"
        />
      ) : null}
    </g>
  )
}
