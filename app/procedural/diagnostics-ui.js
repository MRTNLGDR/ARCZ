import { structuredApiError } from "../core/budget-client.js";
export class DiagnosticsUI{constructor({container,fetchImpl=globalThis.fetch?.bind(globalThis),baseUrl="/api/v2",intervalMs=2000}){Object.assign(this,{container,fetch:fetchImpl,baseUrl,intervalMs});this.timer=null;}
 async refresh(){const r=await this.fetch(`${this.baseUrl}/diagnostics`);const d=await r.json();if(!r.ok)throw structuredApiError(d,r.status);this.container.textContent=JSON.stringify(d,null,2);return d;}
 start(){if(this.timer)return;const tick=()=>this.refresh().catch(e=>{this.container.textContent=String(e?.message||e);});tick();this.timer=setInterval(tick,this.intervalMs);}
 stop(){clearInterval(this.timer);this.timer=null;}}
