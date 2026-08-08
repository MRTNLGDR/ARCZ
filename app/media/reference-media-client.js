import { LocalApiClient } from "../core/api-client.js";
export class ReferenceMediaClient {
  constructor({ api = new LocalApiClient() } = {}) { this.api = api; }
  list(category = null) {
    return this.api.json(`/api/v2/reference-media${category ? `?category=${encodeURIComponent(category)}` : ""}`);
  }
  get(id) { return this.api.json(`/api/v2/reference-media/${encodeURIComponent(id)}`); }
  updateMetadata(id, payload) {
    return this.api.json(`/api/v2/reference-media/${encodeURIComponent(id)}/metadata`, { method: "POST", body: payload });
  }
  upload(file, {
    roles = ["reference"],
    license = { id: "LicenseRef-UserProvided", redistribution_allowed: false },
    provenance = null,
    metadata = {},
    signal,
  } = {}) {
    if (!(file instanceof File)) throw new TypeError("Selecione um arquivo real");
    const encode = value => encodeURIComponent(JSON.stringify(value));
    return this.api.upload("/api/v2/reference-media/upload", file, { signal, headers: {
      "Content-Type": file.type || "application/octet-stream",
      "X-ARCZ-Filename": encodeURIComponent(file.name),
      "X-ARCZ-Roles": encode(roles),
      "X-ARCZ-License": encode(license),
      "X-ARCZ-Provenance": encode(provenance || { source: "browser_upload", source_ref: file.name }),
      "X-ARCZ-Metadata": encode(metadata),
    }});
  }
}
