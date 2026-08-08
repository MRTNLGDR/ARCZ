import { structuredApiError } from "./budget-client.js";

export class JobClient {
  constructor({ baseUrl = "/api/v2", fetchImpl = globalThis.fetch?.bind(globalThis), EventSourceImpl = globalThis.EventSource } = {}) {
    if (!fetchImpl) throw new Error("fetch indisponível");
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.fetch = fetchImpl;
    this.EventSourceImpl = EventSourceImpl;
  }

  async create(kind, request, { generationEpoch = 0, signal } = {}) {
    const response = await this.fetch(`${this.baseUrl}/generation/jobs`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ kind, request, generation_epoch: generationEpoch }), signal
    });
    const data = await response.json();
    if (!response.ok) throw structuredApiError(data, response.status);
    return data;
  }

  async get(id, { signal } = {}) {
    const response = await this.fetch(`${this.baseUrl}/generation/jobs/${encodeURIComponent(id)}`, { signal });
    const data = await response.json();
    if (!response.ok) throw structuredApiError(data, response.status);
    return data;
  }

  async cancel(id, reason = "cancelled_by_user", { signal } = {}) {
    const response = await this.fetch(`${this.baseUrl}/generation/jobs/${encodeURIComponent(id)}/cancel`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ reason }), signal
    });
    const data = await response.json();
    if (!response.ok) throw structuredApiError(data, response.status);
    return data;
  }

  subscribe(id, onEvent, { signal } = {}) {
    if (!this.EventSourceImpl) throw new Error("EventSource indisponível");
    const url = `${this.baseUrl}/generation/jobs/${encodeURIComponent(id)}/events`;
    const source = new this.EventSourceImpl(url);
    const close = () => source.close();
    source.onmessage = event => {
      try { onEvent(JSON.parse(event.data)); }
      catch (erro) { console.error("[ARCZ/jobs] evento inválido", erro, event.data); }
    };
    source.onerror = () => {
      // EventSource reconecta sozinho; fechamento só ocorre por abort explícito.
      if (signal?.aborted) close();
    };
    signal?.addEventListener("abort", close, { once: true });
    return close;
  }

  async wait(id, { signal, pollMs = 750 } = {}) {
    for (;;) {
      if (signal?.aborted) throw signal.reason || new DOMException("Aborted", "AbortError");
      const job = await this.get(id, { signal });
      if (["COMPLETED","CANCELLED","FAILED_RETRYABLE","FAILED_PERMANENT"].includes(job.status)) return job;
      await new Promise((resolve, reject) => {
        const timer = setTimeout(resolve, pollMs);
        signal?.addEventListener("abort", () => { clearTimeout(timer); reject(signal.reason || new DOMException("Aborted", "AbortError")); }, { once: true });
      });
    }
  }
  async readManifest(path, { signal } = {}) {
    if (typeof path !== "string" || !path || path.includes("\0")) throw new TypeError("caminho de manifest inválido");
    const normalized = path.startsWith("/") ? path : `/${path.replace(/^\.\//, "")}`;
    if (normalized.split("/").includes("..")) throw new Error("manifest fora da raiz");
    const response = await this.fetch(normalized, { signal });
    const data = await response.json();
    if (!response.ok) throw structuredApiError(data, response.status);
    return data;
  }

  asServices() {
    return {
      jobCreate: this.create.bind(this),
      jobSubscribe: this.subscribe.bind(this),
      jobWait: this.wait.bind(this),
      jobReadManifest: this.readManifest.bind(this),
      jobProgress: () => {}
    };
  }

}
