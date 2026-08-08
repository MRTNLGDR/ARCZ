function el(tag, className = "", text = "") {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  return node;
}

export function clampPanelWidth(value, fallback = 360) {
  const number = Number(value);
  if (!Number.isFinite(number)) return fallback;
  return Math.min(720, Math.max(240, Math.round(number)));
}

/** Pure helper used by the dock and by keyboard regression tests. */
export function nextPanelTabIndex(current, key, length) {
  if (!Number.isInteger(length) || length <= 0) return -1;
  const safeCurrent = Number.isInteger(current) && current >= 0 && current < length ? current : 0;
  if (key === "Home") return 0;
  if (key === "End") return length - 1;
  if (["ArrowDown", "ArrowRight"].includes(key)) return (safeCurrent + 1) % length;
  if (["ArrowUp", "ArrowLeft"].includes(key)) return (safeCurrent - 1 + length) % length;
  return safeCurrent;
}

export function panelHoverIsAvailable(matchMedia = globalThis.matchMedia) {
  if (typeof matchMedia !== "function") return true;
  return !matchMedia("(hover: none), (pointer: coarse)").matches;
}

function eventInside(node, event) {
  if (!node || !event) return false;
  const path = event.composedPath?.();
  if (Array.isArray(path) && path.includes(node)) return true;
  return node.contains?.(event.target) === true;
}

/**
 * Global panel dock.
 *
 * Invariants:
 * - collapsed by default;
 * - hover only on fine pointers; touch requires an explicit tap;
 * - pin/open/width/active tab persist through estadoApp;
 * - each panel mounts once and keeps its live state while hidden;
 * - tabs use the WAI-ARIA tablist pattern with roving tabindex;
 * - closed content is aria-hidden and inert, so it cannot steal focus;
 * - listeners/timers are always removed by dispose().
 */
export class CollapsiblePanelDock {
  constructor({ estadoApp, id = "global", side = "right", hoverDelayMs = 140 } = {}) {
    if (!estadoApp) throw new Error("estadoApp obrigatório");
    this.estadoApp = estadoApp;
    this.id = id;
    this.side = side === "left" ? "left" : "right";
    this.hoverDelayMs = Math.max(0, Number(hoverDelayMs) || 0);
    this.entries = new Map();
    this.activeId = null;
    this.opened = false;
    this.pinned = false;
    this.width = 360;
    this.hoverTimer = null;
    this.hoverEnabled = true;
    this.cleanups = [];
    this.root = null;
    this.rail = null;
    this.panel = null;
    this.content = null;
  }

  register(entry) {
    if (!entry?.id || typeof entry.mount !== "function") throw new Error("Painel inválido");
    if (this.entries.has(entry.id)) throw new Error(`Painel duplicado: ${entry.id}`);
    this.entries.set(entry.id, { ...entry, mounted: false, host: null, button: null });
    return entry;
  }

  _saved() {
    return this.estadoApp.obter()?.panel_layout?.panels?.[this.id] || {};
  }

  _persist() {
    const state = this.estadoApp.obter();
    const panels = {
      ...(state.panel_layout?.panels || {}),
      [this.id]: {
        active_id: this.activeId,
        pinned: this.pinned,
        collapsed: !this.opened,
        side: this.side,
        width: clampPanelWidth(this.width),
      },
    };
    this.estadoApp.atualizar({ panel_layout: { schema_version: 1, panels } }, "panel_layout");
  }

  _listen(target, type, listener, options) {
    if (!target?.addEventListener) return;
    target.addEventListener(type, listener, options);
    this.cleanups.push(() => target.removeEventListener(type, listener, options));
  }

  mount(parent = document.body) {
    if (this.root) throw new Error(`Dock ${this.id} já foi montado`);
    const saved = this._saved();
    this.activeId = this.entries.has(saved.active_id)
      ? saved.active_id
      : (this.entries.keys().next().value || null);
    this.pinned = Boolean(saved.pinned);
    this.opened = this.pinned && saved.collapsed !== true;
    this.width = clampPanelWidth(saved.width);

    this.root = el("section", `arcz-panel-dock arcz-panel-dock--${this.side}`);
    this.root.dataset.panelDock = this.id;
    this.root.setAttribute("aria-label", "Ferramentas globais");
    this.root.style.setProperty("--arcz-panel-width", `${this.width}px`);

    this.rail = el("nav", "arcz-panel-dock__rail");
    this.rail.setAttribute("aria-label", "Selecionar ferramenta global");
    this.rail.setAttribute("role", "tablist");
    this.rail.setAttribute("aria-orientation", "vertical");

    this.panel = el("aside", "arcz-panel-dock__panel");
    this.panel.id = `arcz-panel-dock-${this.id}`;
    this.panel.setAttribute("aria-label", "Painel de ferramenta global");
    this.panel.setAttribute("aria-hidden", "true");

    this.resizer = el("div", "arcz-panel-dock__resizer");
    this.resizer.tabIndex = -1;
    this.resizer.setAttribute("role", "separator");
    this.resizer.setAttribute("aria-orientation", "vertical");
    this.resizer.setAttribute("aria-label", "Redimensionar painel global");

    const header = el("header", "arcz-panel-dock__header");
    this.title = el("strong", "arcz-panel-dock__title");
    this.pinButton = el("button", "arcz-panel-dock__pin", "Fixar");
    this.pinButton.type = "button";
    this.pinButton.setAttribute("aria-pressed", "false");
    this.closeButton = el("button", "arcz-panel-dock__close", "Recolher");
    this.closeButton.type = "button";
    header.append(this.title, this.pinButton, this.closeButton);
    this.content = el("div", "arcz-panel-dock__content");
    this.panel.append(this.resizer, header, this.content);
    this.root.append(this.rail, this.panel);
    parent.appendChild(this.root);

    for (const entry of this.entries.values()) {
      const tab = el("button", "arcz-panel-dock__tab", entry.shortLabel || entry.label || entry.id);
      const safeId = String(entry.id).replace(/[^a-zA-Z0-9_-]/g, "-");
      const paneId = `arcz-panel-pane-${this.id}-${safeId}`;
      tab.id = `arcz-panel-tab-${this.id}-${safeId}`;
      tab.type = "button";
      tab.dataset.panelId = entry.id;
      tab.title = entry.description || entry.label || entry.id;
      tab.setAttribute("role", "tab");
      tab.setAttribute("aria-label", entry.label || entry.id);
      tab.setAttribute("aria-controls", paneId);
      tab.setAttribute("aria-selected", "false");
      tab.setAttribute("aria-expanded", "false");
      tab.tabIndex = -1;
      this._listen(tab, "click", () => {
        void this.select(entry.id);
        this.setOpen(true, { persist: true });
      });
      this._listen(tab, "focus", () => {
        void this.select(entry.id);
        this.setOpen(true);
      });
      this.rail.appendChild(tab);
      entry.button = tab;

      entry.host = el("section", "arcz-panel-dock__pane");
      entry.host.id = paneId;
      entry.host.dataset.panelPane = entry.id;
      entry.host.hidden = true;
      entry.host.setAttribute("role", "tabpanel");
      entry.host.setAttribute("aria-labelledby", tab.id);
      entry.host.setAttribute("aria-label", entry.label || entry.id);
      entry.host.tabIndex = 0;
      this.content.appendChild(entry.host);
    }

    this._listen(this.pinButton, "click", () => {
      this.pinned = !this.pinned;
      this.setOpen(true);
      this._sync();
      this._persist();
    });
    this._listen(this.closeButton, "click", () => {
      this.pinned = false;
      this.setOpen(false, { persist: true, restoreFocus: true });
    });
    this._listen(this.resizer, "pointerdown", event => this._beginResize(event));
    this._listen(this.resizer, "keydown", event => this._resizeByKeyboard(event));
    this._listen(this.rail, "keydown", event => this._moveTabFocus(event));

    const pointerMedia = globalThis.matchMedia?.("(hover: none), (pointer: coarse)") || null;
    const refreshPointerMode = () => {
      this.hoverEnabled = panelHoverIsAvailable(globalThis.matchMedia);
      this.root?.classList.toggle("is-touch", !this.hoverEnabled);
    };
    refreshPointerMode();
    if (pointerMedia?.addEventListener) {
      pointerMedia.addEventListener("change", refreshPointerMode);
      this.cleanups.push(() => pointerMedia.removeEventListener("change", refreshPointerMode));
    }

    this._listen(this.root, "mouseenter", () => {
      if (!this.hoverEnabled) return;
      clearTimeout(this.hoverTimer);
      this.hoverTimer = setTimeout(() => this.setOpen(true), this.hoverDelayMs);
    });
    this._listen(this.root, "mouseleave", () => {
      if (!this.hoverEnabled) return;
      clearTimeout(this.hoverTimer);
      if (!this.pinned && !this.root.matches(":focus-within")) this.setOpen(false);
    });
    this._listen(this.root, "focusout", event => {
      if (!this.pinned && !this.root.contains(event.relatedTarget)) this.setOpen(false);
    });
    this._listen(this.root, "keydown", event => {
      if (event.key === "Escape") {
        event.preventDefault();
        this.pinned = false;
        this.setOpen(false, { persist: true, restoreFocus: true });
      }
    });
    this._listen(document, "pointerdown", event => {
      if (!this.opened || this.pinned || eventInside(this.root, event)) return;
      this.setOpen(false);
    }, { capture: true });

    if (this.activeId) void this.select(this.activeId);
    this._sync();
    return this;
  }

  _moveTabFocus(event) {
    if (!["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    const buttons = [...this.entries.values()].map(entry => entry.button).filter(Boolean);
    if (!buttons.length) return;
    const current = buttons.indexOf(document.activeElement);
    const next = nextPanelTabIndex(current, event.key, buttons.length);
    if (next < 0) return;
    event.preventDefault();
    buttons[next].focus();
  }

  _setWidth(width, { persist = false } = {}) {
    this.width = clampPanelWidth(width, this.width);
    this.root?.style.setProperty("--arcz-panel-width", `${this.width}px`);
    this.resizer?.setAttribute("aria-valuemin", "240");
    this.resizer?.setAttribute("aria-valuemax", "720");
    this.resizer?.setAttribute("aria-valuenow", String(this.width));
    if (persist) this._persist();
  }

  _beginResize(event) {
    if (event.button !== 0) return;
    event.preventDefault();
    this.setOpen(true);
    this.pinned = true;
    this.resizer.setPointerCapture?.(event.pointerId);
    const startX = event.clientX;
    const startWidth = this.width;
    const direction = this.side === "right" ? -1 : 1;
    const move = current => this._setWidth(startWidth + (current.clientX - startX) * direction);
    const end = current => {
      move(current);
      this.resizer.releasePointerCapture?.(event.pointerId);
      this.resizer.removeEventListener("pointermove", move);
      this.resizer.removeEventListener("pointerup", end);
      this.resizer.removeEventListener("pointercancel", end);
      this._sync();
      this._persist();
    };
    this.resizer.addEventListener("pointermove", move);
    this.resizer.addEventListener("pointerup", end);
    this.resizer.addEventListener("pointercancel", end);
  }

  _resizeByKeyboard(event) {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    this.pinned = true;
    this.setOpen(true);
    const physical = event.key === "ArrowLeft" ? -24 : event.key === "ArrowRight" ? 24 : 0;
    const delta = physical * (this.side === "right" ? -1 : 1);
    this._setWidth(event.key === "Home" ? 240 : event.key === "End" ? 720 : this.width + delta, { persist: true });
    this._sync();
  }

  async select(id) {
    const entry = this.entries.get(id);
    if (!entry) throw new Error(`Painel desconhecido: ${id}`);
    this.activeId = id;
    this.title.textContent = entry.label || id;
    for (const candidate of this.entries.values()) candidate.host.hidden = candidate.id !== id;
    if (!entry.mounted) {
      const loading = el("div", "arcz-panel-state", "Carregando…");
      entry.host.replaceChildren(loading);
      try {
        await entry.mount(entry.host);
        loading.remove();
        entry.mounted = true;
      } catch (error) {
        entry.host.replaceChildren(el("div", "arcz-panel-error", error?.message || String(error)));
      }
    }
    this._sync();
    this._persist();
  }

  setOpen(value, { persist = false, restoreFocus = false } = {}) {
    this.opened = Boolean(value);
    this._sync();
    if (!this.opened && restoreFocus) this.entries.get(this.activeId)?.button?.focus();
    if (persist) this._persist();
  }

  _sync() {
    if (!this.root) return;
    this._setWidth(this.width);
    this.root.classList.toggle("is-open", this.opened);
    this.root.classList.toggle("is-pinned", this.pinned);
    this.root.setAttribute("data-open", this.opened ? "true" : "false");
    this.panel.setAttribute("aria-hidden", this.opened ? "false" : "true");
    this.panel.inert = !this.opened;
    this.resizer.tabIndex = this.opened ? 0 : -1;
    this.pinButton.textContent = this.pinned ? "Desafixar" : "Fixar";
    this.pinButton.setAttribute("aria-pressed", this.pinned ? "true" : "false");
    for (const [id, entry] of this.entries) {
      const selected = id === this.activeId;
      entry.button?.setAttribute("aria-selected", selected ? "true" : "false");
      entry.button?.setAttribute("aria-pressed", selected ? "true" : "false");
      entry.button?.setAttribute("aria-expanded", selected && this.opened ? "true" : "false");
      if (entry.button) entry.button.tabIndex = selected ? 0 : -1;
      entry.host?.setAttribute("aria-hidden", selected && this.opened ? "false" : "true");
    }
  }

  async dispose() {
    clearTimeout(this.hoverTimer);
    for (const cleanup of this.cleanups.splice(0)) cleanup();
    for (const entry of this.entries.values()) if (entry.mounted) await entry.dispose?.();
    this.root?.remove();
    this.root = null;
  }
}
