// ARCZ · Gizmo de verdade, estilo game: alças por eixo que se pega com o mouse,
// destaque no hover, snap por grade/ângulo, leitura numérica ao vivo e
// undo/redo por operação.
//
// Convenção dos eixos (quadro ENU no ponto do objeto):
//   x = leste (vermelho) · y = norte (verde) · z = altura (azul)
// Com "eixos locais" ligado, x/y giram junto com o rumo da peça.
import { estadoApp } from "./estado.js";
import { historicoApp } from "./historico.js";
import { cenaApp } from "./cena.js";
import { feedbackApp } from "./feedback.js";

const MODOS = ["nenhum", "mover", "girar", "escalar"];

const COR = {
  x: "#ff5470",
  y: "#34d399",
  z: "#6d9dff",
  livre: "#e6ecf5",
  anel: "#6d9dff",
  escala: "#ffd166",
  hover: "#ffd166",
  ativo: "#ffffff"
};

export const SNAP = { metros: 0.5, graus: 15, escala: 0.1 };

const DICAS_POR_MODO = {
  mover: [["Arraste", "eixo ou centro"], ["Shift", "snap 0,5 m"], ["E/R", "girar/escalar"], ["Del", "remover"]],
  girar: [["Arraste", "anel"], ["Shift", "snap 15°"], ["W/R", "mover/escalar"], ["Esc", "soltar"]],
  escalar: [["Arraste", "alça amarela"], ["Shift", "snap 0,1"], ["W/E", "mover/girar"], ["Esc", "soltar"]]
};

// ------------------------------------------------------------------ puro
/** Nova posição depois de arrastar do ponto A ao ponto B do terreno. */
export function moverPara(objeto, pontoA, pontoB) {
  return {
    lat: objeto.lat + (pontoB.lat - pontoA.lat),
    lon: objeto.lon + (pontoB.lon - pontoA.lon)
  };
}

/** Rumo em [0,360) depois de arrastar `dx` pixels. */
export function girarPor(rumoAtual, dx, sensibilidade = 0.4) {
  const novo = (rumoAtual || 0) + dx * sensibilidade;
  return ((novo % 360) + 360) % 360;
}

/** Escala limitada entre 1% e 100x depois de arrastar `dy` pixels. */
export function escalarPor(escalaAtual, dy, sensibilidade = 0.004) {
  const fator = Math.exp(-dy * sensibilidade);
  return Math.min(100, Math.max(0.01, (escalaAtual || 1) * fator));
}

/** Arredonda ao múltiplo do passo (passo 0 ou ausente = sem snap). */
export function aplicarSnap(valor, passo) {
  return passo > 0 ? Math.round(valor / passo) * passo : valor;
}

export function normalizarAngulo(angulo) {
  return ((angulo % 360) + 360) % 360;
}

/**
 * Parâmetro `t` do ponto do eixo (origem + t·direção) mais próximo do raio da
 * câmera. É o que faz o objeto deslizar só naquele eixo enquanto o mouse anda
 * livre pela tela. Retorna null quando eixo e raio estão quase paralelos.
 */
export function tNoEixo(origem, direcao, raioOrigem, raioDirecao) {
  const w0 = Cesium.Cartesian3.subtract(origem, raioOrigem, new Cesium.Cartesian3());
  const b = Cesium.Cartesian3.dot(direcao, raioDirecao);
  const d = Cesium.Cartesian3.dot(direcao, w0);
  const e = Cesium.Cartesian3.dot(raioDirecao, w0);
  const den = 1 - b * b;
  if (Math.abs(den) < 1e-6) return null;
  return (b * e - d) / den;
}

export class GizmoManager {
  constructor() {
    this.viewer = null;
    this.modo = "mover";
    this.eixosLocais = false;
    this.tipoPivo = "base";
    this.handler = null;
    this.arraste = null;
    this.hover = null;
    this.shift = false;
    this.snapFixo = false;
    this.eixos = [];        // todas as alças (a camada "Eixos do gizmo" usa isto)
    this.alcas = { mover: [], girar: [], escalar: [] };
  }

  inicializar(viewer) {
    this.viewer = viewer;
    const st = estadoApp.obter();
    this.modo = st.modoGizmo || "mover";
    this.eixosLocais = !!st.posicao.eixos_locais;
    this.snapFixo = localStorage.getItem("arcz.snap") === "1";

    estadoApp.inscrever((estado, origem) => {
      if (origem === "camera") return;
      this.modo = estado.modoGizmo || "mover";
      this.eixosLocais = !!estado.posicao.eixos_locais;
      this.atualizarVisibilidade();
    });

    this.criarAlcas();
    this.configurarArraste();
    this.configurarTeclas();
  }

  // ---------------------------------------------------------- alvo atual
  /** Peça selecionada; sem seleção, o alvo é o prédio principal. */
  alvo() {
    const st = estadoApp.obter();
    if (st.pecaSelecionadaId) {
      const peca = cenaApp.obterPeca(st.pecaSelecionadaId);
      if (peca) return { tipo: "peca", id: peca.id, dados: peca };
    }
    return { tipo: "predio", id: "predio", dados: st.posicao };
  }

  aplicar(alvo, patch, origem) {
    if (alvo.tipo === "peca") {
      cenaApp.atualizarPeca(alvo.id, patch, origem);
    } else {
      estadoApp.atualizar({ posicao: patch }, origem === "gizmo" ? "gizmo_predio" : origem);
      cenaApp.aplicarTransformPredio(estadoApp.obter().posicao);
    }
  }

  // ------------------------------------------------------------ geometria
  /** Origem do gizmo. As alças leem isto a cada quadro: uma exceção aqui
   *  derruba o laço de render do Cesium inteiro, então nunca lança. */
  origemCartesian() {
    const d = this.alvo()?.dados || {};
    const lat = Number(d.lat), lon = Number(d.lon);
    if (!Number.isFinite(lat) || !Number.isFinite(lon)) {
      return this.ultimaOrigem || Cesium.Cartesian3.ZERO;
    }
    this.ultimaOrigem = Cesium.Cartesian3.fromDegrees(lon, lat, Number(d.alt) || 0);
    return this.ultimaOrigem;
  }

  /** Quadro ENU do alvo (girado pelo rumo quando "eixos locais" está ligado). */
  quadro() {
    const q = Cesium.Transforms.eastNorthUpToFixedFrame(this.origemCartesian());
    if (!this.eixosLocais) return q;
    const rot = Cesium.Matrix3.fromRotationZ(-Cesium.Math.toRadians(this.alvo().dados.rumo || 0));
    return Cesium.Matrix4.multiplyByMatrix3(q, rot, new Cesium.Matrix4());
  }

  /** Tamanho em metros que mantém a alça com o mesmo tamanho aparente na tela. */
  tamanho() {
    const dist = Cesium.Cartesian3.distance(this.viewer.camera.positionWC, this.origemCartesian());
    return Cesium.Math.clamp(dist * 0.13, 1.0, 200);
  }

  direcaoEixo(eixo) {
    const local = { x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] }[eixo] || [0, 0, 1];
    const rot = Cesium.Matrix4.getMatrix3(this.quadro(), new Cesium.Matrix3());
    const v = Cesium.Matrix3.multiplyByVector(rot, new Cesium.Cartesian3(...local), new Cesium.Cartesian3());
    return Cesium.Cartesian3.normalize(v, v);
  }

  pontaEixo(eixo, fator = 1) {
    const dir = this.direcaoEixo(eixo);
    const passo = Cesium.Cartesian3.multiplyByScalar(dir, this.tamanho() * fator, new Cesium.Cartesian3());
    return Cesium.Cartesian3.add(this.origemCartesian(), passo, new Cesium.Cartesian3());
  }

  /** Círculo horizontal do anel de rotação. */
  circulo(raio, n = 64) {
    const quadro = Cesium.Transforms.eastNorthUpToFixedFrame(this.origemCartesian());
    const pontos = [];
    for (let i = 0; i <= n; i++) {
      const a = (i / n) * Math.PI * 2;
      pontos.push(Cesium.Matrix4.multiplyByPoint(
        quadro, new Cesium.Cartesian3(Math.cos(a) * raio, Math.sin(a) * raio, 0.05),
        new Cesium.Cartesian3()));
    }
    return pontos;
  }

  /** Ponto sobre o anel no rumo pedido (o "punho" que se agarra). */
  pontoNoAnel(rumoGraus, raio) {
    const quadro = Cesium.Transforms.eastNorthUpToFixedFrame(this.origemCartesian());
    const r = Cesium.Math.toRadians(rumoGraus || 0);
    return Cesium.Matrix4.multiplyByPoint(
      quadro, new Cesium.Cartesian3(Math.sin(r) * raio, Math.cos(r) * raio, 0.05),
      new Cesium.Cartesian3());
  }

  // --------------------------------------------------------------- alças
  corDaAlca(chave, base) {
    const arrastando = this.arraste?.eixo === chave;
    const sob = this.hover === chave;
    const css = arrastando ? COR.ativo : sob ? COR.hover : base;
    return Cesium.Color.fromCssColorString(css).withAlpha(0.96);
  }

  criarAlcas() {
    const cb = fn => new Cesium.CallbackProperty(fn, false);
    const cor = (chave, base) => cb(() => this.corDaAlca(chave, base));
    const corFraca = (chave, base) => cb(() => this.corDaAlca(chave, base).withAlpha(0.32));

    const registrar = (modo, entidade, chave) => {
      entidade.arczGizmo = chave;
      entidade.show = false;
      this.alcas[modo].push(entidade);
      this.eixos.push(entidade);
      return entidade;
    };

    // --- mover: três setas + um ponto central de arraste livre
    for (const eixo of ["x", "y", "z"]) {
      registrar("mover", this.viewer.entities.add({
        polyline: {
          positions: cb(() => [this.origemCartesian(), this.pontaEixo(eixo)]),
          width: 15,
          arcType: Cesium.ArcType.NONE,
          material: new Cesium.PolylineArrowMaterialProperty(cor(eixo, COR[eixo])),
          depthFailMaterial: new Cesium.PolylineArrowMaterialProperty(corFraca(eixo, COR[eixo]))
        }
      }), eixo);
    }
    registrar("mover", this.viewer.entities.add({
      position: cb(() => this.origemCartesian()),
      point: {
        pixelSize: 13,
        color: cor("livre", COR.livre),
        outlineWidth: 2,
        outlineColor: Cesium.Color.fromCssColorString("#0b1017"),
        disableDepthTestDistance: Number.POSITIVE_INFINITY
      }
    }), "livre");

    // --- girar: anel + punho no rumo atual + agulha do norte
    registrar("girar", this.viewer.entities.add({
      polyline: {
        positions: cb(() => this.circulo(this.tamanho())),
        width: 8,
        arcType: Cesium.ArcType.NONE,
        material: new Cesium.ColorMaterialProperty(cor("anel", COR.anel)),
        depthFailMaterial: new Cesium.ColorMaterialProperty(corFraca("anel", COR.anel))
      }
    }), "anel");
    registrar("girar", this.viewer.entities.add({
      position: cb(() => this.pontoNoAnel(this.alvo().dados.rumo || 0, this.tamanho())),
      point: {
        pixelSize: 14,
        color: cor("anel", COR.escala),
        outlineWidth: 2,
        outlineColor: Cesium.Color.fromCssColorString("#0b1017"),
        disableDepthTestDistance: Number.POSITIVE_INFINITY
      }
    }), "anel");
    registrar("girar", this.viewer.entities.add({
      polyline: {
        positions: cb(() => [this.origemCartesian(),
                             this.pontoNoAnel(this.alvo().dados.rumo || 0, this.tamanho())]),
        width: 10,
        arcType: Cesium.ArcType.NONE,
        material: new Cesium.PolylineArrowMaterialProperty(cor("anel", COR.anel))
      }
    }), "anel");

    // --- escalar: haste na vertical com a alça na ponta
    registrar("escalar", this.viewer.entities.add({
      polyline: {
        positions: cb(() => [this.origemCartesian(), this.pontaEixo("z", 0.9)]),
        width: 5,
        arcType: Cesium.ArcType.NONE,
        material: new Cesium.ColorMaterialProperty(cor("escala", COR.escala)),
        depthFailMaterial: new Cesium.ColorMaterialProperty(corFraca("escala", COR.escala))
      }
    }), "escala");
    registrar("escalar", this.viewer.entities.add({
      position: cb(() => this.pontaEixo("z", 0.9)),
      point: {
        pixelSize: 16,
        color: cor("escala", COR.escala),
        outlineWidth: 2,
        outlineColor: Cesium.Color.fromCssColorString("#0b1017"),
        disableDepthTestDistance: Number.POSITIVE_INFINITY
      }
    }), "escala");

    this.atualizarVisibilidade();
  }

  atualizarVisibilidade() {
    for (const modo of Object.keys(this.alcas)) {
      const ligado = this.modo === modo;
      for (const alca of this.alcas[modo]) alca.show = ligado;
    }
    this.viewer?.scene?.requestRender?.();
  }

  // manter o nome antigo: a tela de Camadas chama isto
  atualizarVisibilidadeEixos() {
    this.atualizarVisibilidade();
  }

  // -------------------------------------------------------------- entrada
  configurarTeclas() {
    window.addEventListener("keydown", e => { this.shift = e.shiftKey; }, true);
    window.addEventListener("keyup", e => { this.shift = e.shiftKey; }, true);
  }

  /** Shift inverte o snap: com snap fixo ligado, segurar Shift libera o movimento. */
  snapLigado() {
    return this.snapFixo !== this.shift;
  }

  definirSnap(ligado) {
    this.snapFixo = !!ligado;
    localStorage.setItem("arcz.snap", ligado ? "1" : "0");
  }

  configurarArraste() {
    this.handler = new Cesium.ScreenSpaceEventHandler(this.viewer.scene.canvas);
    this.handler.setInputAction(e => this.aoPressionar(e), Cesium.ScreenSpaceEventType.LEFT_DOWN);
    this.handler.setInputAction(m => this.aoMover(m), Cesium.ScreenSpaceEventType.MOUSE_MOVE);
    this.handler.setInputAction(() => this.aoSoltar(), Cesium.ScreenSpaceEventType.LEFT_UP);
  }

  alcaSob(posicaoTela) {
    const pego = this.viewer.scene.pick(posicaoTela);
    return pego?.id?.arczGizmo || null;
  }

  aoPressionar(evento) {
    if (this.modo === "nenhum" || cenaApp.selecaoBloqueada) return;
    const alvo = this.alvo();
    if (!alvo) return;

    const pego = this.viewer.scene.pick(evento.position);
    const alca = pego?.id?.arczGizmo || null;
    const idPego = pego?.id?.arczId || pego?.primitive?.id?.arczId || null;
    const sobreCorpo = !!idPego && idPego === alvo.id;
    if (!alca && !sobreCorpo) return;

    // Clicar no corpo da peça equivale à alça "livre" do modo atual.
    const eixo = alca || { mover: "livre", girar: "anel", escalar: "escala" }[this.modo];
    const raio = this.viewer.camera.getPickRay(evento.position);
    const origem = this.origemCartesian();

    this.arraste = {
      alvo,
      eixo,
      inicial: { ...alvo.dados },
      origem,
      direcao: ["x", "y", "z"].includes(eixo) ? this.direcaoEixo(eixo) : null,
      t0: null,
      ang0: null,
      dist0: null,
      pontoInicial: cenaApp.pontoNaCena(evento.position.x, evento.position.y),
      telaInicial: { x: evento.position.x, y: evento.position.y }
    };
    if (this.arraste.direcao && raio) {
      this.arraste.t0 = tNoEixo(origem, this.arraste.direcao, raio.origin, raio.direction);
    }
    if (eixo === "anel" && raio) this.arraste.ang0 = this.anguloDoRaio(raio, origem);
    if (eixo === "escala") this.arraste.dist0 = this.distanciaNaTela(evento.position, origem);

    this.viewer.scene.screenSpaceCameraController.enableInputs = false;
    this.viewer.scene.canvas.style.cursor = "grabbing";
  }

  aoMover(movimento) {
    const pos = movimento.endPosition;
    if (!this.arraste) {
      if (this.modo === "nenhum" || cenaApp.selecaoBloqueada) return;
      const alca = this.alcaSob(pos);
      if (alca !== this.hover) {
        this.hover = alca;
        this.viewer.scene.canvas.style.cursor = alca ? "grab" : "";
        this.viewer.scene.requestRender?.();
      }
      return;
    }

    const a = this.arraste;
    if (a.direcao) this.arrastarEixo(a, pos);
    else if (a.eixo === "livre") this.arrastarLivre(a, pos);
    else if (a.eixo === "anel") this.arrastarAnel(a, pos);
    else if (a.eixo === "escala") this.arrastarEscala(a, pos);
    this.viewer.scene.requestRender?.();
  }

  arrastarEixo(a, pos) {
    const raio = this.viewer.camera.getPickRay(pos);
    if (!raio || a.t0 === null) return;
    const t = tNoEixo(a.origem, a.direcao, raio.origin, raio.direction);
    if (t === null) return;

    let delta = t - a.t0;
    if (this.snapLigado()) delta = aplicarSnap(delta, SNAP.metros);

    const destino = Cesium.Cartesian3.add(
      a.origem, Cesium.Cartesian3.multiplyByScalar(a.direcao, delta, new Cesium.Cartesian3()),
      new Cesium.Cartesian3());
    const carto = Cesium.Cartographic.fromCartesian(destino);

    if (a.eixo === "z") {
      const alt = Number(((a.inicial.alt ?? 0) + delta).toFixed(3));
      this.aplicar(a.alvo, { alt }, "gizmo");
      feedbackApp.chip(`Z ${delta >= 0 ? "+" : ""}${delta.toFixed(2)} m · ${alt.toFixed(2)} m`);
    } else {
      // Só o plano horizontal muda: manter a cota evita deriva pela curvatura.
      this.aplicar(a.alvo, {
        lat: Cesium.Math.toDegrees(carto.latitude),
        lon: Cesium.Math.toDegrees(carto.longitude),
        alt: a.inicial.alt
      }, "gizmo");
      const nome = a.eixo === "x" ? (this.eixosLocais ? "→" : "Leste") : (this.eixosLocais ? "↑" : "Norte");
      feedbackApp.chip(`${nome} ${delta >= 0 ? "+" : ""}${delta.toFixed(2)} m`);
    }
  }

  arrastarLivre(a, pos) {
    const ponto = cenaApp.pontoNaCena(pos.x, pos.y);
    if (!ponto || !a.pontoInicial) return;
    let destino = moverPara(a.inicial, a.pontoInicial, ponto);
    let leitura = null;

    if (this.snapLigado()) {
      const preso = this.prenderNaGrade(a.inicial, destino.lat, destino.lon);
      destino = { lat: preso.lat, lon: preso.lon };
      leitura = `E ${preso.dx.toFixed(2)} m · N ${preso.dy.toFixed(2)} m · grade ${SNAP.metros} m`;
    }
    this.aplicar(a.alvo, destino, "gizmo");
    feedbackApp.chip(leitura || `${destino.lat.toFixed(6)}, ${destino.lon.toFixed(6)}`);
  }

  arrastarAnel(a, pos) {
    const raio = this.viewer.camera.getPickRay(pos);
    if (!raio || a.ang0 === null) return;
    const ang = this.anguloDoRaio(raio, a.origem);
    if (ang === null) return;

    let rumo = normalizarAngulo((a.inicial.rumo || 0) + (ang - a.ang0));
    if (this.snapLigado()) rumo = normalizarAngulo(aplicarSnap(rumo, SNAP.graus));
    this.aplicar(a.alvo, { rumo: Number(rumo.toFixed(2)) }, "gizmo");

    const delta = normalizarAngulo(rumo - (a.inicial.rumo || 0));
    feedbackApp.chip(`${rumo.toFixed(0)}°  (Δ ${delta > 180 ? (delta - 360).toFixed(0) : delta.toFixed(0)}°)`);
  }

  arrastarEscala(a, pos) {
    const dist = this.distanciaNaTela(pos, a.origem);
    if (!dist || !a.dist0) return;
    let escala = Math.min(100, Math.max(0.01, (a.inicial.escala || 1) * (dist / a.dist0)));
    if (this.snapLigado()) escala = Math.max(0.01, aplicarSnap(escala, SNAP.escala));
    this.aplicar(a.alvo, { escala: Number(escala.toFixed(3)) }, "gizmo");
    feedbackApp.chip(`×${escala.toFixed(2)}`);
  }

  /** Rumo (graus a partir do norte) do ponto onde o raio corta o plano horizontal. */
  anguloDoRaio(raio, origem) {
    const quadro = Cesium.Transforms.eastNorthUpToFixedFrame(origem);
    const rot = Cesium.Matrix4.getMatrix3(quadro, new Cesium.Matrix3());
    const cima = Cesium.Matrix3.multiplyByVector(rot, new Cesium.Cartesian3(0, 0, 1), new Cesium.Cartesian3());
    const plano = Cesium.Plane.fromPointNormal(origem, Cesium.Cartesian3.normalize(cima, cima));
    const ponto = Cesium.IntersectionTests.rayPlane(raio, plano);
    if (!ponto) return null;
    const inverso = Cesium.Matrix4.inverseTransformation(quadro, new Cesium.Matrix4());
    const local = Cesium.Matrix4.multiplyByPoint(inverso, ponto, new Cesium.Cartesian3());
    return Cesium.Math.toDegrees(Math.atan2(local.x, local.y));
  }

  distanciaNaTela(pos, origem) {
    const tela = Cesium.SceneTransforms.worldToWindowCoordinates(this.viewer.scene, origem);
    if (!tela) return null;
    return Math.max(6, Math.hypot(pos.x - tela.x, pos.y - tela.y));
  }

  /** Prende o deslocamento na grade métrica ENU ancorada na posição inicial. */
  prenderNaGrade(inicial, lat, lon) {
    const origem = Cesium.Cartesian3.fromDegrees(inicial.lon, inicial.lat, inicial.alt ?? 0);
    const quadro = Cesium.Transforms.eastNorthUpToFixedFrame(origem);
    const inverso = Cesium.Matrix4.inverseTransformation(quadro, new Cesium.Matrix4());
    const destino = Cesium.Cartesian3.fromDegrees(lon, lat, inicial.alt ?? 0);
    const local = Cesium.Matrix4.multiplyByPoint(inverso, destino, new Cesium.Cartesian3());
    local.x = aplicarSnap(local.x, SNAP.metros);
    local.y = aplicarSnap(local.y, SNAP.metros);
    local.z = 0;
    const ecef = Cesium.Matrix4.multiplyByPoint(quadro, local, new Cesium.Cartesian3());
    const carto = Cesium.Cartographic.fromCartesian(ecef);
    return {
      lat: Cesium.Math.toDegrees(carto.latitude),
      lon: Cesium.Math.toDegrees(carto.longitude),
      dx: local.x,
      dy: local.y
    };
  }

  // -------------------------------------------------------------- soltar
  aoSoltar() {
    if (!this.arraste) return;
    const { alvo, inicial, eixo } = this.arraste;
    this.arraste = null;
    this.viewer.scene.screenSpaceCameraController.enableInputs = true;
    this.viewer.scene.canvas.style.cursor = this.hover ? "grab" : "";
    feedbackApp.esconderChip();

    const atual = alvo.tipo === "peca" ? cenaApp.obterPeca(alvo.id) : estadoApp.obter().posicao;
    if (!atual) return;

    const campos = ["lat", "lon", "alt", "rumo", "escala"];
    const antes = {}, depois = {};
    let mudou = false;
    for (const c of campos) {
      if (inicial[c] !== atual[c]) {
        antes[c] = inicial[c];
        depois[c] = atual[c];
        mudou = true;
      }
    }
    if (!mudou) return;

    const nomes = { mover: "Mover", girar: "Girar", escalar: "Escalar" };
    const rotulo = alvo.tipo === "peca" ? alvo.dados.nome : "prédio";
    historicoApp.registrar({
      nome: `${nomes[this.modo] || "Editar"} ${rotulo}`,
      fazer: () => this.aplicar(alvo, depois, "gizmo"),
      desfazer: () => this.aplicar(alvo, antes, "gizmo")
    });

    if (this.modo === "girar") {
      feedbackApp.aviso(`${rotulo} · rumo ${Number(atual.rumo || 0).toFixed(0)}°`, "ok", 1400);
    } else if (this.modo === "escalar") {
      feedbackApp.aviso(`${rotulo} · escala ×${Number(atual.escala || 1).toFixed(2)}`, "ok", 1400);
    } else {
      feedbackApp.aviso(`${rotulo} movido${eixo !== "livre" ? ` no eixo ${eixo.toUpperCase()}` : ""}`, "ok", 1400);
    }
  }

  // --------------------------------------------------------------- modos
  definirModo(novoModo) {
    if (!MODOS.includes(novoModo)) return;
    this.modo = novoModo;
    estadoApp.atualizar({ modoGizmo: novoModo }, "gizmo_modo");
    this.atualizarVisibilidade();
    feedbackApp.dicas(DICAS_POR_MODO[novoModo] || null);
  }

  definirEixosLocais(locais) {
    this.eixosLocais = !!locais;
    estadoApp.atualizar({ posicao: { eixos_locais: !!locais } }, "gizmo_eixos");
    this.viewer?.scene?.requestRender?.();
  }

  definirTipoPivo(tipo) {
    this.tipoPivo = tipo;
  }
}

export const gizmoApp = new GizmoManager();
