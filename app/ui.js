// ARCZ Earth · Interface conforme UI SYSTEM/handoff (01-arcz-ui.md + 02-tokens.md):
// cabeçalho persistente, menu lateral por grupos, trilha de painéis à direita,
// rodapé com modos Explorar/Editar/Analisar/Apresentar e métricas ao vivo.
import { estadoApp } from "./estado.js";
import { historicoApp } from "./historico.js";
import { icone, inicializarIcones } from "./icones.js";
import { cenaApp, normalizarPosicao, renderDaPeca, CAMPOS_RENDER } from "./cena.js";
import { cameraApp, mmParaFov } from "./camera.js";
import { ambienteApp } from "./ambiente.js";
import { entornoApp } from "./entorno.js";
import { gizmoApp } from "./gizmo.js";
import { bibliotecaApp, TIPO_ARRASTE } from "./lib.js";
import { posicionadorApp, PASSOS_GRADE } from "./posicionar.js";
import { feedbackApp } from "./feedback.js";
import {
  qualidadeApp, PERFIS, gpuCurta, SUPERAMOSTRAGEM, DETALHE_MAPA
} from "./qualidade.js";
import { textoHora, rumoCardinal } from "./sol.js";
import { corteApp, corteAtual, EIXOS } from "./corte.js";
import { recorteApp, medidasDoPerimetro } from "./recorte.js";
import { NETWORK_MODES } from "./core/network-mode.js";

const $ = id => document.getElementById(id);
const VERSAO = "v2.6.0";
const FONTES_REMOTAS_UI = new Set([
  "satelite", "nuvens_reais", "black_marble", "dia_noite", "mapbox",
  "google3d", "wms_sc", "mapa"
]);
const FONTE_LOCAL_PADRAO = "naturalearth_local";


// Telas do menu lateral. Cada uma acende um conjunto de cartões na trilha.
const TELAS = [
  { grupo: "Navegar", id: "globo", ic: "globo", nome: "Globo 3D", cartoes: ["camadas", "basemap", "camera"] },
  { grupo: "Navegar", id: "offline", ic: "camada", nome: "Dados locais", cartoes: ["armazenamento", "relevo"] },
  { grupo: "Projetos", id: "projeto", ic: "grade", nome: "Projeto ativo", cartoes: ["modelo", "transform", "lugares"] },
  { grupo: "Projetos", id: "cena", ic: "cubo", nome: "Árvore de cena", cartoes: ["pecas", "inspetor"] },
  { grupo: "Projetos", id: "biblioteca", ic: "pasta", nome: "Biblioteca 3D", cartoes: ["biblioteca"] },
  { grupo: "Fluxo 3D", id: "gizmo", ic: "mover", nome: "Posicionar modelo", cartoes: ["gizmo", "inspetor", "material"] },
  { grupo: "Fluxo 3D", id: "medir", ic: "medir", nome: "Medir e cortar", cartoes: ["ferramentas", "etapas_corte", "recorte"] },
  { grupo: "Saída", id: "takes", ic: "play", nome: "Take & Render", cartoes: ["takes", "camera"] },
  { grupo: "Sistema", id: "ambiente", ic: "sol", nome: "Sol e clima", cartoes: ["sol", "clima", "sombras"] },
  { grupo: "Sistema", id: "config", ic: "config", nome: "Configurações", cartoes: ["desempenho", "sobre"] }
];

// Camadas ligáveis do globo (item → ação real na cena).
const CAMADAS = [
  { id: "terreno", nome: "Relevo 3D" },
  { id: "imagery", nome: "Imagem de satélite" },
  { id: "entorno_osm", nome: "Entorno OSM (edifícios 3D)" },
  { id: "predio", nome: "Modelo principal" },
  { id: "pecas", nome: "Peças da cena" },
  { id: "eixos", nome: "Eixos do gizmo" },
  { id: "atmosfera", nome: "Atmosfera e céu" }
];

const MODOS = [
  { id: "explorar", nome: "Explorar" },
  { id: "editar", nome: "Editar" },
  { id: "analisar", nome: "Analisar" },
  { id: "apresentar", nome: "Apresentar" }
];

export class UIManager {
  constructor() {
    this.viewer = null;
    this.tela = "projeto";
    this.modo = "editar";
    this.editandoCampo = false;
    this.recolhidos = new Set();
    this.handlerMedir = null;
    this.entidadesMedida = [];
    this.quadros = 0;
    this.ultimoFps = performance.now();
  }

  inicializar(viewer) {
    this.viewer = viewer;
    inicializarIcones();
    this.tela = localStorage.getItem("arcz.tela") || "projeto";
    this.modo = localStorage.getItem("arcz.modo") || "editar";

    this.montarCabecalho();
    this.montarMenu();
    this.montarRodape();
    this.montarHud();
    this.abrirTela(this.tela, true);
    this.aplicarModo(this.modo, true);
    this.configurarAtalhos();
    this.configurarMetricas();
    corteApp.aoAvisar = (texto, tipo) => this.avisar(texto, tipo);
    recorteApp.aoAvisar = (texto, tipo) => this.avisar(texto, tipo);
    this.carregarLugares();
    this.configurarImportacaoGLB();

    estadoApp.inscrever((st, origem) => this.aoMudarEstado(st, origem));
    this.aoMudarEstado(estadoApp.obter(), "inicial");
  }

  // ------------------------------------------------------------ cabeçalho
  montarCabecalho() {
    $("topbar").innerHTML = `
      <div class="brand">
        <span class="marca-orbe">${icone("globo", 15)}</span>
        <span class="marca">ARCZ</span><span class="produto">Earth</span>
        <span class="versao">${VERSAO}</span>
      </div>
      <span class="selo" id="selo_local">LOCAL OFFLINE</span>
      <div class="busca-global">
        ${icone("busca", 17)}
        <input id="ipt_busca_end" type="text" placeholder="Buscar endereço, lugar ou coordenada…" autocomplete="off">
        <span class="mono" style="font-size:9.5px;color:#4d5b70">Enter</span>
      </div>
      <div class="direita">
        <div class="chip" id="chip_conexao" title="Estado dos dados locais">
          <span class="ponto"></span><span class="rotulo">Local</span>
        </div>
        <button class="btn-icone" id="btn_undo" title="Desfazer (Ctrl+Z)">${icone("desfazer", 17)}</button>
        <button class="btn-icone" id="btn_redo" title="Refazer (Ctrl+Shift+Z)">${icone("refazer", 17)}</button>
        <button class="btn-icone" id="btn_menu_toggle" title="Menu lateral">${icone("menu", 18)}</button>
        <button class="btn-icone" id="btn_trilha_toggle" title="Painéis">${icone("painel", 18)}</button>
        <button class="primario" id="btn_salvar_global">${icone("salvar", 14)} Salvar</button>
        <div class="usuario" title="Projeto local">
          <div class="avatar">AR</div>
          <div class="linhas"><div class="nome">Estúdio ARCZ</div><div class="papel">Projeto local</div></div>
        </div>
      </div>`;

    $("btn_undo").onclick = () => historicoApp.desfazer();
    $("btn_redo").onclick = () => historicoApp.refazer();
    $("btn_menu_toggle").onclick = () => {
      $("app_shell").classList.toggle("menu-oculto");
      $("app_shell").classList.toggle("menu-aberto");
    };
    $("btn_trilha_toggle").onclick = () => {
      $("app_shell").classList.toggle("trilha-oculta");
      $("app_shell").classList.toggle("trilha-aberta");
    };
    $("btn_salvar_global").onclick = async () => {
      const status = await estadoApp.salvarNoServidor();
      this.mostrarStatusSalvar(status);
    };

    const busca = $("ipt_busca_end");
    busca.addEventListener("focus", () => { this.editandoCampo = true; });
    busca.addEventListener("blur", () => { this.editandoCampo = false; });
    busca.addEventListener("keydown", e => { if (e.key === "Enter") this.buscarEndereco(busca.value); });
  }

  // ---------------------------------------------------------- menu lateral
  montarMenu() {
    let html = "";
    let grupoAtual = "";
    for (const tela of TELAS) {
      if (tela.grupo !== grupoAtual) {
        grupoAtual = tela.grupo;
        html += `<div class="grupo-menu">${grupoAtual}</div>`;
      }
      html += `<button class="item-menu" data-tela="${tela.id}">
                 ${icone(tela.ic, 20)}<span class="rotulo">${tela.nome}</span>
                 <span class="contador" id="cont_${tela.id}"></span>
               </button>`;
    }
    html += `<div class="rodape-menu">
               ARCZ Earth ${VERSAO}<br>CesiumJS 1.143 · local
               <div class="estado"><i></i><span id="txt_offline">100% offline</span></div>
             </div>`;
    $("menu").innerHTML = html;

    $("menu").querySelectorAll(".item-menu").forEach(btn => {
      btn.onclick = () => this.abrirTela(btn.dataset.tela);
    });
  }

  abrirTela(id, silencioso = false) {
    this.tela = id;
    localStorage.setItem("arcz.tela", id);
    $("menu").querySelectorAll(".item-menu").forEach(b =>
      b.classList.toggle("ativo", b.dataset.tela === id));
    this.montarTrilha();
    if (!silencioso) this.atualizarRodape();
  }

  // -------------------------------------------------------------- rodapé
  montarRodape() {
    $("statusbar").innerHTML = `
      <div class="segmentado" id="seg_modo">
        ${MODOS.map(m => `<button data-modo-app="${m.id}">${m.nome}</button>`).join("")}
      </div>
      <span class="metrica" id="sb_posicao">—</span>
      <div class="direita">
        <span class="metrica" id="sb_pecas">0 peças</span>
        <span class="metrica" id="sb_tiles">0 tiles</span>
        <span class="metrica" id="sb_mem">— MB</span>
        <span class="metrica" id="sb_fps">— FPS</span>
        <span class="metrica" id="sb_status_salvar">sincronizado</span>
        <button class="btn-icone" id="btn_tela_ant" title="Tela anterior">${icone("seta-esq", 16)}</button>
        <span class="metrica" id="sb_tela">—</span>
        <button class="btn-icone" id="btn_tela_prox" title="Próxima tela">${icone("seta-dir", 16)}</button>
      </div>`;

    $("seg_modo").querySelectorAll("[data-modo-app]").forEach(btn => {
      btn.onclick = () => this.aplicarModo(btn.dataset.modoApp);
    });
    $("btn_tela_ant").onclick = () => this.navegarTela(-1);
    $("btn_tela_prox").onclick = () => this.navegarTela(1);
    this.atualizarRodape();
  }

  navegarTela(passo) {
    const i = TELAS.findIndex(t => t.id === this.tela);
    const proximo = TELAS[(i + passo + TELAS.length) % TELAS.length];
    this.abrirTela(proximo.id);
  }

  atualizarRodape() {
    const i = TELAS.findIndex(t => t.id === this.tela);
    const tela = TELAS[i] || TELAS[0];
    const el = $("sb_tela");
    if (el) el.textContent = `${String(i + 1).padStart(2, "0")}/${TELAS.length} · ${tela.nome}`;
  }

  aplicarModo(modo, silencioso = false) {
    this.modo = modo;
    localStorage.setItem("arcz.modo", modo);
    $("seg_modo")?.querySelectorAll("[data-modo-app]").forEach(b =>
      b.classList.toggle("ativo", b.dataset.modoApp === modo));
    $("app_shell").classList.toggle("apresentando", modo === "apresentar");

    if (modo === "explorar" || modo === "apresentar") {
      gizmoApp.definirModo("nenhum");
      this.desligarMedicao();
    } else if (modo === "editar") {
      gizmoApp.definirModo(estadoApp.obter().modoGizmo === "nenhum" ? "mover" : estadoApp.obter().modoGizmo);
      this.desligarMedicao();
    } else if (modo === "analisar") {
      gizmoApp.definirModo("nenhum");
      if (!silencioso) this.ligarMedicao();
    }
    this.montarTrilha();
  }

  // ---------------------------------------------------------------- HUD
  montarHud() {
    $("hud_viewport").innerHTML = `
      <div class="caixa">
        <button class="btn-icone" id="hud_zoom_mais" title="Aproximar">${icone("mais", 16)}</button>
        <button class="btn-icone" id="hud_zoom_menos" title="Afastar">${icone("menos", 16)}</button>
        <button class="btn-icone" id="hud_topo" title="Vista de topo (2D)">${icone("mapa", 16)}</button>
        <button class="btn-icone" id="hud_norte" title="Apontar ao norte">${icone("bussola", 16)}</button>
        <button class="btn-icone" id="hud_predio" title="Enquadrar o prédio (F)">${icone("alvo", 16)}</button>
        <button class="btn-icone" id="hud_foto" title="Salvar quadro PNG">${icone("foto", 16)}</button>
      </div>`;

    const cam = () => this.viewer.camera;
    $("hud_zoom_mais").onclick = () => cam().zoomIn(cam().positionCartographic.height * 0.3);
    $("hud_zoom_menos").onclick = () => cam().zoomOut(cam().positionCartographic.height * 0.3);
    $("hud_topo").onclick = () => {
      const c = estadoApp.obter().camera;
      cameraApp.definirCamera({ ...c, pitch: -89.9, heading: 0 });
    };
    $("hud_norte").onclick = () => {
      const c = estadoApp.obter().camera;
      cameraApp.definirCamera({ ...c, heading: 0 });
    };
    $("hud_predio").onclick = () => {
      const p = estadoApp.obter().posicao;
      cameraApp.olharPara(p.lon, p.lat, p.alt, 250);
    };
    $("hud_foto").onclick = async () => {
      const dados = await cameraApp.fotografar(`quadro-${Date.now()}`);
      this.avisar(dados ? `Quadro salvo: ${dados.arquivo}` : "Falha ao salvar o quadro");
    };
  }

  /** Rodapé + pílula flutuante sobre o 3D (tipo: info | ok | alerta | erro). */
  avisar(texto, tipo = "info") {
    feedbackApp.aviso(texto, tipo);
    const el = $("sb_status_salvar");
    if (!el) return;
    el.textContent = texto;
    setTimeout(() => this.mostrarStatusSalvar(estadoApp.statusSave), 2500);
  }

  // ----------------------------------------------------- trilha (painéis)
  montarTrilha() {
    const tela = TELAS.find(t => t.id === this.tela) || TELAS[0];
    const trilha = $("trilha");
    trilha.innerHTML = tela.cartoes.map(nome => this.htmlCartao(nome)).join("");

    trilha.querySelectorAll(".cartao > header").forEach(cab => {
      cab.onclick = () => {
        const cartao = cab.parentElement;
        cartao.classList.toggle("recolhido");
        const id = cartao.dataset.cartao;
        if (cartao.classList.contains("recolhido")) this.recolhidos.add(id);
        else this.recolhidos.delete(id);
      };
    });

    for (const nome of tela.cartoes) this.ligarCartao(nome);
    this.aoMudarEstado(estadoApp.obter(), "trilha");
  }

  cartao(id, titulo, corpo, extra = "") {
    const recolhido = this.recolhidos.has(id) ? " recolhido" : "";
    return `<section class="cartao${recolhido}" data-cartao="${id}">
              <header><span class="titulo">${titulo}</span>${extra}</header>
              <div class="corpo">${corpo}</div>
            </section>`;
  }

  htmlCartao(nome) {
    switch (nome) {
      case "globo_universo":
        return this.cartao("globo_universo", "Universo & Terra 3D", `
          <div class="segmentado" id="seg_modo_globo">
            <button data-modo-globo="naturalearth_local">Local</button>
            <button data-modo-globo="satelite" data-rede-remota>Dia</button>
            <button data-modo-globo="nuvens_reais" data-rede-remota>Nuvens</button>
            <button data-modo-globo="black_marble" data-rede-remota>Noite</button>
          </div>
          <label class="linha"><span>Luz do Sol</span><input type="checkbox" id="chk_luz_sol" checked></label>
          <label class="linha"><span>Hora do dia</span>
            <span style="display:flex;align-items:center;gap:6px">
              <input type="range" id="globo_hora_slider" min="0" max="24" step="0.25" value="15">
              <b class="valor" id="globo_hora_txt">15:00</b>
            </span></label>
          <div class="botoes">
            <button id="btn_hora_real">${icone("sol", 14)} Hora Real</button>
            <button id="btn_vis_espaco">${icone("globo", 14)} Espaço</button>
            <button id="btn_vis_projeto">${icone("alvo", 14)} Projeto</button>
          </div>
          <div class="dica">A base local funciona sem rede. Fontes NASA/Esri são conectores opcionais e só habilitam em import_assisted.</div>`);

      case "camera":
        return this.cartao("camera", "Câmera", `
          <label class="linha"><span>Lat / Lon</span><span class="valor" id="cam_coords">—</span></label>
          <label class="linha"><span>Altitude</span><input type="number" id="cam_alt" step="1"></label>
          <label class="linha"><span>Rumo</span><input type="number" id="cam_heading" step="1"></label>
          <label class="linha"><span>Inclinação</span><input type="number" id="cam_pitch" step="1"></label>
          <label class="linha"><span>Lente</span>
            <select id="cam_lente">
              <option value="24">24 mm</option><option value="35">35 mm</option>
              <option value="50" selected>50 mm</option><option value="85">85 mm</option>
              <option value="200">200 mm</option>
            </select></label>
          <label class="linha"><span>Distância ao alvo</span><span class="valor" id="cam_dist">—</span></label>
          <div class="botoes">
            <button id="btn_aplicar_camera">${icone("camera", 14)} Aplicar</button>
            <button id="btn_gravar_take_rapido">${icone("play", 14)} Gravar take</button>
          </div>`);

      case "lugares":
        return this.cartao("lugares", "Lugares salvos", `
          <div class="botoes"><button id="btn_salvar_lugar">${icone("salvar", 14)} Salvar posição atual</button></div>
          <select id="sel_lugares_salvos" class="largo"><option value="">Locais salvos…</option></select>
          <div class="lista" id="res_busca_end"></div>
          <div class="dica">A busca usa exclusivamente o índice geográfico SQLite local. Bases externas entram apenas por importadores autorizados.</div>`);

      case "camadas":
        return this.cartao("camadas", "Camadas",
          CAMADAS.map(c =>
            `<label class="linha"><span>${c.nome}</span><input type="checkbox" data-camada="${c.id}" checked></label>`
          ).join(""));

      case "basemap":
        return this.cartao("basemap", "Mapa base", `
          <label class="linha"><span>Modo de rede</span>
            <select id="amb_network_mode">
              <option value="offline_strict">Offline estrito</option>
              <option value="local_lan">Somente LAN local</option>
              <option value="import_assisted">Importação assistida</option>
            </select></label>
          <div class="segmentado" id="seg_basemap">
            <button data-base="naturalearth_local">Natural Earth local</button>
            <button data-base="satelite" data-rede-remota>Satélite*</button>
            <button data-base="mapa" data-rede-remota>Mapa*</button>
          </div>
          <select id="amb_imagery" class="largo">
            <option value="naturalearth_local">Natural Earth II · pacote local</option>
            <option value="nenhuma">Somente globo/cor base</option>
            <option value="satelite" data-rede-remota>Esri World Imagery · conector opcional</option>
            <option value="nuvens_reais" data-rede-remota>Esri + NASA GIBS · conector opcional</option>
            <option value="black_marble" data-rede-remota>NASA Black Marble · conector opcional</option>
            <option value="dia_noite" data-rede-remota>Dia/Noite NASA · conector opcional</option>
            <option value="mapbox" data-rede-remota>Mapbox Satellite · conector opcional</option>
            <option value="google3d" data-rede-remota>Google Photorealistic 3D · conector opcional</option>
            <option value="wms_sc" data-rede-remota>Ortofoto SIGSC · conector opcional</option>
            <option value="mapa" data-rede-remota>OpenStreetMap · conector opcional</option>
          </select>
          <input type="password" id="amb_token_mapbox" class="largo" autocomplete="off"
                 placeholder="Token Mapbox somente nesta sessão">
          <div class="dica">* Conectores remotos não são core, não são ligados por padrão e não tornam o projeto dependente deles. Dados permanentes devem ser materializados por hash/licença.</div>`);

      case "armazenamento":
        return this.cartao("armazenamento", "Dados no disco",
          `<div class="lista" id="lista_armazenamento"><div class="carregando">medindo…</div></div>`,
          `<span class="mono" id="armazenamento_total">—</span>`);

      case "relevo":
        return this.cartao("relevo", "Relevo", `
          <label class="linha"><span>Terreno 3D (DEM local)</span><input type="checkbox" id="amb_relevo"></label>
          <label class="linha"><span>Cota sob o prédio</span><span class="valor" id="txt_cota">—</span></label>
          <div class="botoes"><button id="btn_assentar_predio">${icone("pivo", 14)} Assentar no terreno</button></div>
          <div class="dica">Somente tiles DEM já materializados no disco. Tile ausente gera erro visível; o ARCZ não inventa cota zero e não baixa da internet em offline_strict.</div>`);

      case "modelo":
        return this.cartao("modelo", "Modelo principal", `
          <label class="linha"><span>Resolução</span>
            <select id="sel_lod_predio">
              <option value="original">Original</option>
              <option value="equilibrado">Equilibrado 1024</option>
              <option value="medio">Médio 512</option>
              <option value="distante">Distante 256</option>
            </select></label>
          <div class="botoes">
            <button id="btn_ir_predio">${icone("alvo", 14)} Enquadrar</button>
            <button id="btn_recarregar_predio">${icone("refazer", 14)} Recarregar</button>
          </div>
          <div class="botoes" style="margin-top:6px">
            <button id="btn_importar_glb" class="primario" style="width:100%">${icone("mais", 14)} Importar GLB / glTF local</button>
            <input type="file" id="input_importar_glb" accept=".glb,.gltf" style="display:none">
          </div>
          <div class="dica">Você também pode arrastar e soltar qualquer arquivo .glb / .gltf direto na tela 3D.</div>`);

      case "transform":
        return this.cartao("transform", "Georreferência", `
          <label class="linha"><span>Latitude</span><input type="number" id="pos_lat" step="0.000001"></label>
          <label class="linha"><span>Longitude</span><input type="number" id="pos_lon" step="0.000001"></label>
          <label class="linha"><span>Altitude (m)</span><input type="number" id="pos_alt" step="0.1"></label>
          <label class="linha"><span>Rumo (°)</span><input type="number" id="pos_rumo" step="1"></label>
          <label class="linha"><span>Escala</span><input type="number" id="pos_escala" step="0.01" min="0.01"></label>`);

      case "pecas":
        return this.cartao("pecas", "Peças na cena",
          `<div class="lista" id="lista_pecas"></div>
           <div class="botoes">
             <button id="btn_duplicar_peca">${icone("duplicar", 14)} Duplicar</button>
             <button id="btn_remover_peca">${icone("lixeira", 14)} Remover</button>
           </div>`,
          `<span class="mono" id="cont_pecas_cartao">0</span>`);

      case "inspetor":
        return this.cartao("inspetor", "Inspetor", `
          <label class="linha"><span>Selecionado</span><span class="valor" id="obj_nome">prédio</span></label>
          <label class="linha"><span>Rumo (°)</span><input type="number" id="obj_rumo" step="1"></label>
          <label class="linha"><span>Escala</span><input type="number" id="obj_escala" step="0.01" min="0.01"></label>
          <label class="linha"><span>Altitude (m)</span><input type="number" id="obj_alt" step="0.1"></label>
          <label class="linha"><span>LOD da peça</span>
            <select id="obj_lod">
              <option value="original">Original</option>
              <option value="equilibrado">1024</option>
              <option value="medio">512</option>
              <option value="distante">256</option>
            </select></label>

          <div class="sub-titulo">Aparência no ambiente</div>
          <div class="empilhada"><span>Sombra</span>
            <div class="segmentado" id="seg_obj_sombra">
              <button data-sombra="projeta" title="Lança e recebe sombra">Projeta</button>
              <button data-sombra="recebe" title="Só recebe sombra">Recebe</button>
              <button data-sombra="nenhuma">Nenhuma</button>
            </div></div>
          <div class="empilhada"><span>Reflexo do céu</span>
            <span class="com-valor">
              <input type="range" id="obj_reflexo" min="0" max="2" step="0.05">
              <b class="valor" id="obj_reflexo_valor">1.00</b>
            </span></div>
          <div class="empilhada"><span>Opacidade (vidro)</span>
            <span class="com-valor">
              <input type="range" id="obj_opacidade" min="0.05" max="1" step="0.05">
              <b class="valor" id="obj_opacidade_valor">1.00</b>
            </span></div>
          <div class="empilhada"><span>Cor base e mistura</span>
            <span class="com-valor">
              <input type="color" id="obj_cor" value="#ffffff">
              <input type="range" id="obj_mistura" min="0" max="1" step="0.05" title="Quanto a cor pesa">
            </span></div>
          <div class="botoes">
            <button id="btn_obj_original">${icone("olho", 14)} Material original</button>
            <button id="btn_obj_todas">${icone("duplicar", 14)} Aplicar a todas</button>
          </div>
          <div class="dica">
            A peça usa o sol, o céu e as sombras da cena. O reflexo do céu no vidro
            acompanha o horário; em perfil sem HDR ele fica desligado.
          </div>`);

      case "gizmo":
        return this.cartao("gizmo", "Gizmo e pivô", `
          <div class="segmentado" id="seg_gizmo">
            <button data-modo="nenhum" title="Q">Nenhum</button>
            <button data-modo="mover" title="W">Mover</button>
            <button data-modo="girar" title="E">Girar</button>
            <button data-modo="escalar" title="R">Escalar</button>
          </div>
          <label class="linha"><span>Eixos locais</span><input type="checkbox" id="chk_eixos_locais"></label>
          <label class="linha"><span>Snap fixo (Shift inverte)</span><input type="checkbox" id="chk_snap"></label>
          <label class="linha"><span>Grade do assistente</span>
            <select id="sel_grade">
              ${PASSOS_GRADE.map(p => `<option value="${p}">${p ? `${p} m` : "livre"}</option>`).join("")}
            </select></label>
          <label class="linha"><span>Pivô</span>
            <span class="valor" title="O gizmo gira e escala em torno da origem do arquivo do modelo">
              origem do arquivo
            </span></label>
          <div class="dica">
            Pegue a <b>seta do eixo</b> (X vermelho · Y verde · Z azul), o <b>anel</b> para girar
            ou a <b>alça amarela</b> para escalar. Q/W/E/R trocam o modo, X liga o snap,
            Del remove, Esc solta a seleção.
          </div>`);

      case "material":
        return this.cartao("material", "Material", `
          <label class="linha"><span>Cor base</span><input type="color" id="pbr_cor_picker" value="#ffffff"></label>
          <label class="linha"><span>Mistura</span><input type="range" id="pbr_blend_slider" min="0" max="1" step="0.05" value="0.5"></label>
          <div class="botoes">
            <button class="primario" id="btn_aplicar_pbr">${icone("sol", 14)} Aplicar</button>
            <button id="btn_limpar_pbr">${icone("olho", 14)} Original</button>
          </div>`);

      case "biblioteca":
        return this.cartao("biblioteca", "Biblioteca 3D", `
          <select id="sel_fonte_biblioteca" class="largo">
            <option value="local">Biblioteca local (CC0)</option>
            <option value="banco">Banco de modelos (CC0 / CC-BY)</option>
            <option value="polyhaven_models">Poly Haven · modelos</option>
            <option value="polyhaven_textures">Poly Haven · texturas</option>
          </select>
          <input type="text" id="busca_lib" class="largo" placeholder="Buscar peça…">
          <div class="chips-lib" id="chips_lib"></div>
          <div class="grid-biblioteca" id="grid_lib"></div>
          <div class="botoes" style="margin-top:6px">
            <button id="btn_importar_glb_lib">${icone("mais", 14)} Importar GLB local</button>
            <input type="file" id="input_importar_glb_lib" accept=".glb,.gltf" style="display:none">
          </div>
          <div class="dica">
            <b>Arraste</b> a peça para o 3D ou <b>clique</b> para entrar no modo posicionar.<br>
            Roda gira 15° · Shift+roda escala · PgUp/PgDn altura · G grade · Shift+clique pousa em série.
          </div>`);

      case "ferramentas":
        return this.cartao("ferramentas", "Medir e cortar", `
          <div class="botoes">
            <button id="btn_ferramenta_medir">${icone("medir", 14)} Medir distância</button>
            <button id="btn_ferramenta_corte">${icone("corte", 14)} Plano de corte</button>
          </div>
          <div class="lista" id="saida_medida"></div>
          <div class="segmentado" id="seg_eixo_corte">
            ${Object.entries(EIXOS).map(([id, e]) => `<button data-eixo="${id}">${e.nome}</button>`).join("")}
          </div>
          <label class="linha"><span>Posição do corte (m)</span>
            <span style="display:flex;align-items:center;gap:8px">
              <input type="range" id="corte_slider" min="-20" max="150" step="0.1" value="12">
              <input type="number" id="corte_altura" step="0.5" value="12" style="width:76px">
            </span></label>
          <label class="linha"><span>Inverter o lado</span><input type="checkbox" id="corte_inverter"></label>
          <label class="linha"><span>Tapar as paredes cortadas</span><input type="checkbox" id="corte_tapar" checked></label>
          <label class="linha"><span>Cor da tampa</span><input type="color" id="corte_cor" value="#aeb8c7"></label>
          <label class="linha"><span>Cortar também as peças</span><input type="checkbox" id="corte_pecas" checked></label>
          <div class="lista" id="corte_saida"></div>
          <div class="dica">
            A tampa é a seção real da malha: cada triângulo que cruza o plano vira contorno e o
            miolo da parede é preenchido — sala continua vazada, parede fica sólida.
          </div>`);

      case "etapas_corte":
        return this.cartao("etapas_corte", "Etapas de corte",
          `<div class="botoes">
             <button class="primario" id="btn_salvar_etapa">${icone("mais", 14)} Salvar etapa atual</button>
             <button id="btn_etapa_anterior" title="Etapa anterior">${icone("seta-esq", 14)}</button>
             <button id="btn_etapa_proxima" title="Próxima etapa">${icone("seta-dir", 14)}</button>
           </div>
           <div class="lista" id="lista_etapas"></div>
           <div class="dica">
             Cada etapa guarda eixo, posição, lado e tampa, e vai junto no projeto.json.
             Clique para aplicar · duplo clique renomeia.
           </div>`,
          `<span class="mono" id="cont_etapas_cartao">0</span>`);

      case "recorte":
        return this.cartao("recorte", "Recorte por perímetro", `
          <div class="botoes">
            <button id="btn_recorte_desenhar">${icone("recorte", 14)} Desenhar área</button>
            <button id="btn_recorte_fechar">Fechar</button>
            <button id="btn_recorte_limpar">${icone("lixeira", 14)} Limpar</button>
          </div>
          <div class="lista" id="recorte_resumo"></div>
          <label class="linha"><span>Formato</span>
            <select id="recorte_formato">
              <option value="glb">GLB (arquivo único)</option>
              <option value="gltf">glTF + .bin</option>
              <option value="obj">OBJ + MTL</option>
            </select></label>
          <label class="linha"><span>Incluir o relevo</span><input type="checkbox" id="recorte_relevo"></label>
          <label class="linha"><span>Malha do relevo (divisões)</span>
            <input type="number" id="recorte_resolucao" min="8" max="400" step="8" value="80"></label>
          <div class="botoes" style="margin-top:6px">
            <button class="primario" id="btn_recorte_exportar" style="width:100%">
              ${icone("baixar", 14)} Exportar recorte
            </button>
          </div>
          <div class="lista" id="recorte_saida"></div>
          <div class="dica">
            Vai tudo que está dentro do perímetro: prédio, peças e (opcional) o relevo real do DEM.
            Os arquivos ficam em <b>exportacoes/</b>. Edifícios OSM e imagem de satélite não entram.
          </div>`);

      case "takes":
        return this.cartao("takes", "Takes",
          `<div class="botoes">
             <button class="primario" id="btn_gravar_take">${icone("camera", 14)} Gravar câmera atual</button>
           </div>
           <div class="lista" id="lista_takes"></div>`,
          `<span class="mono" id="cont_takes_cartao">0</span>`);

      case "sol":
        return this.cartao("sol", "Sol real", `
          <label class="linha"><span>Data</span><input type="date" id="amb_data"></label>
          <label class="linha"><span>Hora local</span>
            <span style="display:flex;align-items:center;gap:8px">
              <input type="range" id="amb_hora" min="0" max="24" step="0.0833" value="15">
              <b class="valor" id="amb_hora_valor">15:00</b>
            </span></label>
          <label class="linha"><span>Fuso automático</span><input type="checkbox" id="amb_fuso_auto" checked></label>
          <label class="linha"><span>UTC</span><input type="number" id="amb_fuso" step="1" min="-12" max="14"></label>
          <div class="botoes">
            <button id="btn_hora_real2">${icone("sol", 14)} Agora</button>
            <button id="btn_sol_nascer">Nascer</button>
            <button id="btn_sol_meiodia">Meio-dia</button>
            <button id="btn_sol_ocaso">Ocaso</button>
          </div>
          <label class="linha"><span>Time-lapse</span><input type="checkbox" id="amb_animar"></label>
          <label class="linha"><span>Velocidade</span>
            <span style="display:flex;align-items:center;gap:8px">
              <input type="range" id="amb_velocidade" min="60" max="3600" step="60" value="300">
              <b class="valor" id="amb_velocidade_valor">300×</b>
            </span></label>
          <label class="linha"><span>Intensidade do sol</span>
            <span style="display:flex;align-items:center;gap:8px">
              <input type="range" id="amb_sol_int" min="0" max="5" step="0.1" value="2.6">
              <b class="valor" id="amb_sol_int_valor">2.6</b>
            </span></label>
          <label class="linha"><span>Exposição</span>
            <span style="display:flex;align-items:center;gap:8px">
              <input type="range" id="amb_exposicao" min="0.4" max="2" step="0.05" value="1">
              <b class="valor" id="amb_exposicao_valor">1.00</b>
            </span></label>
          <label class="linha"><span>Luz do sol</span><input type="checkbox" id="amb_luz_sol" checked></label>
          <div class="lista" id="leitura_sol"></div>
          <div class="botoes"><button id="btn_sol_fachada">${icone("sol", 14)} Sol na fachada</button></div>
          <div class="dica">Posição do sol por efemérides (Simon 1994) para a data, o fuso e as coordenadas do projeto — a mesma que ilumina a cena.</div>`);

      case "clima":
        return this.cartao("clima", "Clima e atmosfera", `
          <label class="linha"><span>Condição</span>
            <select id="amb_condicao">
              <option value="limpo">Céu limpo</option>
              <option value="poucas">Poucas nuvens</option>
              <option value="parcial">Parcialmente nublado</option>
              <option value="nublado">Nublado</option>
              <option value="encoberto">Encoberto</option>
              <option value="chuva">Chuva</option>
              <option value="tempestade">Tempestade</option>
              <option value="neblina">Neblina densa</option>
              <option value="neve">Neve</option>
              <option value="poeira">Poeira / fumaça</option>
            </select></label>
          <label class="linha"><span>Cobertura de nuvem</span>
            <span style="display:flex;align-items:center;gap:8px">
              <input type="range" id="amb_cobertura" min="0" max="100" step="1" value="20">
              <b class="valor" id="amb_cobertura_valor">20%</b>
            </span></label>
          <label class="linha"><span>Base das nuvens</span><input type="number" id="amb_nuvens_altura" step="100" value="1200"></label>
          <label class="linha"><span>Umidade / neblina</span>
            <span style="display:flex;align-items:center;gap:8px">
              <input type="range" id="amb_neblina" min="0" max="100" step="1" value="5">
              <b class="valor" id="amb_neblina_valor">5%</b>
            </span></label>
          <label class="linha"><span>Aerossol / poluição</span>
            <span style="display:flex;align-items:center;gap:8px">
              <input type="range" id="amb_aerossol" min="0" max="100" step="1" value="10">
              <b class="valor" id="amb_aerossol_valor">10%</b>
            </span></label>
          <label class="linha"><span>Vento (km/h)</span><input type="number" id="amb_vento" step="1" value="12"></label>
          <label class="linha"><span>Rumo do vento</span><input type="number" id="amb_vento_rumo" step="5" value="90"></label>
          <label class="linha"><span>Precipitação</span>
            <select id="amb_precipitacao">
              <option value="auto">Da condição</option>
              <option value="nenhuma">Nenhuma</option>
              <option value="chuva">Chuva</option>
              <option value="neve">Neve</option>
            </select></label>
          <label class="linha"><span>Nuvens 3D</span><input type="checkbox" id="amb_nuvens" checked></label>
          <label class="linha"><span>Névoa</span><input type="checkbox" id="amb_fog" checked></label>
          <label class="linha"><span>Bloom</span><input type="checkbox" id="amb_bloom"></label>
          <label class="linha"><span>Estrelas</span><input type="checkbox" id="amb_estrelas" checked></label>
          <label class="linha"><span>Lua</span><input type="checkbox" id="amb_lua" checked></label>
          <div class="dica">A condição repõe cobertura, neblina e aerossol; depois disso cada controle é livre.</div>`);

      case "sombras":
        return this.cartao("sombras", "Sombras", `
          <label class="linha"><span>Sombras</span><input type="checkbox" id="amb_sombra" checked></label>
          <label class="linha"><span>Relevo projeta sombra</span><input type="checkbox" id="amb_sombra_relevo" checked></label>
          <label class="linha"><span>Sombra suave</span><input type="checkbox" id="amb_sombra_suave" checked></label>
          <label class="linha"><span>Alcance (m)</span><input type="number" id="amb_sombra_alcance" step="500" value="4000"></label>
          <div class="dica">Com o relevo projetando sombra, o morro escurece o terreno e o prédio no fim da tarde — não só o modelo faz sombra.</div>`);

      case "desempenho":
        return this.cartao("desempenho", "Desempenho", `
          <label class="linha"><span>Perfil de qualidade</span>
            <select id="sel_perfil_qualidade">
              <option value="alto">Alto (4× MSAA + HDR + Sombras)</option>
              <option value="equilibrado">Equilibrado (2× MSAA + FXAA)</option>
              <option value="leve">Leve (1× MSAA)</option>
              <option value="minimo">Mínimo (Sem GPU)</option>
            </select></label>
          <label class="linha"><span>Adaptar automático</span><input type="checkbox" id="chk_qualidade_auto" checked></label>
          <label class="linha"><span>Resolução do prédio</span>
            <select id="sel_lod_predio_desempenho">
              <option value="original">Original</option>
              <option value="equilibrado">Equilibrado 1024</option>
              <option value="medio">Médio 512</option>
              <option value="distante">Distante 256</option>
            </select></label>
          <div class="sub-titulo">Nitidez do mapa</div>
          <div class="empilhada"><span>Superamostragem <b class="valor" id="cfg_escala">1×</b></span>
            <select id="sel_superamostragem">
              ${SUPERAMOSTRAGEM.map(s => `<option value="${s.valor}">${s.nome}</option>`).join("")}
            </select></div>
          <div class="empilhada"><span>Detalhe do mapa (erro de tela)</span>
            <select id="sel_detalhe_mapa">
              <option value="">Seguir o perfil</option>
              ${DETALHE_MAPA.map(d => `<option value="${d.valor}">${d.nome}</option>`).join("")}
            </select></div>
          <div class="empilhada"><span>Antisserrilhamento (MSAA)</span>
            <select id="sel_msaa">
              <option value="">Seguir o perfil</option>
              <option value="1">1× (desligado)</option>
              <option value="2">2×</option>
              <option value="4">4×</option>
              <option value="8">8×</option>
            </select></div>
          <div class="dica" id="dica_nitidez">
            A imagem de satélite tem teto no provedor: o Esri para em z18 e depois
            o tile é só ampliado. Superamostragem renderiza acima da resolução da
            tela e reamostra — é o que devolve nitidez além desse ponto.
          </div>

          <div class="sub-titulo">Aceleração por GPU</div>
          <label class="linha"><span>Placa</span><span class="valor" id="cfg_gpu_nome">—</span></label>
          <label class="linha"><span>Backend</span><span class="valor" id="cfg_backend">—</span></label>
          <label class="linha"><span>WebGL</span><span class="valor" id="cfg_webgl">—</span></label>
          <label class="linha"><span>Anisotropia / textura</span><span class="valor" id="cfg_aniso">—</span></label>
          <label class="linha"><span>MSAA máx. / HDR</span><span class="valor" id="cfg_msaa_max">—</span></label>
          <label class="linha"><span>Memória JS</span><span class="valor" id="cfg_mem">—</span></label>
          <div id="aviso_gpu_software" style="display:none;background:rgba(234,179,8,0.15);border:1px solid rgba(234,179,8,0.4);color:#fef08a;padding:8px 10px;border-radius:6px;font-size:11px;line-height:1.4;margin:6px 0">
            O navegador está rasterizando por software. Feche o navegador e abra pelo
            <b>ABRIR.cmd</b>, que força a GPU dedicada e deixa escolher o backend.
          </div>
          <div class="dica">
            Trocar o backend (D3D11 · D3D12 · Vulkan · OpenGL) é decisão do navegador,
            não da página: use <b>ABRIR.cmd gpu</b> para listar as modalidades.
          </div>
          <div class="botoes"><button id="btn_salvar_projeto2">${icone("salvar", 14)} Salvar projeto</button></div>`);

      case "sobre":
        return this.cartao("sobre", "Sobre", `
          <label class="linha"><span>Versão</span><span class="valor">${VERSAO}</span></label>
          <label class="linha"><span>Renderizador</span><span class="valor">CesiumJS 1.143</span></label>
          <label class="linha"><span>Servidor</span><span class="valor" id="cfg_host">—</span></label>
          <div class="dica">Dados locais: modelos/, biblioteca/, cache_dem/, cache_glb/.</div>`);

      default:
        return "";
    }
  }

  // ------------------------------------------------------ ligação de UI
  ligarCartao(nome) {
    const acoes = {
      globo_universo: () => this.ligarGloboUniverso(),
      camera: () => this.ligarCamera(),
      lugares: () => this.ligarLugares(),
      camadas: () => this.ligarCamadas(),
      basemap: () => this.ligarImagery(),
      armazenamento: () => this.ligarArmazenamento(),
      relevo: () => this.ligarRelevo(),
      modelo: () => this.ligarModelo(),
      transform: () => this.ligarTransform(),
      pecas: () => this.ligarPecas(),
      inspetor: () => this.ligarInspetor(),
      gizmo: () => this.ligarGizmo(),
      material: () => this.ligarMaterial(),
      biblioteca: () => this.ligarBiblioteca(),
      ferramentas: () => this.ligarFerramentas(),
      etapas_corte: () => this.ligarEtapasDeCorte(),
      recorte: () => this.ligarRecorte(),
      takes: () => this.ligarTakes(),
      sol: () => this.ligarSol(),
      clima: () => this.ligarClima(),
      sombras: () => this.ligarSombras(),
      desempenho: () => this.ligarDesempenho(),
      sobre: () => this.ligarSobre()
    };
    acoes[nome]?.();
  }

  campoNumerico(id, aoMudar) {
    const el = $(id);
    if (!el) return;
    el.addEventListener("focus", () => { this.editandoCampo = true; });
    el.addEventListener("blur", () => { this.editandoCampo = false; });
    el.addEventListener("change", () => {
      const valor = parseFloat(el.value);
      if (!Number.isNaN(valor)) aoMudar(valor);
    });
  }

  comandoPosicao(campo, valor) {
    const anterior = estadoApp.obter().posicao[campo];
    historicoApp.executar({
      nome: `Editar ${campo} do prédio`,
      fazer: () => estadoApp.atualizar({ posicao: { [campo]: valor } }, "posicao"),
      desfazer: () => estadoApp.atualizar({ posicao: { [campo]: anterior } }, "posicao")
    });
  }

  ligarGloboUniverso() {
    const seg = $("seg_modo_globo");
    if (seg) {
      const marcar = () => {
        const atual = estadoApp.obter().ambiente.imagery || FONTE_LOCAL_PADRAO;
        seg.querySelectorAll("[data-modo-globo]").forEach(b =>
          b.classList.toggle("ativo", b.dataset.modoGlobo === atual)
        );
      };
      seg.querySelectorAll("[data-modo-globo]").forEach(btn => {
        btn.onclick = () => {
          const modo = btn.dataset.modoGlobo;
          if (FONTES_REMOTAS_UI.has(modo) && estadoApp.obter().network_mode !== NETWORK_MODES.IMPORT_ASSISTED) {
            this.avisar("conector remoto bloqueado: selecione Importação assistida no Mapa base");
            return;
          }
          estadoApp.atualizar({ ambiente: { imagery: modo } }, "ambiente_ui");
          marcar();
        };
      });
      const remotoPermitido = estadoApp.obter().network_mode === NETWORK_MODES.IMPORT_ASSISTED;
      seg.querySelectorAll("[data-rede-remota]").forEach(btn => { btn.disabled = !remotoPermitido; });
      marcar();
    }

    const chkLuz = $("chk_luz_sol");
    if (chkLuz) {
      chkLuz.checked = estadoApp.obter().ambiente.luz_sol !== false;
      chkLuz.onchange = e =>
        estadoApp.atualizar({ ambiente: { luz_sol: e.target.checked } }, "ambiente_ui");
    }

    const slider = $("globo_hora_slider");
    if (slider) {
      slider.value = estadoApp.obter().ambiente.hora ?? 15;
      slider.oninput = e => {
        const hora = parseFloat(e.target.value);
        estadoApp.atualizar({ ambiente: { hora } }, "ambiente_ui");
        const txt = $("globo_hora_txt");
        if (txt) txt.textContent = this.textoHora(hora);
      };
      const txt = $("globo_hora_txt");
      if (txt) txt.textContent = this.textoHora(estadoApp.obter().ambiente.hora ?? 15);
    }

    $("btn_hora_real")?.addEventListener("click", () => {
      const { hora, data } = ambienteApp.sincronizarHoraReal();
      if (slider) slider.value = hora;
      const txt = $("globo_hora_txt");
      if (txt) txt.textContent = textoHora(hora);
      this.avisar(`sincronizado com ${data} ${textoHora(hora)} no fuso do sítio`);
    });

    $("btn_vis_espaco")?.addEventListener("click", () => {
      ambienteApp.voarParaEspaco();
      this.avisar("visão do espaço");
    });

    $("btn_vis_projeto")?.addEventListener("click", () => {
      const p = estadoApp.obter().posicao;
      cameraApp.olharPara(p.lon, p.lat, p.alt, 250);
      this.avisar("visão do projeto");
    });
  }

  ligarCamera() {
    for (const id of ["cam_alt", "cam_heading", "cam_pitch"]) {
      this.campoNumerico(id, () => this.aplicarCamposCamera());
    }
    $("btn_aplicar_camera").onclick = () => this.aplicarCamposCamera();
    $("cam_lente").onchange = e => cameraApp.definirLenteMm(parseFloat(e.target.value));
    $("btn_gravar_take_rapido").onclick = () => {
      cameraApp.gravarTake(null);
      this.avisar("take gravado");
    };
  }

  aplicarCamposCamera() {
    const cam = estadoApp.obter().camera;
    cameraApp.definirCamera({
      lat: cam.lat, lon: cam.lon,
      alt: parseFloat($("cam_alt")?.value) || cam.alt,
      heading: parseFloat($("cam_heading")?.value) || 0,
      pitch: parseFloat($("cam_pitch")?.value) || 0,
      fov: mmParaFov(parseFloat($("cam_lente")?.value || "50"))
    });
  }

  ligarLugares() {
    $("btn_salvar_lugar").onclick = () => this.salvarLugarAtual();
    $("sel_lugares_salvos").onchange = e => {
      const lugar = (estadoApp.obter().lugares || []).find(l => l.id === e.target.value);
      if (lugar) cameraApp.definirCamera(lugar);
    };
    this.renderizarLugares();
  }

  ligarImagery() {
    const sel = $("amb_imagery");
    const modo = $("amb_network_mode");
    const token = $("amb_token_mapbox");
    const seg = $("seg_basemap");

    const modoAtual = () => estadoApp.obter().network_mode || NETWORK_MODES.OFFLINE_STRICT;
    const remotoPermitido = () => modoAtual() === NETWORK_MODES.IMPORT_ASSISTED;
    const selecionarFonte = fonte => {
      if (FONTES_REMOTAS_UI.has(fonte) && !remotoPermitido()) {
        this.avisar("fonte remota bloqueada em modo local; use Importação assistida explicitamente");
        sel.value = estadoApp.obter().ambiente.imagery || FONTE_LOCAL_PADRAO;
        return false;
      }
      estadoApp.atualizar({ ambiente: { imagery: fonte } }, "ambiente_ui");
      return true;
    };
    const atualizarDisponibilidade = () => {
      const habilitarRemoto = remotoPermitido();
      sel.querySelectorAll("option[data-rede-remota]").forEach(option => { option.disabled = !habilitarRemoto; });
      seg?.querySelectorAll("[data-rede-remota]").forEach(button => { button.disabled = !habilitarRemoto; });
      if (token) token.disabled = !habilitarRemoto;
      if (!habilitarRemoto && FONTES_REMOTAS_UI.has(estadoApp.obter().ambiente.imagery)) {
        estadoApp.atualizar({ ambiente: { imagery: FONTE_LOCAL_PADRAO } }, "ambiente_ui");
      }
    };
    const marcar = () => {
      const atual = estadoApp.obter().ambiente.imagery || FONTE_LOCAL_PADRAO;
      if ([...sel.options].some(option => option.value === atual)) sel.value = atual;
      seg?.querySelectorAll("[data-base]").forEach(button =>
        button.classList.toggle("ativo", button.dataset.base === atual)
      );
    };

    modo.value = modoAtual();
    modo.onchange = event => {
      const escolhido = event.target.value;
      if (!Object.values(NETWORK_MODES).includes(escolhido)) return;
      const patch = { network_mode: escolhido };
      if (escolhido !== NETWORK_MODES.IMPORT_ASSISTED && FONTES_REMOTAS_UI.has(estadoApp.obter().ambiente.imagery)) {
        patch.ambiente = { imagery: FONTE_LOCAL_PADRAO };
      }
      estadoApp.atualizar(patch, "ambiente_ui");
      atualizarDisponibilidade();
      marcar();
      this.avisar(escolhido === NETWORK_MODES.IMPORT_ASSISTED
        ? "conectores remotos liberados nesta configuração; o core continua local"
        : `modo de rede: ${escolhido}`);
    };

    sel.value = estadoApp.obter().ambiente.imagery || FONTE_LOCAL_PADRAO;
    sel.onchange = event => { selecionarFonte(event.target.value); marcar(); };

    if (token) {
      try { token.value = sessionStorage.getItem("arcz.connector.mapbox.token") || ""; } catch { token.value = ""; }
      token.onchange = event => {
        try { sessionStorage.setItem("arcz.connector.mapbox.token", event.target.value.trim()); }
        catch { this.avisar("não foi possível manter o token na sessão"); }
        if (estadoApp.obter().ambiente.imagery === "mapbox") ambienteApp.pedirImagery("mapbox", event.target.value.trim());
      };
    }

    seg?.querySelectorAll("[data-base]").forEach(button => {
      button.onclick = () => { if (selecionarFonte(button.dataset.base)) marcar(); };
    });
    atualizarDisponibilidade();
    marcar();
  }

  /** Camadas do globo: cada interruptor mexe de verdade na cena. */
  ligarCamadas() {
    document.querySelectorAll("[data-camada]").forEach(el => {
      el.checked = this.camadaLigada(el.dataset.camada);
      el.onchange = e => this.alternarCamada(el.dataset.camada, e.target.checked);
    });
  }

  camadaLigada(id) {
    const cena = this.viewer.scene;
    switch (id) {
      case "terreno": return !(this.viewer.terrainProvider instanceof Cesium.EllipsoidTerrainProvider);
      case "imagery": return this.viewer.imageryLayers.length > 0;
      case "entorno_osm": return !!entornoApp.modelo && cena.primitives.contains(entornoApp.modelo);
      case "predio": return !!cenaApp.modeloPredio?.show;
      case "pecas": return [...cenaApp.pecasModelos.values()].every(m => m.show);
      case "eixos": return (gizmoApp.eixos || []).some(e => e.show);
      case "atmosfera": return !!cena.skyAtmosphere?.show;
      default: return true;
    }
  }

  async alternarCamada(id, ligada) {
    const cena = this.viewer.scene;
    if (id === "terreno") {
      if (ligada) {
        const { criarProvedorDeRelevo } = await import("./relevo.js");
        const p = criarProvedorDeRelevo();
        if (p) {
          this.viewer.terrainProvider = p;
          estadoApp.atualizar({ ambiente: { relevo: "dem" } }, "ambiente_ui");
        }
      } else {
        this.viewer.terrainProvider = new Cesium.EllipsoidTerrainProvider();
        estadoApp.atualizar({ ambiente: { relevo: "ellipsoid" } }, "ambiente_ui");
      }
    } else if (id === "imagery") {
      estadoApp.atualizar({ ambiente: { imagery: ligada ? FONTE_LOCAL_PADRAO : "nenhuma" } }, "ambiente_ui");
    } else if (id === "entorno_osm") {
      // Só atualiza o estado — aplicarEstado (ambiente.js) já assina estadoApp
      // e chama entornoApp.alternarEntornoOsm no ramo certo. Chamar direto
      // aqui também rodaria a geração duas vezes para um único clique.
      estadoApp.atualizar({ ambiente: { entorno_osm: ligada } }, "ambiente_ui");
    } else if (id === "predio") {
      if (cenaApp.modeloPredio) cenaApp.modeloPredio.show = ligada;
    } else if (id === "pecas") {
      for (const modelo of cenaApp.pecasModelos.values()) modelo.show = ligada;
    } else if (id === "eixos") {
      for (const eixo of gizmoApp.eixos || []) eixo.show = ligada;
    } else if (id === "atmosfera") {
      if (cena.skyAtmosphere) cena.skyAtmosphere.show = ligada;
      cena.globe.showGroundAtmosphere = ligada;
      if (cena.skyBox) cena.skyBox.show = ligada;
    }
  }

  /** Uso de disco real (cache de relevo, GLB convertido, biblioteca, takes). */
  async ligarArmazenamento() {
    const lista = $("lista_armazenamento");
    if (!lista) return;
    try {
      const res = await fetch("/api/armazenamento");
      const dados = await res.json();
      const total = $("armazenamento_total");
      if (total) total.textContent = `${dados.total_mb} MB`;

      lista.innerHTML = dados.itens.map(i => `
        <div class="linha-item">
          <span class="cresce">${i.nome}</span>
          <span class="num">${i.arquivos} arq</span>
          <span class="num">${i.mb} MB</span>
          ${i.limpavel ? `<button data-limpar="${i.pasta}" title="Limpar cache">${icone("lixeira", 12)}</button>` : ""}
        </div>`).join("");

      lista.querySelectorAll("[data-limpar]").forEach(btn => {
        btn.onclick = async () => {
          const pasta = btn.dataset.limpar;
          if (!confirm(`Limpar ${pasta}? O conteúdo é recriado sob demanda, mas o próximo carregamento fica mais lento.`)) return;
          const r = await fetch("/api/cache/limpar", {
            method: "POST", headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ pasta })
          });
          const d = await r.json();
          this.avisar(d.ok ? `liberado ${d.mb} MB` : "falha ao limpar");
          this.ligarArmazenamento();
        };
      });
    } catch (e) {
      lista.innerHTML = `<div class="erro">não consegui medir o disco</div>`;
    }
  }

  ligarRelevo() {
    $("amb_relevo").checked = !(this.viewer.terrainProvider instanceof Cesium.EllipsoidTerrainProvider);
    $("amb_relevo").onchange = async e => {
      if (e.target.checked) {
        const { criarProvedorDeRelevo } = await import("./relevo.js");
        const p = criarProvedorDeRelevo();
        if (p) {
          this.viewer.terrainProvider = p;
          estadoApp.atualizar({ ambiente: { relevo: "dem" } }, "ambiente_ui");
        }
      } else {
        this.viewer.terrainProvider = new Cesium.EllipsoidTerrainProvider();
        estadoApp.atualizar({ ambiente: { relevo: "ellipsoid" } }, "ambiente_ui");
      }
    };
    $("btn_assentar_predio").onclick = async () => {
      const alt = await cenaApp.assentarNoTerreno();
      this.avisar(alt === null ? "terreno indisponível" : `assentado em ${alt} m`);
      const txt = $("txt_cota");
      if (txt && alt !== null) txt.textContent = `${alt} m`;
    };
    this.mostrarCota();
  }

  async mostrarCota() {
    const txt = $("txt_cota");
    if (!txt) return;
    try {
      const { alturaDoTerreno } = await import("./relevo.js");
      const p = estadoApp.obter().posicao;
      const h = await alturaDoTerreno(p.lon, p.lat);
      txt.textContent = `${h.toFixed(1)} m`;
    } catch (e) {
      txt.textContent = "—";
    }
  }

  ligarModelo() {
    const sel = $("sel_lod_predio");
    sel.value = estadoApp.obter().posicao.lod || "equilibrado";
    sel.onchange = e => estadoApp.atualizar({ posicao: { lod: e.target.value } }, "lod_predio");
    $("btn_ir_predio").onclick = () => {
      const p = estadoApp.obter().posicao;
      cameraApp.olharPara(p.lon, p.lat, p.alt, 250);
    };
    $("btn_recarregar_predio").onclick = () => {
      const st = estadoApp.obter();
      cenaApp.carregarPredio(cenaApp.caminhoPredio, st.posicao, st.posicao.lod);
      this.avisar("recarregando modelo…");
    };
  }

  ligarTransform() {
    for (const [id, campo] of [["pos_lat", "lat"], ["pos_lon", "lon"], ["pos_alt", "alt"],
                               ["pos_rumo", "rumo"], ["pos_escala", "escala"]]) {
      this.campoNumerico(id, valor => this.comandoPosicao(campo, valor));
    }
  }

  ligarPecas() {
    $("btn_remover_peca").onclick = () => cenaApp.removerPecaSelecionada();
    $("btn_duplicar_peca").onclick = () => cenaApp.duplicarPecaSelecionada();
    this.renderizarPecas(estadoApp.obter());
  }

  ligarInspetor() {
    for (const [id, campo] of [["obj_rumo", "rumo"], ["obj_escala", "escala"], ["obj_alt", "alt"]]) {
      this.campoNumerico(id, valor => {
        const sel = estadoApp.obter().pecaSelecionadaId;
        if (!sel) return this.comandoPosicao(campo, valor);
        const anterior = cenaApp.obterPeca(sel)?.[campo];
        historicoApp.executar({
          nome: `Editar ${campo}`,
          fazer: () => cenaApp.atualizarPeca(sel, { [campo]: valor }),
          desfazer: () => cenaApp.atualizarPeca(sel, { [campo]: anterior })
        });
      });
    }
    const lod = $("obj_lod");
    if (lod) {
      lod.onchange = e => {
        const sel = estadoApp.obter().pecaSelecionadaId;
        if (sel) cenaApp.atualizarPeca(sel, { lod: e.target.value });
      };
    }
    this.ligarAparenciaDaPeca();
  }

  /** Um campo de aparência da peça selecionada, com undo. */
  comandoAparencia(patch, nome) {
    const id = estadoApp.obter().pecaSelecionadaId;
    if (!id) return this.avisar("selecione uma peça primeiro", "alerta");
    const peca = cenaApp.obterPeca(id);
    if (!peca) return;
    const antes = {};
    for (const campo of Object.keys(patch)) antes[campo] = peca[campo] ?? null;
    historicoApp.executar({
      nome: `${nome} · ${peca.nome}`,
      fazer: () => cenaApp.atualizarPeca(id, patch),
      desfazer: () => cenaApp.atualizarPeca(id, antes)
    });
  }

  ligarAparenciaDaPeca() {
    const seg = $("seg_obj_sombra");
    seg?.querySelectorAll("[data-sombra]").forEach(btn => {
      btn.onclick = () => {
        this.comandoAparencia({ sombra: btn.dataset.sombra }, "Sombra");
        this.marcarAparencia(cenaApp.obterPeca(estadoApp.obter().pecaSelecionadaId));
      };
    });

    // Slider arrasta ao vivo e só grava um passo de undo ao soltar.
    const aoVivo = (id, campo, rotuloId, formatar, nome) => {
      const el = $(id);
      if (!el) return;
      const rotulo = $(rotuloId);
      let inicial = null;
      el.oninput = e => {
        const v = parseFloat(e.target.value);
        if (rotulo) rotulo.textContent = formatar(v);
        const sel = estadoApp.obter().pecaSelecionadaId;
        if (!sel) return;
        if (inicial === null) inicial = cenaApp.obterPeca(sel)?.[campo] ?? null;
        cenaApp.atualizarPeca(sel, { [campo]: v });
      };
      el.onchange = e => {
        const sel = estadoApp.obter().pecaSelecionadaId;
        const antes = inicial;
        inicial = null;
        if (!sel || antes === null) return;
        const depois = parseFloat(e.target.value);
        historicoApp.registrar({
          nome: `${nome} · ${cenaApp.obterPeca(sel)?.nome || "peça"}`,
          fazer: () => cenaApp.atualizarPeca(sel, { [campo]: depois }),
          desfazer: () => cenaApp.atualizarPeca(sel, { [campo]: antes })
        });
      };
    };
    aoVivo("obj_reflexo", "reflexo", "obj_reflexo_valor", v => v.toFixed(2), "Reflexo");
    aoVivo("obj_opacidade", "opacidade", "obj_opacidade_valor", v => v.toFixed(2), "Opacidade");

    const cor = $("obj_cor");
    const mistura = $("obj_mistura");
    const pintar = () => {
      const sel = estadoApp.obter().pecaSelecionadaId;
      if (!sel) return;
      cenaApp.atualizarPeca(sel, { cor: cor.value, mistura: parseFloat(mistura.value) });
    };
    if (cor) {
      cor.oninput = pintar;
      cor.onchange = () => this.comandoAparencia(
        { cor: cor.value, mistura: parseFloat(mistura.value) }, "Cor");
    }
    if (mistura) mistura.oninput = pintar;

    $("btn_obj_original")?.addEventListener("click", () =>
      this.comandoAparencia({ cor: null, mistura: 0.5, opacidade: 1 }, "Material original"));

    $("btn_obj_todas")?.addEventListener("click", () => {
      const peca = cenaApp.obterPeca(estadoApp.obter().pecaSelecionadaId);
      if (!peca) return this.avisar("selecione uma peça primeiro", "alerta");
      const modelo = {};
      for (const campo of CAMPOS_RENDER) modelo[campo] = peca[campo] ?? null;
      const alvos = estadoApp.obter().pecas.filter(p => p.id !== peca.id);
      if (!alvos.length) return this.avisar("não há outras peças na cena", "alerta");

      const antes = alvos.map(p => [p.id, Object.fromEntries(CAMPOS_RENDER.map(c => [c, p[c] ?? null]))]);
      historicoApp.executar({
        nome: `Aparência de ${peca.nome} em ${alvos.length} peças`,
        fazer: () => alvos.forEach(p => cenaApp.atualizarPeca(p.id, modelo)),
        desfazer: () => antes.forEach(([id, campos]) => cenaApp.atualizarPeca(id, campos))
      });
      this.avisar(`aparência aplicada em ${alvos.length} peças`, "ok");
    });
  }

  /** Reflete no painel a aparência da peça selecionada. */
  marcarAparencia(peca) {
    const r = renderDaPeca(peca);
    $("seg_obj_sombra")?.querySelectorAll("[data-sombra]").forEach(b =>
      b.classList.toggle("ativo", b.dataset.sombra === r.sombra));

    const par = [["obj_reflexo", r.reflexo, "obj_reflexo_valor"],
                 ["obj_opacidade", r.opacidade, "obj_opacidade_valor"],
                 ["obj_mistura", r.mistura, null]];
    for (const [id, valor, rotuloId] of par) {
      const el = $(id);
      if (el && document.activeElement !== el) el.value = String(valor);
      const rotulo = rotuloId ? $(rotuloId) : null;
      if (rotulo) rotulo.textContent = Number(valor).toFixed(2);
    }
    const cor = $("obj_cor");
    if (cor && document.activeElement !== cor) cor.value = r.cor || "#ffffff";

    // Sem peça selecionada não há o que editar: o alvo do gizmo é o prédio.
    const painel = $("seg_obj_sombra")?.closest(".corpo");
    painel?.classList.toggle("sem-peca", !peca);
  }

  ligarGizmo() {
    $("seg_gizmo").querySelectorAll("[data-modo]").forEach(btn => {
      btn.onclick = () => {
        gizmoApp.definirModo(btn.dataset.modo);
        this.marcarModoGizmo(btn.dataset.modo);
      };
    });
    $("chk_eixos_locais").onchange = e => gizmoApp.definirEixosLocais(e.target.checked);

    const snap = $("chk_snap");
    if (snap) {
      snap.checked = gizmoApp.snapFixo;
      snap.onchange = e => {
        gizmoApp.definirSnap(e.target.checked);
        this.avisar(e.target.checked ? "snap ligado" : "snap desligado");
      };
    }
    const grade = $("sel_grade");
    if (grade) {
      grade.value = String(posicionadorApp.grade || 0);
      grade.onchange = e => {
        posicionadorApp.grade = Number(e.target.value);
        localStorage.setItem("arcz.grade", e.target.value);
        this.avisar(posicionadorApp.grade ? `grade ${posicionadorApp.grade} m` : "grade livre");
      };
    }
    this.marcarModoGizmo(estadoApp.obter().modoGizmo);
  }

  /** Pixel do canvas do Cesium a partir de um evento do DOM (o canvas não começa em 0,0). */
  pixelDoCanvas(e) {
    const canvas = this.viewer?.scene?.canvas;
    if (!canvas) return { x: e.clientX, y: e.clientY };
    const r = canvas.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  configurarImportacaoGLB() {
    const processarArquivo = file => {
      if (!file) return;
      const url = URL.createObjectURL(file);
      posicionadorApp.iniciar({ nome: file.name.replace(/\.(glb|gltf)$/i, ""), url });
      this.avisar(`"${file.name}" carregado — clique no 3D para pousar`);
    };

    document.body.addEventListener("click", e => {
      if (e.target.id === "btn_importar_glb" || e.target.closest("#btn_importar_glb")) {
        $("input_importar_glb")?.click();
      }
      if (e.target.id === "btn_importar_glb_lib" || e.target.closest("#btn_importar_glb_lib")) {
        $("input_importar_glb_lib")?.click();
      }
    });

    document.body.addEventListener("change", e => {
      if (e.target.id === "input_importar_glb" || e.target.id === "input_importar_glb_lib") {
        if (e.target.files && e.target.files[0]) processarArquivo(e.target.files[0]);
      }
    });

    const container = $("cesiumContainer") || document.body;

    // Peça vinda da biblioteca: o fantasma já está ligado desde o dragstart e
    // aqui só acompanha o cursor até o usuário soltar.
    const ehPecaDaBiblioteca = dt => Array.from(dt?.types || []).includes(TIPO_ARRASTE);

    container.addEventListener("dragenter", e => {
      if (!ehPecaDaBiblioteca(e.dataTransfer)) return;
      e.preventDefault();
      container.classList.add("recebendo");
    });

    container.addEventListener("dragover", e => {
      e.preventDefault();
      e.dataTransfer.dropEffect = "copy";
      if (!posicionadorApp.ativo) return;
      const p = this.pixelDoCanvas(e);
      posicionadorApp.arrastarPara(p.x, p.y);
    });

    container.addEventListener("dragleave", e => {
      if (e.target === container) container.classList.remove("recebendo");
    });

    container.addEventListener("drop", e => {
      e.preventDefault();
      container.classList.remove("recebendo");
      const p = this.pixelDoCanvas(e);

      const bruto = e.dataTransfer.getData(TIPO_ARRASTE);
      if (bruto) {
        try {
          const item = JSON.parse(bruto);
          if (!posicionadorApp.ativo) posicionadorApp.iniciar(item, { viaArraste: true });
          posicionadorApp.soltarEm(p.x, p.y);
        } catch (erro) {
          console.warn("Payload de arraste invalido:", erro);
        }
        return;
      }

      const files = Array.from(e.dataTransfer.files || []).filter(f => /\.(glb|gltf)$/i.test(f.name));
      if (!files.length) return;

      const ponto = cenaApp.pontoNaCena(p.x, p.y) || normalizarPosicao(estadoApp.obter().posicao);
      for (const file of files) {
        cenaApp.adicionarPeca({
          nome: file.name.replace(/\.(glb|gltf)$/i, ""),
          url: URL.createObjectURL(file),
          lat: ponto.lat, lon: ponto.lon, alt: ponto.alt, escala: 1.0
        });
      }
      this.avisar(`${files.length} modelo(s) solto(s) no ponto do cursor`, "ok");
    });
  }

  marcarModoGizmo(modo) {
    $("seg_gizmo")?.querySelectorAll("[data-modo]").forEach(b =>
      b.classList.toggle("ativo", b.dataset.modo === modo));
  }

  /**
   * Cartão Material = tinta do **prédio principal**.
   * Com uma peça selecionada ele delega para a Aparência do Inspetor: eram dois
   * caminhos escrevendo no mesmo `model.color`, e o daqui (que não passa pelo
   * estado) apagava a opacidade e a cor gravadas na peça no quadro seguinte.
   */
  ligarMaterial() {
    const alvo = () => estadoApp.obter().pecaSelecionadaId;

    const aplicarPbrAoVivo = () => {
      const cor = $("pbr_cor_picker")?.value;
      const mistura = parseFloat($("pbr_blend_slider")?.value || "0.5");
      if (!cor) return;
      const sel = alvo();
      if (sel) cenaApp.atualizarPeca(sel, { cor, mistura });
      else cenaApp.definirCorMaterial("predio", cor, mistura);
    };
    $("pbr_cor_picker")?.addEventListener("input", aplicarPbrAoVivo);
    $("pbr_blend_slider")?.addEventListener("input", aplicarPbrAoVivo);

    $("btn_aplicar_pbr").onclick = () => {
      const cor = $("pbr_cor_picker").value;
      const mistura = parseFloat($("pbr_blend_slider").value);
      if (alvo()) {
        this.comandoAparencia({ cor, mistura }, "Cor");
        return this.avisar("cor aplicada na peça", "ok");
      }
      const ok = cenaApp.definirCorMaterial("predio", cor, mistura);
      this.avisar(ok ? "cor aplicada no prédio" : "modelo não carregado", ok ? "ok" : "alerta");
    };

    $("btn_limpar_pbr").onclick = () => {
      if (alvo()) return this.comandoAparencia({ cor: null, mistura: 0.5 }, "Material original");
      cenaApp.limparCorMaterial("predio");
      this.avisar("material original do prédio");
    };
  }

  ligarBiblioteca() {
    bibliotecaApp.ligarNaUI($("grid_lib"), $("chips_lib"));
    const sel = $("sel_fonte_biblioteca");
    sel.value = bibliotecaApp.fonte;
    sel.onchange = e => bibliotecaApp.definirFonte(e.target.value);
    const busca = $("busca_lib");
    busca.value = bibliotecaApp.termo;
    busca.addEventListener("focus", () => { this.editandoCampo = true; });
    busca.addEventListener("blur", () => { this.editandoCampo = false; });
    busca.oninput = e => bibliotecaApp.definirBusca(e.target.value);
  }

  ligarFerramentas() {
    $("btn_ferramenta_medir").onclick = () =>
      (this.handlerMedir ? this.desligarMedicao() : this.ligarMedicao());
    $("btn_ferramenta_medir").classList.toggle("ativo", !!this.handlerMedir);

    $("btn_ferramenta_corte").onclick = () => {
      const ativo = corteApp.alternar();
      this.mostrarCorte();
      this.avisar(ativo ? `corte em ${corteAtual().distancia} m` : "corte desligado");
    };

    $("seg_eixo_corte")?.querySelectorAll("[data-eixo]").forEach(btn => {
      btn.onclick = () => {
        corteApp.definir({ eixo: btn.dataset.eixo });
        this.mostrarCorte();
      };
    });

    const slider = $("corte_slider");
    const campo = $("corte_altura");
    const mover = valor => {
      if (Number.isNaN(valor)) return;
      if (slider) slider.value = valor;
      if (campo) campo.value = valor;
      corteApp.definir({ distancia: valor });
      this.mostrarCorte(true);
    };
    slider?.addEventListener("input", e => mover(parseFloat(e.target.value)));
    campo?.addEventListener("focus", () => { this.editandoCampo = true; });
    campo?.addEventListener("blur", () => { this.editandoCampo = false; });
    campo?.addEventListener("change", e => mover(parseFloat(e.target.value)));

    const marcar = (id, campoEstado) => {
      const el = $(id);
      if (!el) return;
      el.onchange = e => {
        corteApp.definir({ [campoEstado]: e.target.type === "checkbox" ? e.target.checked : e.target.value });
        this.mostrarCorte();
      };
    };
    marcar("corte_inverter", "invertido");
    marcar("corte_tapar", "tapar");
    marcar("corte_cor", "cor");
    marcar("corte_pecas", "pecas");

    corteApp.aoSecao = info => this.mostrarSecao(info);
    this.mostrarCorte();
  }

  /** Repõe os controles do corte a partir do estado (sem disparar eventos). */
  mostrarCorte(soLeitura = false) {
    const corte = corteAtual();
    $("btn_ferramenta_corte")?.classList.toggle("ativo", corte.ativo);
    $("seg_eixo_corte")?.querySelectorAll("[data-eixo]").forEach(b =>
      b.classList.toggle("ativo", b.dataset.eixo === corte.eixo));

    if (!soLeitura) {
      const slider = $("corte_slider");
      if (slider) {
        // Faixa do controle conforme o tamanho real do modelo carregado.
        try {
          const raio = cenaApp.modeloPredio?.boundingSphere?.radius;
          if (raio > 0) {
            slider.min = Math.round(-raio - 5);
            slider.max = Math.round(raio + 5);
          }
        } catch (e) { /* modelo ainda carregando: fica a faixa padrão */ }
        slider.value = corte.distancia;
      }
      if ($("corte_altura") && !this.editandoCampo) $("corte_altura").value = corte.distancia;
      if ($("corte_inverter")) $("corte_inverter").checked = !!corte.invertido;
      if ($("corte_tapar")) $("corte_tapar").checked = corte.tapar !== false;
      if ($("corte_cor")) $("corte_cor").value = corte.cor || "#aeb8c7";
      if ($("corte_pecas")) $("corte_pecas").checked = corte.pecas !== false;
    }

    const saida = $("corte_saida");
    if (saida && !corte.ativo) saida.innerHTML = `<div class="vazio">corte desligado</div>`;
    else if (saida && !corte.tapar) saida.innerHTML = `<div class="vazio">sem tampa — só o plano de corte</div>`;
    else if (saida && corteApp.ultimaSecao) this.mostrarSecao(corteApp.ultimaSecao);
    this.renderizarEtapas(estadoApp.obter());
  }

  mostrarSecao(info) {
    const saida = $("corte_saida");
    if (!saida) return;
    if (!info || !info.triangulos) {
      saida.innerHTML = `<div class="vazio">nada cruza o plano nesta posição</div>`;
      return;
    }
    saida.innerHTML = `
      <div class="linha-item"><span class="cresce">área de parede cortada</span>
        <b class="num">${info.area.toFixed(2)} m²</b></div>
      <div class="linha-item"><span class="cresce">contornos fechados</span>
        <span class="num">${info.contornos}</span></div>
      <div class="linha-item"><span class="cresce">tampa</span>
        <span class="num">${info.triangulos} tri · ${info.ms} ms</span></div>` +
      (info.forcados
        ? `<div class="linha-item"><span class="cresce">contornos costurados na marra</span>
             <span class="num">${info.forcados}</span></div>`
        : "") +
      (info.falhas
        ? `<div class="linha-item erro"><span class="cresce">contornos sem triangulação</span>
             <span class="num">${info.falhas}</span></div>`
        : "");
  }

  ligarEtapasDeCorte() {
    $("btn_salvar_etapa").onclick = () => {
      const corte = corteAtual();
      const sugestao = corte.eixo === "z" ? `Nível ${corte.distancia} m` : `Corte ${corte.distancia} m`;
      const nome = prompt("Nome da etapa de corte:", sugestao);
      if (nome === null) return;
      const etapa = corteApp.salvarEtapa(nome.trim() || sugestao);
      this.renderizarEtapas(estadoApp.obter());
      this.avisar(`etapa "${etapa.nome}" salva`, "ok");
    };
    $("btn_etapa_anterior").onclick = () => this.irParaEtapa(-1);
    $("btn_etapa_proxima").onclick = () => this.irParaEtapa(1);
    this.renderizarEtapas(estadoApp.obter());
  }

  irParaEtapa(passo) {
    const etapa = corteApp.navegar(passo);
    if (!etapa) return this.avisar("nenhuma etapa salva", "alerta");
    this.mostrarCorte();
    this.avisar(`etapa: ${etapa.nome}`);
  }

  renderizarEtapas(st) {
    const lista = $("lista_etapas");
    const contador = $("cont_etapas_cartao");
    const etapas = corteAtual(st).etapas || [];
    if (contador) contador.textContent = String(etapas.length);
    if (!lista) return;

    if (!etapas.length) {
      lista.innerHTML = `<div class="vazio">nenhuma etapa — ajuste o corte e salve</div>`;
      return;
    }
    const atual = corteApp.indiceDaEtapa();
    lista.innerHTML = etapas.map((e, i) => `
      <div class="linha-item ${i === atual ? "sel" : ""}" data-id="${e.id}">
        <span class="cresce">${e.nome}</span>
        <span class="num">${(EIXOS[e.eixo] || EIXOS.z).nome[0]} ${e.distancia} m</span>
        <button data-acao="atualizar" data-id="${e.id}" title="Gravar o corte atual nesta etapa">${icone("salvar", 12)}</button>
        <button data-acao="excluir" data-id="${e.id}" title="Excluir">${icone("lixeira", 12)}</button>
      </div>`).join("");

    lista.querySelectorAll(".linha-item").forEach(el => {
      el.onclick = ev => {
        if (ev.target.closest("button")) return;
        const etapa = corteApp.aplicarEtapa(el.dataset.id);
        this.mostrarCorte();
        if (etapa) this.avisar(`etapa: ${etapa.nome}`);
      };
      el.ondblclick = () => {
        const etapa = (corteAtual().etapas || []).find(e => e.id === el.dataset.id);
        const nome = prompt("Nome da etapa:", etapa?.nome || "");
        if (nome) {
          corteApp.atualizarEtapa(el.dataset.id, { nome });
          this.renderizarEtapas(estadoApp.obter());
        }
      };
    });
    lista.querySelectorAll("button[data-acao]").forEach(btn => {
      btn.onclick = ev => {
        ev.stopPropagation();
        const corte = corteAtual();
        if (btn.dataset.acao === "atualizar") {
          corteApp.atualizarEtapa(btn.dataset.id, {
            eixo: corte.eixo, distancia: corte.distancia,
            invertido: corte.invertido, tapar: corte.tapar, cor: corte.cor, pecas: corte.pecas
          });
          this.avisar("etapa atualizada", "ok");
        }
        if (btn.dataset.acao === "excluir") corteApp.removerEtapa(btn.dataset.id);
        this.renderizarEtapas(estadoApp.obter());
      };
    });
  }

  ligarRecorte() {
    recorteApp.aoMudar = () => this.renderizarRecorte();
    $("btn_recorte_desenhar").onclick = () => {
      recorteApp.iniciarDesenho();
      this.renderizarRecorte();
    };
    $("btn_recorte_fechar").onclick = () => {
      recorteApp.finalizarDesenho();
      this.renderizarRecorte();
    };
    $("btn_recorte_limpar").onclick = () => {
      recorteApp.limpar();
      const saida = $("recorte_saida");
      if (saida) saida.innerHTML = "";
    };

    const recorte = estadoApp.obter().recorte || {};
    const formato = $("recorte_formato");
    formato.value = recorte.formato || "glb";
    formato.onchange = e =>
      estadoApp.atualizar({ recorte: { ...(estadoApp.obter().recorte || {}), formato: e.target.value } }, "recorte");

    const relevo = $("recorte_relevo");
    relevo.checked = !!recorte.relevo;
    relevo.onchange = e =>
      estadoApp.atualizar({ recorte: { ...(estadoApp.obter().recorte || {}), relevo: e.target.checked } }, "recorte");

    const resolucao = $("recorte_resolucao");
    resolucao.value = recorte.resolucao_relevo || 80;
    this.campoNumerico("recorte_resolucao", v =>
      estadoApp.atualizar({ recorte: { ...(estadoApp.obter().recorte || {}), resolucao_relevo: v } }, "recorte"));

    $("btn_recorte_exportar").onclick = () => this.exportarRecorte();
    this.renderizarRecorte();
  }

  renderizarRecorte() {
    const resumo = $("recorte_resumo");
    if (!resumo) return;
    const perimetro = recorteApp.perimetro();
    $("btn_recorte_desenhar")?.classList.toggle("ativo", recorteApp.desenhando);

    if (perimetro.length < 3) {
      resumo.innerHTML = recorteApp.desenhando
        ? `<div class="vazio">clique no 3D · ${perimetro.length} ponto(s) · duplo clique fecha</div>`
        : `<div class="vazio">nenhum perímetro desenhado</div>`;
      return;
    }
    const medidas = medidasDoPerimetro(perimetro);
    const itens = recorteApp.itensDentro();
    resumo.innerHTML = `
      <div class="linha-item"><span class="cresce">área</span><b class="num">${medidas.area.toFixed(0)} m²</b></div>
      <div class="linha-item"><span class="cresce">perímetro</span><span class="num">${medidas.perimetro.toFixed(1)} m</span></div>
      <div class="linha-item"><span class="cresce">pontos</span><span class="num">${perimetro.length}</span></div>` +
      (itens.length
        ? itens.map(i => `<div class="linha-item"><span class="cresce">${i.nome}</span><span class="num">${i.tipo}</span></div>`).join("")
        : `<div class="vazio">nenhum modelo dentro da área</div>`);
  }

  async exportarRecorte() {
    const botao = $("btn_recorte_exportar");
    const saida = $("recorte_saida");
    const recorte = estadoApp.obter().recorte || {};
    if (botao) botao.disabled = true;
    if (saida) saida.innerHTML = `<div class="vazio">juntando os modelos…</div>`;

    try {
      const dados = await recorteApp.exportar({
        formato: recorte.formato || "glb",
        relevo: !!recorte.relevo,
        resolucao: recorte.resolucao_relevo || 80
      });
      if (saida) {
        saida.innerHTML = `
          <div class="linha-item"><span class="cresce">
            <a href="${dados.url}" download>${dados.arquivos[0]}</a></span>
            <b class="num">${dados.mb} MB</b></div>
          <div class="linha-item"><span class="cresce">modelos juntados</span>
            <span class="num">${dados.modelos}</span></div>` +
          (dados.arquivos.length > 1
            ? `<div class="linha-item"><span class="cresce">+ ${dados.arquivos.slice(1).join(", ")}</span></div>`
            : "") +
          (dados.avisos || []).map(a => `<div class="linha-item"><span class="cresce">${a}</span></div>`).join("");
      }
      this.avisar(`recorte exportado: exportacoes/${dados.arquivos[0]}`, "ok");
    } catch (e) {
      if (saida) saida.innerHTML = `<div class="erro">${e.message}</div>`;
      this.avisar(`exportação: ${e.message}`, "erro");
    } finally {
      if (botao) botao.disabled = false;
    }
  }

  ligarTakes() {
    $("btn_gravar_take").onclick = () => {
      cameraApp.gravarTake(null);
      this.renderizarTakes(estadoApp.obter());
    };
    this.renderizarTakes(estadoApp.obter());
  }

  // ------------------------------------------------------------ sol e clima
  /** Grava um campo do ambiente a partir de um controle. */
  ligarCampoAmbiente(id, campo, ler, evento = "onchange") {
    const el = $(id);
    if (!el) return null;
    el[evento] = e => estadoApp.atualizar({ ambiente: { [campo]: ler(e.target) } }, "ambiente_ui");
    return el;
  }

  ligarSol() {
    const amb = estadoApp.obter().ambiente;

    const data = $("amb_data");
    data.value = amb.data || new Date().toISOString().slice(0, 10);
    data.onchange = e => {
      if (e.target.value) estadoApp.atualizar({ ambiente: { data: e.target.value } }, "ambiente_ui");
    };

    const slider = $("amb_hora");
    slider.value = amb.hora ?? 15;
    slider.oninput = e => {
      const hora = parseFloat(e.target.value);
      estadoApp.atualizar({ ambiente: { hora } }, "ambiente_ui");
      this.mostrarHora(hora);
    };

    const fusoAuto = $("amb_fuso_auto");
    const fuso = $("amb_fuso");
    fusoAuto.checked = amb.fuso_auto !== false;
    fuso.value = ambienteApp.fusoDe();
    fuso.disabled = fusoAuto.checked;
    fusoAuto.onchange = e => {
      fuso.disabled = e.target.checked;
      estadoApp.atualizar(
        { ambiente: { fuso_auto: e.target.checked, fuso: ambienteApp.fusoDe() } },
        "ambiente_ui"
      );
      fuso.value = ambienteApp.fusoDe();
    };
    this.campoNumerico("amb_fuso", v =>
      estadoApp.atualizar({ ambiente: { fuso: v, fuso_auto: false } }, "ambiente_ui"));

    $("btn_hora_real2").onclick = () => {
      const r = ambienteApp.sincronizarHoraReal();
      data.value = r.data;
      slider.value = r.hora;
      this.mostrarHora(r.hora);
      this.avisar(`agora no sítio: ${textoHora(r.hora)}`);
    };

    // Nascer / meio-dia / ocaso vêm das efemérides do dia, não de horas fixas.
    const irPara = (chave, rotulo) => () => {
      const ev = ambienteApp.eventos();
      const h = ev[chave];
      if (h === null || h === undefined) return this.avisar(`sem ${rotulo} nesta data e latitude`);
      estadoApp.atualizar({ ambiente: { hora: h } }, "ambiente_ui");
      slider.value = h;
      this.mostrarHora(h);
      this.avisar(`${rotulo}: ${textoHora(h)}`);
    };
    $("btn_sol_nascer").onclick = irPara("nascer", "nascer do sol");
    $("btn_sol_meiodia").onclick = irPara("meioDia", "meio-dia solar");
    $("btn_sol_ocaso").onclick = irPara("ocaso", "ocaso");

    const animar = $("amb_animar");
    animar.checked = !!amb.animar_sol;
    animar.onchange = e =>
      estadoApp.atualizar({ ambiente: { animar_sol: e.target.checked } }, "ambiente_ui");

    this.ligarDeslizante("amb_velocidade", "velocidade_sol", amb.velocidade_sol ?? 300, v => `${v}×`);
    this.ligarDeslizante("amb_sol_int", "sol_intensidade", amb.sol_intensidade ?? 2.6, v => v.toFixed(1));
    this.ligarDeslizante("amb_exposicao", "brilho_ambiente", amb.brilho_ambiente ?? 1, v => v.toFixed(2));

    const luz = $("amb_luz_sol");
    luz.checked = amb.luz_sol !== false;
    luz.onchange = e => estadoApp.atualizar({ ambiente: { luz_sol: e.target.checked } }, "ambiente_ui");

    $("btn_sol_fachada").onclick = () => {
      const rumo = estadoApp.obter().posicao.rumo || 0;
      const r = ambienteApp.calcularSolNaFachada(rumo);
      if (!r || r.incidencia <= 0) {
        return this.avisar(`a fachada de ${Math.round(rumo)}° não pega sol nesta data`);
      }
      estadoApp.atualizar({ ambiente: { hora: r.hora } }, "ambiente_ui");
      slider.value = r.hora;
      this.mostrarHora(r.hora);
      this.avisar(
        `melhor incidência às ${textoHora(r.hora)} · ${Math.round(r.incidencia * 100)}% de frente, sol a ${r.elevacao.toFixed(0)}°`
      );
    };

    // Leituras ao vivo, inclusive durante o time-lapse (que não escreve estado).
    ambienteApp.aoAtualizarSol = leitura => this.mostrarLeituraSol(leitura, slider);
    this.mostrarHora(amb.hora);
    this.mostrarLeituraSol(ambienteApp.leitura(), slider);
  }

  /** Slider numérico do ambiente com rótulo ao lado. */
  ligarDeslizante(id, campo, valor, formatar) {
    const el = $(id);
    if (!el) return;
    const rotulo = $(`${id}_valor`);
    el.value = valor;
    if (rotulo) rotulo.textContent = formatar(Number(valor));
    el.oninput = e => {
      const v = parseFloat(e.target.value);
      if (rotulo) rotulo.textContent = formatar(v);
      estadoApp.atualizar({ ambiente: { [campo]: v } }, "ambiente_ui");
    };
  }

  mostrarLeituraSol(leitura, slider) {
    const alvo = $("leitura_sol");
    if (!alvo || !leitura) return;
    const ev = leitura.eventos || {};
    const linha = (rotulo, valor) =>
      `<div class="linha-item"><span class="cresce">${rotulo}</span><span class="num">${valor}</span></div>`;

    const duracao = ev.duracaoDia
      ? `${Math.floor(ev.duracaoDia)}h${String(Math.round((ev.duracaoDia % 1) * 60)).padStart(2, "0")}`
      : "—";

    alvo.innerHTML =
      linha("Fase", leitura.fase) +
      linha("Elevação", `${leitura.elevacao.toFixed(1)}°`) +
      linha("Azimute", `${leitura.azimute.toFixed(1)}° ${rumoCardinal(leitura.azimute)}`) +
      linha("Nascer", textoHora(ev.nascer)) +
      linha("Meio-dia solar", textoHora(ev.meioDia)) +
      linha("Ocaso", textoHora(ev.ocaso)) +
      linha("Duração do dia", duracao) +
      linha("Fuso usado", `UTC${leitura.fuso >= 0 ? "+" : ""}${leitura.fuso}`);

    // Durante o time-lapse o estado não é escrito: o slider segue o relógio aqui.
    if (slider && ambienteApp.animando) {
      slider.value = leitura.hora;
      this.mostrarHora(leitura.hora);
      const dataEl = $("amb_data");
      if (dataEl && dataEl.value !== leitura.data) dataEl.value = leitura.data;
    }
  }

  ligarClima() {
    const amb = estadoApp.obter().ambiente;

    const cond = $("amb_condicao");
    cond.value = amb.condicao || "limpo";
    cond.onchange = e => estadoApp.atualizar({ ambiente: { condicao: e.target.value } }, "ambiente_ui");

    this.ligarDeslizante("amb_cobertura", "nuvens_cobertura", amb.nuvens_cobertura ?? 20, v => `${Math.round(v)}%`);
    this.ligarDeslizante("amb_neblina", "neblina", amb.neblina ?? 5, v => `${Math.round(v)}%`);
    this.ligarDeslizante("amb_aerossol", "aerossol", amb.aerossol ?? 10, v => `${Math.round(v)}%`);

    this.preencher("amb_nuvens_altura", amb.nuvens_altura ?? 1200);
    this.campoNumerico("amb_nuvens_altura", v =>
      estadoApp.atualizar({ ambiente: { nuvens_altura: v } }, "ambiente_ui"));
    this.preencher("amb_vento", amb.vento_kmh ?? 12);
    this.campoNumerico("amb_vento", v =>
      estadoApp.atualizar({ ambiente: { vento_kmh: v } }, "ambiente_ui"));
    this.preencher("amb_vento_rumo", amb.vento_rumo ?? 90);
    this.campoNumerico("amb_vento_rumo", v =>
      estadoApp.atualizar({ ambiente: { vento_rumo: v } }, "ambiente_ui"));

    const precip = $("amb_precipitacao");
    precip.value = amb.precipitacao || "auto";
    precip.onchange = e =>
      estadoApp.atualizar({ ambiente: { precipitacao: e.target.value } }, "ambiente_ui");

    for (const [id, campo] of [
      ["amb_fog", "fog"], ["amb_nuvens", "nuvens"], ["amb_bloom", "bloom"],
      ["amb_estrelas", "estrelas"], ["amb_lua", "lua"]
    ]) {
      const el = $(id);
      if (!el) continue;
      el.checked = amb[campo] !== false;
      el.onchange = e => estadoApp.atualizar({ ambiente: { [campo]: e.target.checked } }, "ambiente_ui");
    }
    $("amb_bloom").checked = !!amb.bloom;
  }

  ligarSombras() {
    const amb = estadoApp.obter().ambiente;
    for (const [id, campo] of [
      ["amb_sombra", "sombra"], ["amb_sombra_relevo", "sombra_relevo"], ["amb_sombra_suave", "sombra_suave"]
    ]) {
      const el = $(id);
      if (!el) continue;
      el.checked = amb[campo] !== false;
      el.onchange = e => estadoApp.atualizar({ ambiente: { [campo]: e.target.checked } }, "ambiente_ui");
    }
    this.preencher("amb_sombra_alcance", amb.sombra_alcance ?? 4000);
    this.campoNumerico("amb_sombra_alcance", v =>
      estadoApp.atualizar({ ambiente: { sombra_alcance: v } }, "ambiente_ui"));
  }

  ligarDesempenho() {
    const selPerf = $("sel_perfil_qualidade");
    if (selPerf) {
      selPerf.value = qualidadeApp.perfil || "equilibrado";
      selPerf.onchange = e => {
        qualidadeApp.aplicar(e.target.value);
        this.avisar(`perfil ${PERFIS[e.target.value]?.nome || e.target.value} aplicado`);
      };
    }
    const chkAuto = $("chk_qualidade_auto");
    if (chkAuto) {
      chkAuto.checked = qualidadeApp.automatico;
      chkAuto.onchange = e => qualidadeApp.definirAutomatico(e.target.checked);
    }
    const lim = qualidadeApp.limites();
    const escrever = (id, txt) => { const el = $(id); if (el) el.textContent = txt; };
    escrever("cfg_gpu_nome", gpuCurta(lim.gpu));
    escrever("cfg_backend", lim.software ? `${lim.backend} · software` : lim.backend);
    escrever("cfg_webgl", lim.webgl2 ? "2.0" : (lim.webgl || "1.0"));
    escrever("cfg_aniso", `${lim.anisotropia}× / ${lim.texturaMax}px`);
    escrever("cfg_msaa_max", `${lim.msaaMax}× / ${lim.hdrSuportado ? "sim" : "não"}`);

    const avisoGpu = $("aviso_gpu_software");
    if (avisoGpu) avisoGpu.style.display = lim.software ? "block" : "none";

    const mostrarEscala = () => escrever("cfg_escala", `${qualidadeApp.escalaEfetiva()}×`);
    mostrarEscala();

    const superA = $("sel_superamostragem");
    if (superA) {
      superA.value = String(qualidadeApp.superAmostragem);
      superA.onchange = e => {
        const escala = qualidadeApp.definirSuperAmostragem(e.target.value);
        mostrarEscala();
        this.avisar(`renderizando a ${escala}× da tela`, "ok");
      };
    }

    const detalhe = $("sel_detalhe_mapa");
    if (detalhe) {
      detalhe.value = qualidadeApp.detalheMapa ? String(qualidadeApp.detalheMapa) : "";
      detalhe.onchange = e => {
        const sse = qualidadeApp.definirDetalheMapa(e.target.value);
        this.avisar(sse ? `erro de tela do globo em ${sse}` : "detalhe do mapa segue o perfil");
      };
    }

    const msaa = $("sel_msaa");
    if (msaa) {
      msaa.value = qualidadeApp.msaaManual ? String(qualidadeApp.msaaManual) : "";
      // A GPU pode não aceitar 8×: esconder o que ela não faz evita escolha morta.
      [...msaa.options].forEach(o => {
        if (o.value && Number(o.value) > lim.msaaMax) o.remove();
      });
      msaa.onchange = e => {
        const n = qualidadeApp.definirMsaa(e.target.value);
        this.avisar(n ? `MSAA ${n}×` : "MSAA segue o perfil");
      };
    }

    const selLod = $("sel_lod_predio_desempenho");
    if (selLod) {
      selLod.value = estadoApp.obter().posicao.lod || "equilibrado";
      selLod.onchange = e => estadoApp.atualizar({ posicao: { lod: e.target.value } }, "lod_predio");
    }
    $("btn_salvar_projeto2").onclick = async () =>
      this.mostrarStatusSalvar(await estadoApp.salvarNoServidor());
  }

  ligarSobre() {
    const host = $("cfg_host");
    if (host) host.textContent = location.host;
  }

  // ------------------------------------------------------------- buscas
  async buscarEndereco(termo) {
    const q = (termo || "").trim();
    if (!q) return;
    if (this.tela !== "globo") this.abrirTela("globo");
    const saida = $("res_busca_end");
    if (saida) saida.innerHTML = `<div class="carregando">buscando “${q}”…</div>`;
    try {
      const res = await fetch(`/api/geocode?q=${encodeURIComponent(q)}`);
      const resultados = await res.json();
      if (!Array.isArray(resultados) || !resultados.length) {
        if (saida) saida.innerHTML = `<div class="vazio">nada encontrado</div>`;
        return;
      }
      if (saida) {
        saida.innerHTML = resultados.map((r, i) =>
          `<div class="linha-item" data-i="${i}"><span class="cresce">${r.display_name}</span></div>`).join("");
        saida.querySelectorAll(".linha-item").forEach(el => {
          el.onclick = () => {
            const r = resultados[Number(el.dataset.i)];
            cameraApp.definirCamera({ lat: parseFloat(r.lat), lon: parseFloat(r.lon), alt: 900, pitch: -45 });
          };
        });
      }
      const primeiro = resultados[0];
      cameraApp.definirCamera({ lat: parseFloat(primeiro.lat), lon: parseFloat(primeiro.lon), alt: 900, pitch: -45 });
    } catch (e) {
      if (saida) saida.innerHTML = `<div class="erro">falha na busca</div>`;
    }
  }

  async carregarLugares() {
    try {
      const res = await fetch("/api/lugares");
      const lugares = await res.json();
      if (Array.isArray(lugares) && lugares.length) estadoApp.atualizar({ lugares }, "lugares");
    } catch (e) {
      console.warn("Sem lugares salvos:", e);
    }
    this.renderizarLugares();
  }

  renderizarLugares() {
    const sel = $("sel_lugares_salvos");
    if (!sel) return;
    const lugares = estadoApp.obter().lugares || [];
    sel.innerHTML = `<option value="">Locais salvos…</option>` +
      lugares.map(l => `<option value="${l.id}">${l.nome}</option>`).join("");
  }

  async salvarLugarAtual() {
    const cam = estadoApp.obter().camera;
    const nome = prompt("Nome do local:", `Local ${(estadoApp.obter().lugares || []).length + 1}`);
    if (!nome) return;
    const lugares = [...(estadoApp.obter().lugares || []), {
      id: "lugar_" + Date.now(), nome,
      lat: cam.lat, lon: cam.lon, alt: cam.alt, heading: cam.heading, pitch: cam.pitch
    }];
    estadoApp.atualizar({ lugares }, "lugares");
    this.renderizarLugares();
    try {
      await fetch("/api/lugares", {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify(lugares)
      });
      this.avisar("local salvo");
    } catch (e) {
      this.avisar("falha ao gravar local");
    }
  }

  // -------------------------------------------------------------- listas
  renderizarPecas(st) {
    const lista = $("lista_pecas");
    const contador = $("cont_pecas_cartao");
    if (contador) contador.textContent = String((st.pecas || []).length);
    if (!lista) return;

    const pecas = st.pecas || [];
    if (!pecas.length) {
      lista.innerHTML = `<div class="vazio">nenhuma peça — use a Biblioteca 3D</div>`;
      return;
    }
    lista.innerHTML = pecas.map(p => `
      <div class="linha-item ${p.id === st.pecaSelecionadaId ? "sel" : ""}" data-id="${p.id}">
        <span class="cresce">${p.nome}</span>
        <button data-acao="ver" data-id="${p.id}" title="Enquadrar">${icone("alvo", 12)}</button>
        <button data-acao="ocultar" data-id="${p.id}" title="Mostrar/ocultar">${icone(p.visivel === false ? "olho-fechado" : "olho", 12)}</button>
        <button data-acao="excluir" data-id="${p.id}" title="Remover">${icone("lixeira", 12)}</button>
      </div>`).join("");

    lista.querySelectorAll(".linha-item").forEach(el => {
      el.onclick = ev => {
        if (ev.target.closest("button")) return;
        cenaApp.selecionar(el.dataset.id);
      };
    });
    lista.querySelectorAll("button[data-acao]").forEach(btn => {
      btn.onclick = ev => {
        ev.stopPropagation();
        const id = btn.dataset.id;
        const peca = cenaApp.obterPeca(id);
        if (!peca) return;
        if (btn.dataset.acao === "ver") cameraApp.olharPara(peca.lon, peca.lat, peca.alt, 25);
        if (btn.dataset.acao === "ocultar") cenaApp.atualizarPeca(id, { visivel: !peca.visivel });
        if (btn.dataset.acao === "excluir") {
          cenaApp.selecionar(id);
          cenaApp.removerPecaSelecionada();
        }
      };
    });
  }

  renderizarTakes(st) {
    const lista = $("lista_takes");
    const contador = $("cont_takes_cartao");
    if (contador) contador.textContent = String((st.takes || []).length);
    if (!lista) return;

    const takes = st.takes || [];
    if (!takes.length) {
      lista.innerHTML = `<div class="vazio">nenhum take gravado</div>`;
      return;
    }
    lista.innerHTML = takes.map(t => `
      <div class="linha-item" data-id="${t.id}">
        <span class="cresce">${t.nome}</span>
        <span class="num">${Math.round(t.alt || 0)} m</span>
        <button data-acao="ir" data-id="${t.id}" title="Ir">${icone("camera", 12)}</button>
        <button data-acao="foto" data-id="${t.id}" title="Renderizar PNG">${icone("foto", 12)}</button>
        <button data-acao="dup" data-id="${t.id}" title="Duplicar">${icone("duplicar", 12)}</button>
        <button data-acao="del" data-id="${t.id}" title="Excluir">${icone("lixeira", 12)}</button>
      </div>`).join("");

    lista.querySelectorAll("button[data-acao]").forEach(btn => {
      btn.onclick = () => {
        const id = btn.dataset.id;
        const acao = btn.dataset.acao;
        if (acao === "ir") cameraApp.irParaTake(id);
        if (acao === "dup") cameraApp.duplicarTake(id);
        if (acao === "del") cameraApp.excluirTake(id);
        if (acao === "foto") {
          const take = estadoApp.obter().takes.find(t => t.id === id);
          cameraApp.irParaTake(id);
          setTimeout(async () => {
            const dados = await cameraApp.fotografar(take?.nome || id);
            this.avisar(dados ? `PNG: ${dados.arquivo}` : "falha ao renderizar");
          }, 600);
        }
        this.renderizarTakes(estadoApp.obter());
      };
    });
    lista.querySelectorAll(".linha-item").forEach(el => {
      el.ondblclick = () => {
        const take = estadoApp.obter().takes.find(t => t.id === el.dataset.id);
        const nome = prompt("Nome do take:", take?.nome || "");
        if (nome) {
          cameraApp.renomearTake(el.dataset.id, nome);
          this.renderizarTakes(estadoApp.obter());
        }
      };
    });
  }

  // --------------------------------------------------------- ferramentas
  ligarMedicao() {
    if (this.handlerMedir) return;
    const saida = $("saida_medida");
    if (saida) saida.innerHTML = `<div class="vazio">clique em dois pontos</div>`;
    $("btn_ferramenta_medir")?.classList.add("ativo");

    let inicio = null;
    this.handlerMedir = new Cesium.ScreenSpaceEventHandler(this.viewer.scene.canvas);
    this.handlerMedir.setInputAction(clique => {
      const ponto = this.viewer.scene.pickPosition(clique.position);
      if (!Cesium.defined(ponto)) return;
      this.entidadesMedida.push(this.viewer.entities.add({
        position: ponto,
        point: { pixelSize: 7, color: Cesium.Color.fromCssColorString("#6d9dff"), disableDepthTestDistance: 1e7 }
      }));

      if (!inicio) {
        inicio = ponto;
        if ($("saida_medida")) $("saida_medida").innerHTML = `<div class="vazio">primeiro ponto marcado</div>`;
        return;
      }
      const distancia = Cesium.Cartesian3.distance(inicio, ponto);
      this.entidadesMedida.push(this.viewer.entities.add({
        polyline: {
          positions: [inicio, ponto], width: 2,
          material: Cesium.Color.fromCssColorString("#6d9dff"), arcType: Cesium.ArcType.NONE
        }
      }));
      if ($("saida_medida")) {
        $("saida_medida").innerHTML =
          `<div class="linha-item"><span class="cresce">distância</span><b class="num">${distancia.toFixed(2)} m</b></div>`;
      }
      inicio = null;
    }, Cesium.ScreenSpaceEventType.LEFT_CLICK);
  }

  desligarMedicao() {
    if (this.handlerMedir) {
      this.handlerMedir.destroy();
      this.handlerMedir = null;
    }
    this.entidadesMedida.forEach(e => this.viewer.entities.remove(e));
    this.entidadesMedida = [];
    $("btn_ferramenta_medir")?.classList.remove("ativo");
  }

  // ------------------------------------------------------------ atalhos
  configurarAtalhos() {
    window.addEventListener("keydown", e => {
      const tag = document.activeElement?.tagName?.toLowerCase();
      if (tag === "input" || tag === "textarea" || tag === "select") {
        if (e.key === "Escape") document.activeElement.blur();
        return;
      }
      if (e.ctrlKey || e.metaKey || e.altKey) return;

      // Enquanto o assistente posiciona, o teclado é dele (Esc, G, PgUp/PgDn).
      if (posicionadorApp.ativo) return;

      const tecla = e.key.toLowerCase();
      if (tecla === "x") {
        gizmoApp.definirSnap(!gizmoApp.snapFixo);
        const chk = $("chk_snap");
        if (chk) chk.checked = gizmoApp.snapFixo;
        this.avisar(gizmoApp.snapFixo ? "snap ligado" : "snap desligado");
        return;
      }

      if (tecla === "g") {
        posicionadorApp.alternarGrade();
        const sel = $("sel_grade");
        if (sel) sel.value = String(posicionadorApp.grade);
        return;
      }

      const modo = { q: "nenhum", w: "mover", e: "girar", r: "escalar", s: "escalar" }[tecla];
      if (modo) {
        if (this.modo !== "editar") this.aplicarModo("editar");
        gizmoApp.definirModo(modo);
        this.marcarModoGizmo(modo);
        return;
      }
      if (e.key === "Delete") cenaApp.removerPecaSelecionada();
      if (e.key === "Escape") {
        cenaApp.selecionar(null);
        if (this.modo === "apresentar") this.aplicarModo("editar");
      }
      if (e.key.toLowerCase() === "f") {
        const sel = cenaApp.obterPeca(estadoApp.obter().pecaSelecionadaId);
        const alvo = sel || estadoApp.obter().posicao;
        cameraApp.olharPara(alvo.lon, alvo.lat, alvo.alt, sel ? 25 : 250);
      }
    });
  }

  // ------------------------------------------------------------ métricas
  configurarMetricas() {
    this.viewer.scene.postRender.addEventListener(() => {
      this.quadros++;
      const agora = performance.now();
      if (agora - this.ultimoFps < 1000) return;

      const fps = Math.round((this.quadros * 1000) / (agora - this.ultimoFps));
      this.quadros = 0;
      this.ultimoFps = agora;

      const elFps = $("sb_fps");
      if (elFps) elFps.innerHTML = `<b>${fps}</b> FPS`;

      const tiles = this.viewer.scene.globe.tilesLoaded ? "ok" : "carregando";
      const elTiles = $("sb_tiles");
      if (elTiles) elTiles.textContent = `tiles ${tiles}`;

      const mem = performance.memory ? Math.round(performance.memory.usedJSHeapSize / 1048576) : null;
      const elMem = $("sb_mem");
      if (elMem) elMem.innerHTML = mem ? `<b>${mem}</b> MB` : "— MB";
      const cfgMem = $("cfg_mem");
      if (cfgMem && mem) cfgMem.textContent = `${mem} MB`;
    });
  }

  mostrarStatusSalvar(status) {
    const el = $("sb_status_salvar");
    if (el) el.textContent = status;
  }

  /** Mantido como método por compatibilidade com o resto da UI. */
  textoHora(hora) {
    return textoHora(hora);
  }

  mostrarHora(hora) {
    const el = $("amb_hora_valor");
    if (el && hora !== undefined) el.textContent = textoHora(hora);
  }

  // -------------------------------------------------------------- estado
  aoMudarEstado(st, origem) {
    const pos = st.posicao || {};
    const cam = st.camera || {};

    const coords = $("hud_coords");
    if (coords) {
      coords.textContent =
        `${Number(cam.lat).toFixed(6)}, ${Number(cam.lon).toFixed(6)} · ${Math.round(cam.alt)} m · ` +
        `${Math.round(cam.heading)}° · ${cam.fovMm || "—"} mm`;
    }
    const sbPos = $("sb_posicao");
    if (sbPos) {
      sbPos.textContent =
        `prédio ${Number(pos.lat).toFixed(6)} / ${Number(pos.lon).toFixed(6)} · ` +
        `${Number(pos.alt).toFixed(1)} m · ${Number(pos.rumo).toFixed(0)}°`;
    }
    const sbPecas = $("sb_pecas");
    if (sbPecas) sbPecas.innerHTML = `<b>${(st.pecas || []).length}</b> peças`;
    this.mostrarStatusSalvar(estadoApp.statusSave);

    const camCoords = $("cam_coords");
    if (camCoords) camCoords.textContent = `${Number(cam.lat).toFixed(5)}, ${Number(cam.lon).toFixed(5)}`;
    const camDist = $("cam_dist");
    if (camDist) camDist.textContent = `${cam.distanciaAlvo || 0} m`;

    if (!this.editandoCampo) {
      this.preencher("cam_alt", Math.round(cam.alt ?? 0));
      this.preencher("cam_heading", Math.round(cam.heading ?? 0));
      this.preencher("cam_pitch", Math.round(cam.pitch ?? 0));
    }

    // Painel do corte segue o estado: aplicar etapa ou abrir projeto reposiciona os controles.
    if (origem === "corte" || origem === "carregamento_inicial") this.mostrarCorte();
    if (origem === "recorte" || origem === "carregamento_inicial") this.renderizarRecorte();

    const contPecas = $("cont_pecas");
    if (contPecas) contPecas.textContent = String((st.pecas || []).length || "");
    const contTakes = $("cont_takes");
    if (contTakes) contTakes.textContent = String((st.takes || []).length || "");

    if (origem === "camera") return;

    if (!this.editandoCampo) {
      this.preencher("pos_lat", pos.lat);
      this.preencher("pos_lon", pos.lon);
      this.preencher("pos_alt", pos.alt);
      this.preencher("pos_rumo", pos.rumo);
      this.preencher("pos_escala", pos.escala);

      const sel = st.pecaSelecionadaId ? cenaApp.obterPeca(st.pecaSelecionadaId) : null;
      const alvo = sel || pos;
      this.preencher("obj_rumo", alvo.rumo);
      this.preencher("obj_escala", alvo.escala);
      this.preencher("obj_alt", alvo.alt);
      this.preencher("obj_lod", alvo.lod || "medio");
      const nome = $("obj_nome");
      if (nome) nome.textContent = sel ? sel.nome : "prédio principal";
      this.marcarAparencia(sel);
    }

    const lod = $("sel_lod_predio");
    if (lod && pos.lod && lod.value !== pos.lod) lod.value = pos.lod;
    const eixos = $("chk_eixos_locais");
    if (eixos) eixos.checked = !!pos.eixos_locais;

    const amb = st.ambiente || {};
    const hora = $("amb_hora");
    if (hora && amb.hora !== undefined && Number(hora.value) !== amb.hora) hora.value = amb.hora;
    this.mostrarHora(amb.hora);
    const horaGlobo = $("globo_hora_slider");
    if (horaGlobo && amb.hora !== undefined && Number(horaGlobo.value) !== amb.hora) {
      horaGlobo.value = amb.hora;
      const txt = $("globo_hora_txt");
      if (txt) txt.textContent = textoHora(amb.hora);
    }
    const dataEl = $("amb_data");
    if (dataEl && amb.data && dataEl.value !== amb.data) dataEl.value = amb.data;

    this.marcarCheck("amb_sombra", amb.sombra !== false);
    this.marcarCheck("amb_sombra_relevo", amb.sombra_relevo !== false);
    this.marcarCheck("amb_sombra_suave", amb.sombra_suave !== false);
    this.marcarCheck("amb_fog", amb.fog !== false);
    this.marcarCheck("amb_nuvens", amb.nuvens !== false);
    this.marcarCheck("amb_bloom", amb.bloom);
    this.marcarCheck("amb_estrelas", amb.estrelas !== false);
    this.marcarCheck("amb_lua", amb.lua !== false);
    this.marcarCheck("amb_luz_sol", amb.luz_sol !== false);
    this.marcarCheck("amb_animar", amb.animar_sol);
    this.marcarCheck("chk_luz_sol", amb.luz_sol !== false);

    // Trocar de condição repõe os sliders finos: o painel tem de mostrar isso.
    for (const [id, valor, formatar] of [
      ["amb_cobertura", amb.nuvens_cobertura, v => `${Math.round(v)}%`],
      ["amb_neblina", amb.neblina, v => `${Math.round(v)}%`],
      ["amb_aerossol", amb.aerossol, v => `${Math.round(v)}%`],
      ["amb_sol_int", amb.sol_intensidade, v => v.toFixed(1)],
      ["amb_exposicao", amb.brilho_ambiente, v => v.toFixed(2)],
      ["amb_velocidade", amb.velocidade_sol, v => `${v}×`]
    ]) {
      const el = $(id);
      if (!el || valor === undefined || Number(el.value) === Number(valor)) continue;
      el.value = valor;
      const rotulo = $(`${id}_valor`);
      if (rotulo) rotulo.textContent = formatar(Number(valor));
    }

    const cond = $("amb_condicao");
    if (cond && amb.condicao && cond.value !== amb.condicao) cond.value = amb.condicao;
    const img = $("amb_imagery");
    if (img && amb.imagery && img.value !== amb.imagery) img.value = amb.imagery;

    this.marcarModoGizmo(st.modoGizmo);
    this.renderizarPecas(st);
    if (origem !== "pecas" && origem !== "selecao") this.renderizarTakes(st);
    if (origem === "carregamento_inicial" || origem === "lugares") this.renderizarLugares();
  }

  preencher(id, valor) {
    const el = $(id);
    if (!el || document.activeElement === el || valor === undefined || valor === null) return;
    const texto = String(valor);
    if (el.value !== texto) el.value = texto;
  }

  marcarCheck(id, valor) {
    const el = $(id);
    if (el) el.checked = !!valor;
  }
}

export const uiApp = new UIManager();
