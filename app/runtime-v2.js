import { estadoApp } from "./estado.js";
import { RegionController } from "./region/region-controller.js";
import { pluginRegistry } from "./plugins/registry.js";
import { PluginOrchestrator } from "./plugins/orchestrator.js";
import builtinGenerators from "./plugins/builtin/index.js";
import { createRuntimeServices } from "./core/services.js";

export function initializeV2Runtime({ viewer, telemetry = null } = {}) {
  if (!viewer) throw new Error("viewer obrigatório");
  const deferredJobClient = { create(){throw new Error("jobs ainda não inicializados");} };
  // O controller recebe o JobClient real logo depois da criação dos serviços.
  const regionController = new RegionController({ estadoApp, jobClient: deferredJobClient });
  const runtime = createRuntimeServices({ viewer, estadoApp, regionController, telemetry });
  regionController.jobClient = runtime.jobs;
  for (const plugin of builtinGenerators) if (!pluginRegistry.get(plugin.manifest.id)) pluginRegistry.register(plugin);
  const orchestrator = new PluginOrchestrator({ registry: pluginRegistry,
    contextFactory: (plugin, signal) => runtime.contextFactory(plugin, signal), telemetry });
  const api = Object.freeze({ ...runtime, regionController, pluginRegistry, orchestrator });
  globalThis.ARCZ_V2 = api;
  return api;
}
