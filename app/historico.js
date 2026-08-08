// ARCZ · T04: Módulo de Histórico (Command Pattern + Undo/Redo)
export class HistoricoManager {
  constructor(maxPassos = 50) {
    this.maxPassos = maxPassos;
    this.pilhaUndo = [];
    this.pilhaRedo = [];
    this.listeners = [];

    this.configurarAtalhos();
  }

  executar(comando) {
    if (!comando || typeof comando.fazer !== "function" || typeof comando.desfazer !== "function") {
      console.error("Comando invalido para o historico:", comando);
      return;
    }

    comando.fazer();

    this.pilhaUndo.push(comando);
    if (this.pilhaUndo.length > this.maxPassos) {
      this.pilhaUndo.shift();
    }

    // Limpa a pilha de redo a cada nova acao
    this.pilhaRedo = [];
    this.notificar();
  }

  /** Empilha um comando JÁ aplicado (arraste do gizmo) sem executá-lo de novo. */
  registrar(comando) {
    if (!comando || typeof comando.fazer !== "function" || typeof comando.desfazer !== "function") {
      console.error("Comando invalido para o historico:", comando);
      return;
    }
    this.pilhaUndo.push(comando);
    if (this.pilhaUndo.length > this.maxPassos) this.pilhaUndo.shift();
    this.pilhaRedo = [];
    this.notificar();
  }

  desfazer() {
    if (this.pilhaUndo.length === 0) return false;
    const comando = this.pilhaUndo.pop();
    try {
      comando.desfazer();
      this.pilhaRedo.push(comando);
      this.notificar();
      return true;
    } catch (e) {
      console.error("Erro ao desfazer comando:", e);
      return false;
    }
  }

  refazer() {
    if (this.pilhaRedo.length === 0) return false;
    const comando = this.pilhaRedo.pop();
    try {
      comando.fazer();
      this.pilhaUndo.push(comando);
      this.notificar();
      return true;
    } catch (e) {
      console.error("Erro ao refazer comando:", e);
      return false;
    }
  }

  obterHistorico() {
    return this.pilhaUndo.map(c => c.nome);
  }

  inscrever(fn) {
    this.listeners.push(fn);
  }

  notificar() {
    for (const fn of this.listeners) {
      try {
        fn(this.pilhaUndo, this.pilhaRedo);
      } catch (e) {
        console.error("Erro no listener de historico:", e);
      }
    }
  }

  configurarAtalhos() {
    window.addEventListener("keydown", (e) => {
      // Ignorar se estiver digitando em input ou textarea
      const tag = document.activeElement ? document.activeElement.tagName.toLowerCase() : "";
      if (tag === "input" || tag === "textarea" || tag === "select") return;

      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
        if (e.shiftKey) {
          e.preventDefault();
          this.refazer();
        } else {
          e.preventDefault();
          this.desfazer();
        }
      }
    });
  }
}

export const historicoApp = new HistoricoManager();
