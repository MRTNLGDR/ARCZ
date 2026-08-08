export function normalizeChatToolCall(value, index = 0) {
  if (!value || typeof value !== "object") throw new Error(`Tool call ${index} inválida`);
  const nested = value.function && typeof value.function === "object" ? value.function : null;
  const name = String(nested?.name ?? value.name ?? "").trim();
  if (!name) throw new Error(`Tool call ${index} sem nome`);
  const raw = nested?.arguments ?? value.arguments ?? value.input ?? {};
  let argumentsValue = raw;
  if (typeof raw === "string") {
    try { argumentsValue = JSON.parse(raw); }
    catch { throw new Error(`Argumentos JSON inválidos em ${name}`); }
  }
  if (!argumentsValue || typeof argumentsValue !== "object" || Array.isArray(argumentsValue)) {
    throw new Error(`Argumentos de ${name} precisam ser objeto`);
  }
  return {
    id: String(value.id || `call-${index}`),
    run_id: value.run_id ? String(value.run_id) : null,
    name,
    arguments: argumentsValue,
    status: String(value.status || "PROPOSED"),
    side_effect: String(value.side_effect || "mutate"),
    requires_approval: Boolean(value.requires_approval),
    preview: value.preview ?? null,
    error: value.error ?? null,
  };
}

export function toolResultText(name, result, status = "SUCCEEDED") {
  return JSON.stringify({ tool: name, status, result }, null, 2);
}

export function toolStatusLabel(status) {
  const labels = {
    PROPOSED: "Proposta",
    PREVIEWING: "Gerando preview",
    AWAITING_APPROVAL: "Aguardando aprovação",
    APPROVED: "Aprovada",
    RUNNING: "Executando",
    SUCCEEDED: "Concluída",
    FAILED: "Falhou",
    REJECTED: "Rejeitada",
    CANCELLED: "Cancelada",
  };
  return labels[status] || String(status || "Desconhecido");
}
