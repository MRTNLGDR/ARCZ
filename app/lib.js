// ARCZ · Biblioteca: catálogo local CC0, thumbnails automáticos e Poly Haven.
// Cada card é arrastável para o 3D (drag & drop nativo) e o clique abre o
// assistente de posicionamento — a colocação em si é do posicionar.js.
import { cenaApp } from "./cena.js";
import { icone } from "./icones.js";
import { posicionadorApp } from "./posicionar.js";
import { feedbackApp } from "./feedback.js";

export const TIPO_ARRASTE = "application/arcz-peca";
const CHAVE_FAVORITOS = "arcz.libFavoritos";
const CHAVE_RECENTES = "arcz.libRecentes";
const MAX_RECENTES = 12;

// Classificação por palavra-chave no nome da pasta: a biblioteca não tem
// metadado de categoria, e 125 itens numa grade única é impossível de varrer.
export const CATEGORIAS = [
  { id: "todos", nome: "Todos" },
  { id: "favoritos", nome: "Favoritos" },
  { id: "recentes", nome: "Recentes" },
  { id: "assentos", nome: "Assentos", chaves: ["sofa", "poltrona", "cadeira", "banqueta", "banco", "puff", "espreguicadeira", "chair", "lounge", "stool", "seat"] },
  { id: "mesas", nome: "Mesas", chaves: ["mesa", "table", "bancada", "balcao", "criado", "gaveteiro", "comoda", "aparador"] },
  { id: "guarda", nome: "Guarda", chaves: ["armario", "estante", "prateleira", "guarda-roupa", "rack", "gondola", "arara", "cabinet", "shelf"] },
  { id: "dormir", nome: "Dormir", chaves: ["cama", "colchao", "almofada", "travesseiro", "pillow", "bed"] },
  { id: "cozinha", nome: "Cozinha", chaves: ["cozinha", "cooktop", "geladeira", "micro-ondas", "churrasqueira", "registradora", "jogo-cha", "cafe", "coffee"] },
  { id: "banho", nome: "Banho", chaves: ["banheiro", "chuveiro", "sanitario", "cuba", "box-"] },
  { id: "luz", nome: "Luz", chaves: ["luminaria", "pendente", "lareira", "abajur", "lamp"] },
  { id: "verde", nome: "Verde", chaves: ["planta", "vaso", "floreira", "samambaia", "coqueiro", "suculenta", "fern", "plant", "calathea", "pachira", "jardineira"] },
  { id: "externo", nome: "Externo", chaves: ["ombrelone", "guarda-sol", "piscina", "outdoor", "solares", "rocks", "externa", "sacada", "agua-"] },
  { id: "veiculos", nome: "Veículos", chaves: ["carro", "suv", "bmw", "tesla", "moto", "bike"] },
  { id: "decor", nome: "Decor", chaves: ["quadro", "tapete", "relogio", "espelho", "manequim", "livros", "rug"] }
];

/** Categorias do manifesto do banco → chips desta biblioteca. */
export const CATEGORIA_DO_BANCO = {
  armchairs: "assentos", chairs: "assentos", sofas: "assentos",
  tables: "mesas", cabinets: "guarda", beds: "dormir",
  kitchen: "cozinha", appliances: "cozinha", bathroom: "banho",
  lighting: "luz", outdoor: "externo", decor: "decor", uncategorized: "decor"
};

export function filtrarItens(itens, termo) {
  const t = (termo || "").trim().toLowerCase();
  if (!t) return itens;
  return itens.filter(i => (i.nome || "").toLowerCase().includes(t));
}

export function rotuloDoItem(nome) {
  return String(nome || "").replace(/[-_]+/g, " ").replace(/\s+/g, " ").trim();
}

/** Categoria de um item pelo nome da pasta. Sem palavra-chave conhecida: "decor". */
export function categoriaDoItem(nome) {
  const n = String(nome || "").toLowerCase();
  for (const cat of CATEGORIAS) {
    if (!cat.chaves) continue;
    if (cat.chaves.some(k => n.includes(k))) return cat.id;
  }
  return "decor";
}

function lerLista(chave) {
  try {
    const bruto = JSON.parse(localStorage.getItem(chave) || "[]");
    return Array.isArray(bruto) ? bruto : [];
  } catch (e) {
    return [];
  }
}

export class BibliotecaManager {
  constructor() {
    this.viewer = null;
    this.viewerThumb = null;
    this.containerThumb = null;
    this.itens = [];
    this.itensBanco = [];
    this.itensPolyHaven = [];
    this.fonte = "local";
    this.termo = "";
    this.categoria = "todos";
    this.grid = null;
    this.chips = null;
    this.gerandoThumbs = false;
    this.favoritos = new Set(lerLista(CHAVE_FAVORITOS));
    this.recentes = lerLista(CHAVE_RECENTES);
  }

  inicializar(viewer) {
    this.viewer = viewer;
  }

  /** Liga a biblioteca ao grid da UI (chamado depois que o painel existe). */
  ligarNaUI(grid, chips = null) {
    this.grid = grid;
    this.chips = chips;
    this.renderizarChips();
    this.carregarCatalogo();
  }

  /** Fontes que se comportam como acervo: categoria, favorito, arraste. */
  get acervo() {
    return this.fonte === "local" || this.fonte === "banco";
  }

  definirFonte(fonte) {
    this.fonte = fonte;
    if (fonte === "local") this.carregarCatalogo();
    else if (fonte === "banco") this.carregarBanco();
    else this.carregarPolyHaven(fonte === "polyhaven_textures" ? "textures" : "models");
    this.renderizarChips();
  }

  definirBusca(termo) {
    this.termo = termo;
    this.renderizarGrid();
  }

  definirCategoria(id) {
    this.categoria = id;
    this.renderizarChips();
    this.renderizarGrid();
  }

  // ------------------------------------------------------------ catálogo
  async carregarCatalogo() {
    this.mostrarAviso("Carregando biblioteca local...");
    try {
      const res = await fetch("/api/biblioteca");
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const bruto = await res.json();
      this.itens = bruto.map(i => ({ ...i, categoria: categoriaDoItem(i.nome) }));
      this.renderizarChips();
      this.renderizarGrid();
      this.gerarThumbsFaltantes();
    } catch (e) {
      console.error("Erro ao carregar biblioteca:", e);
      this.mostrarAviso("Falha ao carregar a biblioteca local.");
    }
  }

  /** Banco externo (D:\AVANGARD-ASSETS): 1245 modelos CC0/CC-BY fora do repo. */
  async carregarBanco() {
    if (this.itensBanco.length) {
      this.renderizarChips();
      return this.renderizarGrid();
    }
    this.mostrarAviso("Lendo o banco de modelos...");
    try {
      const res = await fetch("/api/biblioteca/banco");
      const dados = await res.json();
      if (dados.erro) throw new Error(dados.erro);
      this.itensBanco = dados.map(i => ({
        ...i,
        categoria: CATEGORIA_DO_BANCO[i.categoria_banco] || "decor"
      }));
      this.renderizarChips();
      this.renderizarGrid();
    } catch (e) {
      console.error("Erro ao ler o banco de modelos:", e);
      this.mostrarAviso(`Banco indisponível: ${e.message}`);
    }
  }

  async carregarPolyHaven(tipo) {
    this.mostrarAviso("Consultando Poly Haven...");
    try {
      const res = await fetch(`/api/polyhaven/assets?type=${tipo}`);
      const dados = await res.json();
      if (dados.erro) throw new Error(dados.erro);
      this.itensPolyHaven = Object.entries(dados).slice(0, 200).map(([id, meta]) => ({
        id,
        nome: meta.name || id,
        // O navegador não busca thumbnail em CDN. Um importador autorizado
        // deve materializar a prévia localmente; até lá o card usa ícone.
        thumb: null,
        tipo
      }));
      this.renderizarGrid();
    } catch (e) {
      console.error("Erro ao buscar Poly Haven:", e);
      this.mostrarAviso("Poly Haven indisponivel (sem internet?).");
    }
  }

  // --------------------------------------------------------- favoritos
  alternarFavorito(nome) {
    if (this.favoritos.has(nome)) this.favoritos.delete(nome);
    else this.favoritos.add(nome);
    localStorage.setItem(CHAVE_FAVORITOS, JSON.stringify([...this.favoritos]));
    // O chip "Favoritos" só existe quando há favoritos: recontar antes de pintar.
    this.renderizarChips();
    this.renderizarGrid();
  }

  marcarRecente(nome) {
    this.recentes = [nome, ...this.recentes.filter(n => n !== nome)].slice(0, MAX_RECENTES);
    localStorage.setItem(CHAVE_RECENTES, JSON.stringify(this.recentes));
    this.renderizarChips();
  }

  // --------------------------------------------------------------- grid
  acervoAtual() {
    if (this.fonte === "local") return this.itens;
    if (this.fonte === "banco") return this.itensBanco;
    return this.itensPolyHaven;
  }

  itensVisiveis() {
    let lista = filtrarItens(this.acervoAtual(), this.termo);
    if (!this.acervo) return lista;

    if (this.categoria === "favoritos") {
      lista = lista.filter(i => this.favoritos.has(i.nome));
    } else if (this.categoria === "recentes") {
      const ordem = new Map(this.recentes.map((n, i) => [n, i]));
      lista = lista.filter(i => ordem.has(i.nome)).sort((a, b) => ordem.get(a.nome) - ordem.get(b.nome));
    } else if (this.categoria !== "todos") {
      lista = lista.filter(i => i.categoria === this.categoria);
    }
    return lista;
  }

  /** Quantos itens cada categoria tem hoje (chips sem resultado ficam ocultos). */
  contagemPorCategoria() {
    const base = this.acervoAtual();
    const conta = {};
    for (const item of base) conta[item.categoria] = (conta[item.categoria] || 0) + 1;
    conta.todos = base.length;
    conta.favoritos = base.filter(i => this.favoritos.has(i.nome)).length;
    conta.recentes = base.filter(i => this.recentes.includes(i.nome)).length;
    return conta;
  }

  renderizarChips() {
    if (!this.chips) return;
    if (!this.acervo) {
      this.chips.innerHTML = "";
      return;
    }
    const conta = this.contagemPorCategoria();
    this.chips.innerHTML = CATEGORIAS
      .filter(c => (conta[c.id] || 0) > 0 || c.id === this.categoria || c.id === "todos")
      .map(c => `<button class="chip-lib${c.id === this.categoria ? " ativo" : ""}" data-cat="${c.id}">
                   ${c.nome}<span class="n">${conta[c.id] || 0}</span>
                 </button>`)
      .join("");

    this.chips.querySelectorAll("[data-cat]").forEach(btn => {
      btn.onclick = () => this.definirCategoria(btn.dataset.cat);
    });
  }

  mostrarAviso(texto) {
    if (this.grid) this.grid.innerHTML = `<div class="aviso-lib">${texto}</div>`;
  }

  renderizarGrid() {
    if (!this.grid) return;
    const itens = this.itensVisiveis();
    if (!itens.length) {
      this.mostrarAviso("Nenhum item encontrado.");
      return;
    }
    const acervo = this.acervo;

    this.grid.innerHTML = itens
      .map((item, i) => {
        const rotulo = rotuloDoItem(item.nome);
        const thumb = item.thumb
          ? `<img src="${item.thumb}" alt="${rotulo}" loading="lazy" draggable="false">`
          : `<div class="sem-thumb">${icone("cubo", 26)}</div>`;
        const extra = acervo
          ? `${item.mb ?? "?"} MB${item.licenca ? ` · ${item.licenca}` : ""}`
          : (this.fonte === "polyhaven_textures" ? "textura CC0" : "modelo CC0");
        const fav = acervo && this.favoritos.has(item.nome);
        const estrela = acervo
          ? `<button class="fav${fav ? " on" : ""}" data-fav="${i}" title="${fav ? "Tirar dos favoritos" : "Favoritar"}">${icone("faisca", 12)}</button>`
          : "";
        // Licença é obrigação de atribuição do acervo: fica visível no card.
        const selo = item.licenca && item.licenca !== "CC0"
          ? `<span class="selo-licenca" title="${item.licenca} — exige atribuição">${item.licenca}</span>`
          : "";
        return `<div class="item-thumb" data-indice="${i}" ${acervo ? 'draggable="true"' : ""}
                     title="${rotulo} — ${extra}${acervo ? " · arraste para o 3D" : ""}">
                  ${thumb}${estrela}${selo}<div class="nome">${rotulo}</div>
                </div>`;
      })
      .join("");

    this.grid.querySelectorAll(".item-thumb").forEach(el => this.ligarCard(el));
  }

  ligarCard(el) {
    const pegar = () => this.itensVisiveis()[Number(el.dataset.indice)] || null;

    const estrela = el.querySelector("[data-fav]");
    if (estrela) {
      estrela.onclick = ev => {
        ev.stopPropagation();
        const item = pegar();
        if (item) this.alternarFavorito(item.nome);
      };
    }

    el.addEventListener("click", ev => {
      if (ev.target.closest("[data-fav]")) return;
      const item = pegar();
      if (!item) return;
      if (this.acervo) this.posicionar(item);
      else this.baixarDoPolyHaven(item, el);
    });

    if (!this.acervo) return;

    el.addEventListener("dragstart", ev => {
      const item = pegar();
      if (!item) return ev.preventDefault();
      ev.dataTransfer.effectAllowed = "copy";
      ev.dataTransfer.setData(TIPO_ARRASTE, JSON.stringify({ nome: rotuloDoItem(item.nome), url: item.url }));
      ev.dataTransfer.setData("text/plain", rotuloDoItem(item.nome));
      el.classList.add("arrastando");
      posicionadorApp.iniciar({ nome: rotuloDoItem(item.nome), url: item.url }, { viaArraste: true });
    });

    el.addEventListener("dragend", () => {
      el.classList.remove("arrastando");
      // O drop no viewport já encerrou o assistente; sobra só o caso "soltou fora".
      if (posicionadorApp.ativo && posicionadorApp.viaArraste) posicionadorApp.cancelar();
    });
  }

  // ------------------------------------------------------------- colocar
  /** Abre o assistente de posicionamento com a peça escolhida. */
  posicionar(item) {
    this.marcarRecente(item.nome);
    return posicionadorApp.iniciar({ nome: rotuloDoItem(item.nome), url: item.url });
  }

  /** Põe a peça direto no ponto do terreno sob o centro da tela (sem assistente). */
  colocarNoCentro(item) {
    if (!this.viewer) return null;
    const x = this.viewer.canvas.clientWidth / 2;
    const y = this.viewer.canvas.clientHeight / 2;
    return this.colocarNaTela(item, x, y);
  }

  colocarNaTela(item, x, y) {
    if (!this.viewer) return null;
    const ponto = cenaApp.pontoNaCena(x, y);
    if (!ponto) {
      console.warn("Nao achei superficie sob o cursor para pousar a peca.");
      return null;
    }
    this.marcarRecente(item.nome);
    return cenaApp.adicionarPeca({
      nome: rotuloDoItem(item.nome),
      url: item.url,
      lat: ponto.lat,
      lon: ponto.lon,
      alt: ponto.alt,
      lod: "medio"
    });
  }

  async baixarDoPolyHaven(item, elemento) {
    const rota = item.tipo === "textures" ? "/api/polyhaven/baixar_textura" : "/api/polyhaven/baixar";
    if (elemento) elemento.classList.add("baixando");
    feedbackApp.aviso(`Baixando ${item.nome} do Poly Haven…`, "info", 3000);
    try {
      const res = await fetch(rota, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id: item.id, resolution: "1k" })
      });
      const dados = await res.json();
      if (!dados.ok) throw new Error(dados.erro || "falha no download");
      if (item.tipo !== "textures") {
        await this.carregarCatalogo();
        this.fonte = "local";
        const seletor = document.getElementById("sel_fonte_biblioteca");
        if (seletor) seletor.value = "local";
        const novo = this.itens.find(i => i.url === dados.url) || { nome: dados.nome, url: dados.url };
        this.posicionar(novo);
      } else {
        feedbackApp.aviso(`Textura ${item.nome} baixada`, "ok");
      }
      return dados;
    } catch (e) {
      console.error(`Erro ao baixar ${item.id}:`, e);
      feedbackApp.aviso(`Falha ao baixar ${item.nome}`, "erro");
      return null;
    } finally {
      if (elemento) elemento.classList.remove("baixando");
    }
  }

  // ---------------------------------------------------------- thumbnails
  async gerarThumbsFaltantes() {
    if (this.gerandoThumbs) return;
    const faltando = this.itens.filter(i => !i.thumb);
    if (!faltando.length) return;

    this.gerandoThumbs = true;
    this.criarViewerOculto();
    try {
      for (const item of faltando) {
        const url = await this.gerarThumb(item);
        if (url) {
          item.thumb = url;
          this.renderizarGrid();
        }
      }
    } finally {
      this.destruirViewerOculto();
      this.gerandoThumbs = false;
    }
  }

  criarViewerOculto() {
    if (this.containerThumb) return;
    this.containerThumb = document.createElement("div");
    this.containerThumb.style.cssText =
      "position:absolute;width:256px;height:256px;top:-9999px;left:-9999px;";
    document.body.appendChild(this.containerThumb);

    this.viewerThumb = new Cesium.Viewer(this.containerThumb, {
      animation: false, baseLayerPicker: false, fullscreenButton: false, geocoder: false,
      homeButton: false, infoBox: false, sceneModePicker: false, selectionIndicator: false,
      timeline: false, navigationHelpButton: false, scene3DOnly: true,
      imageryProvider: false, terrainProvider: new Cesium.EllipsoidTerrainProvider()
    });
    this.viewerThumb.scene.backgroundColor = Cesium.Color.fromCssColorString("#141922");
    this.viewerThumb.scene.globe.show = false;
    this.viewerThumb.scene.skyBox.show = false;
    this.viewerThumb.scene.skyAtmosphere.show = false;
  }

  destruirViewerOculto() {
    if (this.viewerThumb) {
      this.viewerThumb.destroy();
      this.viewerThumb = null;
    }
    if (this.containerThumb) {
      this.containerThumb.remove();
      this.containerThumb = null;
    }
  }

  async gerarThumb(item) {
    if (!this.viewerThumb) return null;
    try {
      this.viewerThumb.scene.primitives.removeAll();
      const modelo = await Cesium.Model.fromGltfAsync({ url: item.url, scale: 1.0 });
      this.viewerThumb.scene.primitives.add(modelo);

      // Espera o modelo ficar pronto antes de fotografar.
      for (let i = 0; i < 120 && !modelo.ready; i++) {
        this.viewerThumb.render();
        await new Promise(r => setTimeout(r, 16));
      }
      this.viewerThumb.camera.flyToBoundingSphere(modelo.boundingSphere, { duration: 0 });
      this.viewerThumb.render();
      this.viewerThumb.render();

      const png = this.viewerThumb.canvas.toDataURL("image/png");
      const res = await fetch("/api/thumb", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ nome: item.nome, png })
      });
      const dados = await res.json();
      return dados.ok ? dados.url : null;
    } catch (e) {
      console.warn(`Falha ao gerar thumb de ${item.nome}:`, e);
      return null;
    }
  }
}

export const bibliotecaApp = new BibliotecaManager();
