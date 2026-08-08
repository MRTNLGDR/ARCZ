// ARCZ · Assistente de posicionamento estilo game.
//
// Entra em cena quando o usuário arrasta (ou clica) uma peça da biblioteca:
// um fantasma translúcido acompanha o cursor, gruda na superfície sob ele,
// mostra o footprint no chão, gira com a roda do mouse e pousa no clique.
//
// Teclas enquanto o assistente está ligado:
//   roda            gira 15° (Alt = 1°)      Shift+roda  escala
//   Ctrl+roda       aproxima/afasta câmera   PageUp/Down altura ±0,1 m
//   G               ciclo da grade de snap   Esc / botão direito  cancela
//   clique          pousa                    Shift+clique  pousa e continua

import { estadoApp } from "./estado.js";
import { cenaApp, urlDoModelo } from "./cena.js";
import { feedbackApp } from "./feedback.js";

export const PASSOS_GRADE = [0, 0.25, 0.5, 1, 5];

/** Arredonda para o múltiplo mais próximo do passo (passo 0 = sem snap). */
export function snapEmGrade(valor, passo) {
  return passo > 0 ? Math.round(valor / passo) * passo : valor;
}

/** Ângulo em [0,360) arredondado ao passo pedido. */
export function snapAngulo(angulo, passo = 0) {
  const n = passo > 0 ? Math.round(angulo / passo) * passo : angulo;
  return ((n % 360) + 360) % 360;
}

const DICAS = [
  ["Clique", "pousar"],
  ["Shift+clique", "pousar e continuar"],
  ["Roda", "girar"],
  ["Shift+roda", "escala"],
  ["PgUp/PgDn", "altura"],
  ["G", "grade"],
  ["Esc", "cancelar"]
];

export class PosicionadorManager {
  constructor() {
    this.viewer = null;
    this.ativo = false;
    this.item = null;
    this.fantasma = null;
    this.marcadores = [];
    this.ponto = null;          // {lat, lon, alt} da superfície sob o cursor
    this.rumo = 0;
    this.escala = 1;
    this.alturaExtra = 0;
    this.grade = 0;
    this.valido = false;
    this.viaArraste = false;
    this.carregando = false;
    this.handler = null;
  }

  inicializar(viewer) {
    this.viewer = viewer;
    this.grade = Number(localStorage.getItem("arcz.grade") || 0);
    this.criarMarcadores();
    this.configurarEntradas();
  }

  // ------------------------------------------------------------ geometria
  baseCartesian() {
    if (!this.ponto) return null;
    return Cesium.Cartesian3.fromDegrees(
      this.ponto.lon, this.ponto.lat, (this.ponto.alt ?? 0) + this.alturaExtra);
  }

  chaoCartesian() {
    if (!this.ponto) return null;
    return Cesium.Cartesian3.fromDegrees(this.ponto.lon, this.ponto.lat, this.ponto.alt ?? 0);
  }

  /** Raio do footprint. `boundingSphere` só existe depois do GLB pronto. */
  raio() {
    let r = 1.2;
    try {
      if (this.fantasma?.ready) r = this.fantasma.boundingSphere.radius || r;
    } catch (e) {
      // modelo ainda carregando: segue com o raio padrão
    }
    return Math.max(0.7, Math.min(60, r * 1.05));
  }

  cor() {
    return Cesium.Color.fromCssColorString(this.valido ? "#34d399" : "#f59e0b");
  }

  /** Círculo horizontal (ENU) em volta do ponto de pouso. */
  circulo(raio, n = 56) {
    const origem = this.chaoCartesian();
    if (!origem) return [];
    const quadro = Cesium.Transforms.eastNorthUpToFixedFrame(origem);
    const pontos = [];
    for (let i = 0; i <= n; i++) {
      const a = (i / n) * Math.PI * 2;
      pontos.push(Cesium.Matrix4.multiplyByPoint(
        quadro, new Cesium.Cartesian3(Math.cos(a) * raio, Math.sin(a) * raio, 0.02),
        new Cesium.Cartesian3()));
    }
    return pontos;
  }

  /** Ponta da agulha que mostra para onde a peça está virada. */
  agulha() {
    const origem = this.chaoCartesian();
    if (!origem) return [];
    const quadro = Cesium.Transforms.eastNorthUpToFixedFrame(origem);
    const r = Cesium.Math.toRadians(this.rumo || 0);
    const d = this.raio() * 1.6;
    const ponta = Cesium.Matrix4.multiplyByPoint(
      quadro, new Cesium.Cartesian3(Math.sin(r) * d, Math.cos(r) * d, 0.02), new Cesium.Cartesian3());
    return [origem, ponta];
  }

  // ----------------------------------------------------------- marcadores
  criarMarcadores() {
    const cb = fn => new Cesium.CallbackProperty(fn, false);
    const corCb = cb(() => this.cor().withAlpha(0.95));

    const anel = this.viewer.entities.add({
      show: false,
      polyline: {
        positions: cb(() => this.circulo(this.raio())),
        width: 3,
        arcType: Cesium.ArcType.NONE,
        material: new Cesium.ColorMaterialProperty(corCb),
        depthFailMaterial: new Cesium.ColorMaterialProperty(cb(() => this.cor().withAlpha(0.35)))
      }
    });

    const seta = this.viewer.entities.add({
      show: false,
      polyline: {
        positions: cb(() => this.agulha()),
        width: 12,
        arcType: Cesium.ArcType.NONE,
        material: new Cesium.PolylineArrowMaterialProperty(corCb)
      }
    });

    // Haste vertical: só aparece quando a peça está acima da superfície.
    const haste = this.viewer.entities.add({
      show: false,
      polyline: {
        positions: cb(() => {
          const chao = this.chaoCartesian();
          const base = this.baseCartesian();
          return chao && base && Math.abs(this.alturaExtra) > 0.01 ? [chao, base] : [];
        }),
        width: 2,
        arcType: Cesium.ArcType.NONE,
        material: new Cesium.PolylineDashMaterialProperty({ color: corCb, dashLength: 8 })
      }
    });

    this.marcadores = [anel, seta, haste];
  }

  mostrarMarcadores(ligado) {
    for (const m of this.marcadores) m.show = ligado;
    this.viewer.scene.requestRender?.();
  }

  // -------------------------------------------------------------- entrada
  configurarEntradas() {
    this.handler = new Cesium.ScreenSpaceEventHandler(this.viewer.scene.canvas);

    this.handler.setInputAction(mov => {
      if (this.ativo && !this.viaArraste) this.mover(mov.endPosition.x, mov.endPosition.y);
    }, Cesium.ScreenSpaceEventType.MOUSE_MOVE);

    this.handler.setInputAction(clique => {
      if (!this.ativo || this.viaArraste) return;
      this.mover(clique.position.x, clique.position.y);
      this.confirmar(this.shift);
    }, Cesium.ScreenSpaceEventType.LEFT_CLICK);

    this.handler.setInputAction(() => {
      if (this.ativo) this.cancelar();
    }, Cesium.ScreenSpaceEventType.RIGHT_CLICK);

    // Captura no ancestral: chega antes dos listeners que o Cesium põe no canvas.
    const alvo = document.getElementById("cesiumContainer") || this.viewer.scene.canvas;
    alvo.addEventListener("wheel", e => this.aoRolar(e), { capture: true, passive: false });

    window.addEventListener("keydown", e => {
      this.shift = e.shiftKey;
      if (!this.ativo) return;
      if (e.key === "Escape") { e.preventDefault(); this.cancelar(); }
      if (e.key.toLowerCase() === "g") { e.preventDefault(); this.alternarGrade(); }
      if (e.key === "PageUp") { e.preventDefault(); this.ajustarAltura(+0.1); }
      if (e.key === "PageDown") { e.preventDefault(); this.ajustarAltura(-0.1); }
    }, true);
    window.addEventListener("keyup", e => { this.shift = e.shiftKey; }, true);
  }

  aoRolar(e) {
    if (!this.ativo) return;
    e.preventDefault();
    e.stopPropagation();
    const passo = e.deltaY > 0 ? -1 : 1;

    if (e.ctrlKey) {
      const cam = this.viewer.camera;
      const h = Math.max(2, cam.positionCartographic.height * 0.2);
      passo > 0 ? cam.zoomIn(h) : cam.zoomOut(h);
      return;
    }
    if (e.shiftKey) {
      this.escala = Math.min(100, Math.max(0.01, this.escala * (passo > 0 ? 1.1 : 1 / 1.1)));
    } else {
      this.rumo = snapAngulo(this.rumo + passo * (e.altKey ? 1 : 15));
    }
    this.atualizarFantasma();
    this.atualizarLeitura();
  }

  ajustarAltura(delta) {
    this.alturaExtra = Math.round((this.alturaExtra + delta) * 100) / 100;
    this.atualizarFantasma();
    this.atualizarLeitura();
  }

  alternarGrade() {
    const i = PASSOS_GRADE.indexOf(this.grade);
    this.grade = PASSOS_GRADE[(i + 1) % PASSOS_GRADE.length];
    localStorage.setItem("arcz.grade", String(this.grade));
    feedbackApp.aviso(this.grade ? `Grade ${this.grade} m` : "Grade desligada", "info", 1200);
    this.atualizarLeitura();
  }

  // ------------------------------------------------------------ ciclo de vida
  /** Liga o assistente com um item {nome, url, escala?}. */
  async iniciar(item, { viaArraste = false } = {}) {
    if (!this.viewer || !item?.url) return false;
    this.limparFantasma();
    this.ativo = true;
    this.viaArraste = viaArraste;
    this.item = item;
    this.rumo = 0;
    this.escala = Number(item.escala) || 1;
    this.alturaExtra = 0;
    this.ponto = null;
    this.valido = false;

    this.viewer.scene.screenSpaceCameraController.enableZoom = false;
    cenaApp.selecaoBloqueada = true;
    document.body.classList.add("posicionando");
    this.mostrarMarcadores(true);
    if (!viaArraste) feedbackApp.dicas(DICAS);
    feedbackApp.aviso(`Posicionando "${item.nome}"`, "info", 1600);

    await this.carregarFantasma(item);
    return true;
  }

  async carregarFantasma(item) {
    this.carregando = true;
    try {
      const modelo = await Cesium.Model.fromGltfAsync({
        url: urlDoModelo(item.url, "medio"),
        modelMatrix: Cesium.Matrix4.IDENTITY,
        scale: this.escala,
        allowPicking: false,
        shadows: Cesium.ShadowMode.DISABLED,
        color: Cesium.Color.fromCssColorString("#9dc0ff").withAlpha(0.55),
        colorBlendMode: Cesium.ColorBlendMode.MIX,
        colorBlendAmount: 0.35
      });
      // Cancelou (ou trocou de peça) enquanto o GLB carregava.
      if (!this.ativo || this.item !== item) {
        modelo.destroy?.();
        return;
      }
      modelo.show = false;
      this.viewer.scene.primitives.add(modelo);
      this.fantasma = modelo;
      this.atualizarFantasma();
    } catch (e) {
      console.error(`Nao consegui carregar o fantasma de ${item.nome}:`, e);
      feedbackApp.aviso("Falha ao carregar a prévia da peça", "erro");
    } finally {
      this.carregando = false;
    }
  }

  limparFantasma() {
    if (this.fantasma) {
      this.viewer.scene.primitives.remove(this.fantasma);
      this.fantasma = null;
    }
  }

  encerrar() {
    this.ativo = false;
    this.viaArraste = false;
    this.item = null;
    this.ponto = null;
    this.valido = false;
    this.limparFantasma();
    this.mostrarMarcadores(false);
    this.viewer.scene.screenSpaceCameraController.enableZoom = true;
    cenaApp.selecaoBloqueada = false;
    document.body.classList.remove("posicionando");
    feedbackApp.dicas(null);
    feedbackApp.esconderChip();
    this.viewer.scene.requestRender?.();
  }

  cancelar() {
    if (!this.ativo) return;
    const nome = this.item?.nome;
    this.encerrar();
    if (nome) feedbackApp.aviso(`"${nome}" cancelada`, "alerta", 1400);
  }

  // ------------------------------------------------------------- movimento
  /** Recalcula o ponto de pouso a partir de um pixel do canvas. */
  mover(x, y) {
    if (!this.ativo) return;
    const ponto = cenaApp.pontoNaCena(x, y);
    this.valido = !!ponto;
    if (ponto) this.ponto = this.aplicarGrade(ponto);
    this.atualizarFantasma();
    this.atualizarLeitura();
    this.viewer.scene.requestRender?.();
  }

  /** Prende o ponto na grade métrica ancorada no prédio principal. */
  aplicarGrade(ponto) {
    if (!this.grade) return ponto;
    const p = estadoApp.obter().posicao;
    const ancora = Cesium.Cartesian3.fromDegrees(p.lon, p.lat, p.alt || 0);
    const quadro = Cesium.Transforms.eastNorthUpToFixedFrame(ancora);
    const inverso = Cesium.Matrix4.inverseTransformation(quadro, new Cesium.Matrix4());
    const destino = Cesium.Cartesian3.fromDegrees(ponto.lon, ponto.lat, p.alt || 0);
    const local = Cesium.Matrix4.multiplyByPoint(inverso, destino, new Cesium.Cartesian3());
    local.x = snapEmGrade(local.x, this.grade);
    local.y = snapEmGrade(local.y, this.grade);
    local.z = 0;
    const ecef = Cesium.Matrix4.multiplyByPoint(quadro, local, new Cesium.Cartesian3());
    const carto = Cesium.Cartographic.fromCartesian(ecef);
    return {
      lat: Cesium.Math.toDegrees(carto.latitude),
      lon: Cesium.Math.toDegrees(carto.longitude),
      alt: ponto.alt
    };
  }

  atualizarFantasma() {
    if (!this.fantasma) return;
    const base = this.baseCartesian();
    if (!base || !this.valido) {
      this.fantasma.show = false;
      return;
    }
    const hpr = new Cesium.HeadingPitchRoll(Cesium.Math.toRadians(this.rumo || 0), 0, 0);
    this.fantasma.modelMatrix = Cesium.Transforms.headingPitchRollToFixedFrame(base, hpr);
    this.fantasma.scale = this.escala;
    this.fantasma.show = true;
    this.viewer.scene.requestRender?.();
  }

  atualizarLeitura() {
    if (!this.ativo) return;
    if (!this.valido) return feedbackApp.chip("sem superfície aqui");
    const alt = (this.ponto.alt ?? 0) + this.alturaExtra;
    feedbackApp.chip(
      `${this.rumo.toFixed(0)}° · ×${this.escala.toFixed(2)} · ${alt.toFixed(1)} m` +
      (this.grade ? ` · grade ${this.grade} m` : ""));
  }

  // --------------------------------------------------------------- arraste
  /** Chamado pelo `dragover` do viewport (drag & drop nativo). */
  arrastarPara(x, y) {
    if (!this.ativo) return;
    this.mover(x, y);
  }

  /** Chamado pelo `drop`: pousa exatamente onde o cursor soltou. */
  soltarEm(x, y) {
    if (!this.ativo) return null;
    this.viaArraste = false;
    this.mover(x, y);
    return this.confirmar(false);
  }

  // ------------------------------------------------------------- confirmar
  confirmar(serie = false) {
    if (!this.ativo || !this.item) return null;
    if (!this.valido || !this.ponto) {
      feedbackApp.aviso("Sem superfície sob o cursor — mire no terreno ou no prédio", "alerta");
      return null;
    }

    const item = this.item;
    const base = this.baseCartesian();
    const peca = cenaApp.adicionarPeca({
      nome: item.nome,
      url: item.url,
      lat: this.ponto.lat,
      lon: this.ponto.lon,
      alt: (this.ponto.alt ?? 0) + this.alturaExtra,
      rumo: this.rumo,
      escala: this.escala,
      lod: "medio"
    });

    feedbackApp.ping(base, "#34d399", Math.max(1.5, this.raio() * 1.6));
    feedbackApp.aviso(`${peca.nome} pousada · ${this.rumo.toFixed(0)}° · ×${this.escala.toFixed(2)}`, "ok", 1800);

    if (serie) {
      // Modo série: mantém rumo/escala/grade e segue posicionando a mesma peça.
      this.viaArraste = false;
      return peca;
    }
    this.encerrar();
    return peca;
  }
}

export const posicionadorApp = new PosicionadorManager();
