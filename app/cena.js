// ARCZ · Cena: prédio principal, peças de mobiliário, LOD, seleção e assentamento.
import { estadoApp } from "./estado.js";
import { historicoApp } from "./historico.js";
import { alturaDoTerreno } from "./relevo.js";
import { qualidadeApp } from "./qualidade.js";

const RES_POR_LOD = {
  original: 0,      // 0 = sem redução (rota /glb-corrigido)
  equilibrado: 1024,
  medio: 512,
  distante: 256
};

export function normalizarPosicao(pos) {
  if (!pos) return { lat: -27.1545, lon: -48.5022, alt: 10, rumo: 119, escala: 1 };
  const lugar = pos.lugar || {};
  return {
    lat: pos.lat ?? lugar.lat ?? -27.1545,
    lon: pos.lon ?? lugar.lon ?? -48.5022,
    alt: pos.alt ?? lugar.alt ?? 10,
    rumo: pos.rumo ?? lugar.rumo ?? 119,
    escala: pos.escala ?? lugar.escala ?? 1
  };
}

/** Matriz de modelo a partir de lat/lon/alt + rumo. */
export function matrizDe(pos) {
  const p = normalizarPosicao(pos);
  const centro = Cesium.Cartesian3.fromDegrees(p.lon, p.lat, p.alt ?? 0);
  const hpr = new Cesium.HeadingPitchRoll(Cesium.Math.toRadians(p.rumo || 0), 0, 0);
  return Cesium.Transforms.headingPitchRollToFixedFrame(centro, hpr);
}

/** Conversão pura usada também pelos testes do contrato geográfico. */
export function aedifexParaEnu(anchor, xyz) {
  const angle = Number(anchor?.north_rotation_deg || 0) * Math.PI / 180;
  const c = Math.cos(angle), sin = Math.sin(angle);
  const [x, y, z] = xyz.map(Number);
  return [
    c * x + sin * z,
    sin * x - c * z,
    y - Number(anchor?.vertical_offset_m || 0)
  ];
}

/**
 * Matriz do derivado Aedifex no globo. A cena é Y-up, X=leste e Z=sul;
 * o frame local do Cesium é ENU (X=leste, Y=norte, Z=alto).
 */
export function matrizDerivadoFloorplanner(anchor) {
  const origin = anchor?.origin_wgs84;
  if (!Array.isArray(origin) || origin.length !== 3 || !origin.every(Number.isFinite)) {
    throw new TypeError("GeoAnchor inválido para derivado Floorplanner");
  }
  if (anchor.axis_policy !== "AEDIFEX_X_EAST_Y_UP_Z_SOUTH") {
    throw new TypeError(`axis_policy não suportada: ${anchor.axis_policy}`);
  }
  const angle = Number(anchor.north_rotation_deg || 0) * Math.PI / 180;
  const c = Math.cos(angle), sin = Math.sin(angle);
  const offset = Number(anchor.vertical_offset_m || 0);
  // Array column-major: model XYZ -> ENU, incluindo y-offset inverso.
  const local = Cesium.Matrix4.fromArray([
    c, sin, 0, 0,
    0, 0, 1, 0,
    sin, -c, 0, 0,
    0, 0, -offset, 1
  ]);
  const fixed = Cesium.Transforms.eastNorthUpToFixedFrame(
    Cesium.Cartesian3.fromDegrees(Number(origin[0]), Number(origin[1]), Number(origin[2]))
  );
  return Cesium.Matrix4.multiply(fixed, local, new Cesium.Matrix4());
}


/**
 * Aparência de cada peça diante do ambiente. Fica gravada na peça (projeto.json)
 * e é aplicada sem recarregar o GLB — dá para mexer com o gizmo na mão.
 *   sombra    "projeta" (lança e recebe) | "recebe" | "nenhuma"
 *   reflexo   0..2 · quanto do céu (IBL + mapa de ambiente) entra no material
 *   opacidade 0.05..1 · abaixo de 1 o material vira vidro
 *   cor       hex ou null (material original) · mistura = quanto a cor pesa
 */
export const RENDER_PADRAO = {
  sombra: "projeta",
  reflexo: 1,
  opacidade: 1,
  cor: null,
  mistura: 0.5
};

export const CAMPOS_RENDER = Object.keys(RENDER_PADRAO);

/** Só os campos de aparência da peça, com os padrões preenchidos. */
export function renderDaPeca(peca) {
  const saida = { ...RENDER_PADRAO };
  for (const campo of CAMPOS_RENDER) {
    if (peca && peca[campo] !== undefined && peca[campo] !== null) saida[campo] = peca[campo];
  }
  if (peca && peca.cor === null) saida.cor = null;
  return saida;
}

/** Modo de sombra do Cesium, limitado pelo que o perfil de qualidade permite. */
export function modoDeSombra(sombra, perfilTemSombra = true) {
  if (sombra === "nenhuma") return Cesium.ShadowMode.DISABLED;
  if (sombra === "recebe" || !perfilTemSombra) return Cesium.ShadowMode.RECEIVE_ONLY;
  return Cesium.ShadowMode.ENABLED;
}

/** URL do modelo conforme o LOD pedido.
 *  .gltf (JSON + .bin externo) vai direto: o pipeline de LOD só reescreve .glb. */
export function urlDoModelo(caminho, lod) {
  if (/^(blob:|http:\/\/|https:\/\/)/i.test(caminho)) return caminho;
  if (/\.gltf$/i.test(caminho)) return caminho.startsWith("/") ? caminho : `/${caminho}`;
  const res = RES_POR_LOD[lod] ?? 1024;
  // Rota que já resolve sozinha (banco de modelos): só carimba o LOD nela.
  // Reescrevê-la como arquivo transformava /banco-glb?id=… em um caminho.
  if (caminho.includes("?")) {
    return res > 0 ? `${caminho}&tex=${res}` : caminho;
  }
  const arquivo = encodeURIComponent(caminho);
  return res > 0 ? `/glb-lod?arquivo=${arquivo}&tex=${res}` : `/glb-corrigido?arquivo=${arquivo}`;
}

export class CenaManager {
  constructor() {
    this.viewer = null;
    this.modeloPredio = null;
    this.caminhoPredio = null;
    this.lodPredio = null;
    this.modeloFloorplanner = null;
    this.derivadoFloorplanner = null;
    this.pecasModelos = new Map();  // id -> Cesium.Model
    this.aoSelecionar = null;       // callback (id|null)
    this.aoRecarregarPredio = null; // callback (Cesium.Model) — corte reaplica
    this.aoRenderizarPeca = null;   // callback (id, Cesium.Model)
    // Ligado enquanto o assistente de posicionamento está no comando: o clique
    // é dele, não da seleção nem do gizmo.
    this.selecaoBloqueada = false;
  }

  inicializar(viewer) {
    this.viewer = viewer;

    // Troca de perfil de qualidade (inclusive a automática por FPS) repõe
    // sombra, reflexo e vidro em tudo que já está na cena.
    qualidadeApp.aoAjustarModelos = () => {
      if (this.modeloPredio) qualidadeApp.ajustarModelo(this.modeloPredio, { sombra: true });
      if (this.modeloFloorplanner) qualidadeApp.ajustarModelo(this.modeloFloorplanner, { sombra: true });
      for (const [id, modelo] of this.pecasModelos) {
        qualidadeApp.ajustarModelo(modelo, { sombra: true });
        const peca = this.obterPeca(id);
        if (peca) this.aplicarRenderPeca(peca);
      }
    };

    estadoApp.inscrever((st, origem) => {
      if (origem === "camera" || origem === "takes") return;

      if (origem === "posicao" || origem === "gizmo_predio") {
        this.aplicarTransformPredio(st.posicao);
      }
      if (origem === "lod_predio" && this.caminhoPredio) {
        this.carregarPredio(this.caminhoPredio, st.posicao, st.posicao.lod);
      }
      if (origem === "floorplanner_derivative" || origem === "carregamento_inicial") {
        void this.sincronizarDerivadoAtivo(st);
      }
      if (origem === "carregamento_inicial") {
        this.restaurarPecas(st.pecas);
      }
    });

    this.configurarSelecao();
  }

  // ---------------------------------------------------------------- prédio
  async carregarPredio(caminho, pos, lod = "equilibrado") {
    if (!this.viewer) return null;
    if (!caminho) {
      if (this.modeloPredio) this.viewer.scene.primitives.remove(this.modeloPredio);
      this.modeloPredio = null;
      this.caminhoPredio = null;
      return null;
    }
    this.caminhoPredio = caminho;
    this.lodPredio = lod;

    if (this.modeloPredio) {
      this.viewer.scene.primitives.remove(this.modeloPredio);
      this.modeloPredio = null;
    }

    try {
      const modelo = await Cesium.Model.fromGltfAsync({
        url: urlDoModelo(caminho, lod),
        modelMatrix: matrizDe(pos),
        scale: pos.escala || 1.0,
        incrementallyLoadTextures: true,
        shadows: Cesium.ShadowMode.ENABLED
      });
      modelo.id = { arczId: "predio", tipo: "predio" };
      qualidadeApp.ajustarModelo(modelo, { sombra: true });
      // Modelo carregado depois do pôr do sol precisa nascer com a luz de céu certa.
      if (this.iblAtual !== undefined) this.aplicarIluminacaoModelos(this.iblAtual);
      this.viewer.scene.primitives.add(modelo);
      this.modeloPredio = modelo;
      // Quem depende do objeto Model (corte, tampa) precisa saber que ele é outro.
      this.aoRecarregarPredio?.(modelo);
      this.viewer.scene.requestRender?.();
      return modelo;
    } catch (e) {
      console.error("Erro ao carregar predio principal:", e);
      return null;
    }
  }

  async carregarDerivadoFloorplanner(derivado) {
    if (!this.viewer || !derivado) return null;
    if (derivado.readonly !== true) throw new TypeError("Derivado Floorplanner precisa ser readonly");
    if (this.modeloFloorplanner && this.derivadoFloorplanner?.export_id === derivado.export_id) {
      return this.modeloFloorplanner;
    }
    this.removerDerivadoFloorplanner();
    const caminho = derivado.path || derivado.url;
    if (typeof caminho !== "string" || !caminho.includes("data/floorplanner/exports/")) {
      throw new TypeError("Caminho de derivado Floorplanner inválido");
    }
    try {
      const modelo = await Cesium.Model.fromGltfAsync({
        url: urlDoModelo(caminho, derivado.lod || "equilibrado"),
        modelMatrix: matrizDerivadoFloorplanner(derivado.geo_anchor),
        scale: 1,
        incrementallyLoadTextures: true,
        shadows: Cesium.ShadowMode.ENABLED
      });
      modelo.id = {
        arczId: `floorplanner:${derivado.project_id}`,
        tipo: "floorplanner_derivative",
        readonly: true
      };
      qualidadeApp.ajustarModelo(modelo, { sombra: true });
      if (this.iblAtual !== undefined) this.aplicarIbl(modelo, 1);
      this.viewer.scene.primitives.add(modelo);
      this.modeloFloorplanner = modelo;
      this.derivadoFloorplanner = derivado;
      this.viewer.scene.requestRender?.();
      return modelo;
    } catch (error) {
      console.error("Falha ao carregar derivado Floorplanner no globo:", error);
      this.modeloFloorplanner = null;
      this.derivadoFloorplanner = null;
      return null;
    }
  }

  removerDerivadoFloorplanner() {
    if (this.modeloFloorplanner && this.viewer) {
      this.viewer.scene.primitives.remove(this.modeloFloorplanner);
    }
    this.modeloFloorplanner = null;
    this.derivadoFloorplanner = null;
  }

  async sincronizarDerivadoAtivo(st = estadoApp.obter()) {
    const id = st.active_floorplanner_project_id;
    const derivado = (st.floorplanner_derivatives || [])
      .filter(item => item?.project_id === id && item?.status !== "ERROR")
      .sort((a, b) => Number(b.revision || 0) - Number(a.revision || 0))[0] || null;
    if (!derivado) { this.removerDerivadoFloorplanner(); return null; }
    return this.carregarDerivadoFloorplanner(derivado);
  }

  aplicarTransformPredio(pos) {
    if (!this.modeloPredio) return;
    this.modeloPredio.modelMatrix = matrizDe(pos);
    this.modeloPredio.scale = pos.escala || 1.0;
    this.viewer.scene.requestRender?.();
  }

  /** LOD por distância: texturas de 1024 só valem a pena perto do prédio. */
  ligarLodAutomatico() {
    if (this.lodAutoLigado) return;
    this.lodAutoLigado = true;
    let ultimaTroca = 0;

    this.viewer.scene.postRender.addEventListener(() => {
      if (!this.lodAuto || !this.modeloPredio) return;
      const agora = performance.now();
      if (agora - ultimaTroca < 4000) return;   // não troca a cada quadro

      const p = normalizarPosicao(estadoApp.obter().posicao);
      const alvo = Cesium.Cartesian3.fromDegrees(p.lon, p.lat, p.alt || 0);
      const d = Cesium.Cartesian3.distance(this.viewer.camera.position, alvo);
      const desejado = d < 400 ? "equilibrado" : d < 1500 ? "medio" : "distante";

      if (desejado !== this.lodPredio) {
        ultimaTroca = agora;
        this.carregarPredio(this.caminhoPredio, estadoApp.obter().posicao, desejado);
      }
    });
  }

  definirLodAutomatico(ligado) {
    this.lodAuto = !!ligado;
    localStorage.setItem("arcz.lodAuto", ligado ? "1" : "0");
    if (ligado) this.ligarLodAutomatico();
  }

  /** Coloca a base do prédio na cota real do terreno (DEM Terrarium). */
  async assentarNoTerreno() {
    if (!this.viewer) return null;
    const pos = normalizarPosicao(estadoApp.obter().posicao);
    let altura = await alturaDoTerreno(pos.lon, pos.lat);
    if (altura === undefined || altura === null || Number.isNaN(altura)) {
      altura = this.viewer.scene.globe.getHeight(Cesium.Cartographic.fromDegrees(pos.lon, pos.lat));
    }
    if (altura === undefined || altura === null || Number.isNaN(altura)) return null;

    const anterior = pos.alt;
    const nova = Number(altura.toFixed(2));
    historicoApp.executar({
      nome: "Assentar no terreno",
      fazer: () => estadoApp.atualizar({ posicao: { alt: nova } }, "posicao"),
      desfazer: () => estadoApp.atualizar({ posicao: { alt: anterior } }, "posicao")
    });
    return nova;
  }

  // ----------------------------------------------------------------- peças
  restaurarPecas(pecas = []) {
    for (const [id, modelo] of this.pecasModelos) {
      this.viewer.scene.primitives.remove(modelo);
      this.pecasModelos.delete(id);
    }
    for (const peca of pecas) this.renderizarPecaNaCena(peca);
  }

  adicionarPeca(pecaConfig) {
    const novaPeca = {
      id: "peca_" + Date.now() + "_" + Math.floor(Math.random() * 1000),
      nome: pecaConfig.nome || "Peça",
      url: pecaConfig.url,
      lat: pecaConfig.lat,
      lon: pecaConfig.lon,
      alt: pecaConfig.alt ?? 0,
      rumo: pecaConfig.rumo || 0,
      escala: pecaConfig.escala || 1.0,
      lod: pecaConfig.lod || "medio",
      visivel: true,
      trava: false,
      obs: ""
    };

    historicoApp.executar({
      nome: `Adicionar ${novaPeca.nome}`,
      fazer: () => {
        estadoApp.atualizar(
          { pecas: [...estadoApp.obter().pecas, novaPeca], pecaSelecionadaId: novaPeca.id },
          "pecas"
        );
        this.realcarSelecao(novaPeca.id);
        this.renderizarPecaNaCena(novaPeca);
      },
      desfazer: () => {
        estadoApp.atualizar(
          {
            pecas: estadoApp.obter().pecas.filter(p => p.id !== novaPeca.id),
            pecaSelecionadaId: null
          },
          "pecas"
        );
        this.removerPecaDaCena(novaPeca.id);
      }
    });
    return novaPeca;
  }

  async renderizarPecaNaCena(peca) {
    if (!this.viewer) return null;
    this.removerPecaDaCena(peca.id);
    if (peca.visivel === false) return null;

    try {
      const modelo = await Cesium.Model.fromGltfAsync({
        url: urlDoModelo(peca.url, peca.lod || "medio"),
        modelMatrix: matrizDe(peca),
        scale: peca.escala || 1.0,
        shadows: Cesium.ShadowMode.ENABLED
      });
      modelo.id = { arczId: peca.id, tipo: "peca" };
      qualidadeApp.ajustarModelo(modelo, { sombra: true });
      this.aplicarSilhueta(modelo, estadoApp.obter().pecaSelecionadaId === peca.id);
      this.viewer.scene.primitives.add(modelo);
      this.pecasModelos.set(peca.id, modelo);
      // Só depois de entrar no mapa a peça pode receber a aparência dela e a luz
      // de céu da hora atual — senão nasce chapada, ou preta se o sol já se pôs.
      this.aplicarRenderPeca(peca);
      this.aoRenderizarPeca?.(peca.id, modelo);
      this.viewer.scene.requestRender?.();
      return modelo;
    } catch (e) {
      console.error(`Erro ao carregar peca ${peca.nome}:`, e);
      return null;
    }
  }

  /** Move/gira/escala sem recarregar o GLB (usado pelo gizmo, a cada quadro). */
  aplicarTransformPeca(peca) {
    const modelo = this.pecasModelos.get(peca.id);
    if (!modelo) return;
    modelo.modelMatrix = matrizDe(peca);
    modelo.scale = peca.escala || 1.0;
  }

  atualizarPeca(id, patch, origem = "pecas") {
    const pecas = estadoApp.obter().pecas.map(p => (p.id === id ? { ...p, ...patch } : p));
    estadoApp.atualizar({ pecas }, origem);
    const peca = pecas.find(p => p.id === id);
    if (peca) {
      if (patch.lod !== undefined || patch.url !== undefined || patch.visivel !== undefined) {
        this.renderizarPecaNaCena(peca);
      } else {
        this.aplicarTransformPeca(peca);
        // Aparência muda no modelo vivo: recarregar o GLB por causa de um slider
        // de opacidade custaria segundos e piscaria a peça na tela.
        if (CAMPOS_RENDER.some(c => patch[c] !== undefined)) this.aplicarRenderPeca(peca);
      }
    }
    return peca;
  }

  obterPeca(id) {
    return estadoApp.obter().pecas.find(p => p.id === id) || null;
  }

  removerPecaDaCena(id) {
    const modelo = this.pecasModelos.get(id);
    if (modelo) {
      this.viewer.scene.primitives.remove(modelo);
      this.pecasModelos.delete(id);
    }
  }

  removerPecaSelecionada() {
    const id = estadoApp.obter().pecaSelecionadaId;
    if (!id) return false;
    const peca = this.obterPeca(id);
    if (!peca) return false;

    historicoApp.executar({
      nome: `Remover ${peca.nome}`,
      fazer: () => {
        estadoApp.atualizar(
          { pecas: estadoApp.obter().pecas.filter(p => p.id !== id), pecaSelecionadaId: null },
          "pecas"
        );
        this.removerPecaDaCena(id);
      },
      desfazer: () => {
        estadoApp.atualizar({ pecas: [...estadoApp.obter().pecas, peca], pecaSelecionadaId: id }, "pecas");
        this.renderizarPecaNaCena(peca);
      }
    });
    return true;
  }

  duplicarPecaSelecionada() {
    const peca = this.obterPeca(estadoApp.obter().pecaSelecionadaId);
    if (!peca) return null;
    return this.adicionarPeca({ ...peca, nome: `${peca.nome} (copia)`, lon: peca.lon + 0.00003 });
  }

  // -------------------------------------------------------------- seleção
  configurarSelecao() {
    const handler = new Cesium.ScreenSpaceEventHandler(this.viewer.scene.canvas);
    handler.setInputAction((clique) => {
      if (this.selecaoBloqueada) return;
      const alvo = this.viewer.scene.pick(clique.position);
      // Clicar numa alça do gizmo não pode desmarcar quem está sendo editado.
      if (alvo?.id?.arczGizmo) return;
      const metadata = alvo?.id || alvo?.primitive?.id || null;
      const id = metadata?.arczId || null;
      if (metadata?.tipo === "floorplanner_derivative" || metadata?.readonly) {
        this.selecionar(null);
        return;
      }
      this.selecionar(id && id !== "predio" ? id : null);
    }, Cesium.ScreenSpaceEventType.LEFT_CLICK);
    this.handlerSelecao = handler;
  }

  selecionar(id) {
    if (estadoApp.obter().pecaSelecionadaId === id) return;
    estadoApp.atualizar({ pecaSelecionadaId: id }, "selecao");
    this.realcarSelecao(id);
    if (this.aoSelecionar) this.aoSelecionar(id);
  }

  /** Contorno luminoso na peça selecionada — feedback de "isto está no gizmo". */
  realcarSelecao(id) {
    for (const [pid, modelo] of this.pecasModelos) this.aplicarSilhueta(modelo, pid === id);
    this.viewer?.scene?.requestRender?.();
  }

  aplicarSilhueta(modelo, ligada) {
    if (!modelo) return;
    try {
      modelo.silhouetteColor = Cesium.Color.fromCssColorString("#ffd166");
      modelo.silhouetteSize = ligada ? 2.5 : 0;
    } catch (e) {
      // Silhueta exige stencil buffer; sem ele a seleção segue funcionando.
      console.debug("Silhueta de seleção indisponível neste renderer:", e);
    }
  }

  /** Ponto do terreno (ou do modelo) sob um pixel da tela. */
  pontoNaCena(x, y) {
    const posicao = new Cesium.Cartesian2(x, y);
    let ponto = this.viewer.scene.pickPosition(posicao);
    if (!Cesium.defined(ponto)) {
      const raio = this.viewer.camera.getPickRay(posicao);
      if (raio) ponto = this.viewer.scene.globe.pick(raio, this.viewer.scene);
    }
    if (!Cesium.defined(ponto)) return null;
    const carto = Cesium.Cartographic.fromCartesian(ponto);
    return {
      lat: Cesium.Math.toDegrees(carto.latitude),
      lon: Cesium.Math.toDegrees(carto.longitude),
      alt: carto.height,
      cartesiano: ponto
    };
  }

  /**
   * Luz do céu (IBL) em todos os modelos da cena.
   * Sem isto o prédio e as peças ficam pretos assim que o sol se põe: a luz
   * direta cai a zero e não sobra nada iluminando as faces.
   */
  aplicarIluminacaoModelos(fator) {
    const f = Math.max(0, Math.min(1, Number(fator) || 0));
    this.iblAtual = f;

    this.aplicarIbl(this.modeloPredio, 1);
    this.aplicarIbl(this.modeloFloorplanner, 1);
    for (const [id, modelo] of this.pecasModelos) {
      this.aplicarIbl(modelo, renderDaPeca(this.obterPeca(id)).reflexo);
    }
    this.sincronizarMapasDeAmbiente();
  }

  /** Luz do céu num modelo, dosada pelo `reflexo` que a peça pediu. */
  aplicarIbl(modelo, reflexo = 1) {
    if (!modelo?.imageBasedLighting) return;
    const f = Math.max(0, Math.min(2, (this.iblAtual ?? 1) * (Number(reflexo) ?? 1)));
    try {
      modelo.imageBasedLighting.imageBasedLightingFactor = new Cesium.Cartesian2(f, f);
    } catch (e) {
      // Build sem IBL configurável: o material continua visível com a luz direta.
      console.debug("IBL configurável indisponível neste modelo:", e);
    }
  }

  /**
   * Reflexo do céu no vidro seguindo o sol.
   *
   * O DynamicEnvironmentMapManager refaz o cubemap sozinho quando o relógio
   * anda mais que `maximumSecondsDifference` — mas isso vem 3600 s de fábrica.
   * Com uma hora de tolerância, arrastar o horário de 8h para 8h50 mudava a luz
   * direta e a atmosfera enquanto o vidro seguia refletindo o céu de antes.
   * Baixar para 5 min basta: medido no viewer, 5 h de sol refazem o mapa e
   * 36 s não — e sem forçar `reset()`, que regeneraria um cubemap por modelo a
   * cada quadro de arraste do slider. Mudança de clima já dispara sozinha: o
   * Cesium também compara a atmosfera, não só o relógio.
   */
  sincronizarMapasDeAmbiente() {
    const amb = estadoApp.obter().ambiente || {};
    const ajustar = (modelo, reflexo) => {
      const gerente = modelo?.environmentMapManager;
      if (!gerente) return;
      try {
        gerente.enabled = reflexo > 0 && qualidadeApp.hdrLigado();
        gerente.maximumSecondsDifference = 300;
        // Reflexo mais forte = céu mais presente no material.
        gerente.brightness = Math.max(0.05, Math.min(3, reflexo));
        gerente.atmosphereScatteringIntensity = amb.nuvens === false ? 2 : 2.4;
      } catch (e) { /* build sem mapa de ambiente dinâmico */ }
    };

    ajustar(this.modeloPredio, 1);
    ajustar(this.modeloFloorplanner, 1);
    for (const [id, modelo] of this.pecasModelos) {
      ajustar(modelo, renderDaPeca(this.obterPeca(id)).reflexo);
    }
    this.viewer?.scene?.requestRender?.();
  }

  /**
   * Sombra, reflexo, vidro e cor da peça — sem recarregar o GLB.
   * `color` carrega a opacidade junto: alpha < 1 já joga o modelo na passagem
   * translúcida, e `colorBlendAmount` 0 mantém a textura original intacta.
   */
  aplicarRenderPeca(peca) {
    const modelo = this.pecasModelos.get(peca?.id);
    if (!modelo) return null;
    const r = renderDaPeca(peca);
    try {
      modelo.shadows = modoDeSombra(r.sombra, qualidadeApp.sombrasLigadas());

      const base = r.cor ? Cesium.Color.fromCssColorString(r.cor) : Cesium.Color.WHITE;
      modelo.color = base.withAlpha(Math.max(0.05, Math.min(1, r.opacidade)));
      modelo.colorBlendMode = Cesium.ColorBlendMode.MIX;
      modelo.colorBlendAmount = r.cor ? Math.max(0, Math.min(1, r.mistura)) : 0;

      this.aplicarIbl(modelo, r.reflexo);
      this.sincronizarMapasDeAmbiente();
    } catch (e) {
      console.warn(`Nao consegui aplicar a aparencia de ${peca?.nome}:`, e);
    }
    this.viewer?.scene?.requestRender?.();
    return modelo;
  }

  // ------------------------------------------------------------- material
  /** Cor de tinta no modelo. Preserva a opacidade: `color` carrega as duas
   *  coisas, e sobrescrever o alpha aqui apagava o vidro ajustado na Aparência. */
  definirCorMaterial(id, hexColor, blendAmount = 0.5) {
    const modelo = id === "predio" || !id ? this.modeloPredio : this.pecasModelos.get(id);
    if (!modelo) return false;
    try {
      const alpha = modelo.color?.alpha ?? 1;
      modelo.color = Cesium.Color.fromCssColorString(hexColor).withAlpha(alpha);
      modelo.colorBlendMode = Cesium.ColorBlendMode.MIX;
      modelo.colorBlendAmount = blendAmount;
      this.viewer?.scene?.requestRender?.();
      return true;
    } catch (e) {
      console.warn("Falha ao aplicar cor no modelo:", e);
      return false;
    }
  }

  limparCorMaterial(id) {
    const modelo = id === "predio" || !id ? this.modeloPredio : this.pecasModelos.get(id);
    if (!modelo) return;
    const alpha = modelo.color?.alpha ?? 1;
    modelo.colorBlendAmount = 0.0;
    modelo.color = Cesium.Color.WHITE.withAlpha(alpha);
    this.viewer?.scene?.requestRender?.();
  }
}

export const cenaApp = new CenaManager();
