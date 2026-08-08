// ARCZ · Entorno OSM local: prédios, vias e vegetação reais ao redor do
// projeto, gerados pelo arcz-osm-cli (crates/arcz-osm-cli) a partir do
// OpenStreetMap — sem Cesium Ion. O Rust gera o .glb (footprint real ou
// estimado, altura, telhado, cor da própria ortofoto); aqui só se carrega
// como qualquer outro modelo georreferenciado, igual ao prédio principal.
//
// Mesmo padrão de primitive add/remove que o antigo `alternarOsmBuildings`
// (removido de ambiente.js) usava para o tileset do Ion — só troca a fonte
// dos dados por uma local, servida por /api/entorno-osm[.glb].
import { estadoApp } from "./estado.js";
import { normalizarPosicao } from "./cena.js";
import { feedbackApp } from "./feedback.js";

/** Preset fixo de área: precisa bater com PRESETS_LADO_ENTORNO no servidor.py.
 * Sem UI de escolha por enquanto — 150 m cobre o quarteirão ao redor do
 * projeto sem pesar (glTF único, sem tiling/LOD). */
const LADO_M = 150;

/** ~1,1 m de grade: sem arredondar, ruído de float reancorava a cada quadro. */
const arred = v => Math.round(v * 1e5) / 1e5;

export class EntornoManager {
  constructor() {
    this.viewer = null;
    this.modelo = null;
    this.carregando = false;
    this.chaveAtual = null;      // "lat|lon|adensar" já carregada
    this.timerReancorar = null;
  }

  inicializar(viewer) {
    this.viewer = viewer;

    // Assina por conta própria, não via ambiente.js: a posição do projeto
    // pode mudar (arrastar o prédio, editar lat/lon) sem o campo entorno_osm
    // mudar junto — e aí o entorno ficaria ancorado no lugar antigo.
    estadoApp.inscrever((st, origem) => {
      if (origem === "camera") return;
      this.aoMudarPosicao(st);
    });
  }

  aoMudarPosicao(st) {
    if (!this.modelo) return; // desligado: nada pra reancorar
    if (this.timerReancorar) clearTimeout(this.timerReancorar);
    // Mesma janela de debounce de estado.js (agendarSalvar), por consistência.
    this.timerReancorar = setTimeout(() => this.reancorarSeMudou(st), 600);
  }

  reancorarSeMudou(st) {
    const amb = st.ambiente || {};
    const p = normalizarPosicao(st.posicao);
    const chave = `${arred(p.lat)}|${arred(p.lon)}|${amb.entorno_osm_adensar ? 1 : 0}`;
    if (chave === this.chaveAtual) return;
    this.alternarEntornoOsm(true, !!amb.entorno_osm_adensar);
  }

  /**
   * Liga/desliga a camada. `adensar` só é lido quando `ligado` é true.
   * Chamada de dois lugares: o diff de ambiente.js (toggle da UI) e o
   * reancorar acima (posição mudou com o toggle já ligado).
   */
  async alternarEntornoOsm(ligado, adensar = false) {
    if (!this.viewer) return;

    if (!ligado) {
      this.remover();
      return;
    }
    // Pedido já em voo: o próximo aoMudarPosicao (ou o clique seguinte)
    // resolve sozinho depois que este terminar.
    if (this.carregando) return;

    const p = normalizarPosicao(estadoApp.obter().posicao);
    const chave = `${arred(p.lat)}|${arred(p.lon)}|${adensar ? 1 : 0}`;
    if (chave === this.chaveAtual && this.modelo) return; // já é isto que está na tela

    this.carregando = true;
    feedbackApp.aviso("gerando entorno OSM…", "info", 20000);
    try {
      const qs = `lat=${p.lat}&lon=${p.lon}&lado=${LADO_M}&adensar=${adensar ? 1 : 0}`;

      const resMeta = await fetch(`/api/entorno-osm?${qs}`);
      const meta = await resMeta.json();
      if (!resMeta.ok || meta.erro) throw new Error(meta.erro || `HTTP ${resMeta.status}`);

      // O .glb nasce em ENU local com a cota do DEM já embutida nos vértices
      // (arcz-osm-cli amostra o mesmo relevo real que o /dem do viewer usa) —
      // a matriz só ancora no lugar certo do globo, sem reprojetar altura.
      const origem = Cesium.Cartesian3.fromDegrees(p.lon, p.lat, 0);
      const matriz = Cesium.Transforms.eastNorthUpToFixedFrame(origem);
      const novo = await Cesium.Model.fromGltfAsync({
        url: `/api/entorno-osm.glb?${qs}`,
        modelMatrix: matriz,
        heightReference: Cesium.HeightReference.NONE,
        shadows: estadoApp.obter().ambiente.sombra !== false
          ? Cesium.ShadowMode.ENABLED
          : Cesium.ShadowMode.DISABLED
      });

      this.remover();
      this.viewer.scene.primitives.add(novo);
      this.modelo = novo;
      this.chaveAtual = chave;

      const predios = (meta.predios_osm || 0) + (meta.predios_gerados || 0);
      feedbackApp.aviso(
        `entorno OSM: ${predios} prédios, ${meta.vias || 0} vias, ` +
        `${(meta.triangulos || 0).toLocaleString("pt-BR")} triângulos`,
        "ok"
      );
    } catch (e) {
      // Visível de verdade, não só console — era exatamente o defeito do
      // toggle antigo (Ion 401 silencioso, só um aviso no console do dev).
      console.warn("ARCZ: entorno OSM não carregou:", e);
      feedbackApp.aviso(`falha ao gerar entorno OSM: ${e.message || e}`, "erro", 4500);
      this.chaveAtual = null;
    } finally {
      this.carregando = false;
      this.viewer.scene.requestRender?.();
    }
  }

  remover() {
    if (this.modelo && this.viewer?.scene.primitives.contains(this.modelo)) {
      this.viewer.scene.primitives.remove(this.modelo);
    }
    this.modelo = null;
    this.chaveAtual = null;
    this.viewer?.scene.requestRender?.();
  }

  /** Chamado por ambiente.js quando o interruptor global "Sombras" muda. */
  aplicarSombra(ligada) {
    if (this.modelo) {
      this.modelo.shadows = ligada ? Cesium.ShadowMode.ENABLED : Cesium.ShadowMode.DISABLED;
    }
  }
}

export const entornoApp = new EntornoManager();
