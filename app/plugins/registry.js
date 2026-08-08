import { validatePluginManifest } from "../core/schema.js";

export class PluginRegistry {
  constructor() { this._plugins=new Map(); this._states=new Map(); }
  register(plugin) {
    const manifest=validatePluginManifest(plugin?.manifest);
    if (this._plugins.has(manifest.id)) throw new Error(`Plugin duplicado: ${manifest.id}`);
    for (const method of requiredMethods(manifest.tipo)) if (typeof plugin[method]!=="function") throw new Error(`Plugin ${manifest.id} sem ${method}()`);
    this._plugins.set(manifest.id,plugin); this._states.set(manifest.id,{status:"REGISTERED",errors:[],runs:0}); return plugin;
  }
  unregister(id) { this._states.delete(id); return this._plugins.delete(id); }
  get(id) { return this._plugins.get(id)||null; }
  list({mode=null,scale=null,type=null}={}) {
    return [...this._plugins.values()].filter(p=>(!mode||p.manifest.modos.includes(mode))&&(!scale||p.manifest.escalas.includes(scale))&&(!type||p.manifest.tipo===type));
  }
  state(id) { return this._states.get(id)||null; }
  updateState(id,patch) { const current=this.state(id); if(!current) throw new Error(`Plugin não registrado: ${id}`); Object.assign(current,patch); return current; }
  diagnostics() { return [...this._plugins.values()].map(p=>({manifest:p.manifest,state:{...this.state(p.manifest.id)}})); }
}
function requiredMethods(type) { return type==="gerador"?["validar","estimar","preparar","gerar","validarResultado","stage","commit","rollback","limpar","serializar","migrar"]:["ativar","desativar","serializar"]; }
export const pluginRegistry=new PluginRegistry();
