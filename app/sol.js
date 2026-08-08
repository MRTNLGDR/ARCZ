// ARCZ · Posição solar real (efemérides), não aproximação de calendário.
//
// Usa exatamente a mesma fonte que o Cesium usa para iluminar a cena:
// Simon1994PlanetaryPositions + rotação ICRF→ECEF, com o MESMO fallback para
// TEME que o UniformState do Cesium aplica quando os dados de orientação da
// Terra (IAU2006 XYS) não estão carregados. Por isso o azimute/elevação que o
// painel mostra é o que a cena está de fato renderizando.
//
// O código anterior (`calcularSolNaFachada`) fazia `if (!paraFixo) continue;`:
// sem os dados de EOP carregados, `computeIcrfToFixedMatrix` devolve undefined
// e a varredura inteira era descartada — a função respondia sempre 12:00.

const _rot = new Cesium.Matrix3();
const _icrf = new Cesium.Cartesian3();
const _ecef = new Cesium.Cartesian3();
const _enu = new Cesium.Matrix3();
const _enu2 = new Cesium.Matrix3();
const _m4 = new Cesium.Matrix4();

/** Altura aparente do disco solar no nascer/ocaso (refração + semidiâmetro). */
export const ELEV_DISCO = -0.833;
export const ELEV_CIVIL = -6.0;
export const ELEV_NAUTICO = -12.0;
export const ELEV_ASTRO = -18.0;

/** Vetor unitário Terra→Sol em coordenadas fixas na Terra (ECEF). */
export function direcaoSolarEcef(tempo, resultado = new Cesium.Cartesian3()) {
  let rot = Cesium.Transforms.computeIcrfToFixedMatrix(tempo, _rot);
  if (!Cesium.defined(rot)) {
    rot = Cesium.Transforms.computeTemeToPseudoFixedMatrix(tempo, _rot);
  }
  const icrf = Cesium.Simon1994PlanetaryPositions.computeSunPositionInEarthInertialFrame(tempo, _icrf);
  const fixo = Cesium.defined(rot)
    ? Cesium.Matrix3.multiplyByVector(rot, icrf, resultado)
    : Cesium.Cartesian3.clone(icrf, resultado);
  return Cesium.Cartesian3.normalize(fixo, resultado);
}

/**
 * Sol visto de um ponto do terreno.
 * azimute: 0° = Norte geográfico, 90° = Leste. elevação: graus acima do horizonte.
 */
export function solLocal(tempo, lon, lat, alt = 0) {
  const dir = direcaoSolarEcef(tempo, _ecef);
  const origem = Cesium.Cartesian3.fromDegrees(lon, lat, alt || 0);
  const paraEcef = Cesium.Matrix4.getMatrix3(
    Cesium.Transforms.eastNorthUpToFixedFrame(origem, undefined, _m4),
    _enu
  );
  const paraEnu = Cesium.Matrix3.transpose(paraEcef, _enu2);
  const v = Cesium.Matrix3.multiplyByVector(paraEnu, dir, new Cesium.Cartesian3());

  const elevacao = Cesium.Math.toDegrees(Math.asin(Cesium.Math.clamp(v.z, -1, 1)));
  let azimute = Cesium.Math.toDegrees(Math.atan2(v.x, v.y));
  if (azimute < 0) azimute += 360;
  return { elevacao, azimute, enu: v };
}

/** Fuso estimado pela longitude (usado quando o projeto não define um). */
export function fusoDaLongitude(lon) {
  const f = Math.round(Number(lon || 0) / 15);
  return Cesium.Math.clamp(f, -12, 14);
}

export function dataDeHoje() {
  const d = new Date();
  const p = n => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

function partes(dataIso) {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(dataIso || "").trim());
  if (!m) {
    const d = new Date();
    return [d.getFullYear(), d.getMonth() + 1, d.getDate()];
  }
  return [Number(m[1]), Number(m[2]), Number(m[3])];
}

/**
 * Instante UTC (JulianDate) de uma hora CIVIL LOCAL DO SÍTIO.
 * Antes o código montava `new Date(ano, mes, dia, h, m)`, que interpreta a hora
 * no fuso do navegador — a cena de um projeto no Brasil aberta numa máquina em
 * UTC ficava 3 h fora de posição.
 */
export function instanteUtc(dataIso, hora, fuso) {
  const [a, m, d] = partes(dataIso);
  const ms = Date.UTC(a, m - 1, d, 0, 0, 0) + (Number(hora) - Number(fuso)) * 3600000;
  return Cesium.JulianDate.fromDate(new Date(ms));
}

/** Hora civil local (decimal) de um JulianDate. */
export function horaLocalDe(tempo, dataIso, fuso) {
  const [a, m, d] = partes(dataIso);
  const base = Date.UTC(a, m - 1, d, 0, 0, 0);
  const ms = Cesium.JulianDate.toDate(tempo).getTime();
  return (ms - base) / 3600000 + Number(fuso);
}

function elevacaoEm(hora, ctx) {
  return solLocal(instanteUtc(ctx.data, hora, ctx.fuso), ctx.lon, ctx.lat, ctx.alt).elevacao;
}

/** Refina por bisseção a hora em que a elevação cruza `limiar`. */
function refinar(h0, h1, limiar, ctx) {
  let a = h0, b = h1;
  const fa = elevacaoEm(a, ctx) - limiar;
  for (let i = 0; i < 22; i++) {
    const m = (a + b) / 2;
    const fm = elevacaoEm(m, ctx) - limiar;
    if ((fa < 0) === (fm < 0)) a = m; else b = m;
  }
  return (a + b) / 2;
}

/**
 * Nascer, ocaso, meio-dia solar e crepúsculos do dia, no fuso do sítio.
 * Varre o dia de 4 em 4 minutos com a MESMA função de elevação usada na
 * renderização e refina os cruzamentos — vale em qualquer latitude, inclusive
 * sol da meia-noite e noite polar (devolve null quando o evento não ocorre).
 */
export function eventosSolares(dataIso, fuso, lon, lat, alt = 0) {
  const ctx = { data: dataIso, fuso, lon, lat, alt };
  const passo = 4 / 60;
  const amostras = [];
  for (let h = 0; h <= 24 + 1e-9; h += passo) amostras.push([h, elevacaoEm(h, ctx)]);

  const cruzar = (limiar, subindo) => {
    for (let i = 1; i < amostras.length; i++) {
      const [h0, e0] = amostras[i - 1];
      const [h1, e1] = amostras[i];
      if (subindo && e0 < limiar && e1 >= limiar) return refinar(h0, h1, limiar, ctx);
      if (!subindo && e0 >= limiar && e1 < limiar) return refinar(h0, h1, limiar, ctx);
    }
    return null;
  };

  let melhor = amostras[0];
  for (const a of amostras) if (a[1] > melhor[1]) melhor = a;
  // Meio-dia solar por parábola em cima das três amostras vizinhas do máximo.
  const i = amostras.indexOf(melhor);
  let meioDia = melhor[0];
  if (i > 0 && i < amostras.length - 1) {
    const [x0, y0] = amostras[i - 1], [x1, y1] = amostras[i], [x2, y2] = amostras[i + 1];
    const den = (y0 - 2 * y1 + y2);
    if (Math.abs(den) > 1e-9) meioDia = x1 - (passo / 2) * ((y2 - y0) / den);
  }

  const nascer = cruzar(ELEV_DISCO, true);
  const ocaso = cruzar(ELEV_DISCO, false);
  const alvorada = cruzar(ELEV_CIVIL, true);
  const anoitecer = cruzar(ELEV_CIVIL, false);

  return {
    nascer,
    ocaso,
    meioDia,
    alvoradaCivil: alvorada,
    anoitecerCivil: anoitecer,
    elevacaoMaxima: elevacaoEm(meioDia, ctx),
    duracaoDia: nascer !== null && ocaso !== null ? ocaso - nascer : (melhor[1] > ELEV_DISCO ? 24 : 0),
    solDeMeiaNoite: nascer === null && ocaso === null && melhor[1] > ELEV_DISCO,
    noitePolar: nascer === null && ocaso === null && melhor[1] <= ELEV_DISCO
  };
}

/**
 * Hora em que o sol bate mais de frente numa fachada de rumo `rumoGraus`.
 * Varre o dia inteiro na resolução pedida e devolve também a incidência
 * (cosseno entre a normal da fachada e a direção do sol) para a UI poder dizer
 * se a fachada realmente pega sol naquele dia.
 */
export function solNaFachada(dataIso, fuso, lon, lat, rumoGraus, alt = 0) {
  const ctx = { data: dataIso, fuso, lon, lat, alt };
  const rad = Cesium.Math.toRadians(Number(rumoGraus) || 0);
  const normal = new Cesium.Cartesian3(Math.sin(rad), Math.cos(rad), 0);
  let melhor = { hora: null, incidencia: -1, elevacao: 0 };

  for (let h = 0; h <= 24; h += 5 / 60) {
    const s = solLocal(instanteUtc(dataIso, h, fuso), lon, lat, alt);
    if (s.elevacao <= 0) continue;
    const dir = Cesium.Cartesian3.normalize(s.enu, new Cesium.Cartesian3());
    const inc = Cesium.Cartesian3.dot(dir, normal);
    if (inc > melhor.incidencia) melhor = { hora: h, incidencia: inc, elevacao: s.elevacao };
  }
  if (melhor.hora === null) return null;
  void ctx;
  return melhor;
}

/** "15:30" a partir de hora decimal. */
export function textoHora(hora) {
  if (hora === null || hora === undefined || Number.isNaN(hora)) return "—";
  let h = hora % 24;
  if (h < 0) h += 24;
  const hh = Math.floor(h);
  const mm = Math.round((h - hh) * 60);
  return mm === 60 ? `${String((hh + 1) % 24).padStart(2, "0")}:00`
                   : `${String(hh).padStart(2, "0")}:${String(mm).padStart(2, "0")}`;
}

/** Rosa dos ventos em português, para o azimute do sol. */
export function rumoCardinal(az) {
  const nomes = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE",
                 "S", "SSO", "SO", "OSO", "O", "ONO", "NO", "NNO"];
  return nomes[Math.round(((az % 360) + 360) % 360 / 22.5) % 16];
}
