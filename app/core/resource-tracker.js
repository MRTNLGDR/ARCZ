// ARCZ Core · Rastreador de recursos criado por plugin/módulo.
export class ResourceTracker {
  constructor(nome) {
    this.nome = nome;
    this._disposers = new Set();
    this._closed = false;
    this._counters = { timers: 0, listeners: 0, subscriptions: 0, primitives: 0, custom: 0 };
  }

  _guard() {
    if (this._closed) throw new Error(`ResourceTracker fechado: ${this.nome}`);
  }

  add(disposer, tipo = "custom") {
    this._guard();
    if (typeof disposer !== "function") throw new TypeError("disposer precisa ser função");
    const registro = { disposer, tipo, ativo: true };
    this._disposers.add(registro);
    this._counters[tipo] = (this._counters[tipo] || 0) + 1;
    return () => this.release(registro);
  }

  release(registro) {
    if (!registro?.ativo) return false;
    registro.ativo = false;
    this._disposers.delete(registro);
    this._counters[registro.tipo] = Math.max(0, (this._counters[registro.tipo] || 1) - 1);
    try { registro.disposer(); } catch (erro) {
      console.error(`[ARCZ/resources:${this.nome}] falha ao descartar`, erro);
    }
    return true;
  }

  listen(target, type, listener, options) {
    this._guard();
    target.addEventListener(type, listener, options);
    return this.add(() => target.removeEventListener(type, listener, options), "listeners");
  }

  interval(fn, ms) {
    this._guard();
    const id = setInterval(fn, ms);
    return this.add(() => clearInterval(id), "timers");
  }

  timeout(fn, ms) {
    this._guard();
    let release = () => {};
    const id = setTimeout(() => { release(); fn(); }, ms);
    release = this.add(() => clearTimeout(id), "timers");
    return release;
  }

  subscription(unsubscribe) {
    return this.add(unsubscribe, "subscriptions");
  }

  primitive(collection, primitive) {
    this._guard();
    if (!collection || typeof collection.remove !== "function") {
      throw new TypeError("coleção de primitive inválida");
    }
    return this.add(() => {
      if (!primitive?.isDestroyed?.()) collection.remove(primitive);
    }, "primitives");
  }

  snapshot() {
    return { nome: this.nome, fechado: this._closed, total: this._disposers.size, ...this._counters };
  }

  disposeAll() {
    if (this._closed) return this.snapshot();
    const itens = [...this._disposers].reverse();
    for (const item of itens) this.release(item);
    this._closed = true;
    return this.snapshot();
  }
}
