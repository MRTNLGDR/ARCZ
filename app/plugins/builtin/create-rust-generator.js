function assertCompleted(job) {
  if(job?.status!=="COMPLETED") { const e=new Error(job?.error?.message||`Job terminou em ${job?.status}`); Object.assign(e,job?.error||{}); throw e; }
  if(!job.result_manifest) throw new Error("Job completo sem result_manifest");
  return job;
}

export function createRustGeneratorPlugin({manifest,parameters=[],jobKind}) {
  return {
    manifest, parameters,
    replayKey(ctx,params){ return JSON.stringify({plugin:manifest.id,version:manifest.versao,region:ctx.region.read()?.request,params}); },
    async validar(ctx){ const region=ctx.region.read(); if(!region?.request||!region?.context) throw new Error("Região Ativa incompleta"); if(ctx.signal?.aborted) throw ctx.signal.reason; return true; },
    async estimar(ctx,params){ return {...manifest.custoBase,profile:params.quality||"EQUILIBRADO",region_id:ctx.region.read().request.region_id}; },
    async preparar(ctx,params,signal){
      const region=ctx.region.read();
      const assembled=await ctx.inputs.resolve(jobKind,{region,params,signal});
      return {region,assembled,prepared_at:new Date().toISOString()};
    },
    async gerar(ctx,params,signal,progress,prepared){
      const region=ctx.region.read();
      if(!prepared?.assembled?.params) throw new Error("Entradas locais não foram montadas");
      const created=await ctx.jobs.create(jobKind,{plugin_id:manifest.id,plugin_version:manifest.versao,
        params:prepared.assembled.params,region,source_versions:prepared.assembled.source_versions,
        source_packages:prepared.assembled.packages},{signal,generationEpoch:Number(region?.request?.generation_epoch||0)});
      const close=ctx.jobs.subscribe(created.id,event=>progress?.(event),{signal});
      try{return assertCompleted(await ctx.jobs.wait(created.id,{signal}));}finally{close?.();}
    },
    async validarResultado(_ctx,result){ assertCompleted(result); return true; },
    async stage(ctx,result){
      const manifestData=await ctx.jobs.readManifest(result.result_manifest);
      const handles=[];
      const region=ctx.region.read();
      for(const output of manifestData.outputs||[]) if(["glb","gltf","3dtiles"].includes(output.kind)) handles.push(await ctx.scene.stagePrimitive({source:output.path,sha256:output.sha256,owner:`generator:${manifest.id}`,region,output}));
      return {job_id:result.id,manifest:manifestData,handles};
    },
    async commit(ctx,staged){ return ctx.scene.commitStaged(staged.handles,{job_id:staged.job_id,manifest:staged.manifest}); },
    async rollback(_ctx,staged,reason){ for(const handle of [...(staged?.handles||[])].reverse()) await handle.rollback?.(reason); },
    async limpar(){ return true; },
    serializar(){ return {enabled:true,version:manifest.versao}; },
    migrar(savedState,fromVersion){ return {...savedState,version:manifest.versao,migrated_from:fromVersion}; }
  };
}
