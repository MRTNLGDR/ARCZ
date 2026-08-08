import { validateRegionRequest } from "../core/schema.js";
import { ORIGENS } from "../core/origens.js";
import { clampGenerationRadius } from "./scale-policy.js";
import { structuredApiError } from "../core/budget-client.js";

export class RegionController {
  constructor({ estadoApp, jobClient, fetchImpl = globalThis.fetch?.bind(globalThis), baseUrl = "/api/v2" }) {
    if (!estadoApp || !jobClient || !fetchImpl) throw new Error("RegionController exige estado, jobs e fetch");
    this.estadoApp = estadoApp;
    this.jobClient = jobClient;
    this.fetch = fetchImpl;
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.generationEpoch = Number(estadoApp.obter()?.active_region?.generation_epoch || 0);
    this.activeJobs = new Map();
    this.abortController = new AbortController();
  }

  async resolve(query, { scale = "endereco", limit = 8, signal } = {}) {
    const q = String(query || "").trim();
    if (q.length < 4) return [];
    const response = await this.fetch(`${this.baseUrl}/regions/resolve`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ query: q, scale, limit }), signal
    });
    const data = await response.json();
    if (!response.ok) throw structuredApiError(data, response.status);
    return Array.isArray(data) ? data : (data.results || []);
  }

  async activate(input, { cancelPrevious = true } = {}) {
    const radius = clampGenerationRadius(input.scale, input.requested_radius_m);
    const request = validateRegionRequest({ ...input, requested_radius_m: radius, generation_epoch: this.generationEpoch + 1 });
    if (cancelPrevious) await this.cancelAll("region_changed");
    this.abortController.abort("region_changed");
    this.abortController = new AbortController();
    this.generationEpoch += 1;
    request.generation_epoch = this.generationEpoch;

    const response = await this.fetch(`${this.baseUrl}/regions/context`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(request),
      signal: this.abortController.signal
    });
    const data = await response.json();
    if (!response.ok) throw structuredApiError(data, response.status);
    const active = { request, context: data.context || data, generation_epoch: this.generationEpoch, activated_at: new Date().toISOString() };
    this.estadoApp.atualizar({ active_region: active }, ORIGENS.REGIAO);
    return active;
  }

  async startGeneration(kind, request = {}) {
    const active = this.estadoApp.obter().active_region;
    if (!active) throw new Error("Nenhuma Região Ativa");
    const job = await this.jobClient.create(kind, { ...request, active_region: active }, {
      generationEpoch: this.generationEpoch, signal: this.abortController.signal
    });
    this.activeJobs.set(job.id, job);
    return job;
  }

  async cancelAll(reason = "cancelled") {
    const ids = [...this.activeJobs.keys()];
    const results = await Promise.allSettled(ids.map(id => this.jobClient.cancel(id, reason)));
    this.activeJobs.clear();
    return results;
  }

  acceptResult(job) {
    if (Number(job?.generation_epoch) !== this.generationEpoch) return false;
    if (job.status !== "COMPLETED") return false;
    return true;
  }
}
