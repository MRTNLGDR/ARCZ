export class ArczApiError extends Error {
  constructor(message, { code = "API_ERROR", status = 0, retryable = false, details = {}, traceId = null } = {}) {
    super(message); this.name = "ArczApiError"; this.code = code; this.status = status;
    this.retryable = retryable; this.details = details; this.traceId = traceId;
  }
}

function assertLocalPath(path) {
  const value = String(path || "");
  if (!value.startsWith("/api/") || /^(?:https?:)?\/\//i.test(value)) throw new TypeError(`Rota local inválida: ${value}`);
  return value;
}


function normalizeLocalBaseUrl(value) {
  const raw = String(value || "").trim().replace(/\/$/, "");
  if (!raw) return "";
  let url;
  try { url = new URL(raw); }
  catch { throw new TypeError(`Base URL local inválida: ${raw}`); }
  if (url.protocol !== "http:" || !["127.0.0.1", "localhost", "[::1]", "::1"].includes(url.hostname) || url.username || url.password) {
    throw new TypeError(`Base URL deve ser HTTP loopback sem credenciais: ${raw}`);
  }
  if (url.pathname !== "/" || url.search || url.hash) throw new TypeError("Base URL deve conter somente a origem loopback");
  return url.origin;
}

async function decodeResponse(response) {
  const contentType = response.headers.get("content-type") || "";
  const payload = contentType.includes("application/json") ? await response.json() : await response.text();
  if (!response.ok) {
    const error = payload?.error || {};
    throw new ArczApiError(error.message || `HTTP ${response.status}`, {
      code: error.code || `HTTP_${response.status}`, status: response.status,
      retryable: Boolean(error.retryable), details: error.details || {}, traceId: error.trace_id || null
    });
  }
  return payload;
}

export class LocalApiClient {
  constructor({ fetchImpl = globalThis.fetch?.bind(globalThis), baseUrl = "" } = {}) {
    if (!fetchImpl) throw new Error("fetch indisponível");
    this.fetch = fetchImpl; this.baseUrl = normalizeLocalBaseUrl(baseUrl);
  }
  async json(path, { method = "GET", body, headers = {}, signal } = {}) {
    const local = assertLocalPath(path);
    const response = await this.fetch(`${this.baseUrl}${local}`, {
      method, signal, cache: "no-store", credentials: "omit", redirect: "error",
      headers: { Accept: "application/json", ...(body === undefined ? {} : { "Content-Type": "application/json" }), ...headers },
      body: body === undefined ? undefined : JSON.stringify(body)
    });
    return decodeResponse(response);
  }
  async upload(path, file, { headers = {}, signal } = {}) {
    const local = assertLocalPath(path);
    if (!(file instanceof Blob)) throw new TypeError("upload exige Blob/File");
    const response = await this.fetch(`${this.baseUrl}${local}`, {
      method: "POST", signal, cache: "no-store", credentials: "omit", redirect: "error", headers, body: file
    });
    return decodeResponse(response);
  }
}
