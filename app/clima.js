// ARCZ · Modelo de céu e clima.
//
// Converte "elevação real do sol + condição de tempo" num conjunto coerente de
// parâmetros que valem para a CENA INTEIRA: cor e intensidade da luz direta,
// espalhamento Rayleigh/Mie da atmosfera, névoa, sombra, brilho da imagem,
// nuvens e precipitação.
//
// Antes disto a hora só mexia no relógio: `scene.light` ficava fixo em
// (branco, 2.0) e `scene.atmosphere` ficava em zero, então o globo, o céu e a
// névoa não mudavam com o horário — só o modelo importado parecia reagir,
// porque a sombra dele girava.

// Valores de referência do Cesium (Atmosphere), usados como base das curvas.
const RAYLEIGH_BASE = { x: 5.5e-6, y: 13.0e-6, z: 28.4e-6 };
const MIE_BASE = 21.0e-6;
const LUZ_ATM_BASE = 10.0;

export const CONDICOES = {
  limpo:      { nome: "Céu limpo",            cobertura: 0,   neblina: 3,  aerossol: 6,  luz: 1.00, difusa: 0.10, saturacao: 0.04, precipitacao: "nenhuma" },
  poucas:     { nome: "Poucas nuvens",        cobertura: 20,  neblina: 7,  aerossol: 12, luz: 0.94, difusa: 0.25, saturacao: 0.01, precipitacao: "nenhuma" },
  parcial:    { nome: "Parcialmente nublado", cobertura: 45,  neblina: 13, aerossol: 20, luz: 0.78, difusa: 0.45, saturacao: -0.04, precipitacao: "nenhuma" },
  nublado:    { nome: "Nublado",              cobertura: 75,  neblina: 22, aerossol: 30, luz: 0.52, difusa: 0.75, saturacao: -0.13, precipitacao: "nenhuma" },
  encoberto:  { nome: "Encoberto",            cobertura: 95,  neblina: 32, aerossol: 40, luz: 0.32, difusa: 0.95, saturacao: -0.22, precipitacao: "nenhuma" },
  chuva:      { nome: "Chuva",                cobertura: 90,  neblina: 45, aerossol: 34, luz: 0.36, difusa: 1.00, saturacao: -0.28, precipitacao: "chuva" },
  tempestade: { nome: "Tempestade",           cobertura: 100, neblina: 58, aerossol: 46, luz: 0.30, difusa: 1.00, saturacao: -0.36, precipitacao: "chuva" },
  neblina:    { nome: "Neblina densa",        cobertura: 55,  neblina: 85, aerossol: 65, luz: 0.42, difusa: 1.00, saturacao: -0.30, precipitacao: "nenhuma" },
  neve:       { nome: "Neve",                 cobertura: 92,  neblina: 50, aerossol: 38, luz: 0.36, difusa: 1.00, saturacao: -0.30, precipitacao: "neve" },
  poeira:     { nome: "Poeira / fumaça",      cobertura: 18,  neblina: 38, aerossol: 92, luz: 0.62, difusa: 0.55, saturacao: 0.10, precipitacao: "nenhuma" }
};

const COR_ZENITE = { r: 1.00, g: 0.985, b: 0.96 };
const COR_ALTA = { r: 1.00, g: 0.94, b: 0.83 };
const COR_RASANTE = { r: 1.00, g: 0.72, b: 0.44 };
const COR_HORIZONTE = { r: 1.00, g: 0.50, b: 0.28 };
const COR_LUAR = { r: 0.60, g: 0.71, b: 1.00 };

const trava = (v, a, b) => Math.min(b, Math.max(a, v));
const trava01 = v => trava(v, 0, 1);
/** Hermite: tira o "degrau" das transições de fase do dia. */
const suave = v => { const t = trava01(v); return t * t * (3 - 2 * t); };
const mistura = (a, b, t) => a + (b - a) * t;

function misturarCor(a, b, t) {
  return { r: mistura(a.r, b.r, t), g: mistura(a.g, b.g, t), b: mistura(a.b, b.b, t) };
}

/** Cor da luz direta pela altura do sol: branco no zênite, laranja no horizonte. */
function corDoSol(elev) {
  if (elev >= 30) return COR_ZENITE;
  if (elev >= 10) return misturarCor(COR_ALTA, COR_ZENITE, (elev - 10) / 20);
  if (elev >= 3) return misturarCor(COR_RASANTE, COR_ALTA, (elev - 3) / 7);
  if (elev >= -0.833) return misturarCor(COR_HORIZONTE, COR_RASANTE, (elev + 0.833) / 3.833);
  return misturarCor(COR_LUAR, COR_HORIZONTE, trava01((elev + 8) / 7.167));
}

export function faseDoDia(elev) {
  if (elev >= 25) return "Meio-dia";
  if (elev >= 10) return "Dia pleno";
  if (elev >= 3) return "Manhã / tarde";
  if (elev >= -0.833) return "Hora dourada";
  if (elev >= -6) return "Hora azul";
  if (elev >= -18) return "Crepúsculo";
  return "Noite";
}

const num = (v, padrao) => (v === undefined || v === null || Number.isNaN(Number(v)) ? padrao : Number(v));

/**
 * Perfil completo do ambiente para uma elevação solar e um estado de clima.
 * Tudo aqui é dado puro: quem aplica no viewer é o ambiente.js.
 */
export function modeloDoCeu(elevacao, amb = {}) {
  const cond = CONDICOES[amb.condicao] || CONDICOES.limpo;
  const cobertura = trava01(num(amb.nuvens_cobertura, cond.cobertura) / 100);
  const neblina = trava01(num(amb.neblina, cond.neblina) / 100);
  const aerossol = trava01(num(amb.aerossol, cond.aerossol) / 100);
  const ganhoSol = num(amb.sol_intensidade, 2.6) / 2.6;
  const ganhoBrilho = num(amb.brilho_ambiente, 1.0);

  // dia: 0 com o sol a -8°, 1 com o sol a +8°. noite: 0 a +1°, 1 a -9°.
  // A faixa é larga de propósito: o crepúsculo civil real ainda é claro, e a
  // primeira calibragem (-5°/+8°) apagava a cena logo depois do pôr do sol —
  // o terreno media 0,1 de 255 na leitura de pixel.
  const dia = suave((elevacao + 8) / 16);
  const noite = suave(-(elevacao - 1) / 10);
  // rasante: quanto o sol está baixo mas ainda acima do horizonte (hora dourada).
  const rasante = trava01(1 - Math.abs(elevacao - 2.5) / 9) * (1 - noite);

  const atenuacaoNuvem = 1 - cobertura * 0.72;
  const atenuacaoAr = 1 - aerossol * 0.30 - neblina * 0.25;
  const luzRelativa = trava01(cond.luz * atenuacaoNuvem * atenuacaoAr);

  const cor = corDoSol(elevacao);
  // Sob nuvem grossa a luz esfria e perde saturação (vira luz de céu, não de sol).
  const fatorCinza = cobertura * 0.55 + neblina * 0.25;
  const corFinal = misturarCor(cor, { r: 0.82, g: 0.86, b: 0.94 }, trava01(fatorCinza));

  // O piso não é zero: à noite sobra luz de céu e de lua, e uma cena de estudo
  // com o chão em preto absoluto não serve para ler volume nem implantação.
  const intensidadeSol = trava(
    (0.30 + 3.10 * dia) * luzRelativa * ganhoSol * (1 - noite * 0.88) + noite * 0.16 * ganhoSol,
    0.0, 8.0
  );

  // Espalhamento: mais aerossol/neblina = horizonte mais leitoso e menos azul.
  const mie = MIE_BASE * (1 + aerossol * 6 + neblina * 4);
  const rayleigh = 1 - aerossol * 0.35;

  const difusa = trava01(cond.difusa + cobertura * 0.5 + neblina * 0.4);

  return {
    fase: faseDoDia(elevacao),
    elevacao,
    dia, noite, rasante,
    cobertura, neblina, aerossol,
    luzRelativa,

    luz: {
      cor: corFinal,
      intensidade: intensidadeSol
    },

    atmosfera: {
      lightIntensity: LUZ_ATM_BASE * (0.30 + 0.85 * dia) * mistura(1, 0.55, cobertura) * ganhoBrilho,
      hueShift: mistura(0, 0.03, rasante) - noite * 0.02,
      saturationShift: cond.saturacao + rasante * 0.16 - noite * 0.12,
      brightnessShift: -0.28 * noite + 0.10 * rasante - cobertura * 0.12,
      rayleighCoefficient: {
        x: RAYLEIGH_BASE.x * rayleigh,
        y: RAYLEIGH_BASE.y * rayleigh,
        z: RAYLEIGH_BASE.z * rayleigh
      },
      mieCoefficient: { x: mie, y: mie, z: mie },
      mieAnisotropy: trava(0.9 - neblina * 0.35 - aerossol * 0.15, 0.35, 0.95),
      rayleighScaleHeight: 10000 * (1 + neblina * 0.25),
      mieScaleHeight: 3200 * (1 + aerossol * 0.9 + neblina * 1.4)
    },

    // A névoa é o que "cola" o modelo na paisagem: densa e escura à noite,
    // rala e clara ao meio-dia limpo.
    nevoa: {
      density: 0.00006 + Math.pow(neblina, 1.5) * 0.0022 + aerossol * 0.00035,
      minimumBrightness: trava(0.04 + 0.24 * dia + noite * 0.04 - cobertura * 0.06, 0.03, 0.4)
    },

    // Sombra fraca e macia sob nuvem; dura e recortada em céu limpo.
    sombra: {
      darkness: trava(0.16 + difusa * 0.62 + noite * 0.2, 0.14, 0.92),
      suave: difusa > 0.35
    },

    ceu: {
      mostrarSol: elevacao > -4 && cobertura < 0.88,
      glow: trava(0.9 + rasante * 1.8 - cobertura * 0.6, 0.2, 3.0),
      mostrarLua: noite > 0.15 && cobertura < 0.85,
      mostrarEstrelas: noite > 0.05 && cobertura < 0.8
    },

    // Sem luz direta o modelo ficaria preto: a luz do céu (IBL) sobe à noite.
    ibl: trava(0.55 + difusa * 0.35 + noite * 0.45, 0.4, 1.0),
    // Sob tempestade a cena precisa continuar legível: a nuvem tira luz
    // direta, e a exposição devolve parte dela como o olho faria.
    exposicao: trava(ganhoBrilho * (1 + noite * 0.85 + cobertura * 0.14), 0.3, 3.0),

    bloom: {
      contrast: mistura(120, 190, rasante),
      brightness: mistura(-0.35, -0.05, rasante + noite * 0.5),
      delta: 1.4,
      sigma: 2.2 + rasante
    },

    nuvens: {
      quantidade: Math.round(cobertura * 190),
      altura: num(amb.nuvens_altura, 1200),
      brilho: trava(0.45 + dia * 0.55 - cobertura * 0.15, 0.15, 1.0),
      escala: mistura(420, 900, cobertura)
    },

    precipitacao: {
      tipo: amb.precipitacao && amb.precipitacao !== "auto" ? amb.precipitacao : cond.precipitacao,
      intensidade: trava01(0.35 + cobertura * 0.65)
    }
  };
}

/**
 * Perfil de órbita: como o planeta se apresenta visto do espaço.
 *
 * O modelo acima descreve o céu SOBRE O SÍTIO. Aplicá-lo à cena inteira quando
 * a câmera está a milhares de quilômetros é errado e era o que apagava a Terra:
 * com o sítio à noite, `SunLight` caía para 0,19 e `brightnessShift` para
 * −0,3, então o planeta inteiro ficava escuro e sem limbo azul — inclusive o
 * lado que estava em pleno dia. Da órbita quem manda é o Sol de verdade, não o
 * tempo em Itapema.
 */
export function perfilOrbital() {
  return {
    fase: "Órbita",
    luz: { cor: { r: 1, g: 1, b: 1 }, intensidade: 2.4 },
    atmosfera: {
      lightIntensity: LUZ_ATM_BASE,
      hueShift: 0, saturationShift: 0, brightnessShift: 0,
      rayleighCoefficient: { ...RAYLEIGH_BASE },
      mieCoefficient: { x: MIE_BASE, y: MIE_BASE, z: MIE_BASE },
      mieAnisotropy: 0.9,
      rayleighScaleHeight: 10000,
      mieScaleHeight: 3200
    },
    // Névoa é fenômeno de superfície: da órbita ela só lava a imagem.
    nevoa: { density: 0.0, minimumBrightness: 0.03 },
    sombra: { darkness: 0.3, suave: false },
    ceu: { mostrarSol: true, glow: 1.2, mostrarLua: true, mostrarEstrelas: true },
    ibl: 1.0,
    exposicao: 1.0,
    bloom: { contrast: 128, brightness: -0.3, delta: 1.4, sigma: 2.0 },
    nuvens: { quantidade: 0, altura: 1200, brilho: 1, escala: 600 },
    precipitacao: { tipo: "nenhuma", intensidade: 0 },
    orbital: true
  };
}

/** Interpola dois perfis. t=0 é o do sítio, t=1 é o de órbita. */
export function misturarPerfis(local, orbital, t) {
  const k = trava01(t);
  if (k <= 0.001) return local;
  if (k >= 0.999) return { ...orbital, fase: local.fase, elevacao: local.elevacao, azimute: local.azimute };
  const lerpObj = (a, b) => {
    const saida = {};
    for (const chave of Object.keys(b)) {
      const va = a[chave], vb = b[chave];
      if (typeof va === "number" && typeof vb === "number") saida[chave] = mistura(va, vb, k);
      else if (va && vb && typeof va === "object" && typeof vb === "object") saida[chave] = lerpObj(va, vb);
      else saida[chave] = k > 0.5 ? vb : va;
    }
    return saida;
  };
  return {
    ...local,
    luz: { cor: misturarCor(local.luz.cor, orbital.luz.cor, k),
           intensidade: mistura(local.luz.intensidade, orbital.luz.intensidade, k) },
    atmosfera: lerpObj(local.atmosfera, orbital.atmosfera),
    nevoa: lerpObj(local.nevoa, orbital.nevoa),
    sombra: { darkness: mistura(local.sombra.darkness, orbital.sombra.darkness, k), suave: local.sombra.suave },
    ceu: {
      mostrarSol: k > 0.5 ? true : local.ceu.mostrarSol,
      glow: mistura(local.ceu.glow, orbital.ceu.glow, k),
      mostrarLua: k > 0.5 ? true : local.ceu.mostrarLua,
      mostrarEstrelas: k > 0.5 ? true : local.ceu.mostrarEstrelas
    },
    ibl: mistura(local.ibl, orbital.ibl, k),
    exposicao: mistura(local.exposicao, orbital.exposicao, k),
    bloom: lerpObj(local.bloom, orbital.bloom),
    nuvens: { ...local.nuvens, quantidade: Math.round(local.nuvens.quantidade * (1 - k)) },
    precipitacao: { tipo: k > 0.35 ? "nenhuma" : local.precipitacao.tipo,
                    intensidade: local.precipitacao.intensidade * (1 - k) },
    orbital: k > 0.5
  };
}

// ------------------------------------------------------------ precipitação
// Estágios de pós-processamento: valem para o quadro inteiro (céu, terreno e
// modelo), que é o ponto — chuva desenhada só em cima do prédio não é clima.

// A primeira versão misturava para uma cor FIXA cinza-azulada. Sobre areia
// clara ao sol isso subtrai vermelho e o risco aparece como sujeira ESCURA —
// foi o que apareceu na tela: pontinhos pretos espalhados pelo terreno.
// A gota de chuva é sempre mais clara que o fundo: aqui ela é somada, não
// misturada, e por isso funciona tanto contra o céu quanto contra o chão.
const SHADER_CHUVA = `
uniform sampler2D colorTexture;
uniform float intensidade;
uniform float inclinacao;
in vec2 v_textureCoordinates;

float ruido(vec2 c) {
  return fract(sin(dot(c, vec2(12.9898, 78.233))) * 43758.5453);
}

/** Uma camada de riscos; várias camadas dão profundidade. */
float camada(vec2 uv, float escala, float velocidade, float t, float incl, float largura) {
  vec2 p = uv * escala;
  p.x += incl * p.y;
  p.y -= t * velocidade;

  vec2 celula = floor(p);
  vec2 dentro = fract(p);
  float semente = ruido(celula);
  float ativa = step(1.0 - clamp(intensidade, 0.0, 1.0) * 0.55, semente);
  float coluna = fract(semente * 7.13);
  // Risco fino e alongado, com as pontas suaves.
  return ativa
       * smoothstep(largura, 0.0, abs(dentro.x - coluna))
       * smoothstep(0.0, 0.25, dentro.y)
       * smoothstep(1.0, 0.35, dentro.y);
}

void main() {
  vec4 cor = texture(colorTexture, v_textureCoordinates);
  float t = czm_frameNumber / 60.0;
  vec2 razao = vec2(czm_viewport.z / max(czm_viewport.w, 1.0), 1.0);
  vec2 uv = v_textureCoordinates * razao;

  float chuva = camada(uv, 46.0, 15.0, t, inclinacao * 0.8, 0.020) * 0.55
              + camada(uv + 0.37, 30.0, 10.0, t, inclinacao * 0.6, 0.015) * 0.38
              + camada(uv + 0.71, 68.0, 21.0, t, inclinacao * 1.0, 0.026) * 0.30;

  // Aditivo e discreto: chuva clareia, nunca suja a imagem.
  vec3 saida = cor.rgb + vec3(0.55, 0.62, 0.72) * chuva * clamp(intensidade, 0.0, 1.0) * 0.55;
  out_FragColor = vec4(min(saida, vec3(1.0)), cor.a);
}
`;

const SHADER_NEVE = `
uniform sampler2D colorTexture;
uniform float intensidade;
in vec2 v_textureCoordinates;

float ruido(vec2 c) {
  return fract(sin(dot(c, vec2(45.233, 91.719))) * 24634.6345);
}

float flocos(vec2 uv, float escala, float velocidade, float t, float raio) {
  vec2 p = uv * escala;
  p.y -= t * velocidade;
  vec2 celula = floor(p);
  vec2 dentro = fract(p);
  float semente = ruido(celula);
  float ativa = step(1.0 - clamp(intensidade, 0.0, 1.0) * 0.5, semente);
  vec2 centro = vec2(
    clamp(fract(semente * 13.0) + 0.16 * sin(t * 1.6 + semente * 31.0), 0.2, 0.8),
    clamp(fract(semente * 29.0), 0.2, 0.8)
  );
  return ativa * smoothstep(raio, 0.0, distance(dentro, centro));
}

void main() {
  vec4 cor = texture(colorTexture, v_textureCoordinates);
  float t = czm_frameNumber / 60.0;
  vec2 razao = vec2(czm_viewport.z / max(czm_viewport.w, 1.0), 1.0);
  vec2 uv = v_textureCoordinates * razao;

  float neve = flocos(uv, 26.0, 2.4, t, 0.075) * 0.75
             + flocos(uv + 0.43, 16.0, 1.5, t, 0.105) * 0.55
             + flocos(uv + 0.81, 40.0, 3.4, t, 0.055) * 0.40;

  vec3 saida = cor.rgb + vec3(0.85, 0.90, 0.98) * neve * clamp(intensidade, 0.0, 1.0) * 0.5;
  out_FragColor = vec4(min(saida, vec3(1.0)), cor.a);
}
`;

export function criarEstagioPrecipitacao(tipo) {
  const chuva = tipo === "chuva";
  const uniforms = chuva
    ? { intensidade: 0.7, inclinacao: 0.25 }
    : { intensidade: 0.6 };
  return new Cesium.PostProcessStage({
    name: `arcz_${tipo}`,
    fragmentShader: chuva ? SHADER_CHUVA : SHADER_NEVE,
    uniforms
  });
}
