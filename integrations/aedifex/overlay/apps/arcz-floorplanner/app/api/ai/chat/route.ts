import { type NextRequest, NextResponse } from 'next/server'
import { buildSystemPrompt, OPENAI_TOOLS } from '@aedifex/editor/ai/prompt'
import {
  describeChatRequestError,
  validateChatRequest,
} from '@aedifex/editor/ai/contracts'

const DEFAULT_ARCZ_API = 'http://127.0.0.1:8123'
const MAX_ARCZ_RESPONSE_BYTES = 8 * 1024 * 1024

type ToolCall = {
  id: string
  type: 'function'
  function: { name: string; arguments: string }
}

function normalizeLoopbackApi(raw: string | undefined): string {
  let parsed: URL
  try {
    parsed = new URL(raw || DEFAULT_ARCZ_API)
  } catch {
    throw new Error('ARCZ_API_URL inválida')
  }
  if (parsed.protocol !== 'http:') throw new Error('ARCZ_API_URL precisa usar HTTP local')
  const host = parsed.hostname.toLowerCase()
  if (!['127.0.0.1', 'localhost', '::1', '[::1]'].includes(host)) {
    throw new Error('ARCZ_API_URL precisa apontar para loopback')
  }
  if (parsed.username || parsed.password) throw new Error('Credenciais na ARCZ_API_URL são proibidas')
  parsed.pathname = ''
  parsed.search = ''
  parsed.hash = ''
  return parsed.origin
}

function apiBaseFor(request: NextRequest): string {
  // O runtime injeta o endereço real do servidor ARCZ. O cookie é apenas uma
  // recuperação para um sidecar já iniciado manualmente em outra porta local.
  const cookieValue = request.cookies.get('arcz_api_base')?.value
  const fromCookie = cookieValue ? decodeURIComponent(cookieValue) : undefined
  return normalizeLoopbackApi(fromCookie || process.env.ARCZ_API_URL)
}

function normalizeToolCalls(value: unknown): ToolCall[] {
  if (!Array.isArray(value)) return []
  return value.flatMap((item, index) => {
    if (!item || typeof item !== 'object') return []
    const record = item as Record<string, unknown>
    const nested = record.function && typeof record.function === 'object'
      ? record.function as Record<string, unknown>
      : null
    const name = String(nested?.name ?? record.name ?? '').trim()
    if (!name) return []
    const rawArguments = nested?.arguments ?? record.arguments ?? record.input ?? {}
    let args: string
    if (typeof rawArguments === 'string') {
      // Do not silently forward malformed JSON. The Aedifex client validates
      // each tool payload and needs deterministic parseable arguments.
      try {
        JSON.parse(rawArguments)
        args = rawArguments
      } catch {
        return []
      }
    } else {
      args = JSON.stringify(rawArguments ?? {})
    }
    return [{
      id: String(record.id || `call_arcz_${index}_${crypto.randomUUID()}`),
      type: 'function' as const,
      function: { name, arguments: args },
    }]
  })
}

function sseChunk(payload: unknown): Uint8Array {
  return new TextEncoder().encode(`data: ${JSON.stringify(payload)}\n\n`)
}

function openAiCompatibleStream(content: string, toolCalls: ToolCall[]): Response {
  const id = `chatcmpl_arcz_${crypto.randomUUID()}`
  const created = Math.floor(Date.now() / 1000)
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(sseChunk({
        id,
        object: 'chat.completion.chunk',
        created,
        model: 'arcz-local-broker',
        choices: [{ index: 0, delta: { role: 'assistant' }, finish_reason: null }],
      }))
      if (content) {
        controller.enqueue(sseChunk({
          id,
          object: 'chat.completion.chunk',
          created,
          model: 'arcz-local-broker',
          choices: [{ index: 0, delta: { content }, finish_reason: null }],
        }))
      }
      toolCalls.forEach((toolCall, index) => {
        controller.enqueue(sseChunk({
          id,
          object: 'chat.completion.chunk',
          created,
          model: 'arcz-local-broker',
          choices: [{
            index: 0,
            delta: {
              tool_calls: [{
                index,
                id: toolCall.id,
                type: 'function',
                function: toolCall.function,
              }],
            },
            finish_reason: null,
          }],
        }))
      })
      controller.enqueue(sseChunk({
        id,
        object: 'chat.completion.chunk',
        created,
        model: 'arcz-local-broker',
        choices: [{
          index: 0,
          delta: {},
          finish_reason: toolCalls.length ? 'tool_calls' : 'stop',
        }],
      }))
      controller.enqueue(new TextEncoder().encode('data: [DONE]\n\n'))
      controller.close()
    },
  })
  return new Response(stream, {
    headers: {
      'Content-Type': 'text/event-stream; charset=utf-8',
      'Cache-Control': 'no-cache, no-store, must-revalidate',
      Connection: 'keep-alive',
      'X-Content-Type-Options': 'nosniff',
    },
  })
}

async function readBoundedJson(response: Response): Promise<unknown> {
  const contentLength = Number(response.headers.get('content-length') || 0)
  if (contentLength > MAX_ARCZ_RESPONSE_BYTES) throw new Error('Resposta do broker excede o limite')
  const text = await response.text()
  if (new TextEncoder().encode(text).byteLength > MAX_ARCZ_RESPONSE_BYTES) {
    throw new Error('Resposta do broker excede o limite')
  }
  return text ? JSON.parse(text) : {}
}

export async function POST(request: NextRequest) {
  let body: unknown
  try {
    body = await request.json()
  } catch {
    return NextResponse.json({ error: 'Corpo JSON inválido.', code: 'JSON_INVALID' }, { status: 400 })
  }

  const validation = validateChatRequest(body)
  if (!validation.ok) {
    return NextResponse.json({
      error: describeChatRequestError(validation.error),
      code: validation.error,
    }, { status: 400 })
  }

  const {
    messages,
    catalogSummary,
    sceneContext,
    roomPresetSummary,
  } = validation.value
  const systemPrompt = buildSystemPrompt(catalogSummary, sceneContext, roomPresetSummary)

  let apiBase: string
  try {
    apiBase = apiBaseFor(request)
  } catch (error) {
    return NextResponse.json({
      error: error instanceof Error ? error.message : String(error),
      code: 'ARCZ_API_URL_INVALID',
    }, { status: 503 })
  }

  try {
    const upstream = await fetch(`${apiBase}/api/v2/ai/tools`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json',
        'Cache-Control': 'no-cache',
      },
      body: JSON.stringify({
        task: 'chat.global',
        input: {
          mode: 'aedifex-native-agent',
          system_prompt: systemPrompt,
          messages,
          tools: OPENAI_TOOLS,
          catalog_summary: catalogSummary,
          scene_context: sceneContext,
          room_preset_summary: roomPresetSummary,
          response_contract: {
            content: 'string',
            tool_calls: 'OpenAI-compatible function calls only',
          },
        },
      }),
      signal: request.signal,
      cache: 'no-store',
      redirect: 'error',
    })
    const payload = await readBoundedJson(upstream) as Record<string, unknown>
    if (!upstream.ok) {
      const error = payload.error as Record<string, unknown> | undefined
      return NextResponse.json({
        error: String(error?.message || `Broker local retornou HTTP ${upstream.status}`),
        code: String(error?.code || 'ARCZ_LOCAL_AI_FAILED'),
        details: error?.details || null,
      }, { status: upstream.status })
    }
    const result = payload.result
    if (!result || typeof result !== 'object') {
      return NextResponse.json({
        error: 'Broker local não retornou um objeto result.',
        code: 'ARCZ_LOCAL_AI_OUTPUT_INVALID',
      }, { status: 502 })
    }
    const record = result as Record<string, unknown>
    const content = String(record.content ?? record.text ?? '')
    const toolCalls = normalizeToolCalls(record.tool_calls)
    if (!content.trim() && toolCalls.length === 0) {
      return NextResponse.json({
        error: 'Modelo local não retornou texto nem ferramentas.',
        code: 'ARCZ_LOCAL_AI_OUTPUT_EMPTY',
      }, { status: 502 })
    }
    return openAiCompatibleStream(content, toolCalls)
  } catch (error) {
    if (request.signal.aborted || (error as { name?: string })?.name === 'AbortError') {
      return new Response(null, { status: 499 })
    }
    return NextResponse.json({
      error: `Falha ao acessar o broker ARCZ local: ${error instanceof Error ? error.message : String(error)}`,
      code: 'ARCZ_LOCAL_AI_UNREACHABLE',
    }, { status: 503 })
  }
}
