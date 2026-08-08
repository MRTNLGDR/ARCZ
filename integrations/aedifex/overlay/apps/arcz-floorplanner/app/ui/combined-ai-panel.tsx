'use client'

import type { ArczFloorplannerClient, ModelingContext } from '@arcz/aedifex-bridge'
import { ArczChatPanel } from './arcz-chat-panel'

/**
 * Superfície única do agente. As ferramentas do antigo painel Aedifex são
 * descobertas do MCP real e aparecem no catálogo global, evitando dois chats
 * com históricos, políticas e autoridades diferentes sobre a mesma cena.
 */
export function CombinedAiPanel({
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
  return (
    <div className="arcz-combined-ai">
      <ArczChatPanel
        client={client}
        context={context}
        onCommittedRevision={onCommittedRevision}
        revision={revision}
      />
    </div>
  )
}
