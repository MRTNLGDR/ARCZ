import { ChatClient } from "./chat-client.js";
import { normalizeChatToolCall, toolStatusLabel } from "./chat-tool-policy.js";

function n(tag, cls = "", text = "") {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text) node.textContent = text;
  return node;
}

export function safeChatStorageKey(context) {
  const parts = [context?.language || "pt-BR", context?.region_id || "none", context?.floorplanner_project_id || "none"];
  return `arcz:global-chat:v3:${parts.map(value => encodeURIComponent(String(value))).join(":")}`;
}

function roleLabel(role) {
  return role === "user" ? "Você" : role === "assistant" ? "ARCZ" : role === "tool" ? "Ferramenta" : role;
}

function latestRevision(context, call) {
  const previewRevision = call?.preview?.expected_revision;
  if (Number.isInteger(previewRevision) && previewRevision >= 0) return previewRevision;
  const current = context?.expected_revision ?? context?.current_revision ?? context?.floorplanner_revision;
  return Number.isInteger(current) && current >= 0 ? current : undefined;
}

function toolPreviewSummary(call) {
  const preview = call.preview;
  if (!preview || typeof preview !== "object") return null;
  if (preview.diff) return { title: "Diferenças previstas", value: preview.diff };
  if (preview.preflight) return { title: "Preflight do render", value: preview.preflight };
  return { title: "Preview auditável", value: preview };
}

export class GlobalChatPanel {
  constructor({ client = new ChatClient(), contextProvider = () => ({}), attachmentsProvider = () => [] } = {}) {
    this.client = client;
    this.contextProvider = contextProvider;
    this.attachmentsProvider = attachmentsProvider;
    this.messages = [];
    this.disposed = false;
    this.resolvedCalls = new Set();
    this.toolCards = new Map();
  }

  async mount(host) {
    this.disposed = false;
    this.host = host;
    this.root = n("section", "arcz-global-chat");
    const policy = n(
      "div",
      "arcz-chat-policy",
      "Local e auditável · leituras automáticas · alterações somente após preview e aprovação",
    );
    this.messagesHost = n("div", "arcz-chat-messages");
    const form = n("form", "arcz-chat-form");
    this.input = n("textarea", "arcz-textarea");
    this.input.placeholder = "Peça alterações no globo, região, Floorplanner, render, materiais ou cinema…";
    this.send = n("button", "arcz-button arcz-button--primary", "Enviar");
    this.send.type = "submit";
    this.status = n("div", "arcz-panel-state");
    form.append(this.input, this.send);
    this.root.append(policy, this.messagesHost, form, this.status);
    host.append(this.root);

    const ctx = this.contextProvider();
    this.storageKey = safeChatStorageKey(ctx);
    this.status.textContent = "Abrindo histórico SQLite local…";
    try {
      const saved = localStorage.getItem(this.storageKey);
      let session = null;
      if (saved) {
        try { session = await this.client.getSession(saved); }
        catch { localStorage.removeItem(this.storageKey); }
      }
      if (!session) {
        session = await this.client.createSession({
          title: "ARCZ Global",
          scope: ctx.floorplanner_project_id ? "floorplanner" : "global",
          language: ctx.language || "pt-BR",
          context: { ...ctx, tool_policy: "preview_then_explicit_approval" },
        });
        localStorage.setItem(this.storageKey, session.id);
        session.messages = [];
      }
      if (this.disposed) return;
      this.sessionId = session.id;
      for (const message of session.messages || []) {
        if (message.role === "tool" && message.metadata?.tool_call_id) {
          this.resolvedCalls.add(String(message.metadata.tool_call_id));
        }
      }
      for (const message of session.messages || []) this.add(message);
      this.status.textContent = "";
    } catch (error) {
      this.status.textContent = `Chat indisponível: ${error.message}`;
      this.send.disabled = true;
    }

    form.addEventListener("submit", async event => {
      event.preventDefault();
      const content = this.input.value.trim();
      if (!content || this.send.disabled || !this.sessionId) return;
      this.input.value = "";
      this.add({ role: "user", content, attachments: this.attachmentsProvider() });
      this.send.disabled = true;
      this.status.textContent = "Raciocinando no modelo local e consultando ferramentas permitidas…";
      try {
        const ctxNow = this.contextProvider();
        const result = await this.client.respond(this.sessionId, {
          content,
          attachments: this.attachmentsProvider(),
          metadata: { ...ctxNow, submitted_at: new Date().toISOString() },
        });
        const assistants = result.assistant_messages?.length ? result.assistant_messages : [result.assistant];
        assistants.filter(Boolean).forEach(message => this.add(message));
        this.status.textContent = result.pending_approval
          ? "Revise o preview. Nenhuma alteração foi aplicada."
          : "";
      } catch (error) {
        this.status.textContent = `Falha: ${error.message}`;
      } finally {
        this.send.disabled = false;
      }
    });
  }

  add(message) {
    this.messages.push(message);
    const row = n("article", `arcz-chat-message arcz-chat-message--${message.role}`);
    row.append(n("strong", "", roleLabel(message.role)), n("p", "", message.content));
    if (message.attachments?.length) {
      row.append(n("span", "arcz-chat-attachments", `${message.attachments.length} mídia(s) por hash verificado`));
    }
    if (message.tool_calls?.length) {
      const tools = n("div", "arcz-chat-tool-list");
      message.tool_calls.forEach((raw, index) => {
        try { tools.append(this.renderToolCall(normalizeChatToolCall(raw, index))); }
        catch (error) { tools.append(n("div", "arcz-panel-error", error.message)); }
      });
      row.append(tools);
    }
    this.messagesHost.append(row);
    this.messagesHost.scrollTop = this.messagesHost.scrollHeight;
    if (message.role === "tool" && message.metadata?.tool_call_id) {
      this.markCallResolved(String(message.metadata.tool_call_id), message.metadata.tool_status || "SUCCEEDED");
    }
    return row;
  }

  renderToolCall(call) {
    const card = n("section", `arcz-chat-tool-call is-${call.side_effect}`);
    card.dataset.toolCallId = call.id;
    const heading = n("div", "arcz-chat-tool-call__heading");
    const name = n("strong", "", call.name);
    const badge = n("span", "arcz-chat-tool-call__status", toolStatusLabel(call.status));
    heading.append(name, badge);
    const details = n("details", "arcz-chat-tool-details");
    const summary = n("summary", "", "Argumentos e auditoria");
    const argumentsView = n("pre", "arcz-mode-details", JSON.stringify(call.arguments, null, 2));
    details.append(summary, argumentsView);
    const preview = toolPreviewSummary(call);
    if (preview) {
      details.append(n("strong", "", preview.title));
      details.append(n("pre", "arcz-mode-details", JSON.stringify(preview.value, null, 2)));
    }
    if (call.error) details.append(n("pre", "arcz-panel-error", JSON.stringify(call.error, null, 2)));
    card.append(heading, details);

    const actions = n("div", "arcz-actions");
    const approve = n("button", "arcz-button arcz-button--primary", "Aplicar esta alteração");
    const reject = n("button", "arcz-button", "Rejeitar");
    approve.type = reject.type = "button";
    const actionable = call.status === "AWAITING_APPROVAL" && Boolean(call.run_id);
    approve.disabled = reject.disabled = !actionable;
    if (actionable) {
      approve.addEventListener("click", () => { void this.resolveToolCall(call, "APPROVE", { approve, reject, badge }); });
      reject.addEventListener("click", () => { void this.resolveToolCall(call, "REJECT", { approve, reject, badge }); });
      actions.append(approve, reject);
      card.append(actions);
    }
    this.toolCards.set(call.id, { card, approve, reject, badge });
    if (this.resolvedCalls.has(call.id) || ["SUCCEEDED", "FAILED", "REJECTED", "CANCELLED"].includes(call.status)) {
      this.markCallResolved(call.id, call.status);
    }
    return card;
  }

  markCallResolved(id, status) {
    this.resolvedCalls.add(id);
    const view = this.toolCards.get(id);
    if (!view) return;
    view.approve.disabled = true;
    view.reject.disabled = true;
    view.card.classList.add("is-resolved");
    view.badge.textContent = toolStatusLabel(status);
  }

  async resolveToolCall(call, decision, controls) {
    if (this.resolvedCalls.has(call.id) || !this.sessionId || !call.run_id) return;
    controls.approve.disabled = controls.reject.disabled = true;
    controls.badge.textContent = decision === "REJECT" ? "Registrando rejeição…" : "Validando revisão e aplicando…";
    this.status.textContent = decision === "REJECT"
      ? `Rejeitando ${call.name}…`
      : `Aplicando ${call.name} sobre a revisão aprovada…`;
    try {
      let resolution;
      if (decision === "REJECT") {
        resolution = await this.client.rejectToolRun(call.run_id);
      } else {
        resolution = await this.client.approveToolRun(
          call.run_id,
          latestRevision(this.contextProvider(), call),
        );
      }
      if (resolution.tool_message) this.add(resolution.tool_message);
      const finalStatus = resolution.tool_run?.status || (decision === "REJECT" ? "REJECTED" : "SUCCEEDED");
      this.markCallResolved(call.id, finalStatus);
      this.status.textContent = "Atualizando a resposta do modelo local com o resultado auditado…";
      const currentRevision = resolution.result?.current_revision ?? latestRevision(this.contextProvider(), call);
      const continuation = await this.client.continue(this.sessionId, {
        metadata: {
          after_tool_run_id: call.run_id,
          tool_status: finalStatus,
          floorplanner_project_id: this.contextProvider()?.floorplanner_project_id,
          current_revision: Number.isInteger(currentRevision) ? currentRevision : undefined,
        },
      });
      const assistants = continuation.assistant_messages?.length
        ? continuation.assistant_messages
        : [continuation.assistant];
      assistants.filter(Boolean).forEach(message => this.add(message));
      this.status.textContent = continuation.pending_approval
        ? "Uma nova alteração possui preview e aguarda aprovação."
        : "";
    } catch (error) {
      controls.badge.textContent = "Falhou";
      controls.approve.disabled = controls.reject.disabled = false;
      this.status.textContent = `Ferramenta não aplicada: ${error.message}`;
    }
  }

  dispose() {
    this.disposed = true;
    this.root?.remove();
  }
}
