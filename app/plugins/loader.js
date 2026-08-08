import { validatePlugin } from "./validator.js";

export class PluginLoader {
  constructor({registry,allowedRoot="/app/plugins/"}) { this.registry=registry; this.allowedRoot=allowedRoot; }
  async load(entrypoint) {
    const url=new URL(entrypoint,globalThis.location?.origin||"http://127.0.0.1");
    if(!url.pathname.startsWith(this.allowedRoot)) throw new Error(`Entrypoint fora da raiz permitida: ${url.pathname}`);
    if(!url.pathname.endsWith(".js")) throw new Error("Entrypoint precisa ser .js");
    const module=await import(url.href); const plugin=module.default;
    validatePlugin(plugin); this.registry.register(plugin); return plugin;
  }
  async loadMany(entrypoints) {
    const results=[]; for(const ep of entrypoints){try{results.push({entrypoint:ep,ok:true,plugin:await this.load(ep)});}catch(error){results.push({entrypoint:ep,ok:false,error:String(error?.stack||error)});}} return results;
  }
}
