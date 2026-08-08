// ARCZ Core · Proteção obrigatória para callbacks executados no laço do Cesium.
const REGISTRO = new Map();

function agora() {
  return typeof performance !== "undefined" && performance.now ? performance.now() : Date.now();
}

export function safeCallback(nome, fallback, fn, {
  limiteFalhas = 3,
  janelaMs = 10_000,
  pluginId = null,
  entidadeId = null,
  onError = null
} = {}) {
  if (typeof nome !== "string" || !nome) throw new TypeError("safeCallback exige nome");
  if (typeof fn !== "function") throw new TypeError("safeCallback exige função");
  const estado = { nome, pluginId, entidadeId, falhas: [], total: 0, desativado: false, ultimoErro: null };
  REGISTRO.set(nome, estado);

  const wrapper = function (...args) {
    if (estado.desativado) return typeof fallback === "function" ? fallback(...args) : fallback;
    try {
      return fn.apply(this, args);
    } catch (erro) {
      const t = agora();
      estado.total += 1;
      estado.ultimoErro = String(erro?.stack || erro);
      estado.falhas = estado.falhas.filter(x => t - x <= janelaMs);
      estado.falhas.push(t);
      if (estado.falhas.length >= limiteFalhas) estado.desativado = true;
      const detalhe = {
        nome,
        pluginId,
        entidadeId,
        total: estado.total,
        falhasNaJanela: estado.falhas.length,
        desativado: estado.desativado,
        erro: estado.ultimoErro
      };
      console.error("[ARCZ/safe-callback] callback protegido falhou", detalhe);
      try { onError?.(erro, detalhe); } catch (erroSecundario) {
        console.error("[ARCZ/safe-callback] o observador de erro também falhou", erroSecundario);
      }
      return typeof fallback === "function" ? fallback(...args) : fallback;
    }
  };
  Object.defineProperty(wrapper, "arczSafeCallback", { value: estado, enumerable: false });
  return wrapper;
}

export function reativarSafeCallback(nome) {
  const estado = REGISTRO.get(nome);
  if (!estado) return false;
  estado.falhas = [];
  estado.desativado = false;
  estado.ultimoErro = null;
  return true;
}

export function diagnosticoSafeCallbacks() {
  return [...REGISTRO.values()].map(v => ({ ...v, falhas: [...v.falhas] }));
}
