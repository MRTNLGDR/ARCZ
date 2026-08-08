import { deepFreeze } from "./schema.js";
import { ResourceTracker } from "./resource-tracker.js";

export const CAPABILITIES = Object.freeze({
  REGION_READ: "region.read",
  TERRAIN_READ: "terrain.read",
  OSM_READ_CACHED: "osm.read_cached",
  ASSETS_READ: "assets.read",
  SCENE_STAGE: "scene.stage",
  SCENE_COMMIT: "scene.commit",
  BUDGET_RESERVE: "budget.reserve",
  JOB_PROGRESS: "jobs.progress",
  JOB_CREATE: "jobs.create",
  JOB_SUBSCRIBE: "jobs.subscribe",
  JOB_WAIT: "jobs.wait",
  JOB_READ_MANIFEST: "jobs.read_manifest",
  INPUTS_RESOLVE: "inputs.resolve",
  TELEMETRY_WRITE: "telemetry.write",
  LOCAL_AI_REQUEST: "local_ai.request",
  TIMELINE_WRITE: "timeline.write",
  PANORAMA_READ: "panorama.read"
});

function denied(capability) {
  return () => { throw new Error(`Capability não concedida: ${capability}`); };
}

export function criarContextoPlugin({ pluginId, capabilities = [], services = {}, signal = null }) {
  const permitidas = new Set(capabilities);
  const tracker = new ResourceTracker(`plugin:${pluginId}`);
  const obter = (capability, service, fallback = null) => {
    if (!permitidas.has(capability)) return fallback || denied(capability);
    if (service === undefined || service === null) throw new Error(`Serviço ausente para ${capability}`);
    return service;
  };
  const ctx = {
    pluginId,
    signal,
    resources: tracker,
    region: { read: obter(CAPABILITIES.REGION_READ, services.regionRead) },
    terrain: { sample: obter(CAPABILITIES.TERRAIN_READ, services.terrainSample) },
    osm: { queryCached: obter(CAPABILITIES.OSM_READ_CACHED, services.osmQueryCached) },
    assets: { resolveById: obter(CAPABILITIES.ASSETS_READ, services.assetResolve) },
    scene: {
      stagePrimitive: obter(CAPABILITIES.SCENE_STAGE, services.sceneStage),
      commitStaged: obter(CAPABILITIES.SCENE_COMMIT, services.sceneCommit)
    },
    budget: { reserve: obter(CAPABILITIES.BUDGET_RESERVE, services.budgetReserve) },
    inputs: { resolve: obter(CAPABILITIES.INPUTS_RESOLVE, services.inputResolve) },
    jobs: {
      progress: obter(CAPABILITIES.JOB_PROGRESS, services.jobProgress, () => {}),
      create: obter(CAPABILITIES.JOB_CREATE, services.jobCreate),
      subscribe: obter(CAPABILITIES.JOB_SUBSCRIBE, services.jobSubscribe),
      wait: obter(CAPABILITIES.JOB_WAIT, services.jobWait),
      readManifest: obter(CAPABILITIES.JOB_READ_MANIFEST, services.jobReadManifest)
    },
    telemetry: { event: obter(CAPABILITIES.TELEMETRY_WRITE, services.telemetryEvent, () => {}) },
    localAI: { request: obter(CAPABILITIES.LOCAL_AI_REQUEST, services.localAIRequest) },
    timeline: { write: obter(CAPABILITIES.TIMELINE_WRITE, services.timelineWrite) },
    panorama: { read: obter(CAPABILITIES.PANORAMA_READ, services.panoramaRead) }
  };
  // ResourceTracker permanece mutável por desenho; o restante fica congelado.
  for (const [key, value] of Object.entries(ctx)) if (key !== "resources") deepFreeze(value);
  return Object.freeze(ctx);
}
