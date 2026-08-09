import { LatestFrameQueue } from "./frame-coalescer.js";

/**
 * Limita o hot path de arraste do gizmo a uma transformação por frame.
 *
 * O ScreenSpaceEventHandler do Cesium chama `this.aoMover` dinamicamente, então
 * esta camada pode ser instalada depois de `gizmo.inicializar()` sem recriar o
 * handler nem alterar a matemática de mover/girar/escalar. Hover continua
 * imediato; apenas drag ativo é coalescido. O `aoSoltar` faz flush antes do
 * commit de histórico, portanto a última posição do usuário nunca é perdida.
 */
export function installGizmoFrameCoalescing(gizmo, queueOptions = {}) {
  if (!gizmo || typeof gizmo.aoMover !== "function" || typeof gizmo.aoSoltar !== "function") {
    throw new TypeError("gizmo válido com aoMover/aoSoltar é obrigatório");
  }
  if (gizmo.__arczFrameCoalescing) return gizmo.__arczFrameCoalescing;

  const queue = new LatestFrameQueue(queueOptions);
  const originalMove = gizmo.aoMover.bind(gizmo);
  const originalRelease = gizmo.aoSoltar.bind(gizmo);

  const wrappedMove = movimento => {
    if (!gizmo.arraste) return originalMove(movimento);
    const pos = movimento?.endPosition;
    if (!pos || !Number.isFinite(pos.x) || !Number.isFinite(pos.y)) return;
    // Copia coordenadas porque objetos de evento podem ser reutilizados pelo Cesium.
    const latest = { x: Number(pos.x), y: Number(pos.y) };
    queue.push(latest, value => originalMove({ endPosition: value }));
  };

  const wrappedRelease = (...args) => {
    // Flush ainda com `gizmo.arraste` ativo. `originalRelease` é quem fecha a
    // transação/undo e reativa os controles da câmera.
    queue.flush(value => originalMove({ endPosition: value }));
    return originalRelease(...args);
  };

  const controller = {
    queue,
    dispose() {
      queue.clear();
      if (gizmo.aoMover === wrappedMove) gizmo.aoMover = originalMove;
      if (gizmo.aoSoltar === wrappedRelease) gizmo.aoSoltar = originalRelease;
      if (gizmo.__arczFrameCoalescing === controller) delete gizmo.__arczFrameCoalescing;
    },
  };

  gizmo.aoMover = wrappedMove;
  gizmo.aoSoltar = wrappedRelease;
  gizmo.__arczFrameCoalescing = controller;
  return controller;
}
