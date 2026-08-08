import { ReferenceMediaClient } from "./reference-media-client.js";
import { normalizeReferenceRoles, previewKind, REFERENCE_ROLES } from "./reference-media-model.js";

function node(tag, cls = "", text = "") {
  const value = document.createElement(tag);
  if (cls) value.className = cls;
  if (text) value.textContent = text;
  return value;
}

function formatBytes(bytes) {
  const size = Number(bytes || 0);
  if (size < 1024) return `${size} B`;
  if (size < 1024 ** 2) return `${(size / 1024).toFixed(1)} KB`;
  if (size < 1024 ** 3) return `${(size / 1024 ** 2).toFixed(1)} MB`;
  return `${(size / 1024 ** 3).toFixed(2)} GB`;
}

function field(label, control) {
  const wrapper = node("label", "arcz-field");
  wrapper.append(node("span", "", label), control);
  return wrapper;
}

export class ReferenceMediaPanel {
  constructor({ client = new ReferenceMediaClient(), onSelectionChange = null } = {}) {
    this.client = client;
    this.onSelectionChange = onSelectionChange;
    this.selected = new Set();
    this.items = [];
    this.current = null;
  }

  async mount(host) {
    this.host = host;
    const stack = node("div", "arcz-stack arcz-reference-library");
    const form = node("form", "arcz-stack arcz-media-upload");
    this.fileInput = node("input");
    this.fileInput.type = "file";
    this.fileInput.multiple = true;
    this.fileInput.accept = [
      "image/*", "video/*", "audio/*", ".pdf", ".json", ".geojson", ".csv", ".txt", ".md",
      ".kml", ".kmz", ".ies", ".glb", ".gltf", ".obj", ".fbx", ".stl", ".ply", ".blend",
      ".ifc", ".dxf", ".dwg", ".las", ".laz", ".exr", ".hdr", ".avif", ".heic", ".heif",
    ].join(",");
    this.uploadRole = node("select", "arcz-select");
    for (const role of REFERENCE_ROLES) {
      const option = node("option", "", role);
      option.value = role;
      this.uploadRole.append(option);
    }
    const uploadButton = node("button", "arcz-button arcz-button--primary", "Importar mídia real");
    uploadButton.type = "submit";
    this.status = node("div", "arcz-panel-state");
    form.append(this.fileInput, field("Papel inicial", this.uploadRole), uploadButton, this.status);

    const filterRow = node("div", "arcz-media-filter-row");
    this.categoryFilter = node("select", "arcz-select");
    for (const [value, label] of [
      ["", "Todas"], ["image", "Imagens"], ["video", "Vídeos"], ["audio", "Áudio"],
      ["document", "Documentos"], ["model3d", "Modelos 3D"], ["bim", "BIM / IFC"],
      ["cad", "CAD"], ["pointcloud", "Nuvens de pontos"], ["geodata", "Geodados"],
      ["dataset", "Datasets"], ["lighting", "Iluminação / IES"],
    ]) {
      const option = node("option", "", label);
      option.value = value;
      this.categoryFilter.append(option);
    }
    const refreshButton = node("button", "arcz-button", "Atualizar");
    refreshButton.type = "button";
    filterRow.append(this.categoryFilter, refreshButton);

    this.listHost = node("div", "arcz-list arcz-media-list");
    this.previewHost = node("div", "arcz-media-preview");
    this.detailsHost = node("div", "arcz-media-details");
    stack.append(form, filterRow, this.listHost, this.previewHost, this.detailsHost);
    host.append(stack);

    form.addEventListener("submit", async event => {
      event.preventDefault();
      const files = [...this.fileInput.files];
      if (!files.length) return;
      uploadButton.disabled = true;
      try {
        for (const file of files) {
          this.status.textContent = `Validando bytes, formato e hash de ${file.name}…`;
          await this.client.upload(file, { roles: [this.uploadRole.value || "reference"] });
        }
        this.fileInput.value = "";
        await this.refresh();
        this.status.textContent = `${files.length} arquivo(s) importado(s) no content store local.`;
      } catch (error) {
        this.status.textContent = `Falha: ${error.message}`;
      } finally {
        uploadButton.disabled = false;
      }
    });
    this.categoryFilter.addEventListener("change", () => { void this.refresh(); });
    refreshButton.addEventListener("click", () => { void this.refresh(); });
    await this.refresh();
  }

  async refresh({ currentId = null } = {}) {
    try {
      this.items = await this.client.list(this.categoryFilter?.value || null);
      this.listHost.replaceChildren();
      for (const item of this.items) {
        const row = node("div", "arcz-media-row");
        const check = node("input");
        check.type = "checkbox";
        check.checked = this.selected.has(item.content_hash);
        check.setAttribute("aria-label", `Usar ${item.original_name} como referência`);
        check.addEventListener("change", () => {
          check.checked ? this.selected.add(item.content_hash) : this.selected.delete(item.content_hash);
          this.onSelectionChange?.([...this.selected]);
        });
        const inspect = node("button", "arcz-media-row__open");
        inspect.type = "button";
        inspect.append(
          node("strong", "", item.original_name),
          node("span", "", `${item.category} · ${formatBytes(item.bytes)} · ${(item.roles || []).join(", ")}`),
        );
        inspect.addEventListener("click", () => { void this.inspect(item.id); });
        row.append(check, inspect);
        this.listHost.append(row);
      }
      if (!this.items.length) {
        this.listHost.append(node("div", "arcz-panel-state", "Nenhuma mídia local importada."));
        this.previewHost.replaceChildren();
        this.detailsHost.replaceChildren();
        return;
      }
      const target = currentId || this.current?.id || this.items[0].id;
      await this.inspect(target);
    } catch (error) {
      this.listHost.replaceChildren(node("div", "arcz-panel-error", error.message));
    }
  }

  async inspect(id) {
    try {
      const item = await this.client.get(id);
      this.current = item;
      this.renderPreview(item);
      this.renderDetails(item);
    } catch (error) {
      this.previewHost.replaceChildren(node("div", "arcz-panel-error", error.message));
    }
  }

  renderPreview(item) {
    this.previewHost.replaceChildren();
    const title = node("div", "arcz-media-preview__title");
    title.append(node("strong", "", item.original_name), node("span", "", item.integrity?.ok ? "hash íntegro" : "integridade inválida"));
    this.previewHost.append(title);
    if (!item.integrity?.ok) {
      this.previewHost.append(node("div", "arcz-panel-error", item.integrity?.error || "Arquivo corrompido"));
      return;
    }
    const kind = previewKind(item);
    let preview;
    if (kind === "image") {
      preview = node("img", "arcz-media-preview__asset");
      preview.alt = item.original_name;
      preview.loading = "lazy";
      preview.src = item.content_url;
    } else if (kind === "video") {
      preview = node("video", "arcz-media-preview__asset");
      preview.controls = true;
      preview.preload = "metadata";
      preview.src = item.content_url;
    } else if (kind === "audio") {
      preview = node("audio", "arcz-media-preview__audio");
      preview.controls = true;
      preview.preload = "metadata";
      preview.src = item.content_url;
    } else if (kind === "pdf") {
      preview = node("iframe", "arcz-media-preview__document");
      preview.title = `Prévia de ${item.original_name}`;
      preview.loading = "lazy";
      preview.src = item.content_url;
    } else {
      preview = node("div", "arcz-media-preview__fallback");
      preview.append(
        node("strong", "", `${item.category.toUpperCase()} · ${item.mime}`),
        node("span", "", "Prévia visual nativa não disponível; o arquivo real permanece utilizável pelos workers locais."),
      );
    }
    this.previewHost.append(preview);
    const open = node("a", "arcz-media-open", "Abrir arquivo local");
    open.href = item.content_url;
    open.target = "_blank";
    open.rel = "noopener";
    this.previewHost.append(open);
  }

  renderDetails(item) {
    this.detailsHost.replaceChildren();
    const roles = node("fieldset", "arcz-media-roles");
    roles.append(node("legend", "", "Papéis no condicionamento"));
    const selectedRoles = new Set(normalizeReferenceRoles(item.roles));
    this.roleInputs = [];
    for (const role of REFERENCE_ROLES) {
      const label = node("label", "arcz-media-role");
      const input = node("input");
      input.type = "checkbox";
      input.value = role;
      input.checked = selectedRoles.has(role);
      label.append(input, node("span", "", role));
      roles.append(label);
      this.roleInputs.push(input);
    }
    this.weightInput = node("input", "arcz-input");
    this.weightInput.type = "number";
    this.weightInput.min = "0";
    this.weightInput.max = "2";
    this.weightInput.step = "0.05";
    this.weightInput.value = String(Number(item.metadata?.weight ?? 1));
    this.notesInput = node("textarea", "arcz-textarea arcz-textarea--small");
    this.notesInput.placeholder = "Como esta referência deve influenciar imagem, material, câmera ou geometria?";
    this.notesInput.value = String(item.metadata?.notes || "");
    const provenance = node("pre", "arcz-mode-details", JSON.stringify({
      hash: item.content_hash,
      dimensions: item.width && item.height ? `${item.width}×${item.height}` : null,
      license: item.license,
      provenance: item.provenance,
    }, null, 2));
    const save = node("button", "arcz-button arcz-button--primary", "Salvar metadados");
    save.type = "button";
    save.addEventListener("click", () => { void this.saveMetadata(); });
    this.detailsStatus = node("div", "arcz-panel-state");
    this.detailsHost.append(
      roles,
      field("Peso da referência (0–2)", this.weightInput),
      field("Notas", this.notesInput),
      save,
      this.detailsStatus,
      provenance,
    );
  }

  async saveMetadata() {
    if (!this.current) return;
    const roles = normalizeReferenceRoles(this.roleInputs.filter(input => input.checked).map(input => input.value));
    const weight = Number(this.weightInput.value);
    if (!Number.isFinite(weight) || weight < 0 || weight > 2) {
      this.detailsStatus.textContent = "Peso precisa estar entre 0 e 2.";
      return;
    }
    try {
      this.detailsStatus.textContent = "Persistindo metadados…";
      const updated = await this.client.updateMetadata(this.current.id, {
        roles,
        license: this.current.license,
        metadata: { ...this.current.metadata, weight, notes: this.notesInput.value.trim() },
      });
      this.current = updated;
      await this.refresh({ currentId: updated.id });
      this.detailsStatus.textContent = "Metadados versionáveis atualizados no registro local.";
    } catch (error) {
      this.detailsStatus.textContent = `Falha: ${error.message}`;
    }
  }

  getSelected() {
    return [...this.selected];
  }
}
