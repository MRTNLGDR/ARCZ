import { FloorplannerClient } from "./floorplanner-client.js";
import {
  bboxFromRegionState,
  normalizeSiteAuthoringLayout,
  regionSummary,
} from "./site-authoring-layout.js";

function n(tag, cls = "", text = "") {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text) node.textContent = text;
  return node;
}

function blockerText(status) {
  const blocks = status?.blockers || status?.details?.blockers || [];
  return blocks.length
    ? blocks.map(item => `${item.code}${item.files?.length ? ` (${item.files.length} arquivo(s))` : ""}`).join(" · ")
    : "Runtime local não passou na validação";
}

function secureRandomId(prefix = "") {
  const cryptoApi = globalThis.crypto;
  if (cryptoApi?.randomUUID) return `${prefix}${cryptoApi.randomUUID()}`;
  if (cryptoApi?.getRandomValues) {
    const bytes = new Uint8Array(24);
    cryptoApi.getRandomValues(bytes);
    const value = Array.from(bytes, item => item.toString(16).padStart(2, "0")).join("");
    return `${prefix}${value}`;
  }
  const error = new Error("Gerador criptográfico indisponível para o canal Floorplanner");
  error.code = "AEDIFEX_BRIDGE_ENTROPY_UNAVAILABLE";
  throw error;
}

export function createFloorplannerBridgeChannel() {
  return secureRandomId();
}

function requestId() {
  return secureRandomId("floorplanner-export-");
}

function button(label, title = "") {
  const value = n("button", "arcz-button", label);
  value.type = "button";
  if (title) value.title = title;
  return value;
}

/**
 * Hosts the real Aedifex authoring runtime while keeping the Cesium globe
 * visible and navigable. The Aedifex SceneSnapshot is the editable authority;
 * every GLB loaded in Cesium is a versioned, read-only publication derivative.
 */
export class FloorplannerHost {
  constructor({ estadoApp, client = null, sceneManager = null, viewer = null } = {}) {
    if (!estadoApp) throw new Error("estadoApp obrigatório");
    this.estadoApp = estadoApp;
    this.client = client || new FloorplannerClient({ estadoApp });
    this.sceneManager = sceneManager;
    this.viewer = viewer;
    this.project = null;
    this.runtime = null;
    this.active = false;
    this.frameReady = false;
    this.pendingExports = new Map();
    this.autoPublishTimer = null;
    this.messageHandler = event => { void this.onMessage(event); };
    this.unsubscribeState = null;
    this.viewportOrigin = null;
  }

  async mount(host) {
    this.host = host;
    this.surface = n("section", "arcz-floorplanner-host");
    this.surface.hidden = true;

    this.toolbar = n("header", "arcz-floorplanner-toolbar");
    const heading = n("div", "arcz-floorplanner-toolbar__heading");
    this.title = n("strong", "", "Autoria sobre o sítio");
    this.summary = n("span", "", "Selecione uma Região Ativa");
    heading.append(this.title, this.summary);

    const controls = n("div", "arcz-floorplanner-toolbar__controls");
    this.focusButton = button("Enquadrar região", "Leva a câmera Cesium ao lote/região importado");
    this.publishButton = button("Publicar no globo", "Gera GLB versionado da revisão Aedifex atual");
    this.globeToggle = n("label", "arcz-toggle-label");
    this.globeCheckbox = n("input");
    this.globeCheckbox.type = "checkbox";
    this.globeToggle.append(this.globeCheckbox, n("span", "", "Globo"));
    this.autoToggle = n("label", "arcz-toggle-label");
    this.autoCheckbox = n("input");
    this.autoCheckbox.type = "checkbox";
    this.autoToggle.append(this.autoCheckbox, n("span", "", "Publicação automática"));
    controls.append(this.focusButton, this.publishButton, this.globeToggle, this.autoToggle);
    this.toolbar.append(heading, controls);

    this.stage = n("div", "arcz-floorplanner-stage");
    this.globePane = n("section", "arcz-floorplanner-globe");
    this.globePane.setAttribute("aria-label", "Globo e contexto territorial navegável");
    this.globeBadge = n("div", "arcz-floorplanner-globe__badge", "Contexto territorial · Cesium");
    this.globePane.append(this.globeBadge);

    this.splitter = n("div", "arcz-floorplanner-splitter");
    this.splitter.tabIndex = 0;
    this.splitter.setAttribute("role", "separator");
    this.splitter.setAttribute("aria-orientation", "vertical");
    this.splitter.setAttribute("aria-label", "Redimensionar globo e Floorplanner");

    this.editorPane = n("section", "arcz-floorplanner-editor");
    this.editorPane.setAttribute("aria-label", "Aedifex Floorplanner");
    this.state = n("div", "arcz-mode-state");
    this.frame = n("iframe", "arcz-floorplanner-frame");
    this.frame.hidden = true;
    this.frame.title = "ARCZ Floorplanner · Aedifex";
    // Transitional local sidecar isolation. It never becomes a second source
    // of truth; the final desktop shell can mount the same packages in-process.
    this.frame.setAttribute(
      "sandbox",
      "allow-scripts allow-same-origin allow-forms allow-downloads allow-pointer-lock allow-modals",
    );
    this.frame.setAttribute("allow", "fullscreen; clipboard-read; clipboard-write");
    this.frame.referrerPolicy = "no-referrer";
    this.editorPane.append(this.state, this.frame);
    this.stage.append(this.globePane, this.splitter, this.editorPane);
    this.surface.append(this.toolbar, this.stage);
    host.append(this.surface);

    this.focusButton.addEventListener("click", () => this.focusRegion());
    this.publishButton.addEventListener("click", () => { void this.publishNow("manual"); });
    this.globeCheckbox.addEventListener("change", () => {
      this.updateLayout({ show_globe: this.globeCheckbox.checked }, true);
    });
    this.autoCheckbox.addEventListener("change", () => {
      this.updateLayout({ auto_publish: this.autoCheckbox.checked }, true);
      if (!this.autoCheckbox.checked) this.cancelAutoPublish();
    });
    this.splitter.addEventListener("pointerdown", event => this.beginSplitResize(event));
    this.splitter.addEventListener("keydown", event => this.onSplitterKey(event));

    globalThis.addEventListener("message", this.messageHandler);
    this.unsubscribeState = this.estadoApp.inscrever(() => this.refreshRegionSummary());
    this.applyLayout();
    this.refreshRegionSummary();
    this.renderState("Pronto para importar a Região Ativa.");
  }

  layout() {
    return normalizeSiteAuthoringLayout(this.estadoApp.obter()?.floorplanner_layout);
  }

  updateLayout(patch, persist = false) {
    const layout = normalizeSiteAuthoringLayout({ ...this.layout(), ...patch });
    if (persist) this.estadoApp.atualizar({ floorplanner_layout: layout }, "floorplanner_layout");
    this.applyLayout(layout);
    return layout;
  }

  applyLayout(value = this.layout()) {
    const layout = normalizeSiteAuthoringLayout(value);
    this.surface?.style.setProperty("--arcz-site-split", `${(layout.split_ratio * 100).toFixed(3)}%`);
    this.surface?.classList.toggle("is-globe-hidden", !layout.show_globe);
    if (this.globeCheckbox) this.globeCheckbox.checked = layout.show_globe;
    if (this.autoCheckbox) this.autoCheckbox.checked = layout.auto_publish;
    if (this.splitter) {
      this.splitter.hidden = !layout.show_globe;
      this.splitter.setAttribute("aria-valuemin", "20");
      this.splitter.setAttribute("aria-valuemax", "68");
      this.splitter.setAttribute("aria-valuenow", String(Math.round(layout.split_ratio * 100)));
    }
    requestAnimationFrame(() => this.resizeViewer());
  }

  refreshRegionSummary() {
    if (!this.summary) return;
    const info = regionSummary(this.estadoApp.obter());
    this.summary.textContent = `${info.label} · ${info.scale} · ${info.source}`;
    this.globeBadge.textContent = info.region_id
      ? `Contexto territorial · ${info.region_id}`
      : "Contexto territorial · Cesium";
  }

  beginSplitResize(event) {
    if (event.button !== 0) return;
    event.preventDefault();
    this.splitter.setPointerCapture?.(event.pointerId);
    const move = current => {
      const rect = this.stage.getBoundingClientRect();
      if (rect.width <= 0) return;
      const ratio = (current.clientX - rect.left) / rect.width;
      this.updateLayout({ split_ratio: ratio }, false);
    };
    const end = current => {
      move(current);
      this.splitter.releasePointerCapture?.(event.pointerId);
      this.splitter.removeEventListener("pointermove", move);
      this.splitter.removeEventListener("pointerup", end);
      this.splitter.removeEventListener("pointercancel", end);
      this.updateLayout({ split_ratio: this.layoutFromCss() }, true);
    };
    this.splitter.addEventListener("pointermove", move);
    this.splitter.addEventListener("pointerup", end);
    this.splitter.addEventListener("pointercancel", end);
  }

  layoutFromCss() {
    const raw = this.surface?.style.getPropertyValue("--arcz-site-split") || "38%";
    return Number.parseFloat(raw) / 100;
  }

  onSplitterKey(event) {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const current = this.layout().split_ratio;
    const next = event.key === "Home" ? 0.2
      : event.key === "End" ? 0.68
      : current + (event.key === "ArrowRight" ? 0.025 : -0.025);
    this.updateLayout({ split_ratio: next }, true);
  }

  attachGlobe() {
    const viewport = document.getElementById("viewport");
    if (!viewport || !this.globePane || viewport.parentElement === this.globePane) return;
    if (!this.viewportOrigin) {
      this.viewportOrigin = { parent: viewport.parentNode, next: viewport.nextSibling };
    }
    viewport.classList.add("arcz-floorplanner-live-globe");
    viewport.setAttribute("aria-hidden", "false");
    this.globePane.insertBefore(viewport, this.globeBadge);
    this.resizeViewer();
  }

  detachGlobe() {
    const viewport = document.getElementById("viewport");
    const origin = this.viewportOrigin;
    if (!viewport || !origin?.parent) return;
    viewport.classList.remove("arcz-floorplanner-live-globe");
    if (origin.next && origin.next.parentNode === origin.parent) origin.parent.insertBefore(viewport, origin.next);
    else origin.parent.appendChild(viewport);
    this.resizeViewer();
  }

  resizeViewer() {
    try {
      this.viewer?.resize?.();
      this.viewer?.scene?.requestRender?.();
    } catch (error) {
      console.warn("Não foi possível redimensionar o globo após alterar o Floorplanner:", error);
    }
  }

  focusRegion() {
    const bbox = bboxFromRegionState(this.estadoApp.obter());
    if (!bbox || !globalThis.Cesium || !this.viewer?.camera) {
      this.renderState("A Região Ativa ainda não possui limites utilizáveis.", { error: true });
      return false;
    }
    const [west, south, east, north] = bbox;
    try {
      const rectangle = Cesium.Rectangle.fromDegrees(west, south, east, north);
      this.viewer.camera.flyTo({
        destination: rectangle,
        duration: globalThis.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches ? 0 : 1.35,
      });
      this.viewer.scene.requestRender();
      return true;
    } catch (error) {
      console.error("Falha ao enquadrar Região Ativa:", error);
      return false;
    }
  }

  renderState(message, { error = false, retry = false, details = "" } = {}) {
    this.state.replaceChildren();
    this.state.classList.toggle("is-error", error);
    this.state.append(
      n("h2", "", error ? "Floorplanner indisponível" : "Floorplanner georreferenciado"),
      n("p", "", message),
    );
    if (details) this.state.append(n("pre", "arcz-mode-details", details));
    if (retry) {
      const retryButton = n("button", "arcz-button arcz-button--primary", "Tentar novamente");
      retryButton.type = "button";
      retryButton.addEventListener("click", () => { void this.open(); });
      this.state.append(retryButton);
    }
    this.state.hidden = false;
  }

  expectedOrigin() {
    const value = this.runtime?.runtime?.url;
    if (!value) return null;
    try { return new URL(value).origin; }
    catch { return null; }
  }

  async open() {
    if (!this.active) return;
    this.frame.hidden = true;
    this.frameReady = false;
    this.renderState("Preparando contexto territorial, lote e perfis regionais…");
    try {
      this.project = await this.client.ensureProject();
      this.renderState("Validando e iniciando o kernel Aedifex local…");
      this.runtime = await this.client.ensureRuntime();
      this.channel = createFloorplannerBridgeChannel();
      const src = this.client.sidecarUrl(this.project.id, this.runtime, { channel: this.channel });
      if (this.frame.src !== src) this.frame.src = src;
      this.frame.hidden = false;
      this.renderState("Carregando o editor real…");
    } catch (error) {
      this.frame.hidden = true;
      const details = error?.details ? JSON.stringify(error.details, null, 2) : "";
      this.renderState(`${error.message}. ${blockerText(error.details)}`, {
        error: true, retry: true, details,
      });
    }
  }

  resolvePending(id, error, result) {
    const pending = this.pendingExports.get(id);
    if (!pending) return;
    clearTimeout(pending.timer);
    this.pendingExports.delete(id);
    if (error) pending.reject(error);
    else pending.resolve(result);
  }

  scheduleAutoPublish() {
    this.cancelAutoPublish();
    const layout = this.layout();
    if (!this.active || !layout.auto_publish || !this.frameReady) return;
    this.autoPublishTimer = setTimeout(() => {
      this.autoPublishTimer = null;
      void this.publishNow("revision_saved_auto");
    }, layout.auto_publish_delay_ms);
  }

  cancelAutoPublish() {
    if (this.autoPublishTimer) clearTimeout(this.autoPublishTimer);
    this.autoPublishTimer = null;
  }

  async publishNow(reason = "manual") {
    this.publishButton.disabled = true;
    const original = this.publishButton.textContent;
    this.publishButton.textContent = "Publicando…";
    try {
      const result = await this.requestSceneExport({ reason });
      this.publishButton.textContent = result ? `Publicado · r${result.revision}` : "Sem revisão para publicar";
      return result;
    } catch (error) {
      this.publishButton.textContent = "Falha ao publicar";
      console.error("Publicação Floorplanner → globo falhou:", error);
      throw error;
    } finally {
      setTimeout(() => {
        if (!this.publishButton) return;
        this.publishButton.disabled = false;
        this.publishButton.textContent = original;
      }, 1200);
    }
  }

  async onMessage(event) {
    const expectedOrigin = this.expectedOrigin();
    if (!expectedOrigin || event.origin !== expectedOrigin || event.source !== this.frame.contentWindow) return;
    const data = event.data || {};
    if (data.project_id && data.project_id !== this.project?.id) return;
    if (!this.channel || data.channel !== this.channel) return;

    if (data.type === "arcz:aedifex-ready") {
      this.frameReady = true;
      this.project = {
        ...this.project,
        current_revision: Number(data.revision || this.project?.current_revision || 0),
        scene_hash: data.scene_hash || this.project?.scene_hash || null,
      };
      this.state.hidden = true;
      this.frame.hidden = false;
      return;
    }
    if (data.type === "arcz:aedifex-error") {
      this.renderState(data.message || "Erro no editor Aedifex", {
        error: true, retry: true, details: JSON.stringify(data.details || {}, null, 2),
      });
      return;
    }
    if (data.type === "arcz:aedifex-saved" && Number.isInteger(data.revision)) {
      this.project = {
        ...this.project,
        current_revision: data.revision,
        scene_hash: data.scene_hash || null,
      };
      this.estadoApp.atualizar({ active_floorplanner_project_id: this.project.id }, "floorplanner");
      if (data.changed !== false) this.scheduleAutoPublish();
      return;
    }
    if (data.type === "arcz:aedifex-exported") {
      try {
        const derivative = await this.applyDerivative(data);
        this.resolvePending(String(data.request_id || data.result?.requestId || ""), null, derivative);
      } catch (error) {
        this.resolvePending(String(data.request_id || data.result?.requestId || ""), error);
      }
      return;
    }
    if (data.type === "arcz:aedifex-export-error") {
      const error = new Error(data.message || "Falha ao exportar a cena Aedifex");
      error.details = data.details;
      this.resolvePending(String(data.request_id || ""), error);
    }
  }

  async applyDerivative(message) {
    const stored = message?.result?.export;
    if (!stored?.id || !stored?.path || !stored?.sha256) {
      throw new Error("Resposta de export Floorplanner incompleta");
    }
    const semantic = stored.semantic_manifest || {};
    const geoAnchor = semantic.geo_anchor || this.project?.context?.geo_anchor;
    if (!geoAnchor) throw new Error("Export Floorplanner sem GeoAnchor");
    const sceneHash = message.scene_hash || semantic.scene_hash;
    if (!/^[a-f0-9]{64}$/.test(String(sceneHash || ""))) {
      throw new Error("Export Floorplanner sem scene_hash válido");
    }
    const derivative = {
      project_id: this.project.id,
      revision: Number(stored.revision),
      export_id: stored.id,
      path: stored.path,
      url: stored.url || `/${stored.path}`,
      sha256: stored.sha256,
      scene_hash: sceneHash,
      generation_epoch: Math.max(0, Number(this.estadoApp.obter()?.active_region?.generation_epoch || 0)),
      geo_anchor: geoAnchor,
      semantic_manifest: semantic,
      readonly: true,
      status: "ACTIVE",
      lod: "equilibrado",
      updated_at: stored.created_at || new Date().toISOString(),
    };
    const state = this.estadoApp.obter();
    const derivatives = [
      ...(state.floorplanner_derivatives || []).filter(item => item.project_id !== derivative.project_id),
      derivative,
    ];
    this.estadoApp.atualizar({
      active_floorplanner_project_id: derivative.project_id,
      floorplanner_derivatives: derivatives,
    }, "floorplanner_derivative");
    await this.sceneManager?.carregarDerivadoFloorplanner?.(derivative);
    return derivative;
  }

  requestSceneExport({ reason = "host_request", timeoutMs = 120000 } = {}) {
    if (!this.frameReady || !this.frame?.contentWindow || !this.project?.id) return Promise.resolve(null);
    const revision = Number(this.project.current_revision || 0);
    if (revision <= 0) return Promise.resolve(null);
    const current = (this.estadoApp.obter().floorplanner_derivatives || [])
      .find(item => item.project_id === this.project.id && Number(item.revision) === revision);
    if (current) return Promise.resolve(current);
    const origin = this.expectedOrigin();
    if (!origin) return Promise.reject(new Error("Origem local do Aedifex indisponível"));
    const id = requestId();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingExports.delete(id);
        reject(new Error(`Export Floorplanner excedeu ${timeoutMs} ms`));
      }, timeoutMs);
      this.pendingExports.set(id, { resolve, reject, timer });
      this.frame.contentWindow.postMessage({
        type: "arcz:request-scene-export",
        project_id: this.project.id,
        revision,
        request_id: id,
        reason,
        channel: this.channel,
      }, origin);
    });
  }

  async activate() {
    this.active = true;
    this.surface.hidden = false;
    this.attachGlobe();
    this.applyLayout();
    await this.open();
  }

  async deactivate() {
    this.cancelAutoPublish();
    if (this.active) {
      try { await this.requestSceneExport({ reason: "mode_exit" }); }
      catch (error) { console.warn("Floorplanner salvo, mas o derivado do globo não foi atualizado:", error); }
    }
    this.active = false;
    this.detachGlobe();
    this.surface.hidden = true;
  }

  async dispose() {
    this.cancelAutoPublish();
    globalThis.removeEventListener("message", this.messageHandler);
    this.unsubscribeState?.();
    for (const [id, pending] of this.pendingExports) {
      clearTimeout(pending.timer);
      pending.reject(new Error("Floorplanner foi encerrado antes do export"));
      this.pendingExports.delete(id);
    }
    this.detachGlobe();
    this.channel = null;
    this.frame.src = "about:blank";
    this.surface.remove();
  }
}
