# ARCZ Earth — Arquitetura atual e plano de evolução

> **Como ler.** A Parte I descreve o que **existe hoje**, auditado no código, com números
> medidos e não estimados. A Parte II lista defeitos e armadilhas já comprovadas — leia
> antes de propor qualquer coisa. A Parte III é o plano. A Parte IV mapeia o Google Earth
> Studio contra o que temos. A Parte V é o contrato de extensão, escrito para que outra IA
> consiga produzir um plugin funcional sem adivinhar nada.
>
> Toda afirmação aqui foi verificada contra o repositório ou contra o serviço real. Onde a
> medição não existe, está escrito "não medido".

---

# Parte I — Estrutura atual

## I.1 Topologia

Três camadas independentes, sem framework, sem etapa de build no front:

```
navegador  ──HTTP──▶  servidor.py  ──subprocesso──▶  crates/ (Rust)
 app/*.js               24 rotas                      motor geométrico
 CesiumJS 1.143         cache em disco                OSM, terreno, malha
 8.631 js + 537 css     1.088 linhas                  30.932 linhas
```

O front é **ES modules puro**, servido direto do disco. Não há bundler, transpiler nem
`package.json`. Consequência prática: qualquer arquivo em `app/` é editável e recarregável
sem etapa intermediária — e qualquer erro de sintaxe derruba o módulo inteiro em tempo de
carga, não de build.

## I.2 Front-end (`app/`, 8.631 linhas de JS)

| Módulo | Linhas | Responsabilidade | Observação |
|---|---:|---|---|
| `ui.js` | 2.367 | Casca inteira: menu, cartões, rodapé, atalhos | **Grande demais.** Não deve crescer mais |
| `ambiente.js` | 838 | Imagery, atmosfera, céu, nuvens, OSM buildings | Dirige o visual do globo |
| `corte.js` | 830 | Corte com tampa sólida (poché) sobre a malha | Ferramenta de malha, não gerador |
| `gizmo.js` | 561 | Alças por eixo, hover, snap, leitura ao vivo | Reescrito; alças pickáveis |
| `cena.js` | 534 | Prédio, peças, LOD, seleção, aparência por peça | Núcleo da cena |
| `lib.js` | 478 | Biblioteca local, banco externo, Poly Haven | 3 fontes de acervo |
| `posicionar.js` | 426 | Assistente de posicionamento (fantasma, snap, série) | Drag & drop do acervo |
| `qualidade.js` | 409 | Perfis, adaptação por FPS, superamostragem, MSAA | Controla GPU |
| `clima.js` | 361 | Modelo de céu por elevação solar e condição | Alimenta `ambiente` |
| `recorte.js` | 306 | Perímetro desenhado + exportação | Ferramenta, não gerador |
| `estado.js` | 293 | Estado único + persistência com debounce | **Fonte da verdade** |
| `relevo.js` | 232 | Provedor DEM Terrarium via `/dem` | |
| `sol.js` | 207 | Efemérides: elevação, azimute, nascer/ocaso | Puro, testável |
| `camera.js` | 188 | Leitura por quadro, lente, takes | |
| `feedback.js` | 156 | Avisos, chip numérico, dicas, ping 3D | |
| `entorno.js` | 141 | Entorno OSM ancorado na posição | **Único gerador de verdade** |
| `main.js` | 134 | Bootstrap e ordem de inicialização | Ordem importa |
| `historico.js` | 107 | Undo/redo por comando | |
| `icones.js` | 63 | Sprite SVG (regra: nunca emoji) | |

**Ordem de inicialização em `main.js`** — não é arbitrária:
`ambiente → camera → cena → feedback → posicionador → gizmo → biblioteca → ui`.
O gizmo lê `cenaApp.selecaoBloqueada`, que o posicionador escreve; inverter a ordem faz o
clique de pouso virar clique de seleção.

## I.3 Servidor (`servidor.py`, 24 rotas)

`ThreadingHTTPServer` — thread por requisição. Serve a raiz do projeto e mais:

**Projeto e cena:** `/api/projeto` · `/api/cena` · `/api/cenas` · `/api/posicao` ·
`/api/takes` · `/api/lugares` · `/api/foto` · `/api/thumb`
**Acervo:** `/api/biblioteca` · `/api/biblioteca/banco` · `/banco-glb` ·
`/api/polyhaven/assets` · `/api/polyhaven/baixar` · `/api/polyhaven/baixar_textura` ·
`/api/modelos`
**Geometria:** `/glb-lod` · `/glb-corrigido` · `/api/entorno-osm` · `/api/entorno-osm.glb` ·
`/api/exportar`
**Território:** `/dem/{z}/{x}/{y}` · `/api/geocode`
**Manutenção:** `/api/armazenamento` · `/api/cache/limpar`

Regras já embutidas que não devem ser desfeitas:

- `Cache-Control: no-store` para `.html/.css/.js/.json` — sem isso o navegador segura
  módulo antigo e a correção no disco não aparece (**já causou um bug de render**).
- `index.html` recebe `?v=<mtime>` nos assets de primeiro nível.
- `caminho_seguro()` prende qualquer caminho vindo do cliente dentro da raiz.
- O banco externo resolve por **id no manifesto**, nunca por caminho do cliente.
- Catálogos rejeitam arquivo que não abre (`.glb` sem magic `glTF`, `.gltf` sem o `.bin`).

## I.4 Motor Rust (`crates/`, 30.932 linhas)

| Crate | Linhas | Papel |
|---|---:|---|
| `arcz-app` | 15.662 | Aplicação/servidor nativo |
| `arcz-osm` | 4.621 | Overpass, entidades, malha de vias/edifícios, procedural |
| `arcz-biblioteca` | 2.765 | Catálogo de assets |
| `arcz-model` | 2.664 | glTF, materiais |
| `arcz-terrain` | 2.248 | DEM, tiles |
| `arcz-geo` | 1.482 | Projeções, ENU, bbox |
| `arcz-tauri` · `arcz-scene` · `arcz-osm-cli` · `arcz-earth` · `arcz-jobs` · `arcz-provenance` | 1.490 | Empacotamento, cena, CLI, utilidades |

`arcz-osm` **já faz** footprint, altura, lotes, recuos, adensamento procedural e malha de
vias. Qualquer proposta de "IA que levanta prédios" precisa partir daqui, não do zero.

## I.5 Estado único (`projeto.json`)

```
versao · posicao(8 campos) · ambiente(33) · camera(14) · corte(8) · recorte(4)
takes[] · pecas[] · lugares[] · pecaSelecionadaId · modoGizmo · criado_em · atualizado_em
```

Contrato do `estado.js`: `atualizar(patch, origem)` só notifica **quando algo mudou de
verdade** (comparação profunda), e a `origem` é o que permite cada observador ignorar o que
não lhe diz respeito. A câmera escreve a cada quadro; sem esse filtro, o app inteiro reagiria
60×/s.

## I.6 Invariantes — quebrar qualquer um destes causa regressão

1. **Callback de `CallbackProperty` nunca lança.** Uma exceção dentro do laço de render do
   Cesium para a renderização **em definitivo**, sem volta, até recarregar a página.
2. **Todo `origem` novo no estado precisa ser tratado ou ignorado explicitamente** pelos
   observadores de `ambiente`, `cena` e `ui`.
3. **Não existe bundler.** Import circular entre módulos do front quebra em tempo de carga.
4. **Um cartão por vez na tela.** `camera` e `inspetor` aparecem em duas telas cada; os
   lookups são por `id` global. Dois montados ao mesmo tempo = o segundo fica morto.
5. **`ui.js` não cresce.** Código novo vai para subpasta.

---

# Parte II — Defeitos conhecidos e armadilhas medidas

## II.1 Ativos (a corrigir)

**A. O autosave morre com a câmera em movimento.** `notificar()` chama `agendarSalvar()`
sempre; `agendarSalvar()` faz `clearTimeout` + 600 ms. Câmera em movimento contínuo reinicia
o temporizador a cada quadro e ele **nunca dispara**. Com giro automático de globo, o projeto
para de gravar em silêncio. → Teto absoluto de gravação, não só debounce.

**B. `ui.js` com 2.367 linhas** e editado por mais de uma sessão em paralelo.

**C. Cartões `camera` e `inspetor` duplicados entre telas.** Funciona hoje por acidente
(uma tela por vez). Abas simultâneas quebram.

## II.2 Limites externos medidos

| O quê | Medição | Consequência |
|---|---|---|
| Esri World Imagery | z18 = 15.908 B (real); z19/z20/z21 = 2.521 B, **idênticos** (placeholder) | z18 é o teto real. Nitidez além disso só por superamostragem |
| Overpass público | lote 250 m = 15 ways/1,8 s · quarteirão 1 km = 79 ways/1,1 s · **2 requisições seguidas = HTTP 429** | Precisa de fila 1 req/s, backoff e cache por tile |
| Nominatim | Sem geometria de lote: endereço com número devolve `LineString` da **rua** | "Lote" não vem de geocodificação — vem de desenho manual |
| Escala | estado 568×388 km · cidade 18,6×12,4 km · bairro 2,9×5,5 km · **gerador atual: 0,0625 km²** | Cidade = **3.700×** o máximo do gerador. Separar escopo de trabalho de escopo de geração |
| GPU da máquina de referência | RTX 4090 Laptop, D3D11, WebGL 2.0, aniso 16×, textura 16384, MSAA máx 8×, HDR sim | |

## II.3 Capacidades do Cesium 1.143 — o que existe e o que não

**Existe:** `OrthographicFrustum` · `SampledPositionProperty` ·
`HermitePolynomialApproximation` · `EXT_mesh_gpu_instancing` · `Cesium3DTileset` ·
`PostProcessStageLibrary` (DOF, blur, bloom, silhueta, detecção de borda, lens flare) ·
`DynamicEnvironmentMapManager`.

**Não existe:** `ModelInstanceCollection` (removido) · **motion blur** · velocity buffer.

**Render em alta resolução — medido:** `maximumTextureSize` e `MAX_RENDERBUFFER_SIZE` =
16384; viewport até 32767. **8K cabe em um único render target.** Custo de framebuffer:
4K+MSAA4 = 158 MB · 8K+MSAA4 = 633 MB · 8K+MSAA8 = 1.139 MB · 8K sem MSAA = 127 MB.
`resolutionScale = 4` testado ao vivo: canvas foi a **4824×5376 (25,9 MP) e renderizou**.

**Motion blur, então, só offline** — por acumulação de sub-quadros ao longo do trecho da
câmera. É a técnica de render de cinema e funciona porque controlamos a câmera por
sub-quadro. Em tempo real, não há caminho.

**Reflexo de ambiente:** o `DynamicEnvironmentMapManager` gera cubemap **procedural** do céu
mais uma cor sólida de chão. Ele **nunca** reflete outros modelos, terreno ou imagem de
satélite. Reflexo de cenário real exige `specularEnvironmentMaps` (KTX2 assado) — suportado,
hoje `null`.

---

# Parte III — Plano

## III.0 Regras de proteção

1. Nada substitui, tudo acrescenta — módulo novo nasce ao lado, com chave de troca.
2. Padrão = comportamento de hoje (Região Ativa começa em "mundo", plugins desligados).
3. Toda fase entrega o app funcionando; abandonar no meio não deixa o disco quebrado.
4. Código novo em `app/shell/`, `app/plugins/`, `app/cine/` — nunca em `ui.js`.
5. Orçamento declarado ou não entra em cena.

## III.1 Fases

| # | Fase | Entrega | Depende |
|---|---|---|---|
| **0** | Teto de autosave | Corrige II.1-A | — |
| **1** | Região Ativa | Autocomplete (debounce 350 ms, mín. 4 letras, fila 1 req/s) · escalas endereço→estado · lote por desenho reusando `recorte` · **só escopo de trabalho** | 0 |
| **2** | Abertura cinematográfica | Entrada coreografada, giro lento com toggle, transição espaço→sítio | 0 |
| **3** | Geração por tiles | Tiles sob demanda perto da câmera, orçamento, fila com backoff 429 | 1 |
| **4** | Contrato de extensão | `Gerador` + `Ferramenta`, `docs/PLUGINS.md` gerado do contrato, plugin de referência | 3 |
| **5** | Casca nova | 4 modos (Globo · Floorplanner · Render · Walk), navbar inferior, abas — **uma por vez** | — |
| **6** | Cinema | Timeline, keyframes de câmera e objeto, curvas, parâmetros de lente, LUT, grão, DOF, motion blur por acumulação | 4, 5 |
| **7** | Geradores de conteúdo | Grama e árvores (`EXT_mesh_gpu_instancing`), vias/pontes, água, oceano, foto de drone | 3, 4 |
| **8** | Render por difusão | Serviço local, img2img condicionado por profundidade+normal da cena, upscale em ladrilhos até 8K | 6 |
| **9** | Pranchas | Plantas, cortes, elevações ortográficas, estudo solar, blueprint, diagramação | 6, 8 |
| **10** | IA de projeto | Proxy `/api/ia`, chave **no servidor**, chat com ferramentas sobre a API do app | — |

**A fase 3 precede todo gerador.** Sem ela, cada plugin reinventa controle de escala e
orçamento, e a fase 7 vira sete implementações incompatíveis do mesmo problema.

## III.2 Notas por fase de risco

**Fase 6 — Cinema.** `SampledPositionProperty` interpola posição; ângulos por SLERP. Cada
keyframe carrega milimetragem, abertura, foco, DOF, LUT, grão, exposição. Os takes de hoje
viram keyframe único e continuam funcionando.

**Fase 8 — Difusão.** Não é "modelo de linguagem": é modelo de **difusão** (SDXL, Flux).
Difusão nativa dá 1024–1536 px, então 8K é upscale em ladrilhos com sobreposição e semente
fixa, não geração direta. A vantagem decisiva do projeto: a cena exporta **profundidade,
normais e máscara por objeto de graça** — é o condicionamento que preserva a geometria
enquanto o material é trocado. É o que faz "boneco vira pessoa real" sem mudar a arquitetura
ao redor.
**Falha conhecida, dita antes:** continuidade quadro a quadro. Mesmo com profundidade e
semente fixa, difusão cintila entre quadros. Reduz-se muito; não se elimina. Imagem estática
8K é entrega segura; filme inteiro por difusão é aposta.

**Fase 9 — Pranchas.** A mais barata do plano: `OrthographicFrustum` dá elevações reais,
`corte.js` já produz planta com poché, `sol.js` já tem efemérides. É diagramação sobre coisa
pronta.

---

# Parte IV — Google Earth Studio como referência

O Earth Studio é um **animador de câmera sobre dados geográficos** — não modela, não edita
vídeo, não renderiza fisicamente. Serve como referência de **fluxo e vocabulário de
interface**, não de implementação. Nada de código ou dado proprietário do Google entra aqui;
o que se replica é a ideia de timeline sobre território.

| Capacidade | Earth Studio | ARCZ hoje | Fase |
|---|---|---|---|
| Busca de local e ida da câmera | sim | `/api/geocode` sem autocomplete | 1 |
| Quick Starts (órbita, zoom-to, ponto-a-ponto, espiral) | sim | não | 6 |
| Timeline com playhead e keyframes | sim | takes discretos, sem interpolação | 6 |
| Faixa por atributo | sim | não | 6 |
| Easing e editor de curvas | sim | não | 6 |
| Altitude logarítmica | sim | não | 6 |
| Camera Target (mira contínua) | sim | `olharPara` pontual | 6 |
| Hora do dia animável | sim | **superior** — efemérides reais + clima | 6 liga na timeline |
| Multi-View (câmera/topo/lateral) | sim | não | 5 |
| Track Points | sim | não | 6 |
| KML/KMZ com opacidade animável | sim | não | 7 |
| Snapshot | JPG | `/api/foto` PNG | pronto |
| Render em sequência de imagens | sim | não | 6 |
| Exportação de câmera 3D (.jsx / JSON) | sim | não | 6 |
| Guias de enquadramento e safe zones | sim | não | 5 |
| **Modelos próprios na cena** | **não** | sim — biblioteca + banco de 694 | pronto |
| **Corte com poché** | **não** | sim | pronto |
| **Gizmo de edição** | **não** | sim | pronto |
| **Estudo solar com efemérides** | parcial | sim | pronto |
| **Render fotorreal** | **não** | não | 8 |
| **Pranchas arquitetônicas** | **não** | não | 9 |
| **Exportação da cena (GLB/glTF/OBJ)** | **não** | sim | pronto |

**Leitura:** o Earth Studio ganha em *timeline e exportação de câmera*; o ARCZ já ganha em
*edição, corte, acervo e exportação de cena*. A fase 6 fecha a única lacuna estrutural.

**Limites do Earth Studio que o ARCZ não deve herdar:** fachada borrada em câmera baixa,
árvore derretida, carro fundido no terreno. São da malha fotogramétrica. Nossa resposta é
outra: modelo próprio no nível da rua e, na fase 8, difusão condicionada.

---

# Parte V — Contrato de extensão (para outra IA implementar)

Dois tipos, porque forçar um só criaria abstração ruim — `corte` e `recorte` **não são**
geradores de região, e tentar encaixá-los provaria isso tarde demais.

```js
// Tipo 1 — GERADOR: produz geometria numa região.
export default {
  tipo: "gerador",
  id: "arcz.vegetacao.arvores",
  nome: "Árvores", versao: "1.0.0", icone: "arvore",
  escala: ["endereco", "quarteirao"],          // acima disso nem é oferecido
  custo: { triangulos: 2e6, memoriaMB: 400 },  // orçamento declarado

  parametros: [                                 // a UI nasce daqui
    { id: "densidade", tipo: "faixa", min: 0, max: 1, padrao: 0.3 },
    { id: "especie",   tipo: "escolha", opcoes: ["nativa", "urbana"] }
  ],

  async preparar(ctx) {},                       // ctx.regiao, ctx.terreno, ctx.osm, ctx.cena
  async gerar(ctx, params, sinal) {},           // AbortSignal — cancelável sempre
  aplicar(ctx, resultado) {},
  limpar(ctx) {},                               // sai sem deixar rastro
  serializar() {}                               // vai para o projeto.json
};

// Tipo 2 — FERRAMENTA: age sobre a seleção ou a cena.
export default {
  tipo: "ferramenta",
  id: "arcz.medir.distancia",
  modos: ["globo", "floorplanner"],             // em que modos aparece na navbar
  ativar(ctx) {}, desativar(ctx) {}, serializar() {}
};
```

**Cinco regras que impedem o sistema de apodrecer:**

1. **Plugin nunca toca `viewer` direto** — só `ctx`, que é instrumentável e limitável.
2. **Toda geração é cancelável** — trocar de região aborta em vez de acumular trabalho.
3. **Custo é declarado** e o orquestrador recusa o que estoura o orçamento da GPU.
4. **`limpar()` é testado** — plugin que vaza primitiva é rejeitado no carregamento.
5. **Nada de estado global** — o que precisa sobreviver ao recarregamento vai em
   `serializar()`.

**Erros que já custaram caro e o contrato previne:**
callback de render que lança (mata a renderização inteira); modelo carregado sem checar
`ready` antes de ler `boundingSphere`; primitiva removida da cena mas não do `Map` que a
indexa; e escrever no `model.color` por dois caminhos diferentes, onde o segundo apaga a
opacidade do primeiro.
