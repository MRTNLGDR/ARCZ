import { LocalApiClient } from "../core/api-client.js";
import { PanoramaViewer } from "./panorama-viewer.js";
import { StreetSequence } from "./street-sequence.js";

function el(tag, className = "", text = "") { const node=document.createElement(tag);if(className)node.className=className;if(text)node.textContent=text;return node; }
function safeLocalAsset(base, relative) {
  const url = new URL(relative, new URL(base, globalThis.location.origin));
  if (url.origin !== globalThis.location.origin || !url.pathname.startsWith("/data/panoramas/")) throw new Error("Caminho de panorama fora do catálogo local");
  return url.toString();
}

export class WalkWorkspace {
  constructor({ api = new LocalApiClient(), estadoApp = null } = {}) { this.api=api;this.estadoApp=estadoApp;this.active=false; }
  async mount(host) {
    this.root=el("section","arcz-walk-workspace");
    const bar=el("header","arcz-mode-heading");bar.append(el("h2","","Street-level local"),el("p","","Panoramax auto-hospedado, captura própria ou pacote licenciado; nenhum dado Google é copiado."));
    this.select=el("select","arcz-select");this.reload=el("button","arcz-button","Atualizar catálogo");this.reload.type="button";
    const controls=el("div","arcz-walk-controls");this.prev=el("button","arcz-button","← Anterior");this.next=el("button","arcz-button","Próximo →");this.info=el("span","arcz-panel-state","—");controls.append(this.select,this.reload,this.prev,this.next,this.info);
    this.canvas=el("canvas","arcz-panorama-canvas");this.empty=el("div","arcz-mode-state");
    this.root.append(bar,controls,this.canvas,this.empty);host.append(this.root);
    this.viewer=new PanoramaViewer(this.canvas);this.resizeObserver=new ResizeObserver(()=>this.viewer.resize());this.resizeObserver.observe(this.canvas);
    this.reload.addEventListener("click",()=>this.refresh());this.select.addEventListener("change",()=>this.openSequence(this.select.value));
    this.prev.addEventListener("click",()=>this.navigate(-1));this.next.addEventListener("click",()=>this.navigate(1));
    await this.refresh();
  }
  async refresh(){this.empty.hidden=false;this.empty.textContent="Lendo catálogo local de panoramas…";this.select.replaceChildren();try{const rows=await this.api.json("/api/v2/panoramas");this.rows=rows;for(const row of rows){const option=el("option","",`${row.sequence_id} · ${row.frames} imagens`);option.value=row.sequence_id;this.select.append(option);}if(!rows.length){this.empty.textContent="Nenhuma sequência local. Importe uma sequence.json válida com imagens e checksums em data/panoramas/.";this.canvas.hidden=true;return;}this.canvas.hidden=false;await this.openSequence(rows[0].sequence_id);}catch(error){this.empty.textContent=`Catálogo indisponível: ${error.message}`;this.canvas.hidden=true;}}
  async openSequence(id){if(!id)return;this.empty.hidden=false;this.empty.textContent="Verificando imagens e licença…";try{const manifest=await this.api.json(`/api/v2/panoramas/${encodeURIComponent(id)}`);this.sequence=new StreetSequence(manifest);this.baseUrl=manifest.base_url;this.frameIndex=0;await this.showFrame(this.sequence.manifest.frames[0]);this.empty.hidden=true;}catch(error){this.empty.textContent=`Sequência bloqueada: ${error.message}`;}}
  async showFrame(frame){if(!frame)return;this.current=frame;const url=safeLocalAsset(this.baseUrl,frame.image);await this.viewer.load(url);this.viewer.setView({heading:Number(frame.heading||0),pitch:Number(frame.pitch||0),fovDeg:90});this.frameIndex=this.sequence.manifest.frames.findIndex(item=>item.id===frame.id);this.info.textContent=`${this.frameIndex+1}/${this.sequence.manifest.frames.length} · ${Number(frame.lat).toFixed(6)}, ${Number(frame.lon).toFixed(6)} · ${frame.timestamp}`;this.prev.disabled=this.frameIndex<=0;this.next.disabled=this.frameIndex>=this.sequence.manifest.frames.length-1;}
  async navigate(delta){const frames=this.sequence?.manifest?.frames||[];const next=frames[Math.max(0,Math.min(frames.length-1,this.frameIndex+delta))];if(next&&next!==this.current)await this.showFrame(next);}
  async activate(){this.active=true;this.root.hidden=false;this.viewer?.resize();}
  async deactivate(){this.active=false;this.root.hidden=true;}
  async dispose(){this.resizeObserver?.disconnect();this.viewer?.dispose();this.root?.remove();}
}
