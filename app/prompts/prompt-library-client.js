import { LocalApiClient } from "../core/api-client.js";

export class PromptLibraryClient {
  constructor({ api = new LocalApiClient() } = {}) { this.api = api; }
  list({ query = "", category = "", language = "" } = {}) {
    const params = new URLSearchParams();
    if (query) params.set("q", query);
    if (category) params.set("category", category);
    if (language) params.set("language", language);
    return this.api.json(`/api/v2/prompts${params.size ? `?${params}` : ""}`);
  }
  get(id) { return this.api.json(`/api/v2/prompts/${encodeURIComponent(id)}`); }
  versions(id, limit = 100) {
    return this.api.json(`/api/v2/prompts/${encodeURIComponent(id)}/versions?limit=${encodeURIComponent(limit)}`);
  }
  save(prompt) { return this.api.json("/api/v2/prompts", { method: "POST", body: prompt }); }
  duplicate(id, prompt = {}) {
    return this.api.json(`/api/v2/prompts/${encodeURIComponent(id)}/duplicate`, { method: "POST", body: prompt });
  }
  archive(id) {
    return this.api.json(`/api/v2/prompts/${encodeURIComponent(id)}/archive`, { method: "POST", body: {} });
  }
  compile(identifier, variables, context = {}) {
    return this.api.json("/api/v2/prompts/compile", { method: "POST", body: { identifier, variables, context } });
  }
  exportBundle(options = {}) {
    return this.api.json("/api/v2/prompts/export", { method: "POST", body: options });
  }
  importBundle(bundle, { conflict = "duplicate" } = {}) {
    return this.api.json("/api/v2/prompts/import", { method: "POST", body: { bundle, conflict } });
  }
  enhance(payload) { return this.api.json("/api/v2/prompts/enhance", { method: "POST", body: payload }); }
  translate(payload) { return this.api.json("/api/v2/prompts/translate", { method: "POST", body: payload }); }
}
