// ARCZ · Feedback de edição estilo game.
//
// Três canais, todos sem bloquear o mouse do viewport:
//   · aviso(...)  — pílula flutuante no topo do 3D (some sozinha);
//   · chip(...)   — leitura numérica ao vivo colada no cursor durante o gizmo;
//   · dicas(...)  — barra de atalhos contextual no rodapé do 3D;
//   · ping(...)   — anel que expande no ponto 3D onde a ação aconteceu.

const DURACAO_PING = 700;

export class FeedbackManager {
  constructor() {
    this.viewer = null;
    this.pilha = null;
    this.chipEl = null;
    this.dicasEl = null;
    this.cursor = { x: 0, y: 0 };
    this.ultimoAviso = { texto: "", em: 0 };
  }

  inicializar(viewer) {
    this.viewer = viewer;
    this.montar();
    window.addEventListener("pointermove", e => {
      this.cursor.x = e.clientX;
      this.cursor.y = e.clientY;
      if (this.chipEl?.classList.contains("visivel")) this.posicionarChip();
    }, { passive: true });
  }

  montar() {
    if (this.pilha) return;
    const viewport = document.getElementById("viewport") || document.body;

    const hud = document.createElement("div");
    hud.id = "hud_feedback";
    hud.innerHTML = `<div class="fb-pilha" id="fb_pilha"></div>
                     <div class="fb-dicas" id="fb_dicas"></div>`;
    viewport.appendChild(hud);

    // O chip vive no body: acompanha o cursor mesmo fora do viewport.
    const chip = document.createElement("div");
    chip.className = "fb-chip";
    chip.id = "fb_chip";
    document.body.appendChild(chip);

    this.pilha = hud.querySelector("#fb_pilha");
    this.dicasEl = hud.querySelector("#fb_dicas");
    this.chipEl = chip;
  }

  // ------------------------------------------------------------- avisos
  /** tipo: "info" | "ok" | "alerta" | "erro" */
  aviso(texto, tipo = "info", ms = 2400) {
    if (!texto) return;
    this.montar();
    const agora = performance.now();
    // Repetir a mesma frase em sequência só reinicia o cronômetro da pílula.
    if (this.ultimoAviso.texto === texto && agora - this.ultimoAviso.em < 900) {
      const ultimo = this.pilha.lastElementChild;
      if (ultimo) {
        clearTimeout(Number(ultimo.dataset.timer));
        ultimo.dataset.timer = String(setTimeout(() => this.removerAviso(ultimo), ms));
        this.ultimoAviso.em = agora;
        return;
      }
    }
    this.ultimoAviso = { texto, em: agora };

    const el = document.createElement("div");
    el.className = `fb-aviso ${tipo}`;
    el.textContent = texto;
    this.pilha.appendChild(el);
    while (this.pilha.childElementCount > 4) this.pilha.removeChild(this.pilha.firstElementChild);
    requestAnimationFrame(() => el.classList.add("entrou"));
    el.dataset.timer = String(setTimeout(() => this.removerAviso(el), ms));
  }

  removerAviso(el) {
    if (!el || !el.parentElement) return;
    el.classList.remove("entrou");
    setTimeout(() => el.remove(), 180);
  }

  // --------------------------------------------------------------- chip
  chip(texto) {
    this.montar();
    if (!texto) return this.esconderChip();
    this.chipEl.textContent = texto;
    this.chipEl.classList.add("visivel");
    this.posicionarChip();
  }

  posicionarChip() {
    const x = Math.min(this.cursor.x + 18, window.innerWidth - this.chipEl.offsetWidth - 8);
    const y = Math.max(8, this.cursor.y - 34);
    this.chipEl.style.transform = `translate(${Math.round(x)}px, ${Math.round(y)}px)`;
  }

  esconderChip() {
    this.chipEl?.classList.remove("visivel");
  }

  // -------------------------------------------------------------- dicas
  /** itens: [["W", "Mover"], ["Shift", "Snap"]] — passe null para limpar. */
  dicas(itens) {
    this.montar();
    if (!itens || !itens.length) {
      this.dicasEl.classList.remove("visivel");
      this.dicasEl.innerHTML = "";
      return;
    }
    this.dicasEl.innerHTML = itens
      .map(([tecla, texto]) => `<span class="fb-dica"><kbd>${tecla}</kbd>${texto}</span>`)
      .join("");
    this.dicasEl.classList.add("visivel");
  }

  // --------------------------------------------------------------- ping
  /** Anel que expande e apaga no ponto 3D — confirma "caiu aqui". */
  ping(cartesiano, corCss = "#34d399", raioMax = 4) {
    if (!this.viewer || !cartesiano) return;
    let t = 0;
    const cor = Cesium.Color.fromCssColorString(corCss);
    const altura = Cesium.Cartographic.fromCartesian(cartesiano).height + 0.05;
    const raio = () => 0.35 + raioMax * t;

    const entidade = this.viewer.entities.add({
      position: cartesiano,
      ellipse: {
        height: altura,
        semiMajorAxis: new Cesium.CallbackProperty(raio, false),
        semiMinorAxis: new Cesium.CallbackProperty(raio, false),
        material: new Cesium.ColorMaterialProperty(
          new Cesium.CallbackProperty(() => cor.withAlpha(0.28 * (1 - t)), false)),
        outline: true,
        outlineWidth: 2,
        outlineColor: new Cesium.CallbackProperty(() => cor.withAlpha(1 - t), false)
      }
    });

    const inicio = performance.now();
    const passo = () => {
      t = Math.min(1, (performance.now() - inicio) / DURACAO_PING);
      this.viewer.scene.requestRender?.();
      if (t < 1) requestAnimationFrame(passo);
      else {
        this.viewer.entities.remove(entidade);
        this.viewer.scene.requestRender?.();
      }
    };
    requestAnimationFrame(passo);
  }
}

export const feedbackApp = new FeedbackManager();
