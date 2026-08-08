import { FloorplannerClient } from "../floorplanner/floorplanner-client.js";
import { PhotorealClient, buildPhotorealRequest, parseVector3 } from "./photoreal-client.js";

function n(tag, cls = "", text = "") {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text) node.textContent = text;
  return node;
}
function field(labelText, control) {
  const label = n("label", "arcz-field");
  label.append(n("span", "", labelText), control);
  return label;
}
function input(value = "", type = "text") {
  const control = n("input", "arcz-input");
  control.type = type;
  control.value = String(value);
  return control;
}
function select(options) {
  const control = n("select", "arcz-select");
  for (const [value, label] of options) {
    const option = n("option", "", label);
    option.value = value;
    control.append(option);
  }
  return control;
}

const PASS_LABELS = Object.freeze({
  beauty: "Beauty", depth: "Profundidade", normals: "Normais", object_ids: "Object IDs",
  semantic_masks: "Máscaras semânticas", material_masks: "Máscaras de material", sky_mask: "Máscara do céu",
});

export class PhotorealWorkspace {
  constructor({ estadoApp, floorplanner = null, client = new PhotorealClient(), referencesProvider = () => [] } = {}) {
    this.estadoApp = estadoApp;
    this.floorplanner = floorplanner || new FloorplannerClient({ estadoApp });
    this.client = client;
    this.referencesProvider = referencesProvider;
    this.pendingPrompt = { positive: "", negative: "" };
    this.jobAbort = null;
    this.lastPreflight = null;
  }

  async mount(host) {
    this.host = host;
    this.root = n("section", "arcz-render-workspace");
    const title = n("div", "arcz-mode-heading");
    title.append(
      n("h2", "", "Render fotorreal local"),
      n("p", "", "Cena Aedifex versionada + passes estruturais + Blender + difusão/upscale locais. Nenhuma imagem fictícia é criada quando o worker ou modelo estiver ausente."),
    );

    this.project = select([]);
    this.preset = select([
      ["7680x4320", "8K UHD · 16:9"], ["7680x3291", "8K ultrawide · 21:9"],
      ["8192x8192", "8K quadrado"], ["4096x2160", "4K DCI"], ["3840x2160", "4K UHD"],
    ]);
    this.format = select([["png", "PNG"], ["jpg", "JPG"], ["exr", "OpenEXR"]]);
    this.quality = select([
      ["draft", "Rascunho rápido"], ["preview", "Preview"], ["balanced", "Equilibrado"],
      ["high", "Alta qualidade"], ["ultra", "Ultra / final"],
    ]);
    this.quality.value = "high";
    this.engine = select([["cycles", "Cycles"], ["eevee", "Eevee Next"]]);
    this.samples = input(256, "number"); this.samples.min = "1"; this.samples.max = "8192";
    this.device = select([["auto", "GPU automática"], ["gpu", "Forçar GPU"], ["cpu", "CPU"]]);
    this.mode = select([
      ["full_photoreal", "Fotorreal completo"], ["material", "Materiais"], ["vegetation", "Vegetação"],
      ["people_vehicles", "Pessoas e veículos"], ["weather_time", "Clima e horário"], ["none", "Render base sem difusão"],
    ]);
    this.output = input("arcz-photoreal");
    this.seed = input(this.estadoApp.obter().project_seed || 1, "number");
    this.seed.min = "0";
    this.modelId = input("");
    this.modelId.placeholder = "automático pelo registro local";
    this.guard = input(2, "number");
    this.guard.min = "0"; this.guard.max = "64"; this.guard.step = "0.5";

    this.cameraPreset = select([
      ["exterior35", "Exterior · 35 mm"], ["interior24", "Interior · 24 mm"],
      ["detail50", "Detalhe · 50 mm"], ["aerial28", "Aérea · 28 mm"], ["custom", "Personalizada"],
    ]);
    this.position = input("12, 8, 12");
    this.target = input("0, 2, 0");
    this.focal = input(35, "number"); this.focal.min = "8"; this.focal.max = "800";
    this.aperture = input(5.6, "number"); this.aperture.min = "0.7"; this.aperture.max = "64"; this.aperture.step = "0.1";
    this.focusDistance = input(15, "number"); this.focusDistance.min = "0.01"; this.focusDistance.step = "0.1";

    const controls = n("div", "arcz-render-grid");
    controls.append(
      field("Projeto/revisão", this.project), field("Resolução", this.preset), field("Formato", this.format),
      field("Qualidade", this.quality), field("Motor", this.engine), field("Amostras", this.samples),
      field("Dispositivo", this.device), field("Enhancement", this.mode), field("Nome", this.output),
      field("Seed", this.seed), field("Modelo local opcional", this.modelId), field("Geometry guard (px)", this.guard),
    );
    const cameraGrid = n("div", "arcz-render-grid arcz-render-camera-grid");
    cameraGrid.append(
      field("Preset de câmera", this.cameraPreset), field("Posição X,Y,Z (m)", this.position),
      field("Alvo X,Y,Z (m)", this.target), field("Lente (mm)", this.focal),
      field("Abertura (f/)", this.aperture), field("Distância de foco (m)", this.focusDistance),
    );

    this.worldMode = select([["nishita", "Céu físico Nishita"], ["studio", "Estúdio neutro"], ["transparent", "Fundo transparente"]]);
    this.sunElevation = input(25, "number"); this.sunElevation.min = "-10"; this.sunElevation.max = "90";
    this.sunRotation = input(-35, "number"); this.sunRotation.min = "-360"; this.sunRotation.max = "360";
    this.sunEnergy = input(3, "number"); this.sunEnergy.min = "0"; this.sunEnergy.max = "100"; this.sunEnergy.step = "0.1";
    this.haze = input(1, "number"); this.haze.min = "0"; this.haze.max = "10"; this.haze.step = "0.1";
    const environmentGrid = n("div", "arcz-render-grid");
    environmentGrid.append(
      field("Mundo", this.worldMode), field("Elevação solar (°)", this.sunElevation),
      field("Rotação solar (°)", this.sunRotation), field("Energia solar", this.sunEnergy),
      field("Névoa atmosférica", this.haze),
    );

    const passFieldset = n("fieldset", "arcz-render-passes");
    passFieldset.append(n("legend", "", "Passes estruturais"));
    this.passInputs = new Map();
    for (const [id, label] of Object.entries(PASS_LABELS)) {
      const item = n("label", "arcz-render-pass");
      const checkbox = n("input");
      checkbox.type = "checkbox";
      checkbox.checked = true;
      item.append(checkbox, n("span", "", label));
      passFieldset.append(item);
      this.passInputs.set(id, checkbox);
    }

    this.referenceSummary = n("div", "arcz-reference-summary");
    this.prompt = n("textarea", "arcz-textarea");
    this.prompt.placeholder = "Direção visual, materiais, iluminação, lente, qualidade e restrições arquitetônicas…";
    this.prompt.value = this.pendingPrompt.positive || "";
    this.negative = n("textarea", "arcz-textarea arcz-textarea--small");
    this.negative.placeholder = "Deformações, arquitetura alterada, textura derretida, aparência de IA, flicker…";
    this.negative.value = this.pendingPrompt.negative || "";

    const actions = n("div", "arcz-actions");
    this.preflightBtn = n("button", "arcz-button", "Executar preflight real");
    this.submitBtn = n("button", "arcz-button arcz-button--primary", "Gerar imagem");
    this.cancelBtn = n("button", "arcz-button", "Cancelar job");
    this.refreshJobsBtn = n("button", "arcz-button", "Histórico");
    for (const control of [this.preflightBtn, this.submitBtn, this.cancelBtn, this.refreshJobsBtn]) control.type = "button";
    this.submitBtn.disabled = true;
    this.cancelBtn.disabled = true;
    actions.append(this.preflightBtn, this.submitBtn, this.cancelBtn, this.refreshJobsBtn);

    this.status = n("div", "arcz-preflight");
    this.jobsHost = n("div", "arcz-render-history");
    this.root.append(
      title, controls, cameraGrid, environmentGrid, passFieldset, this.referenceSummary,
      field("Prompt", this.prompt), field("Negative prompt", this.negative),
      actions, this.status, this.jobsHost,
    );
    host.append(this.root);

    this.preflightBtn.addEventListener("click", () => { void this.runPreflight(); });
    this.submitBtn.addEventListener("click", () => { void this.submit(); });
    this.cancelBtn.addEventListener("click", () => { void this.cancelJob(); });
    this.refreshJobsBtn.addEventListener("click", () => { void this.refreshJobs(); });
    this.cameraPreset.addEventListener("change", () => this.applyCameraPreset());
    for (const control of [
      this.project, this.preset, this.format, this.quality, this.engine, this.samples, this.device,
      this.mode, this.output, this.seed, this.modelId, this.guard,
      this.position, this.target, this.focal, this.aperture, this.focusDistance,
      this.worldMode, this.sunElevation, this.sunRotation, this.sunEnergy, this.haze,
    ]) control.addEventListener("change", () => this.invalidatePreflight("Configuração alterada; execute o preflight novamente."));
    for (const control of [this.prompt, this.negative]) {
      control.addEventListener("input", () => this.invalidatePreflight("Prompt alterado; execute o preflight novamente."));
    }
    for (const checkbox of this.passInputs.values()) {
      checkbox.addEventListener("change", () => this.invalidatePreflight("Passes alterados; execute o preflight novamente."));
    }
    await this.refreshProjects();
    this.referencesChanged();
    await this.refreshJobs();
  }

  applyCameraPreset() {
    const presets = {
      exterior35: { position: "12, 8, 12", target: "0, 2, 0", focal: 35, aperture: 5.6, focus: 15 },
      interior24: { position: "4, 1.65, 4", target: "0, 1.5, 0", focal: 24, aperture: 4, focus: 5.5 },
      detail50: { position: "7, 3, 7", target: "0, 2.5, 0", focal: 50, aperture: 2.8, focus: 10 },
      aerial28: { position: "28, 24, 28", target: "0, 0, 0", focal: 28, aperture: 8, focus: 40 },
    };
    const value = presets[this.cameraPreset.value];
    if (!value) return;
    this.position.value = value.position;
    this.target.value = value.target;
    this.focal.value = String(value.focal);
    this.aperture.value = String(value.aperture);
    this.focusDistance.value = String(value.focus);
    this.invalidatePreflight("Preset de câmera alterado; execute o preflight novamente.");
  }

  setPrompt(value = {}) {
    this.pendingPrompt = { ...this.pendingPrompt, ...value };
    if (this.prompt && typeof value.positive === "string") this.prompt.value = value.positive;
    if (this.negative && typeof value.negative === "string") this.negative.value = value.negative;
    if (this.root) this.invalidatePreflight("Prompt global atualizado; execute o preflight novamente.");
  }

  referencesChanged() {
    if (!this.referenceSummary) return;
    const references = this.referencesProvider();
    this.referenceSummary.textContent = `${references.length} mídia(s) condicionante(s) selecionada(s) por hash local.`;
    if (this.root) this.invalidatePreflight("Mídias de referência alteradas; execute o preflight novamente.");
  }

  invalidatePreflight(message = "Preflight necessário.") {
    this.lastPreflight = null;
    if (this.submitBtn) this.submitBtn.disabled = true;
    if (this.status && !this.activeJobId) this.status.textContent = message;
  }

  async refreshProjects() {
    try {
      const rows = await this.floorplanner.listProjects();
      this.projects = new Map(rows.map(project => [project.id, project]));
      this.project.replaceChildren();
      for (const project of rows) {
        const option = n("option", "", `${project.name} · rev ${project.current_revision}`);
        option.value = project.id;
        option.disabled = Number(project.current_revision) < 1;
        this.project.append(option);
      }
      const active = this.estadoApp.obter().active_floorplanner_project_id;
      if (active && this.projects.has(active)) this.project.value = active;
      if (!rows.length) this.status.textContent = "Nenhum projeto Floorplanner. Abra o modo Floorplanner e salve ao menos uma revisão.";
    } catch (error) {
      this.status.textContent = `Não foi possível listar projetos: ${error.message}`;
    }
  }

  selectedPasses() {
    return [...this.passInputs].filter(([, inputValue]) => inputValue.checked).map(([id]) => id);
  }

  request() {
    const project = this.projects.get(this.project.value);
    const [width, height] = this.preset.value.split("x").map(Number);
    return buildPhotorealRequest({
      project,
      prompt: this.prompt.value,
      negativePrompt: this.negative.value,
      references: this.referencesProvider(),
      width,
      height,
      mode: this.mode.value,
      outputName: this.output.value,
      seed: Number(this.seed.value),
      format: this.format.value,
      passes: this.selectedPasses(),
      modelId: this.modelId.value.trim() || null,
      geometryGuardPx: Number(this.guard.value),
      generationEpoch: Number(this.estadoApp.obter().save_revision || 0),
      quality: this.quality.value,
      engine: this.engine.value,
      renderSettings: {
        samples: Number(this.samples.value),
        denoise: true,
        device: this.device.value,
        tile_size: 256,
        transparent_background: this.worldMode.value === "transparent",
        color_management: "AgX",
        look: "AgX - Medium High Contrast",
      },
      environment: {
        world_mode: this.worldMode.value,
        sun_elevation_deg: Number(this.sunElevation.value),
        sun_rotation_deg: Number(this.sunRotation.value),
        sun_energy: Number(this.sunEnergy.value),
        haze: Number(this.haze.value),
      },
      camera: {
        position: parseVector3(this.position.value, [12, 8, 12]),
        target: parseVector3(this.target.value, [0, 2, 0]),
        focal_length_mm: Number(this.focal.value),
        aperture: Number(this.aperture.value),
        focus_distance_m: Number(this.focusDistance.value),
      },
    });
  }

  renderPreflight(result) {
    this.lastPreflight = result;
    this.status.replaceChildren();
    const head = n("div", result.ready ? "arcz-ok" : "arcz-error", result.ready ? "Preflight aprovado" : "Render bloqueado");
    this.status.append(head);
    for (const item of result.blockers || []) {
      this.status.append(n("div", "arcz-alert arcz-alert--critical", `${item.code}${item.message ? ` · ${item.message}` : ""}`));
    }
    for (const item of result.warnings || []) {
      this.status.append(n("div", "arcz-alert arcz-alert--medium", `${item.code}${item.message ? ` · ${item.message}` : ""}`));
    }
    this.status.append(n("pre", "arcz-mode-details", JSON.stringify({
      scene: result.scene,
      model: result.model,
      blender: result.blender,
      reference_count: result.reference_count,
      estimate: result.estimate,
    }, null, 2)));
    this.submitBtn.disabled = !result.ready;
  }

  async runPreflight() {
    this.preflightBtn.disabled = true;
    this.submitBtn.disabled = true;
    this.status.textContent = "Validando revisão, passes, mídias, hashes, modelo, VRAM e worker Blender…";
    try { this.renderPreflight(await this.client.preflight(this.request())); }
    catch (error) { this.lastPreflight = null; this.status.textContent = `Falha: ${error.message}`; }
    finally { this.preflightBtn.disabled = false; }
  }

  renderJob(job) {
    this.status.replaceChildren();
    this.status.append(n("div", job.status === "COMPLETED" ? "arcz-ok" : job.status?.startsWith("FAILED") ? "arcz-error" : "", `Job ${job.id} · ${job.status}`));
    if (job.stage) this.status.append(n("div", "arcz-panel-state", `${job.stage} · ${Math.round(Number(job.progress || 0) * 100)}%${job.message ? ` · ${job.message}` : ""}`));
    if (job.error) this.status.append(n("pre", "arcz-mode-details", JSON.stringify(job.error, null, 2)));
    if (job.manifest_path) this.status.append(n("div", "arcz-panel-state", `Manifesto: ${job.manifest_path}`));
  }

  async refreshJobs() {
    try {
      const jobs = await this.client.listJobs(20);
      this.jobsHost.replaceChildren(n("h3", "", "Histórico real de renders"));
      for (const job of jobs) {
        const row = n("button", "arcz-render-history__job");
        row.type = "button";
        row.append(n("strong", "", `${job.kind} · ${job.status}`), n("span", "", `${job.id} · ${job.created_at || ""}`));
        row.addEventListener("click", () => this.renderJob(job));
        this.jobsHost.append(row);
      }
      if (!jobs.length) this.jobsHost.append(n("div", "arcz-panel-state", "Nenhum render foi submetido."));
    } catch (error) {
      this.jobsHost.replaceChildren(n("div", "arcz-panel-error", `Histórico indisponível: ${error.message}`));
    }
  }

  async submit() {
    if (!this.lastPreflight?.ready || this.activeJobId) return;
    this.submitBtn.disabled = true;
    this.preflightBtn.disabled = true;
    try {
      const job = await this.client.submit(this.request());
      this.activeJobId = job.id;
      this.cancelBtn.disabled = false;
      this.jobAbort = new AbortController();
      this.renderJob(job);
      const final = await this.client.waitJob(job.id, { signal: this.jobAbort.signal, onUpdate: value => this.renderJob(value) });
      this.renderJob(final);
      await this.refreshJobs();
    } catch (error) {
      if (error?.name !== "AbortError") this.status.textContent = `Falha: ${error.message}`;
    } finally {
      this.activeJobId = null;
      this.jobAbort = null;
      this.cancelBtn.disabled = true;
      this.preflightBtn.disabled = false;
      this.lastPreflight = null;
      this.submitBtn.disabled = true;
    }
  }

  async cancelJob() {
    if (!this.activeJobId) return;
    this.cancelBtn.disabled = true;
    try { this.renderJob(await this.client.cancelJob(this.activeJobId)); }
    catch (error) { this.status.textContent = `Falha ao cancelar: ${error.message}`; }
    finally { this.jobAbort?.abort(); }
  }

  async activate() {
    this.root.hidden = false;
    await this.refreshProjects();
    await this.refreshJobs();
  }
  async deactivate() { this.root.hidden = true; }
  async dispose() { this.jobAbort?.abort(); this.root.remove(); }
}
