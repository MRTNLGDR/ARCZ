// ARCZ · Estado Único Central + persistência atômica coordenada
// Debounce silencioso: 600 ms · teto absoluto: 5 s · retry exponencial.
//
// Regra que este módulo garante: `notificar` só roda quando algum valor mudou
// de verdade. A versão anterior tratava todo patch de objeto como mudança, e
// como a câmera escreve no estado a cada quadro, todos os observers rodavam
// 60x por segundo (o ambiente chegava a recriar a camada de satélite e as
// nuvens a cada quadro).

const SAVE_DEBOUNCE_MS = 600;
const SAVE_MAX_LATENCY_MS = 5000;
const SAVE_RETRY_MAX_MS = 30000;

function novaSeedProjeto() {
  try {
    const values = new Uint32Array(2);
    globalThis.crypto?.getRandomValues(values);
    return Number((BigInt(values[0]) << 21n) ^ BigInt(values[1] & 0x1fffff));
  } catch {
    return Math.max(1, Math.floor(Date.now() * 1000 + Math.random() * 1000));
  }
}

/** Compara dois valores como o estado os usa: escalares, arrays e objetos rasos. */
export function iguais(a, b) {
  if (a === b) return true;
  if (typeof a === "number" && typeof b === "number") {
    return Number.isNaN(a) && Number.isNaN(b);
  }
  if (a === null || b === null || typeof a !== "object" || typeof b !== "object") return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const chavesA = Object.keys(a);
  const chavesB = Object.keys(b);
  if (chavesA.length !== chavesB.length) return false;
  return chavesA.every(k => iguais(a[k], b[k]));
}

/** Aplica o patch em cima do alvo e diz se alguma coisa mudou. */
export function mesclar(alvo, patch) {
  let mudou = false;
  for (const chave of Object.keys(patch)) {
    const valor = patch[chave];
    const atual = alvo[chave];
    if (valor && typeof valor === "object" && !Array.isArray(valor) &&
        atual && typeof atual === "object" && !Array.isArray(atual)) {
      const mesclado = { ...atual, ...valor };
      if (!iguais(atual, mesclado)) {
        alvo[chave] = mesclado;
        mudou = true;
      }
    } else if (!iguais(atual, valor)) {
      alvo[chave] = valor;
      mudou = true;
    }
  }
  return mudou;
}

/**
 * Converte o projeto.json antigo (posicao.lugar / posicao.cena / posicao.camera)
 * para o formato plano usado hoje. Projeto já novo passa direto.
 */
export function migrarProjeto(dados) {
  if (!dados || !dados.posicao || !dados.posicao.lugar) return dados;
  const antiga = dados.posicao;
  const lugar = antiga.lugar || {};
  const cena = antiga.cena || {};
  const saida = { ...dados };

  saida.posicao = {
    lat: lugar.lat, lon: lugar.lon, alt: lugar.alt,
    rumo: lugar.rumo, escala: lugar.escala, colar: lugar.colar,
    lod: cena.qualidade === "original" ? "original" : (cena.qualidade || "equilibrado")
  };
  saida.ambiente = {
    ...(dados.ambiente || {}),
    imagery: cena.imagery ?? dados.ambiente?.imagery,
    relevo: cena.relevo ?? dados.ambiente?.relevo,
    hora: cena.hora ?? dados.ambiente?.hora,
    sombra: cena.sombra ?? dados.ambiente?.sombra,
    qualidade: cena.qualidade ?? dados.ambiente?.qualidade
  };
  if (antiga.camera && !dados.camera?.lat) saida.camera = { ...antiga.camera };

  // Segredos de conectores nunca pertencem ao projeto. Versões antigas
  // persistiam token_mapbox no JSON; a migração remove esse campo. Conectores
  // opcionais usam cofre/sessão local e continuam desligados por padrão.
  if (saida.ambiente && Object.prototype.hasOwnProperty.call(saida.ambiente, "token_mapbox")) {
    delete saida.ambiente.token_mapbox;
  }
  saida.network_mode = ["offline_strict", "local_lan", "import_assisted"].includes(saida.network_mode)
    ? saida.network_mode
    : "offline_strict";
  return saida;
}

export function estadoInicial() {
  return {
    versao: 1,
    schema_version: 2,
    project_seed: novaSeedProjeto(),
    // A rede é negada por padrão. `import_assisted` só é escolhido pelo usuário
    // para uma sessão de importação/conector e não torna o core dependente dele.
    network_mode: "offline_strict",
    posicao: {
      lat: -27.1545,
      lon: -48.5022,
      alt: 10,
      rumo: 119,
      escala: 1,
      colar: false,
      eixos_locais: false,
      lod: "equilibrado"
    },
    ambiente: {
      // O boot precisa funcionar sem pacotes opcionais e sem internet.
      // DEM local é opt-in; Natural Earth II vem no vendor Cesium local.
      relevo: "ellipsoid",
      imagery: "naturalearth_local",
      qualidade: "equilibrado",

      // Entorno OSM local (prédios/vias reais, gerados pelo arcz-osm-cli —
      // ver app/entorno.js). Desligado por padrão: a imagem de satélite já
      // mostra os prédios reais em foto, e caixas genéricas por cima às
      // vezes pioram o resultado visual. Fica opt-in pra quem quer massing
      // 3D de verdade (sombra, obstrução, caminhada).
      entorno_osm: false,
      entorno_osm_adensar: false,

      // Sol: data + hora CIVIL DO SÍTIO + fuso. A hora sozinha não define a
      // posição do sol — sem data não há estação, e sem fuso a mesma "15:00"
      // cai em céus diferentes conforme o relógio da máquina que abriu.
      data: new Date().toISOString().slice(0, 10),
      hora: 15,
      fuso: -3,
      fuso_auto: true,
      animar_sol: false,
      velocidade_sol: 300,
      luz_sol: true,
      sol_intensidade: 2.6,
      brilho_ambiente: 1.0,

      // Clima.
      condicao: "limpo",
      nuvens: true,
      nuvens_cobertura: 20,
      nuvens_altura: 1200,
      vento_kmh: 12,
      vento_rumo: 90,
      neblina: 5,
      aerossol: 10,
      precipitacao: "auto",

      // Sombras. `sombra_relevo` liga o terreno como PROJETOR de sombra: dá
      // o morro escurecendo o lote no fim da tarde, mas em terreno DEM de
      // resolução baixa produz acne e manchas no solo. Fica opt-in.
      sombra: true,
      sombra_relevo: false,
      sombra_alcance: 4000,
      sombra_suave: true,

      fog: true,
      bloom: false,
      estrelas: true,
      lua: true
    },
    camera: {
      lat: -27.1545,
      lon: -48.5022,
      alt: 150,
      heading: 0,
      pitch: -30,
      roll: 0,
      fov: 60,
      fovMm: 35,
      distanciaAlvo: 0,
      dof: false,
      foco: 120,
      forca: 3,
      formato: "1.7778",
      resolucao: "1920"
    },

    // Ferramenta de corte: plano ativo + etapas salvas pelo usuário.
    corte: {
      ativo: false,
      eixo: "z",
      distancia: 12,
      invertido: false,
      tapar: true,
      cor: "#aeb8c7",
      pecas: true,
      etapas: []
    },

    // Recorte por perímetro (polígono no terreno) usado na exportação.
    recorte: {
      perimetro: [],
      formato: "glb",
      relevo: false,
      resolucao_relevo: 80
    },

    takes: [],
    pecas: [],
    lugares: [],
    pecaSelecionadaId: null,
    modoGizmo: "mover",

    // Extensões V2. Não são cache derivado: são o contrato persistente que
    // permite replay, regeneração diferencial e preservação de edição manual.
    active_region: null,
    region_profiles: {},
    plugins: {},
    procedural_layers: [],
    generation_manifests: [],
    overrides: {},
    tombstones: [],
    timeline: { schema_version: 1, fps: 30, duration_frames: 300, tracks: [] },
    render_jobs: [],
    source_registry: [],

    // Fusion V6: Aedifex é o kernel de autoria do edifício; o ARCZ continua
    // sendo a autoridade geográfica. IDs e revisões são persistidos, mas cenas
    // completas permanecem no banco transacional do Floorplanner.
    workspace_mode: "globo",
    active_floorplanner_project_id: null,
    floorplanner_projects: [],
    floorplanner_north_rotation_deg: 0,
    floorplanner_vertical_offset_m: 0,
    floorplanner_constraints: {},
    // Camadas locais, verificadas por SHA-256 e sempre somente leitura no Aedifex.
    floorplanner_context_layers: [],
    // Publicações GLB derivadas e versionadas. Nunca substituem o SceneSnapshot
    // editável armazenado pelo Floorplanner.
    floorplanner_derivatives: [],
    // Modelo principal opcional da cena legada; null é o default schema-valid.
    primary_model: null,
    floorplanner_layout: {
      schema_version: 1,
      show_globe: true,
      split_ratio: 0.38,
      auto_publish: true,
      auto_publish_delay_ms: 1800
    },
    reference_media: [],
    chat_sessions: [],
    panel_layout: { schema_version: 1, panels: {} },
    earth_presentation: {
      schema_version: 1,
      enabled: true,
      duration_ms: 6500,
      start_altitude_m: 24000000,
      end_altitude_m: 1500000,
      orbit_altitude_m: 1500000,
      clouds: true,
      skip_on_reduced_motion: true,
      atmosphere: true,
      stars: true,
      sun: true,
      moon: true,
      fog: true,
      fog_density: 0.00018,
      hue_shift: 0,
      saturation_shift: -0.05,
      brightness_shift: -0.03,
      orbit_heading_delta_deg: 14,
      cloud_count: 28,
      cloud_radius_m: 85000,
      cloud_altitude_m: 5200,
      cloud_brightness: 0.92,
      cancel_on_interaction: true,
      show_progress: true,
      persistent_procedural_clouds: true
    },
    save_revision: 0,
    content_hash: "",

    criado_em: new Date().toISOString(),
    atualizado_em: new Date().toISOString()
  };
}

export class EstadoManager {
  constructor({ fetchImpl = globalThis.fetch?.bind(globalThis), debounceMs = SAVE_DEBOUNCE_MS,
                maxLatencyMs = SAVE_MAX_LATENCY_MS } = {}) {
    this.estado = estadoInicial();
    this.observers = [];
    this.fetchImpl = fetchImpl;
    this.debounceMs = debounceMs;
    this.maxLatencyMs = maxLatencyMs;
    this.timerSalvar = null;
    this.timerFlushMaximo = null;
    this.timerRetry = null;
    this.salvamentoEmAndamento = null;
    this.salvarNovamente = false;
    this.serialMudanca = 0;
    this.falhasConsecutivas = 0;
    this.statusSave = "sincronizado"; // sincronizado | pendente | salvando | erro
    // Só grava depois que o projeto do disco foi lido: senão o estado padrão
    // sobrescreve a posição real do prédio antes mesmo de ela chegar.
    this.persistir = false;
  }

  obter() {
    return this.estado;
  }

  inscrever(observer) {
    this.observers.push(observer);
    return () => {
      this.observers = this.observers.filter(o => o !== observer);
    };
  }

  notificar(origem = "") {
    this.serialMudanca += 1;
    this.estado.atualizado_em = new Date().toISOString();
    for (const obs of this.observers) {
      try {
        obs(this.estado, origem);
      } catch (e) {
        console.error("Erro no observer de estado:", e);
      }
    }
    this.agendarSalvar();
  }

  /** Aplica o patch. Só notifica (e só agenda gravação) quando muda de verdade. */
  atualizar(patch, origem = "") {
    if (!patch) return false;
    const mudou = mesclar(this.estado, patch);
    if (mudou) this.notificar(origem);
    return mudou;
  }

  agendarSalvar() {
    if (!this.persistir) return;
    this.statusSave = "pendente";
    if (this.timerRetry) { clearTimeout(this.timerRetry); this.timerRetry = null; }
    if (this.timerSalvar) clearTimeout(this.timerSalvar);
    this.timerSalvar = setTimeout(() => {
      this.timerSalvar = null;
      void this.salvarNoServidor("debounce");
    }, this.debounceMs);
    // O teto não é reiniciado por movimento contínuo da câmera. Essa é a
    // correção do bug em que o debounce de 600 ms nunca disparava.
    if (!this.timerFlushMaximo) {
      this.timerFlushMaximo = setTimeout(() => {
        this.timerFlushMaximo = null;
        void this.salvarNoServidor("flush_maximo");
      }, this.maxLatencyMs);
    }
  }

  _limparTimersDeSave() {
    if (this.timerSalvar) clearTimeout(this.timerSalvar);
    if (this.timerFlushMaximo) clearTimeout(this.timerFlushMaximo);
    this.timerSalvar = null;
    this.timerFlushMaximo = null;
  }

  async salvarNoServidor(motivo = "manual") {
    if (!this.persistir) return this.statusSave;
    if (!this.fetchImpl) { this.statusSave = "erro"; return this.statusSave; }
    if (this.salvamentoEmAndamento) {
      this.salvarNovamente = true;
      return this.salvamentoEmAndamento;
    }
    this._limparTimersDeSave();
    const serialEnviado = this.serialMudanca;
    const snapshot = JSON.stringify(this.estado);
    this.statusSave = "salvando";
    this.salvamentoEmAndamento = (async () => {
      try {
        const res = await this.fetchImpl("/api/projeto", {
          method: "POST",
          headers: { "Content-Type": "application/json", "X-ARCZ-Save-Reason": motivo },
          body: snapshot
        });
        let data = null;
        try { data = await res.json(); } catch { /* erro HTTP sem JSON */ }
        if (!res.ok || !data?.ok) {
          const erro = new Error(data?.erro || data?.error?.message || `HTTP ${res.status}`);
          erro.status = res.status;
          throw erro;
        }
        if (Number.isInteger(data.save_revision)) this.estado.save_revision = data.save_revision;
        if (typeof data.content_hash === "string") this.estado.content_hash = data.content_hash;
        this.falhasConsecutivas = 0;
        const mudouDepoisDoSnapshot = this.serialMudanca !== serialEnviado || this.salvarNovamente;
        this.salvarNovamente = false;
        this.statusSave = mudouDepoisDoSnapshot ? "pendente" : "sincronizado";
        if (mudouDepoisDoSnapshot) this.agendarSalvar();
      } catch (e) {
        console.warn("Falha ao persistir projeto:", e);
        this.statusSave = "erro";
        this.falhasConsecutivas += 1;
        this.salvarNovamente = false;
        const atraso = Math.min(SAVE_RETRY_MAX_MS, 1000 * 2 ** Math.min(5, this.falhasConsecutivas - 1));
        if (this.persistir && !this.timerRetry) {
          this.timerRetry = setTimeout(() => {
            this.timerRetry = null;
            void this.salvarNoServidor("retry");
          }, atraso);
        }
      } finally {
        this.salvamentoEmAndamento = null;
      }
      return this.statusSave;
    })();
    return this.salvamentoEmAndamento;
  }

  async flushAgora() {
    this._limparTimersDeSave();
    if (this.timerRetry) { clearTimeout(this.timerRetry); this.timerRetry = null; }
    return this.salvarNoServidor("flush_manual");
  }

  async carregarDoServidor() {
    try {
      const res = await fetch("/api/projeto");
      if (!res.ok) return false;
      const dados = await res.json();
      if (!dados || Object.keys(dados).length === 0) return false;

      const migrado = migrarProjeto(dados);
      if (migrado?.ambiente && Object.prototype.hasOwnProperty.call(migrado.ambiente, "token_mapbox")) {
        migrado.ambiente = { ...migrado.ambiente };
        delete migrado.ambiente.token_mapbox;
      }
      if (!["offline_strict", "local_lan", "import_assisted"].includes(migrado?.network_mode)) {
        migrado.network_mode = "offline_strict";
      }

      // Carregamento não pode disparar gravação de volta no servidor.
      this.persistir = false;
      const base = estadoInicial();
      for (const chave of Object.keys(migrado)) {
        const valor = migrado[chave];
        if (valor && typeof valor === "object" && !Array.isArray(valor) && base[chave]) {
          base[chave] = { ...base[chave], ...valor };
        } else if (valor !== null && valor !== undefined) {
          base[chave] = valor;
        }
      }

      // Projeto antigo guardava takes como dicionário; a UI espera lista.
      for (const chave of ["takes", "pecas", "lugares"]) {
        const valor = base[chave];
        if (Array.isArray(valor)) continue;
        base[chave] = valor && typeof valor === "object" ? Object.values(valor) : [];
      }

      this.estado = base;
      this.notificar("carregamento_inicial");
      this.persistir = true;
      this.statusSave = "sincronizado";
      return true;
    } catch (e) {
      console.warn("Nao foi possivel carregar projeto do servidor:", e);
      this.persistir = true;
      return false;
    }
  }
}

export const estadoApp = new EstadoManager();
