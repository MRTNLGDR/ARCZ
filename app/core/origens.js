// ARCZ Core · Registro central e fechado de origens de mutação.
// Nunca use strings livres ao chamar estadoApp.atualizar(). Origem desconhecida
// é um defeito de integração porque impede observadores de filtrar atualizações.
export const ORIGENS = Object.freeze({
  CAMERA: "camera",
  CENA: "cena",
  REGIAO: "regiao",
  GERADOR: "gerador",
  IA: "ia",
  HISTORICO: "historico",
  SISTEMA: "sistema",
  CARREGAMENTO_INICIAL: "carregamento_inicial",
  PLUGIN: "plugin",
  CINEMA: "cinema",
  WALK: "walk",
  UI: "ui"
});

const CONHECIDAS = new Set(Object.values(ORIGENS));

export function origemConhecida(origem) {
  return typeof origem === "string" && CONHECIDAS.has(origem);
}

export function registrarOrigem(nome) {
  if (typeof nome !== "string" || !/^[a-z][a-z0-9_.-]{1,63}$/.test(nome)) {
    throw new TypeError(`Origem inválida: ${String(nome)}`);
  }
  if (CONHECIDAS.has(nome)) return nome;
  CONHECIDAS.add(nome);
  return nome;
}

export function validarOrigem(origem, { desenvolvimento = true } = {}) {
  if (origemConhecida(origem)) return origem;
  const mensagem = `Origem de estado não registrada: ${String(origem)}`;
  if (desenvolvimento) console.warn(`[ARCZ/origens] ${mensagem}`);
  return ORIGENS.SISTEMA;
}

export function politicaDeOrigem({ processar = [], ignorar = [] } = {}) {
  const aceitas = new Set(processar);
  const ignoradas = new Set(ignorar);
  for (const origem of [...aceitas, ...ignoradas]) validarOrigem(origem);
  return Object.freeze({
    deveProcessar(origem) {
      const normalizada = validarOrigem(origem);
      if (ignoradas.has(normalizada)) return false;
      return aceitas.size === 0 || aceitas.has(normalizada);
    },
    processar: Object.freeze([...aceitas]),
    ignorar: Object.freeze([...ignoradas])
  });
}
