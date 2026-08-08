import { structuredApiError } from "./budget-client.js";

export class LocalAIClient {
  constructor({ baseUrl = "/api/v2", fetchImpl = globalThis.fetch?.bind(globalThis) } = {}) {
    if (!fetchImpl) throw new Error("fetch indisponível");
    this.baseUrl = baseUrl.replace(/\/$/, ""); this.fetch = fetchImpl;
  }
  async request(task, input, { modelId = null, timeoutSeconds = null, signal } = {}) {
    const response = await this.fetch(`${this.baseUrl}/ai/tools`, {
      method: "POST", headers: { "Content-Type": "application/json" }, signal,
      body: JSON.stringify({ task, input, model_id: modelId, timeout_seconds: timeoutSeconds })
    });
    const data = await response.json();
    if (!response.ok) throw structuredApiError(data, response.status);
    return data;
  }
  asServices() { return { localAIRequest: this.request.bind(this) }; }
}
