// ARCZ Core · Event bus local, síncrono e descartável.
export class EventBus {
  constructor({ onError = (erro, contexto) => console.error("[ARCZ/eventos]", contexto, erro) } = {}) {
    this._listeners = new Map();
    this._onError = onError;
  }

  on(tipo, listener, { signal } = {}) {
    if (typeof tipo !== "string" || !tipo) throw new TypeError("tipo de evento obrigatório");
    if (typeof listener !== "function") throw new TypeError("listener precisa ser função");
    if (signal?.aborted) return () => {};
    const conjunto = this._listeners.get(tipo) || new Set();
    conjunto.add(listener);
    this._listeners.set(tipo, conjunto);
    const off = () => {
      conjunto.delete(listener);
      if (conjunto.size === 0) this._listeners.delete(tipo);
    };
    signal?.addEventListener("abort", off, { once: true });
    return off;
  }

  once(tipo, listener, opcoes = {}) {
    let off = () => {};
    off = this.on(tipo, payload => {
      off();
      listener(payload);
    }, opcoes);
    return off;
  }

  emit(tipo, payload) {
    const listeners = [...(this._listeners.get(tipo) || [])];
    for (const listener of listeners) {
      try {
        listener(payload);
      } catch (erro) {
        this._onError(erro, { tipo, payload });
      }
    }
    return listeners.length;
  }

  clear(tipo = null) {
    if (tipo === null) this._listeners.clear();
    else this._listeners.delete(tipo);
  }

  count(tipo = null) {
    if (tipo !== null) return this._listeners.get(tipo)?.size || 0;
    let total = 0;
    for (const listeners of this._listeners.values()) total += listeners.size;
    return total;
  }
}

export const eventosApp = new EventBus();
