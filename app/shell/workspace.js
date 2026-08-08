import { ModeRegistry } from "./modos/registry.js";
export class Workspace {
  constructor({ host, navbar = null, context = {} }) { if (!(host instanceof Element)) throw new TypeError("host obrigatório"); this.host=host;this.navbar=navbar;this.context=context;this.registry=new ModeRegistry();this.active=null; }
  register(mode){return this.registry.register(mode);}
  async activate(id){
    const next=this.registry.get(id); if(!next) throw new Error(`Modo não registrado: ${id}`); if(this.active?.id===id)return next;
    if(this.active) await this.active.deactivate(this.context);
    if(!next._mounted){await next.mount(this.host,this.context);next._mounted=true;}
    await next.activate(this.context);this.active=next;this.navbar?.setActive?.(id);return next;
  }
  async dispose(){if(this.active)await this.active.deactivate(this.context);for(const mode of [...this.registry.list()].reverse())await mode.dispose(this.context);this.active=null;}
}
