export function slugifyPrompt(value, fallback = "prompt") {
  const normalized = String(value || "")
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/[-_.]{2,}/g, "-")
    .replace(/^[-_.]+|[-_.]+$/g, "");
  const safe = normalized || fallback;
  return safe.length < 2 ? `${safe}-prompt` : safe.slice(0, 120);
}

export function parsePromptTags(value) {
  return [...new Set(String(value || "").split(",").map(item => item.trim()).filter(Boolean))];
}

export function extractInferenceText(envelope) {
  const result = envelope?.result;
  const value = result?.text ?? result?.prompt ?? result?.translation ?? result;
  if (typeof value !== "string" || !value.trim()) {
    throw new Error("Modelo local retornou contrato textual inválido");
  }
  return value.trim();
}
