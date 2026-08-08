import { loadPlugin } from '@aedifex/core'
import { createAedifexMcpServer, SceneBridge } from '@aedifex/mcp'
import { builtinPlugin } from '@aedifex/nodes'
import { treesPlugin } from '@aedifex/plugin-trees'
import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js'
import { createHash, timingSafeEqual } from 'node:crypto'

/**
 * Server-only execution bridge. It instantiates the real Aedifex MCP server
 * against an isolated SceneBridge loaded from an ARCZ revision. A mutation is
 * never written unless `dryRun === false`, an approval id exists and the
 * expected revision still matches.
 */

export type ToolSideEffect = 'read' | 'export' | 'mutate' | 'destructive'
export type ToolCatalogEntry = {
  name: string
  description?: string
  inputSchema: Record<string, unknown>
  sideEffect: ToolSideEffect
  requiresApproval: boolean
  namespace: 'aedifex'
}

const READ_ONLY = new Set([
  'get_scene','get_node','describe_node','find_nodes','query_scene','measure','validate_scene','check_collisions',
  'list_scenes','get_scene_metadata','list_templates','get_template','list_variants','get_variant',
])
const EXPORT = new Set(['export_json','export_glb'])
const DESTRUCTIVE = new Set(['delete_node','delete_scene'])
let bootstrapPromise: Promise<void> | null = null

export function classifyTool(name: string): ToolSideEffect {
  if (READ_ONLY.has(name) || name.startsWith('get_') || name.startsWith('find_') || name.startsWith('list_')) return 'read'
  if (EXPORT.has(name) || name.startsWith('export_')) return 'export'
  if (DESTRUCTIVE.has(name) || name.startsWith('delete_')) return 'destructive'
  return 'mutate'
}

async function bootstrap(): Promise<void> {
  bootstrapPromise ??= (async () => {
    // `loadPlugin` is mandatory: it enforces apiVersion=2 and preserves the
    // plugin ownership map. Direct registerNode calls are intentionally banned.
    for (const plugin of [builtinPlugin, treesPlugin]) {
      try { await loadPlugin(plugin) }
      catch (error) {
        const message = error instanceof Error ? error.message : String(error)
        // A Next dev reload can re-evaluate this module after the editor already
        // loaded the same plugin. Only the explicit duplicate error is tolerated.
        if (!message.includes('duplicate node kind')) throw error
      }
    }
  })()
  await bootstrapPromise
}

function stable(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stable).join(',')}]`
  if (value && typeof value === 'object') {
    return `{${Object.keys(value as Record<string, unknown>).sort().map((key) => `${JSON.stringify(key)}:${stable((value as any)[key])}`).join(',')}}`
  }
  return JSON.stringify(value)
}

function sha256(value: unknown): string {
  return createHash('sha256').update(stable(value)).digest('hex')
}

export function verifyBridgeToken(header: string | null): void {
  const expected = process.env.ARCZ_AEDIFEX_BRIDGE_TOKEN || ''
  const received = header?.startsWith('Bearer ') ? header.slice(7) : ''
  const a = Buffer.from(expected); const b = Buffer.from(received)
  if (!expected || a.length !== b.length || !timingSafeEqual(a, b)) {
    const error = new Error('ARCZ Aedifex bridge token inválido') as Error & { status?: number; code?: string }
    error.status = 401; error.code = 'AEDIFEX_BRIDGE_UNAUTHORIZED'; throw error
  }
}

function apiBase(): string {
  const value = process.env.ARCZ_API_URL || 'http://127.0.0.1:8123'
  const url = new URL(value)
  if (url.protocol !== 'http:' || !new Set(['127.0.0.1','localhost','::1','[::1]']).has(url.hostname)) {
    throw new Error('ARCZ_API_URL precisa ser loopback HTTP')
  }
  return url.origin
}

async function arczJson(path: string, init?: RequestInit): Promise<any> {
  const response = await fetch(`${apiBase()}${path}`, {
    ...init,
    headers: { Accept:'application/json', ...(init?.body ? {'Content-Type':'application/json'} : {}), ...(init?.headers || {}) },
    cache:'no-store', credentials:'omit', redirect:'error',
  })
  const payload = await response.json().catch(() => ({}))
  if (!response.ok) {
    const error = new Error(payload?.error?.message || `ARCZ HTTP ${response.status}`) as Error & { code?: string; details?: unknown; status?: number }
    error.code = payload?.error?.code; error.details = payload?.error?.details; error.status = response.status; throw error
  }
  return payload
}

async function withMcpScene<T>(scene: any, run: (client: Client, bridge: SceneBridge) => Promise<T>): Promise<T> {
  await bootstrap()
  const bridge = new SceneBridge()
  if (scene?.nodes && Array.isArray(scene?.rootNodeIds)) bridge.loadJSON(scene)
  else bridge.loadDefault()
  const server = createAedifexMcpServer({ bridge, name:'arcz-aedifex-mcp', version:'1.0.0' })
  const [serverTransport, clientTransport] = InMemoryTransport.createLinkedPair()
  const client = new Client({ name:'arcz-global-chat', version:'1.0.0' })
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)])
  try { return await run(client, bridge) }
  finally { await Promise.allSettled([client.close(), server.close()]) }
}

export async function listAedifexTools(): Promise<ToolCatalogEntry[]> {
  return withMcpScene(null, async (client) => {
    const response = await client.listTools()
    return response.tools.map((tool) => {
      const effect = classifyTool(tool.name)
      return {
        name:`aedifex.${tool.name}`,
        description:tool.description,
        inputSchema:(tool.inputSchema || {type:'object'}) as Record<string, unknown>,
        sideEffect:effect,
        requiresApproval:effect === 'mutate' || effect === 'destructive',
        namespace:'aedifex' as const,
      }
    }).sort((a,b) => a.name.localeCompare(b.name))
  })
}

type InvokeInput = {
  name: string
  arguments: Record<string, unknown>
  projectId: string
  expectedRevision: number
  dryRun: boolean
  approvalId?: string | null
}

function sceneDiff(before: any, after: any) {
  const a = before?.nodes || {}; const b = after?.nodes || {}
  const created = Object.keys(b).filter((id) => !(id in a))
  const deleted = Object.keys(a).filter((id) => !(id in b))
  const updated = Object.keys(b).filter((id) => id in a && stable(a[id]) !== stable(b[id]))
  const byType = (ids: string[], source: any) => ids.reduce<Record<string,number>>((out,id) => {
    const type = String(source[id]?.type || 'unknown'); out[type]=(out[type]||0)+1; return out
  }, {})
  return { created, updated, deleted, counts:{before:Object.keys(a).length,after:Object.keys(b).length},
    by_type:{created:byType(created,b),updated:byType(updated,b),deleted:byType(deleted,a)} }
}

export async function invokeAedifexTool(input: InvokeInput): Promise<Record<string, unknown>> {
  if (!input.projectId?.trim()) throw new Error('projectId obrigatório')
  if (!Number.isInteger(input.expectedRevision) || input.expectedRevision < 0) throw new Error('expectedRevision inválida')
  const nativeName = input.name.startsWith('aedifex.') ? input.name.slice(9) : input.name
  const effect = classifyTool(nativeName)
  if (!input.dryRun && (effect === 'mutate' || effect === 'destructive') && !input.approvalId?.trim()) {
    const error = new Error('Mutações exigem approvalId') as Error & { code?: string; status?: number }
    error.code='AEDIFEX_TOOL_APPROVAL_REQUIRED'; error.status=409; throw error
  }
  const project = await arczJson(`/api/v2/floorplanner/projects/${encodeURIComponent(input.projectId)}?include_scene=1`)
  const currentRevision = Number(project.current_revision || 0)
  if (currentRevision !== input.expectedRevision) {
    const error = new Error(`Revisão mudou: esperado ${input.expectedRevision}, atual ${currentRevision}`) as Error & {code?:string;status?:number;details?:unknown}
    error.code='FLOORPLANNER_VERSION_CONFLICT'; error.status=409; error.details={expected_revision:input.expectedRevision,current_revision:currentRevision}; throw error
  }
  const before = project.scene_revision?.scene || null
  return withMcpScene(before, async (client, bridge) => {
    const tool = await client.callTool({ name:nativeName, arguments:input.arguments || {} })
    const after = bridge.exportJSON(); const beforeHash=sha256(before || {}); const afterHash=sha256(after)
    const changed = beforeHash !== afterHash; const diff=sceneDiff(before || {nodes:{}},after)
    if (input.dryRun || !changed || effect === 'read' || effect === 'export') {
      return { schema_version:1, name:`aedifex.${nativeName}`, side_effect:effect, dry_run:true, changed,
        expected_revision:currentRevision, before_hash:beforeHash, after_hash:afterHash, diff,
        tool_result:{ content:tool.content, structuredContent:tool.structuredContent, isError:tool.isError === true } }
    }
    const committed = await arczJson(`/api/v2/floorplanner/projects/${encodeURIComponent(input.projectId)}/revisions`, {
      method:'POST', body:JSON.stringify({ scene:after, expected_revision:currentRevision, origin:'chat.mcp',
        metadata:{tool:`aedifex.${nativeName}`,approval_id:input.approvalId,before_hash:beforeHash,after_hash:afterHash,diff} }),
    })
    return { schema_version:1, name:`aedifex.${nativeName}`, side_effect:effect, dry_run:false, changed:true,
      expected_revision:currentRevision, current_revision:Number(committed.current_revision), before_hash:beforeHash,
      after_hash:afterHash, diff, tool_result:{content:tool.content,structuredContent:tool.structuredContent,isError:tool.isError===true} }
  })
}
