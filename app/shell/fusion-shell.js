import { Workspace } from "./workspace.js";
import { ShellNavbar } from "./navbar/navbar.js";
import { CollapsiblePanelDock } from "./paineis/collapsible-panel-dock.js";
import { FusionSharedState } from "./fusion-shared-state.js";
import { FloorplannerHost } from "../floorplanner/floorplanner-host.js";
import { PhotorealWorkspace } from "../render/photoreal-workspace.js";
import { WalkWorkspace } from "../walk/walk-workspace.js";
import { ReferenceMediaPanel } from "../media/reference-media-panel.js";
import { PromptLibraryPanel } from "../prompts/prompt-library-panel.js";
import { GlobalChatPanel } from "../chat/global-chat-panel.js";
import { GovernancePanel } from "../governance/governance-panel.js";
import { CinematicGlobeIntro } from "../earth/cinematic-globe.js";
import { cenaApp } from "../cena.js";

const MODES = Object.freeze([
  { id: "globo", label: "Globo" },
  { id: "floorplanner", label: "Floorplanner" },
  { id: "render", label: "Render" },
  { id: "walk", label: "Street" },
]);

function element(tag, className = "") { const node=document.createElement(tag);if(className)node.className=className;return node; }

class LegacyGlobeMode {
  constructor(shell){this.id="globo";this.shell=shell;}
  async mount(){/* Cesium already owns the legacy viewport; never remount it. */}
  async activate(){this.shell.setVisualMode("globo");}
  async deactivate(){}
  async dispose(){}
}
function wrapMode(id, instance, shell){return{
  id,
  async mount(host){await instance.mount(host);},
  async activate(){shell.setVisualMode(id);await instance.activate();},
  async deactivate(){await instance.deactivate();},
  async dispose(){await instance.dispose();}
};}

export class FusionShell {
  constructor({viewer,estadoApp,runtime=null}={}){
    if(!viewer||!estadoApp)throw new Error("viewer e estadoApp obrigatórios");
    this.viewer=viewer;this.estadoApp=estadoApp;this.runtime=runtime;this.shared=new FusionSharedState();
  }
  async mount(){
    const topbar=document.getElementById("topbar"),body=document.getElementById("corpo"),shell=document.getElementById("app_shell");
    if(!topbar||!body||!shell)throw new Error("Casca ARCZ incompleta");this.shell=shell;
    this.navHost=element("nav","arcz-fusion-nav");this.navHost.id="fusion_mode_nav";
    const right=topbar.querySelector(".direita");topbar.insertBefore(this.navHost,right||null);
    this.overlay=element("div","arcz-fusion-workspace");this.overlay.id="fusion_workspace";body.appendChild(this.overlay);
    this.navbar=new ShellNavbar({element:this.navHost,modes:MODES,onSelect:id=>this.activate(id)});this.navbar.mount();
    this.workspace=new Workspace({host:this.overlay,navbar:this.navbar,context:{viewer:this.viewer,estadoApp:this.estadoApp,runtime:this.runtime}});
    this.floorplanner=new FloorplannerHost({estadoApp:this.estadoApp,sceneManager:cenaApp,viewer:this.viewer});
    this.renderWorkspace=new PhotorealWorkspace({estadoApp:this.estadoApp,referencesProvider:()=>this.shared.references});
    this.onSharedPrompt=event=>this.renderWorkspace.setPrompt(event.detail||{});
    this.onSharedReferences=()=>this.renderWorkspace.referencesChanged();
    this.shared.addEventListener("prompt",this.onSharedPrompt);
    this.shared.addEventListener("references",this.onSharedReferences);
    this.walkWorkspace=new WalkWorkspace({estadoApp:this.estadoApp});
    this.workspace.register(new LegacyGlobeMode(this));
    this.workspace.register(wrapMode("floorplanner",this.floorplanner,this));
    this.workspace.register(wrapMode("render",this.renderWorkspace,this));
    this.workspace.register(wrapMode("walk",this.walkWorkspace,this));
    this.mountGlobalPanels();
    await this.activate("globo",{persist:false});
    this.intro=new CinematicGlobeIntro({viewer:this.viewer,estadoApp:this.estadoApp});
    try{await this.intro.play(this.estadoApp.obter().camera);}catch(error){console.error("Abertura cinematográfica ignorada:",error);}
    return this;
  }
  mountGlobalPanels(){
    this.mediaPanel=new ReferenceMediaPanel({onSelectionChange:values=>{this.shared.setReferences(values);this.estadoApp.atualizar({reference_media:values},"reference_media");}});
    for(const value of this.estadoApp.obter().reference_media||[])this.mediaPanel.selected.add(value);
    this.promptPanel=new PromptLibraryPanel({onPromptChange:value=>this.shared.setPrompt(value)});
    this.chatPanel=new GlobalChatPanel({attachmentsProvider:()=>this.shared.references,contextProvider:()=>({scope:this.workspace?.active?.id||"global",language:"pt-BR",region_id:this.estadoApp.obter().active_region?.request?.region_id||null,floorplanner_project_id:this.estadoApp.obter().active_floorplanner_project_id||null})});
    this.governancePanel=new GovernancePanel();
    this.dock=new CollapsiblePanelDock({estadoApp:this.estadoApp,id:"global-tools",side:"right"});
    this.dock.register({id:"media",label:"Mídias de referência",shortLabel:"Mídia",description:"Imagens, vídeos, áudio, PDFs e modelos locais",mount:host=>this.mediaPanel.mount(host),dispose:()=>this.mediaPanel.dispose?.()});
    this.dock.register({id:"prompts",label:"Biblioteca de prompts",shortLabel:"Prompt",description:"Prompts, aprimoramento e tradução local",mount:host=>this.promptPanel.mount(host),dispose:()=>this.promptPanel.dispose?.()});
    this.dock.register({id:"chat",label:"Chat global ARCZ",shortLabel:"Chat",description:"Globo, Floorplanner, render e cinema",mount:host=>this.chatPanel.mount(host),dispose:()=>this.chatPanel.dispose?.()});
    this.dock.register({id:"governance",label:"Governança",shortLabel:"Gov",description:"Tasks, bugs, alertas, testes e evidências",mount:host=>this.governancePanel.mount(host),dispose:()=>this.governancePanel.dispose?.()});
    this.dock.mount(document.body);
  }
  async activate(id,{persist=true}={}){try{await this.workspace.activate(id);if(persist)this.estadoApp.atualizar({workspace_mode:id},"workspace");}catch(error){console.error(`Falha no modo ${id}:`,error);if(id!=="globo")await this.workspace.activate("globo");throw error;}}
  setVisualMode(id){
    for(const mode of MODES)this.shell.classList.toggle(`fusion-mode-${mode.id}`,mode.id===id);
    this.overlay.hidden=id==="globo";
    const legacyInteractive=id==="globo"||id==="floorplanner";document.getElementById("viewport")?.setAttribute("aria-hidden",legacyInteractive?"false":"true");
    if(legacyInteractive)this.viewer.scene.requestRender();
  }
  async dispose(){this.intro?.cancel();this.shared.removeEventListener("prompt",this.onSharedPrompt);this.shared.removeEventListener("references",this.onSharedReferences);await this.dock?.dispose();await this.workspace?.dispose();this.navbar?.dispose();this.overlay?.remove();this.navHost?.remove();}
}

export async function initializeFusionShell(options){const shell=new FusionShell(options);await shell.mount();globalThis.ARCZ_FUSION=shell;return shell;}
