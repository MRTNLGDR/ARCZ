import { diagnosticoSafeCallbacks } from "./safe-callback.js";

export function coletarDiagnosticoLocal({ estadoApp = null, pluginRegistry = null, resourceTrackers = [] } = {}) {
  const nav = globalThis.navigator || {};
  const performanceMemory = globalThis.performance?.memory;
  return {
    timestamp: new Date().toISOString(),
    runtime: {
      userAgent: nav.userAgent || "unknown",
      hardwareConcurrency: nav.hardwareConcurrency || null,
      deviceMemoryGB: nav.deviceMemory || null,
      jsHeapUsed: performanceMemory?.usedJSHeapSize || null,
      jsHeapLimit: performanceMemory?.jsHeapSizeLimit || null
    },
    state: estadoApp ? { saveStatus: estadoApp.statusSave, saveRevision: estadoApp.saveRevision || 0 } : null,
    plugins: pluginRegistry?.diagnostics?.() || [],
    safeCallbacks: diagnosticoSafeCallbacks(),
    resources: resourceTrackers.map(r => r.snapshot())
  };
}
