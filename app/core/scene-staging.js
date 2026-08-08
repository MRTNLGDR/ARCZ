import { matrizDe } from "../cena.js";

function localUrl(path) {
  if (typeof path !== "string" || !path) throw new TypeError("source obrigatório");
  if (/^(https?:|data:|blob:)/i.test(path)) throw new Error("Scene staging aceita apenas artefatos locais");
  const normalized = path.startsWith("/") ? path : `/${path}`;
  if (normalized.split("/").includes("..")) throw new Error("source fora da raiz local");
  return normalized;
}

/**
 * Adaptador transacional real para Cesium.Model.
 *
 * Um modelo entra na collection com `show=false`; só aparece após commit.
 * Rollback remove a primitive e qualquer índice associado. Não existe primitive
 * fictícia para manter a UI feliz: viewer/Cesium ausentes geram erro imediato.
 */
export class CesiumSceneStagingAdapter {
  constructor({ viewer, estadoApp, onCommitted = null } = {}) {
    if (!viewer?.scene?.primitives) throw new Error("viewer Cesium obrigatório");
    if (!globalThis.Cesium?.Model?.fromGltfAsync) throw new Error("Cesium.Model.fromGltfAsync indisponível");
    this.viewer = viewer;
    this.estadoApp = estadoApp;
    this.onCommitted = onCommitted;
    this.handles = new Map();
  }

  async stagePrimitive({ source, sha256, owner, region = null, output = null }) {
    const contextOrigin = region?.context?.origin_wgs84;
    const focus = region?.request?.focus;
    const origin = Array.isArray(contextOrigin)
      ? contextOrigin
      : (focus ? [focus.lon, focus.lat, 0] : null);
    if (!origin || !Number.isFinite(Number(origin[0])) || !Number.isFinite(Number(origin[1]))) {
      throw new Error("Região sem origin_wgs84/focus para posicionar GLB local");
    }
    const placement = output?.placement || {};
    const pos = {
      lon: Number(placement.lon ?? origin[0]), lat: Number(placement.lat ?? origin[1]),
      alt: Number(placement.alt ?? origin[2] ?? 0), rumo: Number(placement.heading ?? 0), escala: Number(placement.scale ?? 1)
    };
    const model = await Cesium.Model.fromGltfAsync({
      url: localUrl(source), modelMatrix: matrizDe(pos), scale: pos.escala,
      incrementallyLoadTextures: true, shadows: Cesium.ShadowMode.ENABLED
    });
    const id = `staged:${crypto.randomUUID?.() || `${Date.now()}-${Math.random()}`}`;
    model.id = { arczId: id, tipo: "procedural", owner, sha256 };
    model.show = false;
    this.viewer.scene.primitives.add(model);
    let state = "STAGED";
    const handle = Object.freeze({
      id, model, source, owner, sha256,
      commit: async () => {
        if (state === "ROLLED_BACK") throw new Error(`Handle ${id} já revertido`);
        if (state === "COMMITTED") return model;
        model.show = true; state = "COMMITTED"; this.viewer.scene.requestRender?.();
        return model;
      },
      rollback: async () => {
        if (state === "ROLLED_BACK") return false;
        this.viewer.scene.primitives.remove(model); this.handles.delete(id); state = "ROLLED_BACK";
        this.viewer.scene.requestRender?.(); return true;
      },
      state: () => state
    });
    this.handles.set(id, handle);
    return handle;
  }

  async commitStaged(handles, metadata = {}) {
    const committed = [];
    try {
      for (const handle of handles) { await handle.commit(); committed.push(handle); }
    } catch (error) {
      for (const handle of [...committed].reverse()) await handle.rollback(error);
      throw error;
    }
    if (this.estadoApp) {
      const current = this.estadoApp.obter().procedural_layers || [];
      const layer = {
        id: `layer:${metadata.job_id || Date.now()}`, owner: handles[0]?.owner || "generator:unknown",
        manifest: metadata.manifest || null, handles: handles.map(h => h.id), state: "GENERATED"
      };
      this.estadoApp.atualizar({ procedural_layers: [...current, layer] }, "gerador");
    }
    this.onCommitted?.({ handles, metadata });
    return { handles, metadata };
  }

  async remove(handleId) {
    const handle = this.handles.get(handleId);
    return handle ? handle.rollback("removed") : false;
  }

  diagnostics() {
    return [...this.handles.values()].map(handle => ({ id: handle.id, source: handle.source, state: handle.state() }));
  }
}
