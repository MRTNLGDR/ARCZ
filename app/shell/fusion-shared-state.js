/** Small in-memory coordination bus for global panels and workspaces. */
export class FusionSharedState extends EventTarget {
  constructor() { super(); this.references = []; this.prompt = { positive: "", negative: "" }; }
  setReferences(values) {
    this.references = [...new Set((values || []).filter(value => /^[a-f0-9]{64}$/.test(String(value))))];
    this.dispatchEvent(new CustomEvent("references", { detail: this.references }));
  }
  setPrompt(value) {
    this.prompt = { ...this.prompt, ...(value || {}) };
    this.dispatchEvent(new CustomEvent("prompt", { detail: this.prompt }));
  }
}
