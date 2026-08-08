// ARCZ · Recorte por perímetro: desenha uma área no terreno, lista o que está
// dentro dela e exporta esse pedaço da cena (modelos + relevo) em GLB, glTF
// separado ou OBJ. A junção dos arquivos é feita no servidor (arcz_export.py).
import { estadoApp } from "./estado.js";
import { cenaApp, matrizDe, normalizarPosicao } from "./cena.js";
import { correcaoDeEixos } from "./corte.js";

const COR_AREA = "#6d9dff";

/** Ponto dentro do polígono (lon/lat), regra par-ímpar. */
export function dentroDoPerimetro(lon, lat, perimetro) {
  let dentro = false;
  for (let i = 0, j = perimetro.length - 1; i < perimetro.length; j = i++) {
    const { lon: xi, lat: yi } = perimetro[i];
    const { lon: xj, lat: yj } = perimetro[j];
    if ((yi > lat) !== (yj > lat)) {
      const corte = ((xj - xi) * (lat - yi)) / ((yj - yi) || 1e-12) + xi;
      if (lon < corte) dentro = !dentro;
    }
  }
  return dentro;
}

/** Área em m² e perímetro em m, projetando em metros no centro do polígono. */
export function medidasDoPerimetro(perimetro) {
  if (perimetro.length < 3) return { area: 0, perimetro: 0 };
  const latMedia = perimetro.reduce((s, p) => s + p.lat, 0) / perimetro.length;
  const lonMedia = perimetro.reduce((s, p) => s + p.lon, 0) / perimetro.length;
  const mLat = 111320;
  const mLon = 111320 * Math.cos((latMedia * Math.PI) / 180);
  const planos = perimetro.map(p => [(p.lon - lonMedia) * mLon, (p.lat - latMedia) * mLat]);

  let area = 0;
  let volta = 0;
  for (let i = 0, j = planos.length - 1; i < planos.length; j = i++) {
    area += planos[j][0] * planos[i][1] - planos[i][0] * planos[j][1];
    volta += Math.hypot(planos[i][0] - planos[j][0], planos[i][1] - planos[j][1]);
  }
  return { area: Math.abs(area) / 2, perimetro: volta };
}

export class RecorteManager {
  constructor() {
    this.viewer = null;
    this.desenhando = false;
    this.handler = null;
    this.entidades = [];
    this.previa = null;      // ponto sob o cursor enquanto desenha
    this.aoMudar = null;     // callback para a UI redesenhar
    this.aoAvisar = null;
  }

  inicializar(viewer) {
    this.viewer = viewer;
    if (this.perimetro().length >= 3) this.desenharEntidades();
  }

  avisar(texto, tipo = "info") {
    this.aoAvisar?.(texto, tipo);
  }

  notificar() {
    this.aoMudar?.(this.perimetro());
    this.viewer?.scene?.requestRender?.();
  }

  // ------------------------------------------------------------- perímetro
  perimetro() {
    return estadoApp.obter().recorte?.perimetro || [];
  }

  definirPerimetro(pontos) {
    estadoApp.atualizar({ recorte: { ...(estadoApp.obter().recorte || {}), perimetro: pontos } }, "recorte");
    this.desenharEntidades();
    this.notificar();
  }

  iniciarDesenho() {
    if (this.desenhando) return;
    this.desenhando = true;
    cenaApp.selecaoBloqueada = true;
    this.definirPerimetro([]);

    this.handler = new Cesium.ScreenSpaceEventHandler(this.viewer.scene.canvas);
    this.handler.setInputAction(clique => {
      const ponto = cenaApp.pontoNaCena(clique.position.x, clique.position.y);
      if (!ponto) return this.avisar("clique sobre o terreno ou o modelo", "alerta");
      this.definirPerimetro([...this.perimetro(), { lon: ponto.lon, lat: ponto.lat, alt: ponto.alt }]);
    }, Cesium.ScreenSpaceEventType.LEFT_CLICK);

    this.handler.setInputAction(movimento => {
      const ponto = cenaApp.pontoNaCena(movimento.endPosition.x, movimento.endPosition.y);
      if (ponto) {
        this.previa = ponto;
        this.viewer.scene.requestRender?.();
      }
    }, Cesium.ScreenSpaceEventType.MOUSE_MOVE);

    this.handler.setInputAction(() => this.finalizarDesenho(), Cesium.ScreenSpaceEventType.LEFT_DOUBLE_CLICK);
    this.handler.setInputAction(() => this.desfazerPonto(), Cesium.ScreenSpaceEventType.RIGHT_CLICK);

    this.desenharEntidades();
    this.avisar("clique para marcar o perímetro · duplo clique fecha · botão direito desfaz");
  }

  finalizarDesenho() {
    if (!this.desenhando) return null;
    this.pararHandler();
    const pontos = this.perimetro();
    if (pontos.length < 3) {
      this.avisar("perímetro precisa de pelo menos 3 pontos", "alerta");
      this.definirPerimetro([]);
      return null;
    }
    const medidas = medidasDoPerimetro(pontos);
    this.desenharEntidades();
    this.notificar();
    this.avisar(`perímetro fechado: ${medidas.area.toFixed(0)} m²`, "ok");
    return medidas;
  }

  cancelarDesenho() {
    this.pararHandler();
    this.definirPerimetro([]);
  }

  pararHandler() {
    this.desenhando = false;
    this.previa = null;
    cenaApp.selecaoBloqueada = false;
    if (this.handler) {
      this.handler.destroy();
      this.handler = null;
    }
  }

  desfazerPonto() {
    const pontos = this.perimetro();
    if (!pontos.length) return;
    this.definirPerimetro(pontos.slice(0, -1));
  }

  limpar() {
    this.pararHandler();
    this.definirPerimetro([]);
  }

  // -------------------------------------------------------------- desenho
  cartesianos(comPrevia = false) {
    const pontos = this.perimetro().map(p => Cesium.Cartesian3.fromDegrees(p.lon, p.lat, (p.alt || 0) + 0.4));
    if (comPrevia && this.desenhando && this.previa) {
      pontos.push(Cesium.Cartesian3.fromDegrees(this.previa.lon, this.previa.lat, (this.previa.alt || 0) + 0.4));
    }
    return pontos;
  }

  desenharEntidades() {
    this.apagarEntidades();
    if (!this.viewer) return;
    const cor = Cesium.Color.fromCssColorString(COR_AREA);

    this.entidades.push(this.viewer.entities.add({
      polygon: {
        hierarchy: new Cesium.CallbackProperty(() => new Cesium.PolygonHierarchy(this.cartesianos(true)), false),
        material: cor.withAlpha(0.18),
        perPositionHeight: true,
        outline: false
      }
    }));

    this.entidades.push(this.viewer.entities.add({
      polyline: {
        positions: new Cesium.CallbackProperty(() => {
          const pontos = this.cartesianos(true);
          return pontos.length > 1 ? [...pontos, pontos[0]] : pontos;
        }, false),
        width: 2.5,
        material: cor,
        arcType: Cesium.ArcType.NONE,
        clampToGround: false
      }
    }));

    for (const ponto of this.perimetro()) {
      this.entidades.push(this.viewer.entities.add({
        position: Cesium.Cartesian3.fromDegrees(ponto.lon, ponto.lat, (ponto.alt || 0) + 0.4),
        point: { pixelSize: 8, color: cor, outlineColor: Cesium.Color.WHITE, outlineWidth: 1.5, disableDepthTestDistance: 1e7 }
      }));
    }
    this.viewer.scene.requestRender?.();
  }

  apagarEntidades() {
    for (const entidade of this.entidades) this.viewer?.entities?.remove(entidade);
    this.entidades = [];
  }

  // -------------------------------------------------------------- conteúdo
  /** O que está dentro do perímetro: prédio principal e peças. */
  itensDentro() {
    const perimetro = this.perimetro();
    const st = estadoApp.obter();
    const itens = [];
    if (perimetro.length < 3) return itens;

    const pos = normalizarPosicao(st.posicao);
    if (dentroDoPerimetro(pos.lon, pos.lat, perimetro)) {
      itens.push({
        tipo: "predio",
        nome: (cenaApp.caminhoPredio || "predio").split("/").pop().replace(/\.(glb|gltf)$/i, ""),
        arquivo: cenaApp.caminhoPredio,
        posicao: pos
      });
    }
    for (const peca of st.pecas || []) {
      if (peca.visivel === false) continue;
      if (!dentroDoPerimetro(peca.lon, peca.lat, perimetro)) continue;
      itens.push({ tipo: "peca", nome: peca.nome, arquivo: peca.url, posicao: peca });
    }
    return itens;
  }

  centro() {
    const perimetro = this.perimetro();
    const soma = perimetro.reduce(
      (s, p) => ({ lon: s.lon + p.lon, lat: s.lat + p.lat, alt: s.alt + (p.alt || 0) }),
      { lon: 0, lat: 0, alt: 0 }
    );
    const n = perimetro.length || 1;
    return { lon: soma.lon / n, lat: soma.lat / n, alt: soma.alt / n };
  }

  /**
   * Matriz do item no espaço do recorte, já em Y para cima (padrão glTF).
   * A correção de eixos é a mesma que o Cesium aplica ao desenhar — sem ela o
   * recorte sairia girado 90° em relação ao que se vê na tela.
   */
  matrizNoRecorte(posicao, inversaDoCentro, modelo = null) {
    const m = Cesium.Matrix4.multiply(inversaDoCentro, matrizDe(posicao), new Cesium.Matrix4());
    Cesium.Matrix4.multiplyByUniformScale(m, posicao.escala || 1, m);
    Cesium.Matrix4.multiply(m, correcaoDeEixos(modelo), m);
    Cesium.Matrix4.multiply(Cesium.Axis.Z_UP_TO_Y_UP, m, m);
    return Cesium.Matrix4.toArray(m);
  }

  // ------------------------------------------------------------ exportação
  async exportar({ formato = "glb", relevo = false, resolucao = 80, nome = "" } = {}) {
    const perimetro = this.perimetro();
    if (perimetro.length < 3) throw new Error("desenhe o perímetro antes de exportar");

    const itens = this.itensDentro();
    const centro = this.centro();
    const inversa = Cesium.Matrix4.inverse(
      Cesium.Transforms.eastNorthUpToFixedFrame(
        Cesium.Cartesian3.fromDegrees(centro.lon, centro.lat, centro.alt)
      ),
      new Cesium.Matrix4()
    );

    const fora = [];
    const carga = [];
    for (const item of itens) {
      const arquivo = String(item.arquivo || "");
      if (/^(blob:|data:|https?:)/i.test(arquivo)) {
        fora.push(item.nome);   // arquivo só existe na memória do navegador
        continue;
      }
      const modelo = item.tipo === "predio"
        ? cenaApp.modeloPredio
        : cenaApp.pecasModelos.get(item.posicao?.id);
      carga.push({
        nome: item.nome,
        arquivo: arquivo.replace(/^\//, ""),
        matriz: this.matrizNoRecorte(normalizarPosicao(item.posicao), inversa, modelo)
      });
    }

    if (!carga.length && !relevo) {
      throw new Error(fora.length ? "os modelos do perímetro não estão em arquivo no projeto" : "nada dentro do perímetro");
    }

    const resposta = await fetch("/api/exportar", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        nome: nome || `recorte-${new Date().toISOString().slice(0, 10)}`,
        formato,
        itens: carga,
        relevo: {
          incluir: !!relevo,
          poligono: perimetro.map(p => ({ lon: p.lon, lat: p.lat })),
          centro,
          resolucao,
          zoom: 14
        }
      })
    });

    const dados = await resposta.json();
    if (!resposta.ok || !dados.ok) throw new Error(dados.erro || `falha na exportação (${resposta.status})`);
    if (fora.length) (dados.avisos ||= []).push(`fora do arquivo: ${fora.join(", ")}`);
    return dados;
  }
}

export const recorteApp = new RecorteManager();
