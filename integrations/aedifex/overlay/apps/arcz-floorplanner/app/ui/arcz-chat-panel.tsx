'use client'

import type { ArczFloorplannerClient, ModelingContext } from '@arcz/aedifex-bridge'
import { type FormEvent, useEffect, useMemo, useState } from 'react'

type ToolCall = {
  id: string
  run_id?: string | null
  name: string
  arguments: Record<string, unknown>
  status: string
  side_effect?: string
  requires_approval?: boolean
  preview?: any
  error?: any
}

type ChatMessage = {
  id?: string
  role: string
  content: string
  attachments?: string[]
  tool_calls?: ToolCall[]
  metadata?: Record<string, any>
}

const STATUS: Record<string, string> = {
  PROPOSED: 'Proposta', PREVIEWING: 'Gerando preview', AWAITING_APPROVAL: 'Aguardando aprovação',
  APPROVED: 'Aprovada', RUNNING: 'Executando', SUCCEEDED: 'Concluída', FAILED: 'Falhou',
  REJECTED: 'Rejeitada', CANCELLED: 'Cancelada',
}

function statusLabel(value: string) {
  return STATUS[value] || value
}

function previewValue(call: ToolCall) {
  if (call.preview?.diff) return { label: 'Diferenças previstas', value: call.preview.diff }
  if (call.preview?.preflight) return { label: 'Preflight fotorreal', value: call.preview.preflight }
  if (call.preview) return { label: 'Preview auditável', value: call.preview }
  return null
}

export function ArczChatPanel({
  client,
  context,
  revision,
  onCommittedRevision,
}: {
  client: ArczFloorplannerClient
  context: ModelingContext | null
  revision: number
  onCommittedRevision?: (revision: number) => void
}) {
  const [session, setSession] = useState<string | null>(null)
  const [text, setText] = useState('')
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [prompts, setPrompts] = useState<any[]>([])
  const [toolSummary, setToolSummary] = useState({ available: 0, unavailable: 0 })
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [busyRun, setBusyRun] = useState<string | null>(null)
  const storageKey = useMemo(
    () => `arcz:floorplanner-chat:v3:${client.projectId}:${context?.context_hash || 'loading'}`,
    [client.projectId, context?.context_hash],
  )

  useEffect(() => {
    let alive = true
    async function initialize() {
      setError('')
      try {
        const saved = localStorage.getItem(storageKey)
        let current: any = null
        if (saved) {
          try { current = await client.getChat(saved) } catch { localStorage.removeItem(storageKey) }
        }
        if (!current) {
          current = await client.createChat({
            title: 'ARCZ Floorplanner Global',
            scope: 'floorplanner',
            language: 'pt-BR',
            context: {
              floorplanner_project_id: client.projectId,
              region_id: context?.region_id,
              context_hash: context?.context_hash,
              tool_policy: 'preview_then_explicit_approval',
            },
          })
          localStorage.setItem(storageKey, current.id)
        }
        const [full, library, catalog] = await Promise.all([
          current.messages ? current : client.getChat(current.id),
          client.prompts('architecture'),
          client.chatTools(),
        ])
        if (!alive) return
        setSession(full.id)
        setMessages(full.messages || [])
        setPrompts(library || [])
        const tools = Array.isArray(catalog?.tools) ? catalog.tools : []
        setToolSummary({
          available: tools.filter((item: any) => item.available !== false).length,
          unavailable: tools.filter((item: any) => item.available === false).length,
        })
      } catch (caught) {
        if (alive) setError(caught instanceof Error ? caught.message : String(caught))
      }
    }
    void initialize()
    return () => { alive = false }
  }, [client, context?.context_hash, context?.region_id, storageKey])

  function addAssistantResponse(response: any) {
    const next = Array.isArray(response?.assistant_messages) && response.assistant_messages.length
      ? response.assistant_messages
      : response?.assistant ? [response.assistant] : []
    if (next.length) setMessages((current) => [...current, ...next])
  }

  async function submit(event: FormEvent) {
    event.preventDefault()
    if (!session || !text.trim() || busy) return
    const input = text.trim()
    setText('')
    setMessages((current) => [...current, { role: 'user', content: input }])
    setBusy(true)
    setError('')
    try {
      const response = await client.respond(session, {
        content: input,
        attachments: context?.reference_media || [],
        metadata: {
          floorplanner_project_id: client.projectId,
          context_hash: context?.context_hash,
          expected_revision: revision,
          require_preview_before_mutation: true,
        },
      })
      addAssistantResponse(response)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      setBusy(false)
    }
  }

  async function resolve(call: ToolCall, decision: 'approve' | 'reject') {
    if (!session || !call.run_id || busyRun) return
    setBusyRun(call.run_id)
    setError('')
    try {
      const expected = Number.isInteger(call.preview?.expected_revision)
        ? Number(call.preview.expected_revision)
        : revision
      const response = decision === 'approve'
        ? await client.approveToolRun(call.run_id, expected)
        : await client.rejectToolRun(call.run_id)
      if (response.tool_message) setMessages((current) => [...current, response.tool_message])
      const committed = Number(response?.result?.current_revision)
      if (Number.isInteger(committed) && committed > revision) onCommittedRevision?.(committed)
      const continuation = await client.continueChat(session, {
        metadata: {
          floorplanner_project_id: client.projectId,
          expected_revision: Number.isInteger(committed) ? committed : revision,
          after_tool_run_id: call.run_id,
          tool_status: response?.tool_run?.status,
        },
      })
      addAssistantResponse(continuation)
      setMessages((current) => current.map((message) => ({
        ...message,
        tool_calls: message.tool_calls?.map((item) => item.run_id === call.run_id
          ? { ...item, status: response?.tool_run?.status || (decision === 'approve' ? 'SUCCEEDED' : 'REJECTED') }
          : item),
      })))
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      setBusyRun(null)
    }
  }

  return (
    <div className="arcz-chat-panel">
      <header className="arcz-panel-copy">
        <strong>Agente global ARCZ + Aedifex</strong>
        <span>
          {toolSummary.available} ferramenta(s) local(is) ativa(s)
          {toolSummary.unavailable ? ` · ${toolSummary.unavailable} indisponível(is)` : ''}
          {' · '}leituras automáticas; geometria e render exigem preview.
        </span>
      </header>
      {prompts.length > 0 && (
        <div className="arcz-chat-suggestions">
          {prompts.slice(0, 8).map((prompt) => (
            <button key={prompt.id} onClick={() => setText(prompt.template || prompt.title || '')} type="button">
              {prompt.title || prompt.id}
            </button>
          ))}
        </div>
      )}
      <div className="arcz-chat-messages">
        {messages.length === 0 && (
          <div className="arcz-empty-state">
            Descreva a construção, revisão, medição, importação IFC, material ou imagem fotorreal desejada.
          </div>
        )}
        {messages.map((message, index) => (
          <article className={`arcz-chat-message is-${message.role}`} key={message.id || index}>
            <strong>{message.role === 'assistant' ? 'ARCZ' : message.role === 'user' ? 'Você' : 'Ferramenta'}</strong>
            <p>{message.content}</p>
            {!!message.attachments?.length && <small>{message.attachments.length} referência(s) verificada(s)</small>}
            {!!message.tool_calls?.length && (
              <div className="arcz-chat-tools">
                {message.tool_calls.map((call, callIndex) => {
                  const preview = previewValue(call)
                  const actionable = call.status === 'AWAITING_APPROVAL' && Boolean(call.run_id)
                  return (
                    <section className={`arcz-chat-tool is-${call.side_effect || 'mutate'}`} key={call.id || callIndex}>
                      <header>
                        <strong>{call.name}</strong>
                        <span>{statusLabel(call.status)}</span>
                      </header>
                      <details>
                        <summary>Argumentos e auditoria</summary>
                        <pre>{JSON.stringify(call.arguments, null, 2)}</pre>
                        {preview && <><b>{preview.label}</b><pre>{JSON.stringify(preview.value, null, 2)}</pre></>}
                        {call.error && <pre className="is-error">{JSON.stringify(call.error, null, 2)}</pre>}
                      </details>
                      {actionable && (
                        <div className="arcz-chat-tool-actions">
                          <button
                            disabled={busyRun === call.run_id}
                            onClick={() => void resolve(call, 'approve')}
                            type="button"
                          >
                            {busyRun === call.run_id ? 'Aplicando…' : 'Aplicar alteração'}
                          </button>
                          <button
                            disabled={busyRun === call.run_id}
                            onClick={() => void resolve(call, 'reject')}
                            type="button"
                          >
                            Rejeitar
                          </button>
                        </div>
                      )}
                    </section>
                  )
                })}
              </div>
            )}
          </article>
        ))}
      </div>
      {error && <div className="arcz-inline-error" role="alert">{error}</div>}
      <form className="arcz-chat-form" onSubmit={submit}>
        <textarea
          onChange={(event) => setText(event.target.value)}
          placeholder="Ex.: modele dois pavimentos respeitando lote, relevo e norte; depois prepare render 8K com esta referência…"
          value={text}
        />
        <button disabled={busy || !session || !text.trim()} type="submit">
          {busy ? 'Processando localmente…' : 'Enviar'}
        </button>
      </form>
    </div>
  )
}
