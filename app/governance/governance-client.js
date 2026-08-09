import { LocalApiClient } from "../core/api-client.js";

export class GovernanceClient {
  constructor({ api = new LocalApiClient() } = {}) {
    this.api = api;
  }

  snapshot() {
    return this.api.json("/api/governance/snapshot");
  }

  async runtime() {
    const entries = await Promise.allSettled([
      this.api.json("/api/v2/health"),
      this.api.json("/api/v2/aedifex/status"),
      this.api.json("/api/v2/models"),
      this.api.json("/api/v2/diagnostics"),
    ]);
    const names = ["health", "aedifex", "models", "diagnostics"];
    const output = {};
    for (let index = 0; index < entries.length; index += 1) {
      const result = entries[index];
      const name = names[index];
      if (result.status === "fulfilled") {
        output[name] = { ok: true, value: result.value };
      } else {
        output[name] = {
          ok: false,
          error: result.reason?.message || String(result.reason),
        };
      }
    }
    return output;
  }
}
