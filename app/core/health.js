export async function inicializarModulos(modulos, ctx, { onResult = null } = {}) {
  const resultados = [];
  for (const modulo of modulos) {
    const nome = modulo?.id || modulo?.nome || modulo?.constructor?.name || "modulo-desconhecido";
    try {
      if (typeof modulo?.init !== "function") throw new Error("init(ctx) ausente");
      const resultado = await modulo.init(ctx);
      const normalizado = {
        nome, ok: resultado?.ok === true, versao: String(resultado?.versao || "0.0.0"),
        dependencias: resultado?.dependencias || [], avisos: resultado?.avisos || []
      };
      if (!normalizado.ok) normalizado.erro = resultado?.erro || "health check recusou o módulo";
      resultados.push(normalizado); onResult?.(normalizado);
    } catch (erro) {
      const resultado = { nome, ok: false, versao: "unknown", dependencias: [], avisos: [], erro: String(erro?.stack || erro) };
      resultados.push(resultado); onResult?.(resultado);
    }
  }
  return { ok: resultados.every(r => r.ok), resultados };
}

export function criarModuloSaudavel({ id, versao, dependencias = [], smokeTest = null, init = null }) {
  return Object.freeze({
    id,
    async init(ctx) {
      await init?.(ctx);
      await smokeTest?.(ctx);
      return { ok: true, versao, dependencias, avisos: [] };
    }
  });
}
