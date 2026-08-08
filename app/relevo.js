// ARCZ · Relevo 3D real a partir dos tiles Terrarium servidos em /dem/{z}/{x}/{y}.png.
//
// O visualizador rodava com EllipsoidTerrainProvider (terreno plano): o morro de
// Bombinhas não aparecia e "Assentar no terreno" devolvia sempre ~0 m. Aqui o PNG
// terrarium é decodificado no cliente (altura = R*256 + G + B/256 - 32768) e
// entregue ao Cesium como heightmap.

const Z_MAX_TERRARIUM = 15;
const LADO_TILE = 256;
/** Latitude onde a projeção de Mercator estoura (tan → ∞). */
const LIMITE_MERCATOR = 85.05112877980659;
/** Teto de tiles-fonte por tile de saída, para não explodir em níveis baixos. */
const MAX_TILES_FONTE = 9;
const cache = new Map();      // "z/x/y" -> Float32Array(256*256)
const emVoo = new Map();      // "z/x/y" -> Promise
let canvasCompartilhado = null;
let ctxCompartilhado = null;

function obterContexto2D() {
  if (!ctxCompartilhado) {
    canvasCompartilhado = typeof OffscreenCanvas !== "undefined"
      ? new OffscreenCanvas(LADO_TILE, LADO_TILE)
      : document.createElement("canvas");
    canvasCompartilhado.width = LADO_TILE;
    canvasCompartilhado.height = LADO_TILE;
    ctxCompartilhado = canvasCompartilhado.getContext("2d", { willReadFrequently: true });
  }
  return ctxCompartilhado;
}

function chave(z, x, y) {
  return `${z}/${x}/${y}`;
}

async function baixarTile(z, x, y) {
  const k = chave(z, x, y);
  if (cache.has(k)) return cache.get(k);
  if (emVoo.has(k)) return emVoo.get(k);

  const promessa = (async () => {
    try {
      const resposta = await fetch(`/dem/${z}/${x}/${y}.png`);
      if (!resposta.ok) throw new Error(`HTTP ${resposta.status}`);
      const bitmap = await createImageBitmap(await resposta.blob());
      const ctx = obterContexto2D();
      ctx.drawImage(bitmap, 0, 0, LADO_TILE, LADO_TILE);
      const pixels = ctx.getImageData(0, 0, LADO_TILE, LADO_TILE).data;

      const alturas = new Float32Array(LADO_TILE * LADO_TILE);
      for (let i = 0, p = 0; i < alturas.length; i++, p += 4) {
        alturas[i] = pixels[p] * 256 + pixels[p + 1] + pixels[p + 2] / 256 - 32768;
      }
      cache.set(k, alturas);
      return alturas;
    } catch (e) {
      if (z > 0) {
        try {
          const paiZ = z - 1;
          const paiX = Math.floor(x / 2);
          const paiY = Math.floor(y / 2);
          const paiAlturas = await baixarTile(paiZ, paiX, paiY);
          const offX = (x % 2) * 0.5;
          const offY = (y % 2) * 0.5;
          const amostraPai = amostrar(paiAlturas, offX, offY, 0.5, LADO_TILE, LADO_TILE);
          cache.set(k, amostraPai);
          return amostraPai;
        } catch (errPai) {
          // O pai também não existe: propaga erro explícito abaixo. Não é
          // permitido transformar ausência de dado em terreno plano fictício.
        }
      }
      const erro = new Error(`DEM local ausente para ${k}`);
      erro.code = "DEM_LOCAL_MISSING";
      erro.details = { z, x, y, endpoint: `/dem/${z}/${x}/${y}.png` };
      throw erro;
    } finally {
      emVoo.delete(k);
    }
  })();

  emVoo.set(k, promessa);
  return promessa;
}

/**
 * Amostra um recorte do tile pai quando o Cesium pede um nível mais fino do
 * que o DEM tem (z > 15).
 */
function amostrar(alturas, offX, offY, fracao, largura, altura) {
  const saida = new Float32Array(largura * altura);
  for (let linha = 0; linha < altura; linha++) {
    const v = offY + (linha / (altura - 1)) * fracao;
    const py = Math.min(LADO_TILE - 1, Math.max(0, Math.round(v * (LADO_TILE - 1))));
    for (let coluna = 0; coluna < largura; coluna++) {
      const u = offX + (coluna / (largura - 1)) * fracao;
      const px = Math.min(LADO_TILE - 1, Math.max(0, Math.round(u * (LADO_TILE - 1))));
      saida[linha * largura + coluna] = alturas[py * LADO_TILE + px];
    }
  }
  return saida;
}

const trava = (v, a, b) => Math.min(b, Math.max(a, v));

/**
 * Amostra bilinear dentro de um tile de alturas.
 *
 * A versão anterior usava o pixel mais próximo (`Math.floor`). Num heightmap de
 * 256×256 esticado em 65×65 postos isso terraceia a encosta: o relevo sai em
 * degraus e as bordas ficam serrilhadas. Interpolar custa três `mix` por posto
 * e tira o degrau.
 */
function bilinear(alturas, u, v) {
  const fx = u * LADO_TILE - 0.5;
  const fy = v * LADO_TILE - 0.5;
  const x0 = Math.floor(fx), y0 = Math.floor(fy);
  const dx = fx - x0, dy = fy - y0;
  const cx0 = trava(x0, 0, LADO_TILE - 1), cx1 = trava(x0 + 1, 0, LADO_TILE - 1);
  const cy0 = trava(y0, 0, LADO_TILE - 1), cy1 = trava(y0 + 1, 0, LADO_TILE - 1);
  const h00 = alturas[cy0 * LADO_TILE + cx0], h10 = alturas[cy0 * LADO_TILE + cx1];
  const h01 = alturas[cy1 * LADO_TILE + cx0], h11 = alturas[cy1 * LADO_TILE + cx1];
  return (h00 + (h10 - h00) * dx) * (1 - dy) + (h01 + (h11 - h01) * dx) * dy;
}

/**
 * Acima de ±85,05° não existe dado: a amostragem satura na borda da projeção.
 * No Ártico essa borda vale ~−2700 m (batimetria), o que renderizaria a calota
 * como um platô afundado — troca de artefato, não conserto. Aqui a altura vai a
 * zero (nível do mar sob o gelo) nos graus finais.
 */
function atenuacaoPolar(latGraus) {
  const excesso = (Math.abs(latGraus) - LIMITE_MERCATOR) / (90 - LIMITE_MERCATOR);
  if (excesso <= 0) return 1;
  const t = Math.min(1, excesso);
  return 1 - t * t * (3 - 2 * t);
}

/** Coordenada de tile Mercator (fracionária) de uma lat/lon, no zoom z. */
function paraMercator(lonGraus, latGraus, z) {
  const n = 1 << z;
  const rad = (trava(latGraus, -LIMITE_MERCATOR, LIMITE_MERCATOR) * Math.PI) / 180;
  return {
    xf: ((lonGraus + 180) / 360) * n,
    yf: ((1 - Math.log(Math.tan(rad) + 1 / Math.cos(rad)) / Math.PI) / 2) * n
  };
}

/**
 * Provedor de terreno do ARCZ (heightmap 65x65 por tile).
 *
 * O esquema é GEOGRÁFICO, não Web Mercator. Com `WebMercatorTilingScheme` o
 * quadtree do globo só existia entre ±85,05° — a projeção de Mercator não vai
 * além disso —, então as calotas polares simplesmente não tinham geometria: da
 * órbita aparecia um buraco preto de borda serrilhada no topo do planeta,
 * mostrando o `globe.baseColor` por trás. O DEM continua vindo em tiles
 * Terrarium (Mercator); a conversão é feita aqui, posto a posto, e acima de
 * 85° a amostragem satura na borda da projeção (calota de gelo, ~0 m).
 */
export function criarProvedorDeRelevo(largura = 65, altura = 65) {
  if (!Cesium.CustomHeightmapTerrainProvider) return null;

  const esquema = new Cesium.GeographicTilingScheme();
  const G = Cesium.Math.toDegrees;

  return new Cesium.CustomHeightmapTerrainProvider({
    width: largura,
    height: altura,
    tilingScheme: esquema,
    credit: "ARCZ · DEM local materializado",
    callback: async (x, y, nivel) => {
      const ret = esquema.tileXYToRectangle(x, y, nivel);
      const oeste = G(ret.west), leste = G(ret.east);
      const sul = G(ret.south), norte = G(ret.north);

      // Um tile geográfico do nível L cobre a mesma longitude que um tile
      // Mercator do nível L+1: é essa a fonte com detalhe equivalente.
      let z = Math.min(nivel + 1, Z_MAX_TERRARIUM);
      let canto0, canto1, txMin, txMax, tyMin, tyMax;
      for (;;) {
        canto0 = paraMercator(oeste, norte, z);
        canto1 = paraMercator(leste, sul, z);
        const n = 1 << z;
        txMin = trava(Math.floor(canto0.xf), 0, n - 1);
        txMax = trava(Math.floor(canto1.xf - 1e-9), 0, n - 1);
        tyMin = trava(Math.floor(canto0.yf), 0, n - 1);
        tyMax = trava(Math.floor(canto1.yf - 1e-9), 0, n - 1);
        const quantos = (txMax - txMin + 1) * (tyMax - tyMin + 1);
        if (quantos <= MAX_TILES_FONTE || z === 0) break;
        z--;   // nível baixo demais pediria o mundo inteiro em tiles finos
      }

      // Busca os tiles-fonte uma vez só e depois amostra sem esperar nada.
      const fontes = new Map();
      const pedidos = [];
      for (let tx = txMin; tx <= txMax; tx++) {
        for (let ty = tyMin; ty <= tyMax; ty++) {
          const k = `${tx}/${ty}`;
          pedidos.push(baixarTile(z, tx, ty).then(a => fontes.set(k, a)));
        }
      }
      await Promise.all(pedidos);

      const n = 1 << z;
      const saida = new Float32Array(largura * altura);
      for (let linha = 0; linha < altura; linha++) {
        // Linha 0 do heightmap é a borda NORTE do tile.
        const lat = norte - (linha / (altura - 1)) * (norte - sul);
        const polar = atenuacaoPolar(lat);
        for (let coluna = 0; coluna < largura; coluna++) {
          const lon = oeste + (coluna / (largura - 1)) * (leste - oeste);
          const { xf, yf } = paraMercator(lon, lat, z);
          const tx = trava(Math.floor(xf), 0, n - 1);
          const ty = trava(Math.floor(yf), 0, n - 1);
          const fonte = fontes.get(`${tx}/${ty}`);
          if (!fonte) {
            const erro = new Error(`DEM local não montado para ${z}/${tx}/${ty}`);
            erro.code = "DEM_LOCAL_MISSING";
            throw erro;
          }
          saida[linha * largura + coluna] = bilinear(fonte, xf - tx, yf - ty) * polar;
        }
      }
      return saida;
    }
  });
}

/** Altura do terreno (m) numa coordenada, direto do DEM — sem depender do render. */
export async function alturaDoTerreno(lon, lat, z = 14) {
  const n = 1 << z;
  const { xf, yf } = paraMercator(lon, lat, z);
  const alturas = await baixarTile(
    z, trava(Math.floor(xf), 0, n - 1), trava(Math.floor(yf), 0, n - 1)
  );
  const px = Math.min(LADO_TILE - 1, Math.floor((xf % 1) * LADO_TILE));
  const py = Math.min(LADO_TILE - 1, Math.floor((yf % 1) * LADO_TILE));
  return alturas[py * LADO_TILE + px];
}
