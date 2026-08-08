import { structuredApiError } from "../core/budget-client.js";
export class ProfileUI {
  constructor({container,fetchImpl=globalThis.fetch?.bind(globalThis),baseUrl="/api/v2"}){this.container=container;this.fetch=fetchImpl;this.baseUrl=baseUrl;this.onSelect=null;}
  async load({signal}={}){const r=await this.fetch(`${this.baseUrl}/profiles`,{signal});const d=await r.json();if(!r.ok)throw structuredApiError(d,r.status);this.render(d.profiles||[]);return d.profiles||[];}
  render(profiles){this.container.replaceChildren();for(const p of profiles){const b=document.createElement("button");b.type="button";b.className="profile-card";b.dataset.profileId=p.id;const title=document.createElement("strong");title.textContent=p.id;const meta=document.createElement("span");meta.textContent=`v${p.version} · confiança ${(Number(p.metadata?.confidence||0)*100).toFixed(0)}%`;b.append(title,meta);b.addEventListener("click",()=>this.onSelect?.(p));this.container.appendChild(b);}}
}
