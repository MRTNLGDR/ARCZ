export class PluginLifecycle {
  constructor({registry}) { this.registry=registry; this.active=new Map(); }
  async activate(id,ctx) {
    const plugin=this.registry.get(id); if(!plugin) throw new Error(`Plugin ausente: ${id}`);
    if(this.active.has(id)) return this.active.get(id);
    const result=plugin.manifest.tipo==="ferramenta"?await plugin.ativar(ctx):{ok:true};
    this.active.set(id,{plugin,ctx,result}); this.registry.updateState(id,{status:"ACTIVE"}); return result;
  }
  async deactivate(id) {
    const active=this.active.get(id); if(!active) return false;
    try { if(active.plugin.manifest.tipo==="ferramenta") await active.plugin.desativar(active.ctx); else await active.plugin.limpar(active.ctx); }
    finally { active.ctx.resources?.disposeAll?.(); this.active.delete(id); this.registry.updateState(id,{status:"REGISTERED"}); }
    return true;
  }
  async disposeAll() { for(const id of [...this.active.keys()].reverse()) await this.deactivate(id); }
}
