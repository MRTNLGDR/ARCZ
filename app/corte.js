// ARCZ · Corte: plano de corte com TAMPA de verdade e etapas salvas.
//
// O plano de corte do Cesium só apaga pixels: onde ele atravessa uma parede
// sobra o furo (dá para ver o miolo oco da malha por cima). Aqui a seção é
// calculada de verdade — a malha do GLB é lida, cada triângulo que cruza o
// plano vira um segmento, os segmentos viram contornos fechados e os contornos
// viram um poché sólido desenhado exatamente na altura do corte.
import { estadoApp } from "./estado.js";
import { cenaApp, matrizDe, urlDoModelo } from "./cena.js";

// Eixo do corte no espaço local do modelo (Z para cima, como o Cesium usa).
// U/V são a base 2D do plano; a seção é resolvida nessas coordenadas.
export const EIXOS = {
  z: { indice: 2, nome: "Horizontal", U: [1, 0, 0], V: [0, 1, 0] },
  x: { indice: 0, nome: "Leste-Oeste", U: [0, 1, 0], V: [0, 0, 1] },
  y: { indice: 1, nome: "Norte-Sul", U: [0, 0, 1], V: [1, 0, 0] }
};

export const CORTE_PADRAO = {
  ativo: false,
  eixo: "z",
  distancia: 12,
  invertido: false,
  tapar: true,
  cor: "#aeb8c7",
  pecas: true,
  etapas: []
};

export function corteAtual(st = estadoApp.obter()) {
  return { ...CORTE_PADRAO, ...(st.corte || {}) };
}

// ------------------------------------------------------------------ glTF
const MAGICA_GLTF = 0x46546c67;
const CHUNK_JSON = 0x4e4f534a;
const CHUNK_BIN = 0x004e4942;

const CONSTRUTOR = {
  5120: Int8Array, 5121: Uint8Array, 5122: Int16Array,
  5123: Uint16Array, 5125: Uint32Array, 5126: Float32Array
};
const ITENS = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 };

function base64ParaBytes(texto) {
  const cru = atob(texto);
  const saida = new Uint8Array(cru.length);
  for (let i = 0; i < cru.length; i++) saida[i] = cru.charCodeAt(i);
  return saida.buffer;
}

async function resolverBuffers(doc, chunkBin, url) {
  const saida = [];
  for (const buffer of doc.buffers || []) {
    const uri = buffer.uri;
    if (!uri) {
      saida.push(chunkBin);
    } else if (uri.startsWith("data:")) {
      const [cabeca, corpo] = uri.split(",");
      saida.push(cabeca.includes(";base64") ? base64ParaBytes(corpo) : new TextEncoder().encode(decodeURIComponent(corpo)).buffer);
    } else {
      const res = await fetch(new URL(uri, new URL(url, location.href)).href);
      saida.push(await res.arrayBuffer());
    }
  }
  return saida;
}

/** Lê um .glb ou .gltf e devolve { doc, buffers }. */
export async function lerGltf(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`não consegui baixar a malha (${res.status})`);
  const bruto = await res.arrayBuffer();
  const vista = new DataView(bruto);

  if (bruto.byteLength >= 12 && vista.getUint32(0, true) === MAGICA_GLTF) {
    let deslocamento = 12;
    let doc = null;
    let chunkBin = null;
    while (deslocamento + 8 <= bruto.byteLength) {
      const tamanho = vista.getUint32(deslocamento, true);
      const tipo = vista.getUint32(deslocamento + 4, true);
      const inicio = deslocamento + 8;
      if (tipo === CHUNK_JSON) doc = JSON.parse(new TextDecoder().decode(new Uint8Array(bruto, inicio, tamanho)));
      else if (tipo === CHUNK_BIN && chunkBin === null) chunkBin = bruto.slice(inicio, inicio + tamanho);
      deslocamento = inicio + tamanho;
    }
    if (!doc) throw new Error("glb sem bloco JSON");
    return { doc, buffers: await resolverBuffers(doc, chunkBin, url) };
  }

  const doc = JSON.parse(new TextDecoder().decode(bruto));
  return { doc, buffers: await resolverBuffers(doc, null, url) };
}

function lerAcessor(doc, buffers, indice) {
  const acessor = (doc.accessors || [])[indice];
  if (!acessor || acessor.bufferView === undefined) return null;
  const itens = ITENS[acessor.type] || 1;
  const Tipo = CONSTRUTOR[acessor.componentType];
  if (!Tipo) return null;

  const view = doc.bufferViews[acessor.bufferView];
  const buffer = buffers[view.buffer || 0];
  if (!buffer) return null;
  const bytes = Tipo.BYTES_PER_ELEMENT;
  const inicio = (view.byteOffset || 0) + (acessor.byteOffset || 0);
  const passo = view.byteStride || itens * bytes;

  // Caminho rápido: dados contíguos e alinhados viram uma view direta.
  if (passo === itens * bytes && inicio % bytes === 0) {
    return new Tipo(buffer, inicio, acessor.count * itens);
  }
  const saida = new Tipo(acessor.count * itens);
  const dados = new DataView(buffer);
  const ler = {
    5120: (o) => dados.getInt8(o), 5121: (o) => dados.getUint8(o),
    5122: (o) => dados.getInt16(o, true), 5123: (o) => dados.getUint16(o, true),
    5125: (o) => dados.getUint32(o, true), 5126: (o) => dados.getFloat32(o, true)
  }[acessor.componentType];
  for (let i = 0; i < acessor.count; i++) {
    for (let k = 0; k < itens; k++) saida[i * itens + k] = ler(inicio + i * passo + k * bytes);
  }
  return saida;
}

function matrizDoNo(no) {
  if (no.matrix) return Cesium.Matrix4.fromColumnMajorArray(no.matrix);
  const t = no.translation || [0, 0, 0];
  const r = no.rotation || [0, 0, 0, 1];
  const s = no.scale || [1, 1, 1];
  return Cesium.Matrix4.fromTranslationQuaternionRotationScale(
    new Cesium.Cartesian3(t[0], t[1], t[2]),
    new Cesium.Quaternion(r[0], r[1], r[2], r[3]),
    new Cesium.Cartesian3(s[0], s[1], s[2])
  );
}

/**
 * Correção de eixos que o Cesium aplica ao carregar um glTF.
 *
 * São DUAS: Y-para-cima → Z-para-cima e Z-para-frente → X-para-frente. Aplicar
 * só a primeira deixa a malha girada 90° em torno do eixo vertical — a altura
 * bate, a planta não, e a tampa do corte aparece deslocada sobre o prédio
 * (paredes "tapadas" onde não há parede e buracos onde há). Quando o Model já
 * está carregado usamos a matriz dele, que é a fonte da verdade.
 */
export function correcaoDeEixos(modelo) {
  const doModelo = modelo?.sceneGraph?._axisCorrectionMatrix || modelo?._sceneGraph?._axisCorrectionMatrix;
  if (doModelo) return Cesium.Matrix4.clone(doModelo, new Cesium.Matrix4());
  return Cesium.Matrix4.multiply(
    Cesium.Axis.Y_UP_TO_Z_UP, Cesium.Axis.Z_UP_TO_X_UP, new Cesium.Matrix4()
  );
}

/**
 * Triângulos do modelo no espaço local do Cesium (Z para cima, metros),
 * que é o mesmo espaço em que o plano de corte é definido.
 * Devolve { triangulos: Float64Array (9 por triângulo), comprimidas }.
 */
export function trianglesDoGltf(doc, buffers, correcao = correcaoDeEixos()) {
  const nos = doc.nodes || [];
  const cena = (doc.scenes || [{}])[doc.scene || 0] || {};
  const pilha = (cena.nodes || nos.map((_, i) => i)).map(i => [i, matrizDoNo(nos[i] || {})]);
  const blocos = [];
  let total = 0;
  let comprimidas = 0;
  const ponto = new Cesium.Cartesian3();

  while (pilha.length) {
    const [indice, mundo] = pilha.pop();
    const no = nos[indice];
    if (!no) continue;

    for (const filho of no.children || []) {
      pilha.push([filho, Cesium.Matrix4.multiply(mundo, matrizDoNo(nos[filho] || {}), new Cesium.Matrix4())]);
    }
    if (no.mesh === undefined) continue;

    for (const prim of (doc.meshes[no.mesh] || {}).primitives || []) {
      if ((prim.extensions || {}).KHR_draco_mesh_compression) { comprimidas++; continue; }
      if ((prim.mode ?? 4) !== 4) continue;
      const posicoes = lerAcessor(doc, buffers, (prim.attributes || {}).POSITION);
      if (!posicoes) continue;
      const indices = prim.indices !== undefined ? lerAcessor(doc, buffers, prim.indices) : null;
      const quantidade = indices ? indices.length : posicoes.length / 3;
      const bloco = new Float64Array(quantidade * 3);
      // Hierarquia do glTF e correção de eixos numa matriz só.
      const paraCesium = Cesium.Matrix4.multiply(correcao, mundo, new Cesium.Matrix4());

      for (let i = 0; i < quantidade; i++) {
        const v = indices ? indices[i] : i;
        ponto.x = posicoes[v * 3];
        ponto.y = posicoes[v * 3 + 1];
        ponto.z = posicoes[v * 3 + 2];
        Cesium.Matrix4.multiplyByPoint(paraCesium, ponto, ponto);
        bloco[i * 3] = ponto.x;
        bloco[i * 3 + 1] = ponto.y;
        bloco[i * 3 + 2] = ponto.z;
      }
      blocos.push(bloco);
      total += bloco.length;
    }
  }

  const triangulos = new Float64Array(total);
  let escrita = 0;
  for (const bloco of blocos) {
    triangulos.set(bloco, escrita);
    escrita += bloco.length;
  }
  return { triangulos, comprimidas };
}

// ------------------------------------------------------------------ seção
/**
 * Segmentos 2D (u1,v1,u2,v2) onde os triângulos cruzam o plano.
 *
 * O plano é amostrado 0,1 mm adiante da cota pedida. Cortar exatamente em
 * "3,00 m" num modelo desenhado com a laje em 3,00 põe o plano em cima de
 * milhares de vértices e faces coplanares: a interseção vira caso degenerado
 * e a seção sai picotada. Deslocada, a mesma cota cai limpa entre faces.
 */
export function seccionar(triangulos, eixo, distancia, epsilon = 1e-4) {
  const { indice, U, V } = EIXOS[eixo] || EIXOS.z;
  const cota = distancia + epsilon;
  const segmentos = [];
  const s = [0, 0, 0];
  const px = [0, 0], py = [0, 0], pz = [0, 0];

  for (let t = 0; t + 8 < triangulos.length; t += 9) {
    s[0] = triangulos[t + indice] - cota;
    s[1] = triangulos[t + 3 + indice] - cota;
    s[2] = triangulos[t + 6 + indice] - cota;
    if ((s[0] > 0 && s[1] > 0 && s[2] > 0) || (s[0] < 0 && s[1] < 0 && s[2] < 0)) continue;

    let achados = 0;
    for (let a = 0; a < 3 && achados < 2; a++) {
      const b = (a + 1) % 3;
      const sa = s[a], sb = s[b];
      if ((sa < 0 && sb < 0) || (sa >= 0 && sb >= 0)) continue;
      const f = sa / (sa - sb);
      const ia = t + a * 3, ib = t + b * 3;
      px[achados] = triangulos[ia] + (triangulos[ib] - triangulos[ia]) * f;
      py[achados] = triangulos[ia + 1] + (triangulos[ib + 1] - triangulos[ia + 1]) * f;
      pz[achados] = triangulos[ia + 2] + (triangulos[ib + 2] - triangulos[ia + 2]) * f;
      achados++;
    }
    if (achados < 2) continue;

    const u1 = px[0] * U[0] + py[0] * U[1] + pz[0] * U[2];
    const v1 = px[0] * V[0] + py[0] * V[1] + pz[0] * V[2];
    const u2 = px[1] * U[0] + py[1] * U[1] + pz[1] * U[2];
    const v2 = px[1] * V[0] + py[1] * V[1] + pz[1] * V[2];
    if (Math.abs(u1 - u2) > 1e-6 || Math.abs(v1 - v2) > 1e-6) segmentos.push(u1, v1, u2, v2);
  }
  return segmentos;
}

/**
 * Costura os segmentos em contornos fechados.
 *
 * Duas decisões, as duas medidas no zenite.glb a 1,2 m (607 contornos):
 *
 * 1. Solda por célula de grade, com a vizinhança consultada só a 0,01 mm.
 *    Soldar por raio de 1,5 mm parece mais robusto e é o contrário: junta as
 *    duas faces de uma parede fina no mesmo vértice, cria junção de grau 4 e
 *    a caminhada se perde — 6 contornos abertos viram 276.
 * 2. Cadeia que não fecha só é costurada quando o vão é pequeno (5 cm). Fechar
 *    qualquer vão enche a planta de triângulos atravessando sala.
 */
export function encadear(segmentos, tolerancia = 0.002, costura = 0.05) {
  const pontos = [];
  const celulas = new Map();     // célula da grade -> id do vértice
  const grade = new Map();       // célula -> ids, para a busca de borda
  const arestas = [];
  const vizinhos = new Map();
  const FINO = 1e-5;             // 0,01 mm: só reencontra o MESMO ponto

  const idDe = (u, v) => {
    const cu = Math.round(u / tolerancia);
    const cv = Math.round(v / tolerancia);
    const chave = `${cu}:${cv}`;
    const existente = celulas.get(chave);
    if (existente !== undefined) return existente;

    // Célula vizinha só é consultada com tolerância mínima: dois pontos a 1 mm
    // são vértices diferentes (faces opostas de uma parede fina), mas o MESMO
    // ponto calculado por dois triângulos pode cair do outro lado da borda da
    // célula. Soldar por raio largo aqui gera junções de grau 4 e arrebenta a
    // costura — medido: 6 contornos abertos viram 276.
    for (let du = -1; du <= 1; du++) {
      for (let dv = -1; dv <= 1; dv++) {
        if (!du && !dv) continue;
        const lista = grade.get(`${cu + du}:${cv + dv}`);
        if (!lista) continue;
        for (const id of lista) {
          if (Math.abs(pontos[id * 2] - u) <= FINO && Math.abs(pontos[id * 2 + 1] - v) <= FINO) return id;
        }
      }
    }
    const id = pontos.length / 2;
    pontos.push(u, v);
    celulas.set(chave, id);
    const lista = grade.get(chave);
    if (lista) lista.push(id);
    else grade.set(chave, [id]);
    return id;
  };

  for (let i = 0; i + 3 < segmentos.length; i += 4) {
    const a = idDe(segmentos[i], segmentos[i + 1]);
    const b = idDe(segmentos[i + 2], segmentos[i + 3]);
    if (a === b) continue;
    const aresta = arestas.length;
    arestas.push(a, b);
    if (!vizinhos.has(a)) vizinhos.set(a, []);
    if (!vizinhos.has(b)) vizinhos.set(b, []);
    vizinhos.get(a).push(aresta);
    vizinhos.get(b).push(aresta);
  }

  const usada = new Uint8Array(arestas.length / 2 || 1);
  const contornos = [];
  let abertos = 0;
  let forcados = 0;

  const proxima = (vertice) => {
    for (const aresta of vizinhos.get(vertice) || []) {
      if (!usada[aresta / 2]) return aresta;
    }
    return -1;
  };

  for (let inicio = 0; inicio < arestas.length; inicio += 2) {
    if (usada[inicio / 2]) continue;
    usada[inicio / 2] = 1;
    const primeiro = arestas[inicio];
    let atual = arestas[inicio + 1];
    const contorno = [primeiro, atual];

    while (atual !== primeiro) {
      const aresta = proxima(atual);
      if (aresta < 0) break;
      usada[aresta / 2] = 1;
      atual = arestas[aresta] === atual ? arestas[aresta + 1] : arestas[aresta];
      contorno.push(atual);
    }

    if (atual === primeiro) {
      contorno.pop();   // o último repete o primeiro
    } else {
      abertos++;
      // Vão pequeno é falha de costura: fecha. Vão grande é superfície aberta
      // de verdade (fachada de vidro sem espessura) — fechar ali inventaria um
      // triangulão atravessando a sala, que é pior que o buraco.
      const vao = Math.hypot(
        pontos[atual * 2] - pontos[primeiro * 2],
        pontos[atual * 2 + 1] - pontos[primeiro * 2 + 1]
      );
      if (vao > costura) continue;
      forcados++;
    }

    if (contorno.length < 3) continue;
    contornos.push(contorno.map(id => [pontos[id * 2], pontos[id * 2 + 1]]));
  }
  return { contornos, abertos, forcados };
}

function area(contorno) {
  let soma = 0;
  for (let i = 0, j = contorno.length - 1; i < contorno.length; j = i++) {
    soma += (contorno[j][0] + contorno[i][0]) * (contorno[j][1] - contorno[i][1]);
  }
  return -soma / 2;
}

function dentroDe(ponto, contorno) {
  let dentro = false;
  for (let i = 0, j = contorno.length - 1; i < contorno.length; j = i++) {
    const [xi, yi] = contorno[i];
    const [xj, yj] = contorno[j];
    if ((yi > ponto[1]) !== (yj > ponto[1])) {
      const corte = ((xj - xi) * (ponto[1] - yi)) / ((yj - yi) || 1e-12) + xi;
      if (ponto[0] < corte) dentro = !dentro;
    }
  }
  return dentro;
}

/**
 * "Este contorno está dentro daquele?" por votação em pontos médios de aresta.
 *
 * Testar um vértice não serve: paredes que se encostam compartilham vértice,
 * o ponto cai exatamente na borda do outro contorno e o teste par-ímpar
 * responde qualquer coisa — é assim que um trecho sólido vira "vazio" e abre
 * buraco na tampa. Ponto médio de aresta quase nunca cai na borda alheia.
 */
function contidoEm(contorno, externo) {
  let dentro = 0;
  let fora = 0;
  const amostras = Math.min(5, contorno.length);
  const passo = Math.max(1, Math.floor(contorno.length / amostras));
  for (let i = 0; i < contorno.length && dentro + fora < amostras; i += passo) {
    const a = contorno[i];
    const b = contorno[(i + 1) % contorno.length];
    if (dentroDe([(a[0] + b[0]) / 2, (a[1] + b[1]) / 2], externo)) dentro++;
    else fora++;
  }
  return dentro > fora;
}

/** Tira pontos repetidos e vértices colineares — earcut engasga nos dois. */
function limparContorno(contorno, tolerancia = 1e-4) {
  const limpo = [];
  for (const p of contorno) {
    const ultimo = limpo[limpo.length - 1];
    if (ultimo && Math.abs(ultimo[0] - p[0]) < tolerancia && Math.abs(ultimo[1] - p[1]) < tolerancia) continue;
    limpo.push(p);
  }
  while (limpo.length > 1) {
    const a = limpo[0];
    const b = limpo[limpo.length - 1];
    if (Math.abs(a[0] - b[0]) < tolerancia && Math.abs(a[1] - b[1]) < tolerancia) limpo.pop();
    else break;
  }
  if (limpo.length < 3) return limpo;

  const saida = [];
  for (let i = 0; i < limpo.length; i++) {
    const a = limpo[(i - 1 + limpo.length) % limpo.length];
    const b = limpo[i];
    const c = limpo[(i + 1) % limpo.length];
    const cruz = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
    if (Math.abs(cruz) > 1e-9) saida.push(b);
  }
  return saida.length >= 3 ? saida : limpo;
}

/**
 * Contornos -> triângulos 2D. Contorno dentro de contorno vira vazio (a sala
 * fica aberta, só a alvenaria é preenchida), como num corte de arquitetura.
 * `falhas` conta grupos que a triangulação recusou, para a UI não fingir que
 * a tampa saiu inteira quando não saiu.
 */
export function triangularContornos(contornos, areaMinima = 1e-4) {
  const validos = contornos
    .map(c => limparContorno(c))
    .filter(c => c.length >= 3)
    .map(c => ({ pontos: c, area: area(c) }))
    .filter(c => Math.abs(c.area) >= areaMinima)
    .sort((a, b) => Math.abs(b.area) - Math.abs(a.area));

  // Só faz sentido testar contenção em quem tem área maior — daí o j < i.
  const profundidade = [];
  const paiDe = [];
  for (let i = 0; i < validos.length; i++) {
    let nivel = 0;
    let pai = -1;
    for (let j = 0; j < i; j++) {
      if (!contidoEm(validos[i].pontos, validos[j].pontos)) continue;
      nivel++;
      if (pai < 0 || Math.abs(validos[j].area) < Math.abs(validos[pai].area)) pai = j;
    }
    profundidade.push(nivel);
    paiDe.push(pai);
  }

  const saida = { posicoes: [], indices: [], falhas: 0 };
  const anexar = (planos, contorno, sentidoPositivo) => {
    const pontos = (area(contorno) >= 0) === sentidoPositivo ? contorno : [...contorno].reverse();
    for (const [u, v] of pontos) planos.push(new Cesium.Cartesian2(u, v));
  };
  const gravar = (planos, indices) => {
    const base = saida.posicoes.length;
    for (const p of planos) saida.posicoes.push([p.x, p.y]);
    for (const indice of indices) saida.indices.push(base + indice);
  };

  for (let i = 0; i < validos.length; i++) {
    if (profundidade[i] % 2 !== 0) continue;   // ímpar = vazio interno
    const furos = validos.filter((_, j) => paiDe[j] === i && profundidade[j] % 2 === 1);

    const planos = [];
    const inicioFuros = [];
    anexar(planos, validos[i].pontos, true);
    for (const furo of furos) {
      inicioFuros.push(planos.length);
      anexar(planos, furo.pontos, false);
    }

    let indices = null;
    try {
      indices = Cesium.PolygonPipeline.triangulate(planos, inicioFuros);
    } catch (e) {
      indices = null;
    }
    if (indices && indices.length >= 3) {
      gravar(planos, indices);
      continue;
    }

    // Recusou com os furos: preenche ao menos o contorno externo e devolve os
    // furos como sólidos próprios — sobra material, mas não abre buraco.
    const soExterno = [];
    anexar(soExterno, validos[i].pontos, true);
    try {
      const indicesSimples = Cesium.PolygonPipeline.triangulate(soExterno, []);
      if (indicesSimples && indicesSimples.length >= 3) {
        gravar(soExterno, indicesSimples);
        continue;
      }
    } catch (e) { /* nem simples: cai no contador de falhas */ }
    saida.falhas++;
  }
  return saida;
}

// ------------------------------------------------------------------ gerente
export class CorteManager {
  constructor() {
    this.viewer = null;
    this.tampa = null;
    this.colecoes = new Map();       // id do alvo -> ClippingPlaneCollection
    this.cacheMalha = new Map();     // url -> { triangulos, comprimidas }
    this.timerTampa = null;
    this.aoAvisar = null;
    this.aoSecao = null;             // callback com o resumo da última seção
    this.ultimaSecao = null;         // { triangulos, area, contornos }
  }

  inicializar(viewer) {
    this.viewer = viewer;

    // Recarregar o prédio (troca de LOD, novo GLB) cria outro Model: o corte
    // precisa ser reposto no objeto novo, senão some sozinho.
    const anteriorPredio = cenaApp.aoRecarregarPredio;
    cenaApp.aoRecarregarPredio = (modelo) => {
      anteriorPredio?.(modelo);
      this.cacheMalha.clear();
      if (corteAtual().ativo) this.aplicar();
    };
    const anteriorPeca = cenaApp.aoRenderizarPeca;
    cenaApp.aoRenderizarPeca = (id, modelo) => {
      anteriorPeca?.(id, modelo);
      if (corteAtual().ativo) this.aplicarNasPecas();
    };

    estadoApp.inscrever((st, origem) => {
      if (origem === "camera" || origem === "takes" || !corteAtual(st).ativo) return;
      if (origem === "corte") return;              // aplicar() já cuidou
      this.atualizarTransformacoes();
    });
  }

  avisar(texto, tipo = "info") {
    this.aoAvisar?.(texto, tipo);
  }

  // ---------------------------------------------------------- estado
  definir(patch, { historico = false } = {}) {
    const antes = corteAtual();
    estadoApp.atualizar({ corte: { ...antes, ...patch } }, "corte");
    this.aplicar();
    return corteAtual();
  }

  alternar() {
    return this.definir({ ativo: !corteAtual().ativo }).ativo;
  }

  // ---------------------------------------------------------- clipping
  planoDe(corte) {
    const eixo = EIXOS[corte.eixo] || EIXOS.z;
    const sinal = corte.invertido ? 1 : -1;
    const normal = new Cesium.Cartesian3(
      eixo.indice === 0 ? sinal : 0,
      eixo.indice === 1 ? sinal : 0,
      eixo.indice === 2 ? sinal : 0
    );
    return new Cesium.ClippingPlane(normal, -sinal * corte.distancia);
  }

  colecaoNova(corte, modelMatrix) {
    const colecao = new Cesium.ClippingPlaneCollection({
      planes: [this.planoDe(corte)],
      edgeWidth: 1.0,
      edgeColor: Cesium.Color.fromCssColorString("#6d9dff")
    });
    if (modelMatrix) colecao.modelMatrix = modelMatrix;
    return colecao;
  }

  /** Matriz que leva o plano (definido no prédio) para o espaço da peça. */
  matrizRelativa(peca) {
    const inversa = Cesium.Matrix4.inverse(matrizDe(peca), new Cesium.Matrix4());
    return Cesium.Matrix4.multiply(inversa, matrizDe(estadoApp.obter().posicao), new Cesium.Matrix4());
  }

  aplicarNasPecas() {
    const corte = corteAtual();
    for (const [id, modelo] of cenaApp.pecasModelos) {
      if (!corte.ativo || !corte.pecas) {
        modelo.clippingPlanes = undefined;
        continue;
      }
      const peca = cenaApp.obterPeca(id);
      if (!peca) continue;
      modelo.clippingPlanes = this.colecaoNova(corte, this.matrizRelativa(peca));
    }
  }

  atualizarTransformacoes() {
    const corte = corteAtual();
    if (!corte.ativo) return;
    if (corte.pecas) this.aplicarNasPecas();
    if (this.tampa) {
      this.tampa.modelMatrix = this.matrizDaTampa();
      this.viewer?.scene?.requestRender?.();
    }
  }

  matrizDaTampa() {
    const pos = estadoApp.obter().posicao;
    return Cesium.Matrix4.multiplyByUniformScale(matrizDe(pos), pos.escala || 1, new Cesium.Matrix4());
  }

  /** Liga/atualiza o corte na cena inteira. */
  aplicar() {
    const corte = corteAtual();
    const predio = cenaApp.modeloPredio;

    if (!corte.ativo) {
      if (predio) predio.clippingPlanes = undefined;
      this.aplicarNasPecas();
      this.removerTampa();
      this.viewer?.scene?.requestRender?.();
      return false;
    }
    if (!predio) {
      this.avisar("modelo principal ainda não carregou", "alerta");
      return false;
    }

    predio.clippingPlanes = this.colecaoNova(corte);
    this.aplicarNasPecas();

    if (corte.tapar) this.agendarTampa();
    else this.removerTampa();

    this.viewer?.scene?.requestRender?.();
    return true;
  }

  agendarTampa(atraso = 140) {
    if (this.timerTampa) clearTimeout(this.timerTampa);
    this.timerTampa = setTimeout(() => {
      this.timerTampa = null;
      this.gerarTampa().catch(e => {
        console.warn("Falha ao gerar a tampa do corte:", e);
        this.avisar(`tampa do corte: ${e.message}`, "alerta");
      });
    }, atraso);
  }

  removerTampa() {
    if (this.tampa) {
      this.viewer?.scene?.primitives?.remove(this.tampa);
      this.tampa = null;
    }
  }

  async malhaDoPredio() {
    const url = urlDoModelo(cenaApp.caminhoPredio, cenaApp.lodPredio || "equilibrado");
    if (this.cacheMalha.has(url)) return this.cacheMalha.get(url);
    const { doc, buffers } = await lerGltf(url);
    const malha = trianglesDoGltf(doc, buffers, correcaoDeEixos(cenaApp.modeloPredio));
    this.cacheMalha.set(url, malha);
    return malha;
  }

  /** Calcula a seção e desenha o poché sólido na altura do corte. */
  async gerarTampa() {
    const corte = corteAtual();
    if (!corte.ativo || !corte.tapar || !cenaApp.modeloPredio) return null;

    const inicio = performance.now();
    const { triangulos, comprimidas } = await this.malhaDoPredio();
    if (!triangulos.length) {
      this.removerTampa();
      if (comprimidas) this.avisar("malha comprimida (Draco): sem tampa", "alerta");
      return null;
    }

    const segmentos = seccionar(triangulos, corte.eixo, corte.distancia);
    const { contornos, abertos, forcados } = encadear(segmentos);
    const { posicoes, indices, falhas } = triangularContornos(contornos);

    this.removerTampa();
    if (!indices.length) {
      this.ultimaSecao = {
        triangulos: 0, area: 0, contornos: contornos.length, abertos, forcados, falhas, ms: 0
      };
      this.aoSecao?.(this.ultimaSecao);
      return null;
    }

    const eixo = EIXOS[corte.eixo] || EIXOS.z;
    // 4 mm para dentro do lado que sobra: sem isso a tampa briga em z com o
    // próprio plano de corte e pisca.
    const recuo = corte.invertido ? 0.004 : -0.004;
    const valores = new Float64Array(posicoes.length * 3);
    for (let i = 0; i < posicoes.length; i++) {
      const [u, v] = posicoes[i];
      for (let k = 0; k < 3; k++) {
        valores[i * 3 + k] = u * eixo.U[k] + v * eixo.V[k];
      }
      valores[i * 3 + eixo.indice] = corte.distancia + recuo;
    }

    const geometria = new Cesium.Geometry({
      attributes: {
        position: new Cesium.GeometryAttribute({
          componentDatatype: Cesium.ComponentDatatype.DOUBLE,
          componentsPerAttribute: 3,
          values: valores
        })
      },
      indices: new Uint32Array(indices),
      primitiveType: Cesium.PrimitiveType.TRIANGLES,
      boundingSphere: Cesium.BoundingSphere.fromVertices(Array.from(valores))
    });

    this.tampa = this.viewer.scene.primitives.add(new Cesium.Primitive({
      geometryInstances: new Cesium.GeometryInstance({
        geometry: geometria,
        attributes: {
          color: Cesium.ColorGeometryInstanceAttribute.fromColor(
            Cesium.Color.fromCssColorString(corte.cor || CORTE_PADRAO.cor)
          )
        }
      }),
      appearance: new Cesium.PerInstanceColorAppearance({ flat: true, closed: false, translucent: false }),
      modelMatrix: this.matrizDaTampa(),
      asynchronous: false,
      allowPicking: false
    }));

    let areaTotal = 0;
    for (let i = 0; i + 2 < indices.length; i += 3) {
      const a = posicoes[indices[i]], b = posicoes[indices[i + 1]], c = posicoes[indices[i + 2]];
      areaTotal += Math.abs((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])) / 2;
    }
    this.ultimaSecao = {
      triangulos: indices.length / 3,
      area: areaTotal,
      contornos: contornos.length,
      abertos,
      forcados,
      falhas,
      ms: Math.round(performance.now() - inicio)
    };
    this.aoSecao?.(this.ultimaSecao);
    this.viewer.scene.requestRender?.();
    return this.ultimaSecao;
  }

  // ---------------------------------------------------------- etapas
  salvarEtapa(nome) {
    const corte = corteAtual();
    const etapa = {
      id: "corte_" + Date.now(),
      nome: nome || `Etapa ${(corte.etapas || []).length + 1}`,
      eixo: corte.eixo,
      distancia: corte.distancia,
      invertido: corte.invertido,
      tapar: corte.tapar,
      cor: corte.cor,
      pecas: corte.pecas
    };
    this.definir({ etapas: [...(corte.etapas || []), etapa] });
    return etapa;
  }

  aplicarEtapa(id) {
    const corte = corteAtual();
    const etapa = (corte.etapas || []).find(e => e.id === id);
    if (!etapa) return null;
    this.definir({
      ativo: true,
      eixo: etapa.eixo,
      distancia: etapa.distancia,
      invertido: !!etapa.invertido,
      tapar: etapa.tapar !== false,
      cor: etapa.cor || corte.cor,
      pecas: etapa.pecas !== false
    });
    return etapa;
  }

  atualizarEtapa(id, patch) {
    const etapas = (corteAtual().etapas || []).map(e => (e.id === id ? { ...e, ...patch } : e));
    this.definir({ etapas });
  }

  removerEtapa(id) {
    this.definir({ etapas: (corteAtual().etapas || []).filter(e => e.id !== id) });
  }

  /** Etapa atual pela distância/eixo; -1 quando o corte está fora das etapas. */
  indiceDaEtapa() {
    const corte = corteAtual();
    return (corte.etapas || []).findIndex(
      e => e.eixo === corte.eixo && Math.abs(e.distancia - corte.distancia) < 1e-6
    );
  }

  navegar(passo) {
    const etapas = corteAtual().etapas || [];
    if (!etapas.length) return null;
    const atual = this.indiceDaEtapa();
    const alvo = atual < 0
      ? (passo > 0 ? 0 : etapas.length - 1)
      : (atual + passo + etapas.length) % etapas.length;
    return this.aplicarEtapa(etapas[alvo].id);
  }
}

export const corteApp = new CorteManager();
