import { LocalApiClient } from "../core/api-client.js";
import { buildModelingContextRequest } from "./modeling-context.js";

function isLoopbackUrl(value) {
  try {
    const url = new URL(value);
    return url.protocol === "http:" && ["127.0.0.1", "localhost", "[::1]", "::1"].includes(url.hostname);
  } catch { return false; }
}

export class FloorplannerClient {
  constructor({ api = new LocalApiClient(), estadoApp = null } = {}) { this.api = api; this.estadoApp = estadoApp; }
  status() { return this.api.json("/api/v2/aedifex/status"); }
  startRuntime(options = {}) { return this.api.json("/api/v2/aedifex/start", { method: "POST", body: options }); }
  stopRuntime(options = {}) { return this.api.json("/api/v2/aedifex/stop", { method: "POST", body: options }); }
  createContext(payload) { return this.api.json("/api/v2/floorplanner/context", { method: "POST", body: payload }); }
  listProjects(regionId = null) { return this.api.json(`/api/v2/floorplanner/projects${regionId ? `?region_id=${encodeURIComponent(regionId)}` : ""}`); }
  getProject(id, includeScene = true) { return this.api.json(`/api/v2/floorplanner/projects/${encodeURIComponent(id)}?include_scene=${includeScene ? 1 : 0}`); }
  createProject(payload) { return this.api.json("/api/v2/floorplanner/projects", { method: "POST", body: payload }); }

  async ensureProject({ name = null, referenceMedia = [] } = {}) {
    if (!this.estadoApp) throw new Error("estadoApp obrigatório para preparar o Floorplanner");
    const state = this.estadoApp.obter();
    const context = await this.createContext(buildModelingContextRequest(state, { referenceMedia }));
    const projects = await this.listProjects(context.region_id);
    let project = projects.find(item => item.context_hash === context.context_hash) || null;
    if (project) project = await this.getProject(project.id, true);
    else project = await this.createProject({
      arcz_project_id: state.id || null,
      name: name || `Modelo · ${state.active_region?.request?.scale || "região"}`,
      context, origin: "import",
      metadata: { created_by: "arcz.floorplanner.host", local_first: true }
    });
    const known = [...(state.floorplanner_projects || []).filter(item => item.id !== project.id), {
      id: project.id, region_id: project.region_id, context_hash: project.context_hash,
      name: project.name, current_revision: project.current_revision, updated_at: project.updated_at
    }];
    this.estadoApp.atualizar({ active_floorplanner_project_id: project.id, floorplanner_projects: known }, "floorplanner");
    return project;
  }

  async ensureRuntime() {
    let status = await this.status();
    if (!status.ready) status = await this.startRuntime({ wait_seconds: 25 });
    const url = status?.runtime?.url;
    if (!status?.ready || !isLoopbackUrl(url)) {
      const error = new Error("Runtime Aedifex local indisponível"); error.code = "AEDIFEX_RUNTIME_NOT_READY";
      error.details = status; throw error;
    }
    return status;
  }

  sidecarUrl(projectId, status, { channel } = {}) {
    const base = status?.runtime?.url;
    if (!isLoopbackUrl(base)) throw new Error("URL de sidecar não é loopback");
    if (!/^[a-f0-9-]{32,64}$/i.test(String(channel || ""))) {
      const error = new Error("Canal de sessão Floorplanner inválido");
      error.code = "AEDIFEX_BRIDGE_CHANNEL_INVALID";
      throw error;
    }
    const url = new URL(base); url.searchParams.set("project", projectId);
    url.searchParams.set("api", globalThis.location?.origin || "http://127.0.0.1:8123");
    url.searchParams.set("channel", channel);
    return url.toString();
  }
}
