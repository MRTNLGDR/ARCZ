// ARCZ Earth · Qualidade de imagem, uso de GPU e adaptação automática.
//
// Existe porque o visualizador rodava com msaaSamples 1, sem HDR, sem FXAA e
// sem sombra suave (imagem chapada) e, ao mesmo tempo, podia estar renderizando
// por software — no navegador embutido a GPU relatada era
// "Microsoft Basic Render Driver" (~5 FPS) mesmo com uma RTX 4090 na máquina.

export const PERFIS = {
  alto: {
    nome: "Alto",
    msaa: 4, fxaa: true, hdr: true, oit: true,
    resolucao: 1.0, sseGlobo: 1.5, cacheTiles: 300,
    sombras: true, sombraSuave: true, sombraTamanho: 2048, sombraDistancia: 2500,
    nuvens: true, atmosferaDinamica: true
  },
  equilibrado: {
    nome: "Equilibrado",
    msaa: 2, fxaa: true, hdr: true, oit: false,
    resolucao: 1.0, sseGlobo: 2, cacheTiles: 200,
    sombras: true, sombraSuave: true, sombraTamanho: 1024, sombraDistancia: 1500,
    nuvens: true, atmosferaDinamica: true
  },
  leve: {
    nome: "Leve",
    msaa: 1, fxaa: true, hdr: true, oit: false,
    resolucao: 0.9, sseGlobo: 2.5, cacheTiles: 120,
    sombras: false, sombraSuave: false, sombraTamanho: 1024, sombraDistancia: 800,
    nuvens: false, atmosferaDinamica: false
  },
  minimo: {
    nome: "Mínimo (sem GPU)",
    msaa: 1, fxaa: false, hdr: false, oit: false,
    // sseGlobo era 6. Acima de ~4 o terreno DEM salta vários níveis de LOD
    // entre tiles vizinhos e as saias não cobrem a diferença: aparecem
    // rachaduras e "buracos" no solo. 4 já é leve e não rasga o terreno.
    resolucao: 0.75, sseGlobo: 4, cacheTiles: 60,
    sombras: false, sombraSuave: false, sombraTamanho: 512, sombraDistancia: 400,
    nuvens: false, atmosferaDinamica: false
  }
};

/** Do mais pesado para o mais leve. */
export const ORDEM_PERFIS = ["alto", "equilibrado", "leve", "minimo"];

// Ajustes finos por cima do perfil, que o usuário controla e ficam gravados.
// Existem porque o teto de detalhe da imagem de satélite é do provedor, não da
// máquina: o Esri World Imagery para em z18 (z19 devolve o tile "sem dado").
// Passado esse ponto a única forma de ganhar nitidez é renderizar mais pixels
// do que a tela tem e reamostrar — supersampling — e ir buscar o tile um nível
// antes com o erro de tela menor.
export const SUPERAMOSTRAGEM = [
  { valor: 1, nome: "1× (nativo)" },
  { valor: 1.25, nome: "1,25× (+56% pixels)" },
  { valor: 1.5, nome: "1,5× (+125% pixels)" },
  { valor: 2, nome: "2× (4× pixels)" }
];

export const DETALHE_MAPA = [
  { valor: 1.0, nome: "Máximo (tile 1 nível antes)" },
  { valor: 1.5, nome: "Alto" },
  { valor: 2, nome: "Equilibrado" },
  { valor: 3, nome: "Leve" }
];

/** Backend de aceleração que o Chrome/Edge escolheu, lido da string da GPU. */
export function backendDaGpu(nome = "") {
  const m = String(nome).match(/\b(D3D11on12|D3D11|D3D12|D3D9|Vulkan|OpenGL|Metal)\b/i);
  return m ? m[1] : "—";
}

const MARCAS_SOFTWARE = ["swiftshader", "basic render", "llvmpipe", "software", "microsoft basic"];

/** Identifica a GPU real por trás do contexto WebGL. */
export function identificarGpu(cena) {
  const info = { nome: "desconhecida", fabricante: "", software: true, webgl: "" };
  try {
    const gl = cena.context._gl;
    info.webgl = gl.getParameter(gl.VERSION);
    const dbg = gl.getExtension("WEBGL_debug_renderer_info");
    if (dbg) {
      info.nome = gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL) || "desconhecida";
      info.fabricante = gl.getParameter(dbg.UNMASKED_VENDOR_WEBGL) || "";
    } else {
      info.nome = gl.getParameter(gl.RENDERER) || "desconhecida";
    }
    const alvo = info.nome.toLowerCase();
    info.software = MARCAS_SOFTWARE.some(m => alvo.includes(m));
  } catch (e) {
    /* mantém desconhecida */
  }
  return info;
}

/** Nome curto para caber no rodapé. */
export function gpuCurta(nome) {
  const txt = String(nome);
  // O Chrome entrega "ANGLE (NVIDIA, NVIDIA GeForce RTX 4090 Laptop GPU (0x…)
  // Direct3D11 …, D3D11)". Casar só até a primeira vírgula devolvia "NVIDIA" —
  // o modelo, que é o que interessa, vem depois dela.
  const modelo = txt.match(
    /((?:GeForce|Radeon|Arc|Iris|UHD|HD Graphics|Quadro|RTX|GTX|Apple)[^,()]*)/i);
  if (modelo) {
    // Corta a cauda de API que a ANGLE cola no nome ("… Direct3D11 vs_5_0").
    return modelo[1]
      .replace(/\s+(Direct3D|D3D|OpenGL|Vulkan|vs_|ps_).*$/i, "")
      .replace(/\s+/g, " ").trim().slice(0, 34);
  }
  const marca = txt.match(
    /(NVIDIA[^,)]*|AMD[^,)]*|Intel[^,)]*|Apple[^,)]*|Basic Render[^,)]*|SwiftShader[^,)]*|llvmpipe[^,)]*)/i);
  return (marca ? marca[1] : txt).replace(/\s*\(0x[0-9A-Fa-f]+\)/, "").trim().slice(0, 34);
}

export class Qualidade {
  constructor() {
    this.viewer = null;
    this.perfil = "equilibrado";
    this.automatico = true;
    this.gpu = null;
    this.fps = 0;
    this.quadros = 0;
    this.marco = 0;
    this.janelasRuins = 0;
    this.janelasBoas = 0;
    this.aoMedir = null;
    this.aoTrocarPerfil = null;   // ambiente.js reaplica o clima depois da troca
    this.aoAjustarModelos = null; // cena.js repõe sombra/reflexo nos modelos vivos
    // Ajustes finos por cima do perfil (null = segue o perfil).
    this.superAmostragem = 1;
    this.msaaManual = null;
    this.detalheMapa = null;
  }

  /** Capacidades reais do contexto WebGL — o que a GPU aceita, não o que se pede. */
  limites() {
    const cena = this.viewer?.scene;
    const L = Cesium.ContextLimits || {};
    return {
      gpu: this.gpu?.nome || "—",
      backend: backendDaGpu(this.gpu?.nome),
      software: !!this.gpu?.software,
      webgl: this.gpu?.webgl || "—",
      webgl2: !!cena?.context?.webgl2,
      anisotropia: L.maximumTextureFilterAnisotropy ?? 0,
      texturaMax: L.maximumTextureSize ?? 0,
      msaaMax: L.maximumSamples ?? 1,
      hdrSuportado: !!cena?.highDynamicRangeSupported
    };
  }

  /** Pixels realmente renderizados por pixel de tela. */
  escalaEfetiva() {
    return Number((this.superAmostragem * Math.min(window.devicePixelRatio || 1, 1.5)).toFixed(2));
  }

  definirSuperAmostragem(valor) {
    this.superAmostragem = Math.max(0.5, Math.min(2, Number(valor) || 1));
    localStorage.setItem("arcz.superAmostragem", String(this.superAmostragem));
    this.aplicar(this.perfil);
    return this.escalaEfetiva();
  }

  definirMsaa(amostras) {
    const max = Cesium.ContextLimits?.maximumSamples ?? 4;
    this.msaaManual = amostras ? Math.max(1, Math.min(Number(amostras), max)) : null;
    localStorage.setItem("arcz.msaa", this.msaaManual ? String(this.msaaManual) : "");
    this.aplicar(this.perfil);
    return this.msaaManual;
  }

  definirDetalheMapa(sse) {
    this.detalheMapa = sse ? Number(sse) : null;
    localStorage.setItem("arcz.detalheMapa", this.detalheMapa ? String(this.detalheMapa) : "");
    this.aplicar(this.perfil);
    return this.detalheMapa;
  }

  /** O perfil atual permite HDR (e, com ele, mapa de ambiente no vidro)? */
  hdrLigado() {
    return !!PERFIS[this.perfil]?.hdr;
  }

  /** O perfil atual desenha sombra projetada? */
  sombrasLigadas() {
    return !!PERFIS[this.perfil]?.sombras;
  }

  inicializar(viewer) {
    this.viewer = viewer;
    this.gpu = identificarGpu(viewer.scene);

    const salvo = localStorage.getItem("arcz.perfil");
    this.automatico = localStorage.getItem("arcz.perfilAuto") !== "0";
    this.superAmostragem = Number(localStorage.getItem("arcz.superAmostragem")) || 1;
    this.msaaManual = Number(localStorage.getItem("arcz.msaa")) || null;
    this.detalheMapa = Number(localStorage.getItem("arcz.detalheMapa")) || null;
    this.perfil = salvo && PERFIS[salvo] ? salvo : (this.gpu.software ? "minimo" : "alto");

    // "Mínimo (sem GPU)" gravado numa máquina COM GPU só pode ter vindo de uma
    // queda espúria do adaptador (janela minimizada, aba em segundo plano ou
    // uma rajada de carregamento de tiles). Ele fica preso no localStorage e a
    // pessoa passa a ver terreno rachado e imagem borrada com uma RTX na
    // máquina. Se a GPU é real, esse perfil não é uma escolha válida.
    if (!this.gpu.software && this.perfil === "minimo") {
      console.warn("ARCZ: perfil 'mínimo' salvo, mas a GPU é real — subindo para 'equilibrado'.");
      this.perfil = "equilibrado";
    }

    this.aplicar(this.perfil);
    this.medirContinuamente();
    return this.gpu;
  }

  /** Piso de qualidade: com GPU real nunca se cai até o perfil de emergência. */
  perfilMaisBaixo() {
    return this.gpu?.software ? "minimo" : "leve";
  }

  aplicar(nome) {
    const p = PERFIS[nome];
    if (!p || !this.viewer) return null;
    this.perfil = nome;
    localStorage.setItem("arcz.perfil", nome);

    const v = this.viewer;
    const cena = v.scene;

    // Antisserrilhamento e tonemapping.
    // O limite só é conhecido depois que o contexto WebGL existe. Enquanto for
    // desconhecido não dá para cortar a escolha do usuário: cair no valor do
    // perfil transformava "MSAA 8×" em 2× logo no arranque.
    const msaaMax = Cesium.ContextLimits?.maximumSamples;
    const pedido = this.msaaManual || p.msaa;
    try {
      cena.msaaSamples = msaaMax > 0 ? Math.min(pedido, msaaMax) : pedido;
    } catch (e) { /* GPU sem MSAA */ }
    try { cena.postProcessStages.fxaa.enabled = p.fxaa; } catch (e) { /* sem FXAA */ }
    if (cena.highDynamicRangeSupported) cena.highDynamicRange = p.hdr;
    try { cena.orderIndependentTranslucency = p.oit; } catch (e) { /* sem OIT */ }

    // Resolução interna e detalhe do terreno. A superamostragem multiplica por
    // cima do perfil: é ela que devolve nitidez quando o zoom passa do último
    // nível de tile que o provedor de imagem tem.
    v.resolutionScale =
      p.resolucao * this.superAmostragem * Math.min(window.devicePixelRatio || 1, 1.5);
    cena.globe.maximumScreenSpaceError = this.detalheMapa || p.sseGlobo;
    cena.globe.tileCacheSize = p.cacheTiles;
    cena.globe.preloadSiblings = p.sseGlobo <= 2;

    // Sombras.
    v.shadows = p.sombras;
    if (v.shadowMap) {
      v.shadowMap.enabled = p.sombras;
      v.shadowMap.softShadows = p.sombraSuave;
      v.shadowMap.size = p.sombraTamanho;
      v.shadowMap.maximumDistance = p.sombraDistancia;
      v.shadowMap.darkness = 0.32;
      v.shadowMap.normalOffset = true;
    }

    // Só o que é decisão de desempenho. Cor da luz, atmosfera, névoa e sombra
    // são do clima (ambiente.js) e são reaplicadas pelo callback abaixo.
    this.aplicarLuz(p);
    cena.requestRender?.();
    this.aoTrocarPerfil?.(nome, p);
    // O adaptador de FPS troca de perfil no meio do uso: sem isto, a peça
    // carregada em "leve" nunca ganha o mapa de ambiente ao voltar para "alto",
    // e a que nasceu em "alto" continua projetando sombra num perfil sem sombra.
    this.aoAjustarModelos?.(nome, p);
    return p;
  }

  /**
   * Ajustes de luz que dependem do PERFIL, não da hora.
   * A versão anterior recriava `scene.light` em branco 2.0 e reescrevia a
   * névoa em valores fixos toda vez que o adaptador de FPS trocava de perfil —
   * ou seja, no meio do uso o pôr do sol simplesmente voltava a meio-dia.
   */
  aplicarLuz(p) {
    const cena = this.viewer.scene;
    const globo = cena.globe;

    globo.showGroundAtmosphere = true;
    if ("translucency" in globo) globo.translucency.enabled = false;

    if (cena.atmosphere && Cesium.DynamicAtmosphereLightingType) {
      cena.atmosphere.dynamicLighting = p.atmosferaDinamica
        ? Cesium.DynamicAtmosphereLightingType.SUNLIGHT
        : Cesium.DynamicAtmosphereLightingType.NONE;
    }
    if (cena.skyAtmosphere) cena.skyAtmosphere.show = true;
    if (cena.skyBox) cena.skyBox.show = true;

    cena.fog.enabled = true;
    cena.fog.screenSpaceErrorFactor = p.sseGlobo <= 2 ? 2 : 4;
  }

  /** Deixa o modelo importado reagir à luz do ambiente em vez de ficar chapado. */
  ajustarModelo(modelo, { sombra = true } = {}) {
    if (!modelo) return;
    const p = PERFIS[this.perfil];
    try {
      modelo.shadows = sombra && p.sombras
        ? Cesium.ShadowMode.ENABLED
        : Cesium.ShadowMode.RECEIVE_ONLY;
      if (modelo.imageBasedLighting) {
        const f = p.hdr ? 1.0 : 0.6;
        modelo.imageBasedLighting.imageBasedLightingFactor = new Cesium.Cartesian2(f, f);
      }
      if ("environmentMapManager" in modelo && modelo.environmentMapManager) {
        modelo.environmentMapManager.enabled = p.hdr;   // reflexo do céu no vidro
      }
      modelo.backFaceCulling = false;   // modelo de SketchUp costuma ter normal invertida
      modelo.lightColor = undefined;    // usa a luz da cena, não uma cor fixa
    } catch (e) {
      console.warn("Nao consegui ajustar a iluminacao do modelo:", e);
    }
  }

  medirContinuamente() {
    this.marco = performance.now();
    this.viewer.scene.postRender.addEventListener(() => {
      this.quadros++;
      const agora = performance.now();
      const dt = agora - this.marco;
      if (dt < 1000) return;

      this.fps = Math.round((this.quadros * 1000) / dt);
      this.quadros = 0;
      this.marco = agora;
      this.aoMedir?.(this.fps, this.perfil);
      if (this.automatico) this.adaptar();
    });

    // Voltar de uma aba em segundo plano começa com a janela de medição suja
    // (nenhum quadro desenhado enquanto esteve oculta). Zerar aqui evita que o
    // primeiro segundo de volta seja lido como "a máquina não aguenta".
    document.addEventListener("visibilitychange", () => {
      this.quadros = 0;
      this.marco = performance.now();
      this.janelasRuins = 0;
      this.janelasBoas = 0;
    });
  }

  /**
   * Cai de perfil quando trava; volta a subir quando sobra folga.
   *
   * Duas travas que faltavam e custaram caro: sem elas, uma janela minimizada
   * (0 quadro por segundo) ou uma rajada de carregamento de tiles derrubava o
   * perfil até "Mínimo (sem GPU)" — e o valor ficava gravado no localStorage.
   * O resultado é terreno rachado e imagem em 60% de resolução numa máquina
   * com RTX 4090.
   */
  adaptar() {
    // Sem janela visível não há quadro para medir: FPS aqui não diz nada.
    if (document.hidden || !document.hasFocus()) {
      this.janelasRuins = 0;
      this.janelasBoas = 0;
      return;
    }
    // Carregar terreno e textura trava o quadro por motivo de rede, não de GPU.
    if (!this.viewer.scene.globe.tilesLoaded) {
      this.janelasRuins = 0;
      return;
    }

    const ordem = ORDEM_PERFIS;
    const i = ordem.indexOf(this.perfil);
    const piso = ordem.indexOf(this.perfilMaisBaixo());

    if (this.fps > 0 && this.fps < 18) {
      this.janelasBoas = 0;
      if (++this.janelasRuins >= 5 && i < piso) {
        this.janelasRuins = 0;
        this.aplicar(ordem[i + 1]);
        console.warn(`ARCZ: ${this.fps} FPS — caindo para o perfil ${PERFIS[ordem[i + 1]].nome}`);
      }
      return;
    }
    if (this.fps > 45 && !this.gpu.software) {
      this.janelasRuins = 0;
      if (++this.janelasBoas >= 5 && i > 0) {
        this.janelasBoas = 0;
        this.aplicar(ordem[i - 1]);
        console.info(`ARCZ: ${this.fps} FPS — subindo para o perfil ${PERFIS[ordem[i - 1]].nome}`);
      }
      return;
    }
    this.janelasRuins = 0;
    this.janelasBoas = 0;
  }

  definirAutomatico(ligado) {
    this.automatico = !!ligado;
    localStorage.setItem("arcz.perfilAuto", ligado ? "1" : "0");
  }

  /** Texto pronto para a barra de estado. */
  resumo() {
    return {
      gpu: gpuCurta(this.gpu?.nome || "—"),
      software: !!this.gpu?.software,
      perfil: PERFIS[this.perfil].nome,
      fps: this.fps
    };
  }
}

export const qualidadeApp = new Qualidade();
