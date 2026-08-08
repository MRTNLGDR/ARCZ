import { z } from 'zod'

/**
 * Public bridge between the ARCZ world core and the Aedifex authoring kernel.
 *
 * SECURITY INVARIANT:
 * - only a loopback HTTP ARCZ API is accepted;
 * - credentials embedded in the URL are rejected;
 * - callers never receive an unrestricted fetch helper.
 */

export const GeoAnchorSchema = z.object({
  origin_wgs84: z.tuple([z.number(), z.number(), z.number()]),
  north_rotation_deg: z.number(),
  vertical_offset_m: z.number(),
  axis_policy: z.literal('AEDIFEX_X_EAST_Y_UP_Z_SOUTH'),
})
export type GeoAnchor = z.infer<typeof GeoAnchorSchema>

export const ContextLayerSchema = z.object({
  id: z.string().min(1).max(256),
  role: z.enum(['terrain', 'surroundings', 'roads', 'buildings', 'vegetation', 'imagery', 'survey', 'reference']),
  format: z.enum(['glb', 'geojson']),
  asset_path: z.string().min(2).max(2048).refine(
    (value) => value.startsWith('/') && !value.includes('..') && !/^[a-z][a-z0-9+.-]*:/i.test(value),
    'asset_path deve ser local ao ARCZ',
  ),
  sha256: z.string().regex(/^[a-f0-9]{64}$/),
  readonly: z.literal(true),
  visible: z.boolean(),
  opacity: z.number().min(0).max(1),
  coordinate_space: z.enum(['AEDIFEX_LOCAL', 'ENU_LOCAL']),
  lod: z.enum(['hero', 'near', 'medium', 'distant', 'reference']).optional(),
  transform: z.object({
    position_m: z.tuple([z.number(), z.number(), z.number()]),
    rotation_euler_rad: z.tuple([z.number(), z.number(), z.number()]),
    scale: z.tuple([z.number().positive(), z.number().positive(), z.number().positive()]),
  }),
  provenance: z.record(z.string(), z.unknown()),
  metadata: z.record(z.string(), z.unknown()).optional(),
})
export type ContextLayer = z.infer<typeof ContextLayerSchema>

export const ModelingContextSchema = z.object({
  schema_version: z.literal(1),
  region_id: z.string().min(1),
  generation_epoch: z.number().int().nonnegative(),
  scale: z.string().min(1),
  geo_anchor: GeoAnchorSchema,
  selection: z.object({
    selection_id: z.string(),
    kind: z.string(),
    bbox_wgs84: z.array(z.number()).length(4),
    parcel_polygon_wgs84: z.array(z.array(z.number()).min(2).max(3)),
    parcel_polygon_enu_m: z.array(z.tuple([z.number(), z.number()])),
    parcel_polygon_aedifex_xyz_m: z.array(z.tuple([z.number(), z.number(), z.number()])),
    source: z.record(z.string(), z.unknown()),
  }),
  terrain: z.record(z.string(), z.unknown()),
  urban: z.record(z.string(), z.unknown()),
  environment: z.record(z.string(), z.unknown()),
  regional_profiles: z.array(z.union([z.string(), z.record(z.string(), z.unknown())])),
  constraints: z.record(z.string(), z.unknown()),
  source_packages: z.array(z.union([z.string(), z.record(z.string(), z.unknown())])),
  reference_media: z.array(z.string().regex(/^[a-f0-9]{64}$/)),
  context_layers: z.array(ContextLayerSchema).default([]),
  warnings: z.array(z.union([z.string(), z.record(z.string(), z.unknown())])),
  context_hash: z.string().regex(/^[a-f0-9]{64}$/),
})
export type ModelingContext = z.infer<typeof ModelingContextSchema>

export type ArczBridgeError = Error & {
  code?: string
  details?: unknown
  retryable?: boolean
  traceId?: string
  httpStatus?: number
}

const LOOPBACK_HOSTS = new Set(['127.0.0.1', 'localhost', '::1', '[::1]'])

export function normalizeLoopbackApiBase(value?: string | null): string {
  const candidate = value || process.env.NEXT_PUBLIC_ARCZ_BASE_URL || 'http://127.0.0.1:8123'
  let url: URL
  try {
    url = new URL(candidate)
  } catch {
    throw new Error('ARCZ API URL inválida')
  }
  if (url.protocol !== 'http:' || !LOOPBACK_HOSTS.has(url.hostname) || url.username || url.password) {
    throw new Error('ARCZ API deve usar HTTP loopback sem credenciais')
  }
  if (url.pathname !== '/' && url.pathname !== '') {
    throw new Error('ARCZ API deve apontar para a origem, sem caminho')
  }
  return url.origin
}

async function readResponsePayload(response: Response): Promise<unknown> {
  const type = response.headers.get('content-type') || ''
  if (type.includes('application/json')) return response.json()
  const text = await response.text()
  return text ? { error: { message: text } } : {}
}

export class ArczFloorplannerClient {
  readonly apiBaseUrl: string

  constructor(readonly projectId: string, options: { apiBaseUrl?: string | null } = {}) {
    if (!projectId.trim()) throw new Error('projectId obrigatório')
    this.apiBaseUrl = normalizeLoopbackApiBase(options.apiBaseUrl)
  }

  private async jsonRequest<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await fetch(`${this.apiBaseUrl}${path}`, {
      ...init,
      headers: {
        Accept: 'application/json',
        ...(init?.body ? { 'Content-Type': 'application/json' } : {}),
        ...(init?.headers || {}),
      },
      cache: 'no-store',
      credentials: 'omit',
      redirect: 'error',
    })
    const payload = (await readResponsePayload(response)) as any
    if (!response.ok) {
      const error = new Error(payload?.error?.message || `ARCZ HTTP ${response.status}`) as ArczBridgeError
      error.code = payload?.error?.code
      error.details = payload?.error?.details
      error.retryable = payload?.error?.retryable
      error.traceId = payload?.error?.trace_id
      error.httpStatus = response.status
      throw error
    }
    return payload as T
  }

  getProject(includeScene = true) {
    return this.jsonRequest<any>(
      `/api/v2/floorplanner/projects/${encodeURIComponent(this.projectId)}?include_scene=${includeScene ? 1 : 0}`,
    )
  }

  saveScene(
    scene: unknown,
    expectedRevision: number,
    origin = 'editor',
    metadata: Record<string, unknown> = {},
  ) {
    if (!Number.isInteger(expectedRevision) || expectedRevision < 0) {
      throw new Error('expectedRevision inválida')
    }
    return this.jsonRequest<any>(
      `/api/v2/floorplanner/projects/${encodeURIComponent(this.projectId)}/revisions`,
      {
        method: 'POST',
        body: JSON.stringify({
          scene,
          expected_revision: expectedRevision,
          origin,
          metadata: {
            bridge: '@arcz/aedifex-bridge@2',
            coordinate_policy: 'AEDIFEX_X_EAST_Y_UP_Z_SOUTH',
            ...metadata,
          },
        }),
      },
    )
  }

  async uploadGlbExport(input: {
    revision: number
    sceneHash: string
    bytes: ArrayBuffer
    semanticManifest: Record<string, unknown>
  }) {
    if (!Number.isInteger(input.revision) || input.revision <= 0) {
      throw new Error('revision inválida para export')
    }
    if (!/^[a-f0-9]{64}$/.test(input.sceneHash)) {
      throw new Error('sceneHash inválido para export')
    }
    const manifest = encodeURIComponent(JSON.stringify(input.semanticManifest || {}))
    // A rota é binária: não usa jsonRequest para não serializar/corromper o GLB.
    const response = await fetch(
      `${this.apiBaseUrl}/api/v2/floorplanner/projects/${encodeURIComponent(this.projectId)}/exports/upload`,
      {
        method: 'POST',
        headers: {
          Accept: 'application/json',
          'Content-Type': 'model/gltf-binary',
          'X-ARCZ-Revision': String(input.revision),
          'X-ARCZ-Format': 'glb',
          'X-ARCZ-Scene-Hash': input.sceneHash,
          'X-ARCZ-Semantic-Manifest': manifest,
        },
        body: input.bytes,
        cache: 'no-store',
        credentials: 'omit',
        redirect: 'error',
      },
    )
    const payload = (await readResponsePayload(response)) as any
    if (!response.ok) {
      const error = new Error(payload?.error?.message || `ARCZ HTTP ${response.status}`) as ArczBridgeError
      error.code = payload?.error?.code
      error.details = payload?.error?.details
      error.retryable = payload?.error?.retryable
      error.traceId = payload?.error?.trace_id
      error.httpStatus = response.status
      throw error
    }
    return payload
  }

  assetUrl(path: string): string {
    if (!path.startsWith('/') || path.includes('..') || /^[a-z][a-z0-9+.-]*:/i.test(path)) {
      throw new Error('Caminho de asset ARCZ inválido')
    }
    return `${this.apiBaseUrl}${path}`
  }

  async fetchVerifiedAsset(layer: ContextLayer, options: { signal?: AbortSignal } = {}): Promise<ArrayBuffer> {
    const parsed = ContextLayerSchema.parse(layer)
    const response = await fetch(this.assetUrl(parsed.asset_path), {
      cache: 'no-store', credentials: 'omit', redirect: 'error', headers: { Accept: '*/*' },
      signal: options.signal,
    })
    if (!response.ok) throw new Error(`Asset de contexto indisponível (${response.status})`)
    const bytes = await response.arrayBuffer()
    const digest = await crypto.subtle.digest('SHA-256', bytes)
    const actual = Array.from(new Uint8Array(digest), (value) => value.toString(16).padStart(2, '0')).join('')
    if (actual !== parsed.sha256) {
      const error = new Error(`Hash divergente no asset ${parsed.id}`) as ArczBridgeError
      error.code = 'CONTEXT_LAYER_HASH_MISMATCH'
      error.details = { expected: parsed.sha256, actual, path: parsed.asset_path }
      throw error
    }
    return bytes
  }

  events(after = 0) {
    const value = Number.isInteger(after) && after >= 0 ? after : 0
    return new EventSource(
      `${this.apiBaseUrl}/api/v2/floorplanner/projects/${encodeURIComponent(this.projectId)}/events?stream=1&after=${value}`,
      { withCredentials: false },
    )
  }

  prompts(query = '') {
    return this.jsonRequest<any[]>(`/api/v2/prompts?q=${encodeURIComponent(query)}`)
  }

  createChat(input: unknown) {
    return this.jsonRequest<any>('/api/v2/chat/sessions', {
      method: 'POST',
      body: JSON.stringify(input),
    })
  }

  getChat(sessionId: string) {
    return this.jsonRequest<any>(`/api/v2/chat/sessions/${encodeURIComponent(sessionId)}`)
  }

  respond(sessionId: string, input: unknown) {
    return this.jsonRequest<any>(`/api/v2/chat/sessions/${encodeURIComponent(sessionId)}/respond`, {
      method: 'POST',
      body: JSON.stringify(input),
    })
  }

  continueChat(sessionId: string, input: unknown = {}) {
    return this.jsonRequest<any>(`/api/v2/chat/sessions/${encodeURIComponent(sessionId)}/continue`, {
      method: 'POST',
      body: JSON.stringify(input),
    })
  }

  chatTools() {
    return this.jsonRequest<any>('/api/v2/chat/tools')
  }

  toolRuns(sessionId?: string) {
    const query = sessionId ? `?session_id=${encodeURIComponent(sessionId)}` : ''
    return this.jsonRequest<any[]>(`/api/v2/chat/tool-runs${query}`)
  }

  toolRun(runId: string) {
    return this.jsonRequest<any>(`/api/v2/chat/tool-runs/${encodeURIComponent(runId)}`)
  }

  approveToolRun(runId: string, expectedRevision?: number) {
    const body: Record<string, unknown> = {}
    if (Number.isInteger(expectedRevision) && Number(expectedRevision) >= 0) {
      body.expected_revision = expectedRevision
    }
    return this.jsonRequest<any>(`/api/v2/chat/tool-runs/${encodeURIComponent(runId)}/approve`, {
      method: 'POST',
      body: JSON.stringify(body),
    })
  }

  rejectToolRun(runId: string, reason = 'explicit_user_rejection') {
    return this.jsonRequest<any>(`/api/v2/chat/tool-runs/${encodeURIComponent(runId)}/reject`, {
      method: 'POST',
      body: JSON.stringify({ reason }),
    })
  }
}

export function enuToAedifex(anchorValue: GeoAnchor, enu: [number, number, number]): [number, number, number] {
  const anchor = GeoAnchorSchema.parse(anchorValue)
  const angle = (anchor.north_rotation_deg * Math.PI) / 180
  const c = Math.cos(angle)
  const s = Math.sin(angle)
  const [east, north, up] = enu
  return [c * east + s * north, up + anchor.vertical_offset_m, s * east - c * north]
}

export function aedifexToEnu(anchorValue: GeoAnchor, xyz: [number, number, number]): [number, number, number] {
  const anchor = GeoAnchorSchema.parse(anchorValue)
  const angle = (anchor.north_rotation_deg * Math.PI) / 180
  const c = Math.cos(angle)
  const s = Math.sin(angle)
  const [x, y, z] = xyz
  return [c * x + s * z, s * x - c * z, y - anchor.vertical_offset_m]
}
