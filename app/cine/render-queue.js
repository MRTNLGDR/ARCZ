import { structuredApiError } from "../core/budget-client.js";

const TERMINAL = new Set(["CANCELLED", "COMPLETED", "FAILED_RETRYABLE", "FAILED_PERMANENT"]);

export class RenderQueue {
  constructor({ fetchImpl = globalThis.fetch?.bind(globalThis), baseUrl = "/api/v2" } = {}) {
    if (!fetchImpl) throw new Error("fetch indisponível");
    this.fetch = fetchImpl;
    this.baseUrl = baseUrl.replace(/\/$/, "");
  }

  async enqueue(job, { signal } = {}) {
    const response = await this.fetch(`${this.baseUrl}/render/jobs`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify(job), signal,
    });
    const data = await response.json();
    if (!response.ok) throw structuredApiError(data, response.status);
    return data;
  }

  async status(id, { signal } = {}) {
    const response = await this.fetch(`${this.baseUrl}/render/jobs/${encodeURIComponent(id)}`, { signal });
    const data = await response.json();
    if (!response.ok) throw structuredApiError(data, response.status);
    return data;
  }

  async cancel(id, reason = "cancelled_by_user", { signal } = {}) {
    const response = await this.fetch(`${this.baseUrl}/render/jobs/${encodeURIComponent(id)}/cancel`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ reason }), signal,
    });
    const data = await response.json();
    if (!response.ok) throw structuredApiError(data, response.status);
    return data;
  }

  async wait(id, { signal, pollMs = 750 } = {}) {
    for (;;) {
      if (signal?.aborted) throw signal.reason ?? new DOMException("Abortado", "AbortError");
      const job = await this.status(id, { signal });
      if (TERMINAL.has(job.status)) return job;
      await new Promise((resolve, reject) => {
        const timer = setTimeout(resolve, pollMs);
        signal?.addEventListener("abort", () => { clearTimeout(timer); reject(signal.reason); }, { once: true });
      });
    }
  }
}
