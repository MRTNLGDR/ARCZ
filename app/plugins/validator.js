import { validatePluginManifest, ValidationError } from "../core/schema.js";

export function validatePlugin(plugin) {
  const manifest=validatePluginManifest(plugin?.manifest);
  const errors=[];
  if (manifest.deterministico && !plugin.replayKey) errors.push("plugin determinístico precisa expor replayKey(ctx, params)");
  if (manifest.tipo==="gerador" && manifest.custoBase.triangulos===0 && manifest.backend_kind!=="metadata") errors.push("gerador geométrico não pode declarar custo zero");
  if (errors.length) throw new ValidationError(`Plugin ${manifest.id} rejeitado`,{path:"$",code:"PLUGIN_INVALID",details:{errors}});
  return {ok:true,manifest};
}

export function validatePluginParameters(plugin, params) {
  if (!params || typeof params!=="object" || Array.isArray(params)) throw new ValidationError("parâmetros precisam ser objeto",{path:"$.params"});
  for (const field of plugin.parameters||[]) {
    const value=params[field.id] ?? field.default;
    if (field.required && value===undefined) throw new ValidationError(`parâmetro obrigatório: ${field.id}`,{path:`$.params.${field.id}`});
    if (value===undefined) continue;
    if (field.type==="number") {
      if(typeof value!=="number"||!Number.isFinite(value)) throw new ValidationError("número esperado",{path:`$.params.${field.id}`});
      if(field.min!==undefined&&value<field.min) throw new ValidationError("abaixo do mínimo",{path:`$.params.${field.id}`});
      if(field.max!==undefined&&value>field.max) throw new ValidationError("acima do máximo",{path:`$.params.${field.id}`});
    }
    if(field.type==="boolean"&&typeof value!=="boolean") throw new ValidationError("boolean esperado",{path:`$.params.${field.id}`});
    if(field.type==="enum"&&!field.options.includes(value)) throw new ValidationError("opção inválida",{path:`$.params.${field.id}`});
  }
  return true;
}
