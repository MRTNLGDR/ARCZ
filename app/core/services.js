import { BudgetClient } from "./budget-client.js";
import { JobClient } from "./job-client.js";
import { LocalAIClient } from "./local-ai-client.js";
import { LocalInputClient } from "./input-client.js";
import { criarContextoPlugin } from "./contexto.js";
import { CesiumSceneStagingAdapter } from "./scene-staging.js";

function unavailable(name) { return () => { throw new Error(`Serviço local não conectado: ${name}`); }; }

export function createRuntimeServices({ viewer, estadoApp, regionController, timeline = null,
                                        terrainSample = null, osmQueryCached = null,
                                        assetResolve = null, telemetry = null,
                                        fetchImpl = globalThis.fetch?.bind(globalThis) } = {}) {
  const jobs = new JobClient({ fetchImpl });
  const budget = new BudgetClient({ fetchImpl });
  const localAI = new LocalAIClient({ fetchImpl });
  const inputs = new LocalInputClient({ fetchImpl });
  const scene = new CesiumSceneStagingAdapter({ viewer, estadoApp });
  const services = {
    regionRead: () => estadoApp.obter().active_region,
    terrainSample: terrainSample || (point => {
      if (!viewer?.scene?.globe || !globalThis.Cesium) throw new Error("Terrain sampler indisponível");
      const cartographic = Cesium.Cartographic.fromDegrees(Number(point.lon), Number(point.lat));
      const height = viewer.scene.globe.getHeight(cartographic);
      if (!Number.isFinite(height)) throw new Error("Cota ainda não carregada no tile local");
      return height;
    }),
    osmQueryCached: osmQueryCached || unavailable("osm.read_cached"),
    assetResolve: assetResolve || unavailable("assets.read"),
    sceneStage: scene.stagePrimitive.bind(scene),
    sceneCommit: scene.commitStaged.bind(scene),
    ...budget.asServices(), ...jobs.asServices(), ...localAI.asServices(), ...inputs.asServices(),
    telemetryEvent: (type, detail) => telemetry?.event?.(type, detail),
    timelineWrite: update => {
      if (typeof timeline?.apply !== "function") return unavailable("timeline.write")();
      return timeline.apply(update);
    },
    panoramaRead: unavailable("panorama.read")
  };
  return Object.freeze({ jobs, budget, localAI, inputs, scene, services,
    contextFactory(plugin, signal) {
      return criarContextoPlugin({ pluginId: plugin.manifest.id,
        capabilities: plugin.manifest.capacidades, services, signal });
    }
  });
}
