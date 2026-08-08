import { PromptLibraryClient } from "./prompt-library-client.js";
import { extractInferenceText, parsePromptTags, slugifyPrompt } from "./prompt-library-model.js";

function n(tag, cls = "", text = "") {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text) node.textContent = text;
  return node;
}

function field(label, control) {
  const wrapper = n("label", "arcz-field");
  wrapper.append(n("span", "", label), control);
  return wrapper;
}

function textInput(placeholder = "") {
  const input = n("input", "arcz-input");
  input.type = "text";
  input.placeholder = placeholder;
  return input;
}

const LANGUAGES = [
  "pt-BR", "en", "es", "fr", "de", "it", "ja", "ko", "zh-CN", "zh-TW", "ru", "ar", "hi",
];

export class PromptLibraryPanel {
  constructor({ client = new PromptLibraryClient(), languageProvider = () => "pt-BR", onPromptChange = null } = {}) {
    this.client = client;
    this.languageProvider = languageProvider;
    this.onPromptChange = onPromptChange;
    this.rows = [];
    this.current = null;
    this.dirty = false;
  }

  async mount(host) {
    this.host = host;
    const stack = n("div", "arcz-stack arcz-prompt-library");

    const filters = n("div", "arcz-prompt-filters");
    this.search = textInput("Buscar título, slug ou tag…");
    this.categoryFilter = textInput("Categoria");
    this.languageFilter = textInput("Idioma");
    filters.append(this.search, this.categoryFilter, this.languageFilter);

    this.list = n("select", "arcz-select arcz-prompt-list");
    this.list.size = 6;
    this.list.setAttribute("aria-label", "Prompts disponíveis");

    const meta = n("div", "arcz-prompt-meta-grid");
    this.titleInput = textInput("Título");
    this.slugInput = textInput("slug-estavel");
    this.categoryInput = textInput("render, arquitetura, cinema…");
    this.purposeInput = textInput("Finalidade do prompt");
    this.languageInput = textInput("pt-BR");
    this.languageInput.setAttribute("list", "arcz_prompt_languages");
    const languageList = n("datalist");
    languageList.id = "arcz_prompt_languages";
    for (const language of LANGUAGES) {
      const option = n("option");
      option.value = language;
      languageList.append(option);
    }
    this.tagsInput = textInput("tags, separadas, por vírgula");
    meta.append(
      field("Título", this.titleInput),
      field("Slug", this.slugInput),
      field("Categoria", this.categoryInput),
      field("Finalidade", this.purposeInput),
      field("Idioma", this.languageInput),
      field("Tags", this.tagsInput),
      languageList,
    );

    this.editor = n("textarea", "arcz-textarea");
    this.editor.placeholder = "Prompt positivo versionado";
    this.negative = n("textarea", "arcz-textarea arcz-textarea--small");
    this.negative.placeholder = "Negative prompt";

    const management = n("div", "arcz-actions");
    this.newButton = n("button", "arcz-button", "Novo");
    this.saveButton = n("button", "arcz-button arcz-button--primary", "Salvar versão");
    this.copyButton = n("button", "arcz-button", "Salvar cópia");
    this.archiveButton = n("button", "arcz-button", "Arquivar");
    this.exportButton = n("button", "arcz-button", "Exportar bundle");
    this.importButton = n("button", "arcz-button", "Importar bundle");
    this.importInput = n("input");
    this.importInput.type = "file";
    this.importInput.accept = "application/json,.json";
    this.importInput.hidden = true;
    for (const item of [
      this.newButton, this.saveButton, this.copyButton, this.archiveButton,
      this.exportButton, this.importButton,
    ]) item.type = "button";
    management.append(
      this.newButton, this.saveButton, this.copyButton, this.archiveButton,
      this.exportButton, this.importButton, this.importInput,
    );

    const inference = n("div", "arcz-prompt-inference");
    this.targetLanguage = textInput(this.languageProvider());
    this.targetLanguage.value = this.languageProvider();
    this.targetLanguage.setAttribute("list", "arcz_prompt_languages");
    this.enhancePositive = n("button", "arcz-button", "Aprimorar positivo");
    this.enhanceNegative = n("button", "arcz-button", "Aprimorar negativo");
    this.translateBoth = n("button", "arcz-button", "Traduzir ambos");
    for (const item of [this.enhancePositive, this.enhanceNegative, this.translateBoth]) item.type = "button";
    inference.append(field("Idioma de destino", this.targetLanguage), this.enhancePositive, this.enhanceNegative, this.translateBoth);

    const versions = n("div", "arcz-prompt-versions");
    this.versionSelect = n("select", "arcz-select");
    this.versionSelect.setAttribute("aria-label", "Histórico de versões");
    this.versionInfo = n("span", "arcz-panel-state", "Sem histórico carregado");
    versions.append(this.versionSelect, this.versionInfo);

    this.status = n("div", "arcz-panel-state");
    stack.append(
      filters,
      this.list,
      meta,
      field("Prompt positivo", this.editor),
      field("Prompt negativo", this.negative),
      management,
      inference,
      versions,
      this.status,
    );
    host.append(stack);

    const reloadDebounced = () => {
      clearTimeout(this.timer);
      this.timer = setTimeout(() => { void this.load(); }, 240);
    };
    this.search.addEventListener("input", reloadDebounced);
    this.categoryFilter.addEventListener("input", reloadDebounced);
    this.languageFilter.addEventListener("input", reloadDebounced);
    this.list.addEventListener("change", () => {
      const row = this.rows.find(item => item.id === this.list.value);
      if (row) void this.apply(row);
    });
    for (const control of [
      this.titleInput, this.slugInput, this.categoryInput, this.purposeInput,
      this.languageInput, this.tagsInput, this.editor, this.negative,
    ]) {
      control.addEventListener("input", () => {
        this.dirty = true;
        this.emit();
        this.updateControls();
      });
    }
    this.titleInput.addEventListener("change", () => {
      if (!this.slugInput.value.trim() || !this.current) this.slugInput.value = slugifyPrompt(this.titleInput.value);
    });
    this.newButton.addEventListener("click", () => this.startNew());
    this.saveButton.addEventListener("click", () => { void this.save(); });
    this.copyButton.addEventListener("click", () => { void this.saveCopy(); });
    this.archiveButton.addEventListener("click", () => { void this.archive(); });
    this.exportButton.addEventListener("click", () => { void this.exportBundle(); });
    this.importButton.addEventListener("click", () => this.importInput.click());
    this.importInput.addEventListener("change", () => { void this.importBundle(); });
    this.enhancePositive.addEventListener("click", () => { void this.inferField("enhance", this.editor); });
    this.enhanceNegative.addEventListener("click", () => { void this.inferField("enhance", this.negative); });
    this.translateBoth.addEventListener("click", () => { void this.translateAll(); });
    this.versionSelect.addEventListener("change", () => this.loadSelectedVersion());

    await this.load();
  }

  async load({ selectId = null } = {}) {
    try {
      this.status.textContent = "Carregando biblioteca local…";
      this.rows = await this.client.list({
        query: this.search.value.trim(),
        category: this.categoryFilter.value.trim(),
        language: this.languageFilter.value.trim(),
      });
      this.list.replaceChildren();
      for (const row of this.rows) {
        const option = n("option", "", `${row.title} · ${row.language} · v${row.version}${row.builtin ? " · base" : ""}`);
        option.value = row.id;
        this.list.append(option);
      }
      const selected = this.rows.find(item => item.id === selectId)
        || this.rows.find(item => item.id === this.current?.id)
        || this.rows[0];
      if (selected) {
        this.list.value = selected.id;
        await this.apply(selected);
        this.status.textContent = `${this.rows.length} prompt(s) local(is).`;
      } else {
        this.startNew();
        this.status.textContent = "Nenhum prompt encontrado. Crie um template local.";
      }
    } catch (error) {
      this.status.textContent = `Falha: ${error.message}`;
    }
  }

  async apply(row, { loadVersions = true } = {}) {
    this.current = row;
    this.titleInput.value = row.title || "";
    this.slugInput.value = row.slug || "";
    this.categoryInput.value = row.category || "";
    this.purposeInput.value = row.purpose || "";
    this.languageInput.value = row.language || this.languageProvider();
    this.tagsInput.value = (row.tags || []).join(", ");
    this.editor.value = row.template || "";
    this.negative.value = row.negative_template || "";
    this.dirty = false;
    this.emit();
    this.updateControls();
    if (loadVersions) await this.loadVersions(row.id);
  }

  startNew() {
    this.current = null;
    this.titleInput.value = "Novo prompt";
    this.slugInput.value = `novo-prompt-${Date.now().toString(36)}`;
    this.categoryInput.value = "render";
    this.purposeInput.value = "render_photoreal";
    this.languageInput.value = this.languageProvider();
    this.tagsInput.value = "";
    this.editor.value = "";
    this.negative.value = "";
    this.versionSelect.replaceChildren();
    this.versionInfo.textContent = "Novo template ainda sem versões";
    this.dirty = true;
    this.emit();
    this.updateControls();
    this.editor.focus();
  }

  candidate() {
    const template = this.editor.value.trim();
    if (!template) throw new Error("O prompt positivo não pode ficar vazio");
    const title = this.titleInput.value.trim() || "Prompt sem título";
    return {
      ...(this.current || {}),
      schema_version: 1,
      id: this.current?.id || globalThis.crypto?.randomUUID?.() || `prompt-${Date.now()}`,
      slug: slugifyPrompt(this.slugInput.value || title),
      title,
      category: this.categoryInput.value.trim() || "general",
      purpose: this.purposeInput.value.trim() || "general",
      language: this.languageInput.value.trim() || this.languageProvider(),
      template,
      negative_template: this.negative.value.trim(),
      tags: parsePromptTags(this.tagsInput.value),
      variables: this.current?.variables || {},
      version: this.current?.version || 1,
      builtin: false,
      active: true,
      created_at: this.current?.created_at || new Date().toISOString(),
      updated_at: new Date().toISOString(),
      content_hash: this.current?.content_hash || "0".repeat(64),
    };
  }

  async save() {
    if (this.current?.builtin) {
      this.status.textContent = "Templates base são imutáveis. Use Salvar cópia.";
      return;
    }
    try {
      this.status.textContent = "Salvando versão no SQLite local…";
      const saved = await this.client.save(this.candidate());
      this.current = saved;
      this.dirty = false;
      await this.load({ selectId: saved.id });
      this.status.textContent = `Salvo como ${saved.slug} · versão ${saved.version}.`;
    } catch (error) {
      this.status.textContent = `Falha ao salvar: ${error.message}`;
    }
  }

  async saveCopy() {
    try {
      const candidate = this.candidate();
      candidate.id = globalThis.crypto?.randomUUID?.() || `prompt-${Date.now()}`;
      candidate.slug = slugifyPrompt(`${candidate.slug}-copy-${Date.now().toString(36)}`);
      candidate.title = `${candidate.title} · cópia`;
      candidate.version = 1;
      candidate.builtin = false;
      const saved = this.current
        ? await this.client.duplicate(this.current.id, candidate)
        : await this.client.save(candidate);
      await this.load({ selectId: saved.id });
      this.status.textContent = `Cópia independente criada: ${saved.slug}.`;
    } catch (error) {
      this.status.textContent = `Falha ao criar cópia: ${error.message}`;
    }
  }

  async exportBundle() {
    try {
      this.status.textContent = "Gerando bundle íntegro da biblioteca local…";
      const bundle = await this.client.exportBundle({ include_builtins: true, include_versions: true });
      const blob = new Blob([JSON.stringify(bundle, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `arcz-prompts-${new Date().toISOString().replace(/[:.]/g, "-")}.json`;
      anchor.click();
      URL.revokeObjectURL(url);
      this.status.textContent = `${bundle.prompt_count || 0} prompt(s) exportado(s) · ${bundle.bundle_hash?.slice(0, 12) || "sem hash"}.`;
    } catch (error) {
      this.status.textContent = `Falha ao exportar: ${error.message}`;
    }
  }

  async importBundle() {
    const file = this.importInput.files?.[0];
    if (!file) return;
    try {
      this.status.textContent = `Validando hash e importando ${file.name}…`;
      const bundle = JSON.parse(await file.text());
      const result = await this.client.importBundle(bundle, { conflict: "duplicate" });
      await this.load({ selectId: result.imported?.[0]?.id || null });
      this.status.textContent = [
        `${result.imported?.length || 0} importado(s)`,
        `${result.skipped?.length || 0} ignorado(s)`,
        `${result.errors?.length || 0} erro(s)`,
      ].join(" · ");
    } catch (error) {
      this.status.textContent = `Bundle recusado: ${error.message}`;
    } finally {
      this.importInput.value = "";
    }
  }

  async archive() {
    if (!this.current) return;
    if (this.current.builtin) {
      this.status.textContent = "Templates base não podem ser arquivados; crie uma cópia editável.";
      return;
    }
    if (!globalThis.confirm?.(`Arquivar “${this.current.title}”? O histórico permanecerá no banco local.`)) return;
    try {
      await this.client.archive(this.current.id);
      const archivedId = this.current.id;
      this.current = null;
      await this.load();
      this.status.textContent = `Prompt ${archivedId} arquivado sem apagar versões.`;
    } catch (error) {
      this.status.textContent = `Falha ao arquivar: ${error.message}`;
    }
  }

  async loadVersions(id) {
    try {
      this.versions = await this.client.versions(id);
      this.versionSelect.replaceChildren();
      for (const item of this.versions) {
        const option = n("option", "", `v${item.version} · ${item.created_at}`);
        option.value = String(item.version);
        this.versionSelect.append(option);
      }
      this.versionSelect.value = String(this.current?.version || this.versions[0]?.version || "");
      this.versionInfo.textContent = this.versions.length
        ? `${this.versions.length} versão(ões) preservada(s)`
        : "Sem histórico";
    } catch (error) {
      this.versionInfo.textContent = `Histórico indisponível: ${error.message}`;
    }
  }

  loadSelectedVersion() {
    const selected = this.versions?.find(item => String(item.version) === this.versionSelect.value);
    if (!selected?.snapshot) return;
    // Keep current identity so Save creates a new version rather than mutating
    // or replacing the historical snapshot selected for inspection.
    const identity = this.current;
    void this.apply({ ...selected.snapshot, id: identity.id, slug: identity.slug, builtin: identity.builtin }, { loadVersions: false });
    this.dirty = true;
    this.status.textContent = `Versão ${selected.version} carregada no editor. Salve para criar uma nova versão.`;
    this.updateControls();
  }

  emit() {
    this.onPromptChange?.({
      positive: this.editor?.value || "",
      negative: this.negative?.value || "",
      prompt_id: this.current?.id || null,
      language: this.languageInput?.value || this.languageProvider(),
      category: this.categoryInput?.value || "general",
    });
  }

  updateControls() {
    if (!this.saveButton) return;
    this.saveButton.disabled = Boolean(this.current?.builtin) || !this.editor.value.trim();
    this.archiveButton.disabled = !this.current || Boolean(this.current?.builtin);
    this.copyButton.disabled = !this.editor.value.trim();
  }

  async inferField(kind, control, target = null) {
    const text = control.value.trim();
    if (!text) return;
    this.status.textContent = "Executando modelo local…";
    try {
      const result = kind === "enhance"
        ? await this.client.enhance({
          text,
          language: this.languageInput.value || this.languageProvider(),
          purpose: this.purposeInput.value || "general",
        })
        : await this.client.translate({ text, target_language: target });
      control.value = extractInferenceText(result);
      this.dirty = true;
      this.emit();
      this.updateControls();
      this.status.textContent = `Concluído localmente${result?.model ? ` · ${result.model}` : ""}.`;
      return control.value;
    } catch (error) {
      this.status.textContent = `Inferência indisponível: ${error.message}`;
      return null;
    }
  }

  async translateAll() {
    const target = this.targetLanguage.value.trim();
    if (!target) {
      this.status.textContent = "Informe qualquer código/nome de idioma de destino.";
      return;
    }
    const positive = await this.inferField("translate", this.editor, target);
    if (positive !== null && this.negative.value.trim()) await this.inferField("translate", this.negative, target);
    this.languageInput.value = target;
    this.emit();
  }

  dispose() {
    clearTimeout(this.timer);
  }
}
