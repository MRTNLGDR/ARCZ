// ARCZ Core · Transação assíncrona com rollback LIFO.
export class TransacaoAbortada extends Error {
  constructor(message = "Transação abortada") { super(message); this.name = "TransacaoAbortada"; }
}

export class Transacao {
  constructor(nome, { signal = null, telemetry = null } = {}) {
    this.nome = nome;
    this.signal = signal;
    this.telemetry = telemetry;
    this.estado = "OPEN";
    this._rollbacks = [];
    this._commits = [];
    this._context = new Map();
  }

  verificarAbortada() {
    if (this.signal?.aborted) throw new TransacaoAbortada(String(this.signal.reason || "AbortSignal"));
    if (this.estado !== "OPEN") throw new Error(`Transação ${this.nome} não está aberta (${this.estado})`);
  }

  set(chave, valor) { this.verificarAbortada(); this._context.set(chave, valor); return valor; }
  get(chave) { return this._context.get(chave); }
  onRollback(fn) { this.verificarAbortada(); if (typeof fn !== "function") throw new TypeError("rollback inválido"); this._rollbacks.push(fn); }
  onCommit(fn) { this.verificarAbortada(); if (typeof fn !== "function") throw new TypeError("commit inválido"); this._commits.push(fn); }

  async etapa(nome, executar, desfazer = null) {
    this.verificarAbortada();
    this.telemetry?.event?.("transaction.stage.start", { transaction: this.nome, stage: nome });
    const valor = await executar(this);
    if (desfazer) this.onRollback(() => desfazer(valor, this));
    this.telemetry?.event?.("transaction.stage.done", { transaction: this.nome, stage: nome });
    return valor;
  }

  async commit() {
    this.verificarAbortada();
    try {
      for (const fn of this._commits) {
        if (this.signal?.aborted) throw new TransacaoAbortada(String(this.signal.reason || "AbortSignal"));
        await fn(this);
      }
      this.estado = "COMMITTED";
      this._rollbacks.length = 0;
      return true;
    } catch (erro) {
      await this.rollback(erro);
      throw erro;
    }
  }

  async rollback(causa = null) {
    if (this.estado === "ROLLED_BACK") return [];
    if (this.estado === "COMMITTED") throw new Error(`Transação já confirmada: ${this.nome}`);
    const erros = [];
    for (const fn of [...this._rollbacks].reverse()) {
      try { await fn(this, causa); } catch (erro) { erros.push(erro); }
    }
    this._rollbacks.length = 0;
    this.estado = "ROLLED_BACK";
    this.telemetry?.event?.("transaction.rollback", { transaction: this.nome, errors: erros.map(String) });
    return erros;
  }
}

export async function executarTransacao(nome, fn, opcoes = {}) {
  const tx = new Transacao(nome, opcoes);
  try {
    const resultado = await fn(tx);
    await tx.commit();
    return resultado;
  } catch (erro) {
    if (tx.estado === "OPEN") await tx.rollback(erro);
    throw erro;
  }
}
