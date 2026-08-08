import { executarTransacao } from "../core/transacao.js";
import { validatePluginParameters } from "./validator.js";

export class PluginOrchestrator {
  constructor({registry,contextFactory,telemetry=null}) { this.registry=registry; this.contextFactory=contextFactory; this.telemetry=telemetry; this.running=new Map(); }
  async run(id,params={},externalSignal=null) {
    const plugin=this.registry.get(id); if(!plugin) throw new Error(`Plugin não registrado: ${id}`);
    if(plugin.manifest.tipo!=="gerador") throw new Error(`${id} não é gerador`);
    validatePluginParameters(plugin,params);
    const controller=new AbortController();
    if(externalSignal){ if(externalSignal.aborted) controller.abort(externalSignal.reason); else externalSignal.addEventListener("abort",()=>controller.abort(externalSignal.reason),{once:true}); }
    const ctx=this.contextFactory(plugin,controller.signal); this.running.set(id,{controller,ctx});
    this.registry.updateState(id,{status:"RUNNING",runs:(this.registry.state(id)?.runs||0)+1});
    try {
      return await executarTransacao(`plugin:${id}`,async tx=>{
        await plugin.validar(ctx,params);
        const estimate=await plugin.estimar(ctx,params);
        const reservation=await ctx.budget.reserve({plugin_id:id,estimate});
        tx.onRollback(()=>reservation?.release?.());
        const prepared=await plugin.preparar(ctx,params,controller.signal);
        const result=await plugin.gerar(ctx,params,controller.signal,p=>ctx.jobs.progress(p),prepared);
        await plugin.validarResultado(ctx,result);
        const staged=await plugin.stage(ctx,result);
        tx.onRollback(()=>plugin.rollback(ctx,staged,"transaction_failed"));
        tx.onCommit(()=>plugin.commit(ctx,staged));
        tx.onCommit(()=>reservation?.commit?.());
        return {estimate,prepared,result,staged};
      },{signal:controller.signal,telemetry:this.telemetry});
    } catch(error) {
      this.registry.updateState(id,{status:"ERROR",errors:[...(this.registry.state(id)?.errors||[]),String(error?.stack||error)].slice(-20)}); throw error;
    } finally {
      try{await plugin.limpar(ctx);}finally{ctx.resources?.disposeAll?.();this.running.delete(id);if(this.registry.state(id)?.status!=="ERROR")this.registry.updateState(id,{status:"REGISTERED"});}
    }
  }
  cancel(id,reason="cancelled") { const run=this.running.get(id); if(!run)return false; run.controller.abort(reason); return true; }
}
