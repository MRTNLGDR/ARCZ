// Controlador de autocomplete local. Não consulta provedor diretamente.
export class RegionSelector {
  constructor({ controller, input, results, minChars = 4, debounceMs = 350, scaleProvider = () => "endereco" }) {
    if (!controller || !input || !results) throw new Error("RegionSelector exige controller, input e results");
    this.controller = controller; this.input = input; this.results = results;
    this.minChars = minChars; this.debounceMs = debounceMs; this.scaleProvider = scaleProvider;
    this.timer = null; this.abort = null; this.onSelect = null;
    this._boundInput = () => this.schedule();
    input.addEventListener("input", this._boundInput);
  }

  schedule() {
    clearTimeout(this.timer);
    this.abort?.abort("new_query");
    const q = this.input.value.trim();
    if (q.length < this.minChars) { this.render([]); return; }
    this.timer = setTimeout(() => this.search(q), this.debounceMs);
  }

  async search(q) {
    this.abort = new AbortController();
    this.results.setAttribute("aria-busy", "true");
    try { this.render(await this.controller.resolve(q, { scale: this.scaleProvider(), signal: this.abort.signal })); }
    catch (erro) { if (erro?.name !== "AbortError") this.renderError(erro); }
    finally { this.results.removeAttribute("aria-busy"); }
  }

  render(items) {
    this.results.replaceChildren();
    for (const item of items) {
      const button = document.createElement("button");
      button.type = "button"; button.className = "region-result";
      button.textContent = item.display_name || item.name || item.id;
      button.addEventListener("click", () => this.onSelect?.(item));
      this.results.appendChild(button);
    }
  }

  renderError(error) {
    this.results.replaceChildren();
    const el = document.createElement("div"); el.className = "region-error";
    el.textContent = error?.code === "DATASET_NOT_INSTALLED"
      ? "Índice geográfico local não instalado. Importe um pacote local na área de dados."
      : String(error?.message || error);
    this.results.appendChild(el);
  }

  dispose() {
    clearTimeout(this.timer); this.abort?.abort("disposed");
    this.input.removeEventListener("input", this._boundInput); this.results.replaceChildren();
  }
}
