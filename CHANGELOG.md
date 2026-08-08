# CHANGELOG

## V11.1.0 world foundation — 2026-08-08

- repository target corrected to `MRTNLGDR/ARCZ`;
- added `arcz-world` contracts for object-to-planet scope, world layers, LOD/cells and streaming budgets;
- added `arcz-plugin-host` capability registry and permission gate;
- expanded modular catalog to 66 plugin families / 253 capabilities while preserving honest `partial`/`contract_ready` states;
- expanded agent scope through country, continent and planet;
- documented product, world-scale architecture, plugin model, upstream conversion, address-to-project pipeline, AI agent, security, validation, license boundaries and dependency-ordered master roadmap;
- retained Aedifex/Cesium/Kepler/IfcOpenShell/Bonsai as pinned upstream boundaries rather than destructive blind merges.

## V11.0.0 foundation — 2026-08-08

- retained the supplied Global V10.1 implementation and regression gates;
- added pinned immutable upstream manifest for Aedifex, CesiumJS, Kepler.gl and IfcOpenShell/Bonsai;
- added `arcz-plugin-sdk` Rust contracts and provider-agnostic `arcz-agent` plan validation;
- added the first modular plugin catalog;
- added upstream materialization and plugin catalog verification tools;
- documented Rust-first Web/Desktop architecture and staged Aedifex parity conversion.

## V10.1 Matriz canônica e fechamento de lacunas — 2026-08-06

- `CONVERSION_MATRIX.json` passou a ser gerada do lock e cobre exatamente 7 pacotes, 2 apps, 1 plugin, 46 node kinds nativos, 3 extensões e 21 famílias MCP.
- Hash canônico da matriz: `4718a2cb82d5cd83c6eeb72bb1bbd014fac9fad6a8fa27d9c3a9b3a7f9b5aaa4`; divergência, símbolo desconhecido ou cobertura incompleta falham os testes e o verificador.
- Chat global passou de catálogo passivo para execução MCP em fonte: leitura real; mutação em cópia isolada; diff; aprovação/rejeição; `expected_revision`; auditoria e rollback.
- Prompt library ganhou bundles import/export com checksum e política de conflitos.
- Mídias de referência ampliadas para BIM/CAD/geodados/HDR/nuvem de pontos, sempre com bytes, magic/header, hash, licença e provenance.
- Render `high`/`ultra` exige GLB real exportado da revisão Aedifex; ausência bloqueia em vez de produzir reconstrução parcial silenciosa.
- Worker Blender passou a importar GLB, configurar câmera física, Cycles/Eevee, passes semânticos e manifest; execução permanece bloqueada sem Blender.
- Dock ganhou semântica de tabs, teclado, touch, foco, pin, resize e reduced motion; globo cinematográfico ganhou estado cancelável e restauração segura.
- Suíte local após as correções: 119 testes Python aprovados, 3 ignorados por pré-condição, 41 testes JavaScript aprovados e 19 arquivos TypeScript do overlay com sintaxe válida.
- Checkout upstream, Bun build, Cesium local, Rust, Blender, pesos locais e E2E Windows continuam bloqueios objetivos; não foram declarados aprovados.

## V10 Aedifex Mass Conversion — 2026-08-06

- Floorplanner passou a manter o globo Cesium visível, navegável e redimensionável durante a autoria.
- Revisões salvas podem publicar automaticamente GLB readonly com `GeoAnchor` e `generation_epoch`.
- Chat consolidado em uma única superfície global com ferramentas ARCZ + MCP Aedifex, dry-run, aprovação/rejeição e revision guard.
- Prompt library ampliada com versões, duplicação, archive, import/export e idiomas arbitrários.
- Reference media passou a servir bytes reais, preview, roles, weights, notas, licença e verificação de integridade.
- Workspace fotorreal ganhou câmera física, perfis 4K/8K, passes, referências e preflight estrito.
- Abertura cinematográfica corrigida para aguardar callbacks reais de `Cesium.Camera.flyTo()`.
- Inventário/coverage fail-closed documentados para todos os pacotes, 46 node kinds, MCP, rotas e network surfaces.
- Rotas de versões/duplicação/archive de prompts e detalhe/rejeição de tool runs corrigidas.
- Migração V1→V2 atualizada para preencher todos os parâmetros cinematográficos.
- Ledger de capacidades e plano de conversão massiva adicionados.
- Corrigido o schema de modelos locais para aceitar as tarefas realmente usadas pelo runtime: `chat.global`, `prompt.enhance`, `prompt.translate`, `render-diffusion` e `upscale`.
- Adicionados QUICKSTART, preflight fail-closed, run/stop/install/uninstall para Windows/Linux e alternativa Docker Compose.
- PowerShell/Docker e instalação limpa permanecem gates bloqueados, não testes presumidos.
- Build integral Aedifex, Cesium real, Rust, Blender, modelos e E2E continuam bloqueios explícitos.


## V9 Aedifex Global — 2026-08-06

- Decisão consolidada: ARCZ permanece World Core; Aedifex torna-se Building
  Authoring Kernel revisionado.
- Região/lote agora geram `ModelingContextPackage` e `GeoAnchor` explícitos.
- Projetos Floorplanner, revisões, SSE, conflito otimista, mídia, prompts e chat
  persistem em stores locais.
- Overlay Aedifex substitui IA remota pelo Local AI Broker e reúne Aedifex Agent
  e ARCZ Global.
- Cena R3F real é exportada por `GLTFExporter`, validada como GLB binário,
  armazenada por SHA-256 e publicada no globo como derivado readonly.
- Projeto novo deixou de carregar `modelos/zenite.glb`; modelo principal passa a
  ser explícito.
- Shell Globo/Floorplanner/Render/Walk, painéis collapsed/hover/pin e abertura
  cinematográfica foram integrados em módulos novos sem ampliar `ui.js`.
- Worker Blender/Cycles, preflight e manifestos foram implementados; runtime
  continua bloqueado porque Blender/modelos não estão instalados.
- Auditoria comunitária impede blind merge de forks divergentes.
- Validação dirigida após as correções: 92 testes Python aprovados, 3 ignorados
  por fixtures externas e 23 testes JavaScript aprovados. O relatório final é
  regenerado por `tools/verify_handoff.py`.
- Bloqueios honestos: Aedifex/Cesium não materializados, Rust/Bun/Blender/modelos
  ausentes e E2E visual não executável neste ambiente.

Registro do que passou a funcionar de verdade, com a evidência que comprova.
Nada entra aqui por ter compilado — só por ter sido executado e verificado.

## V5 Local-first handoff — 2026-08-06

> Esta seção é a evidência auditada da entrega V5. As seções históricas abaixo
> foram preservadas do material de origem e **não foram revalidadas nesta
> sessão**. Em caso de conflito, prevalecem `IMPLEMENTATION_STATUS.json`,
> `TASKS.json` e `docs/audit/VALIDATION_REPORT.md`.

- Core local-first: `offline_strict`, sem CDN, provider ou IA externa
  obrigatória.
- API V2 local, pacotes imutáveis, jobs persistentes, cancelamento, orçamento,
  plugins, RegionContext, perfis, panoramas e pranchas SVG.
- Módulos ES sem bundler para core, região, plugins, procedural, shell, cinema,
  walk, render e sheets.
- Crates Rust de determinismo, orçamento, validação, região, tiles, telhados,
  fachadas, vegetação, procedural e worker GLB entregues em fonte.
- Migração `projeto.json` V1→V2 pura, idempotente, validada e atômica com
  backup.
- Corrida de cancelamento corrigida: progresso não pode sobrescrever
  `CANCEL_REQUESTED`; stress 100/100 aprovado.
- Verificação: 67 testes Python passaram, 3 foram ignorados por pré-condição;
  12 testes JavaScript passaram; 78 módulos JS têm sintaxe válida; 15 gates
  passaram, nenhum falhou.
- Bloqueios honestos: `cargo fmt/check/test` não executados por ausência de
  toolchain e bundle CesiumJS local não veio no RAR.

## Não lançado

### Persistência

- **`project.sqlite` fecha o ciclo.** A sessão ligada a um projeto grava a cena
  e um diário de comandos, e retoma a posição na abertura. Verificado movendo
  para rumo 90°, matando o processo e reabrindo com a CLI passando 59,98° — o
  gravado vence o argumento.
- `NodeType` sobrevive ao ciclo. A coluna era gravada e descartada na leitura:
  todo nó voltava como `Building`, e um terreno reabria como edificação.
- O diário continua da sequência onde parou, em vez de sobrescrever a sessão
  anterior.

### Seleção e edição

- **Clique seleciona qualquer objeto**, incluindo os do entorno gerado. Rota
  `/picar` devolve o nó sob o cursor com nome, proveniência e dimensões.
- Refinamento por triângulo (Möller–Trumbore). Só a caixa envolvente fazia o
  clique acertar sempre a rua: a caixa de "Vias — local" mede 715 × 805 m e
  engloba o bairro inteiro, inclusive o ar acima das construções.
- O modelo importado passou a existir no `Editor`. Antes era desenhado por
  caminho próprio e ficava invisível ao clique — justo o objeto que mais se
  quer manipular.
- **Snapping no núcleo.** Quem arredondava era o JavaScript; agora o cliente
  informa o passo e o Rust alinha. A CLI e o render em lote passam a produzir
  a mesma posição que a tela mostra.

### Entorno procedural

- **Contexto GIS a partir do OpenStreetMap**: ruas, quadras, água e vegetação,
  mais as edificações que faltam, alinhadas às vias reais e assentadas no DEM.
  Medido em Bombinhas: 4 prédios mapeados + 458 gerados, 469 objetos.
- Cor de cada edificação amostrada da ortofoto do próprio lote, com filtro que
  descarta copa de árvore — sem ele, casa com árvore no quintal saía verde-mata.
- **Telhado de duas águas** nas casas baixas. O tipo existia e o gerador o
  ignorava; tudo saía com laje e o bairro parecia maquete de caixas.
- Espécie e porte de cada árvore resolvidos pela posição, de forma
  reproduzível. Antes toda árvore era sorteada na mesma faixa, com a mesma
  esbeltez.

### Interface

- UI portada do design contract ARCZ Earth: cabeçalho, rail com os cinco
  grupos, sistema de cartões, Inspector e telas de estado.
- Outliner e Inspector sincronizados com a seleção do viewport, nos dois
  sentidos.
- A página abria preta e muda. `null.onclick` no topo do script abortava tudo
  abaixo e deixava as declarações seguintes presas na temporal dead zone.

### Corte e recorte (visualizador web)

- **O corte tapa as paredes.** O plano do Cesium só apagava pixels e deixava o
  furo da parede aberto por cima. Agora a malha do GLB é lida no navegador, os
  triângulos que cruzam o plano viram segmentos, os segmentos viram contornos
  fechados e o miolo da alvenaria é preenchido com um poché sólido na cota do
  corte — sala continua vazada, parede fica sólida. Medido no `zenite.glb`
  (936.506 triângulos), 10 cotas de 1,2 m a 24 m: 80 a 687 contornos por cota,
  0 falhas de triangulação, 0 a 20 contornos abertos (≤3%, metade costurada),
  11 a 99 ms por cota.
- **A tampa saía girada 90° sobre o prédio** — era o buraco que sobrava. O
  Cesium aplica DUAS correções de eixo ao carregar glTF (Y-para-cima→Z-para-cima
  *e* Z-para-frente→X-para-frente) e a leitura da malha só aplicava a primeira.
  A altura batia (as duas levam Y do glTF para Z), a planta não. Agora a matriz
  vem do próprio `Model` (`sceneGraph._axisCorrectionMatrix`). Conferido em
  render 3D real: o poché cobre exatamente o topo de cada parede cortada; e a
  mesma correção no recorte fecha com o que o Cesium desenha com erro de 0 m
  num ponto de teste.
- Corte em três eixos (horizontal, leste-oeste, norte-sul), lado invertível,
  cor da tampa e opção de cortar também as peças. A peça recebe o mesmo plano
  transformado para o referencial dela: plano a 4 m no prédio cai em z 1,0 m
  numa peça 3 m acima, com o rumo de 45° aplicado.
- **Etapas de corte** nomeadas pelo usuário, gravadas no `projeto.json`, com
  aplicar, atualizar, renomear, excluir e navegação anterior/próxima.
- **Recorte por perímetro**: polígono desenhado no terreno, com área, perímetro
  e lista do que está dentro; exporta o pedaço da cena em **GLB, glTF+bin ou
  OBJ+MTL** por `POST /api/exportar` (`arcz_export.py` junta os documentos
  remapeando bufferViews, accessors, materiais, texturas e nós). Verificado de
  ponta a ponta com o modelo real: 124 MB de GLB, 14.159 malhas, 63 materiais,
  40 texturas — zero índice pendurado e toda imagem com magia válida.
- O relevo real entra no recorte quando pedido: malha gerada dos tiles
  Terrarium só dentro do perímetro. Medido num perímetro de 133 × 119 m:
  2.401 vértices, caixa ±59,5 m × ±66,5 m, 4 m de desnível, saída em Y para
  cima (padrão glTF).

### Entorno OSM no visualizador web (sem Ion)

- **Edifícios 3D locais sem depender do Cesium Ion.** O toggle antigo
  "Edifícios 3D OSM" chamava `Cesium.createOsmBuildingsAsync()` — tileset
  hospedado só no Ion; com o token zerado (`Cesium.Ion.defaultAccessToken =
  undefined`), a chamada falha com HTTP 401 e o `catch` só soltava um aviso no
  console, sem nada visível na tela. Substituído por um toggle real: a mesma
  pipeline `arcz-osm` (Overpass → recorte → adensamento → malha contra o DEM
  real) roda headless via `arcz-osm-cli` (binário novo, sem wgpu/winit/GPU),
  chamada por `servidor.py` (`/api/entorno-osm` + `/api/entorno-osm.glb`) e
  cacheada em disco de verdade — diferente da rota `/entorno.glb` do servidor
  Rust nativo, que regenerava da memória a cada chamada e perdia tudo ao
  reiniciar.
- Medido com o CLI isolado (raio de 250 m, adensamento ligado): 60 edificações
  geradas ao longo de 16 vias reais, 62 malhas, 858 triângulos, `.glb` de
  122.368 bytes. Pela rota do servidor: chamada fria ~8,7 s, chamada quente
  (cache já gravado) 0,03 s. Confirmado ao vivo no navegador: ligar o toggle
  soma uma primitiva à cena (3→4), desligar remove exatamente essa (4→3).
- Desligado por padrão — a imagem de satélite já mostra os prédios reais em
  foto, e caixas genéricas por cima às vezes pioram o resultado. Fica opt-in
  para quem quer massing 3D de verdade (sombra, obstrução, caminhada).

### Qualidade

- Clippy sem avisos no workspace.
- 476 testes.
- 50 testes Python (`python -m unittest discover -s tests_python`), sendo 14
  novos de exportação e 6 da rota `/api/exportar`.

## Limitações conhecidas

- O entorno OSM do visualizador web gera um único `.glb` por área (sem tiling
  3D Tiles), por isso o raio é limitado a presets fixos (100–250 m) em vez de
  livre. A persistência do campo `entorno_osm` no `projeto.json` não foi
  confirmada ao vivo nesta rodada — outra sessão ativa salvava o mesmo arquivo
  em paralelo e sobrescreveu o teste antes da confirmação; o mecanismo é o
  mesmo merge genérico já usado por `imagery`/`sombra`.
- A tampa do corte não sai em malha comprimida com Draco (não há decodificador
  no caminho puro do navegador). Contorno que não fecha só é costurado quando o
  vão é menor que 5 cm: fechar vão grande inventaria triângulo atravessando a
  sala (testado — enche a planta de blocos falsos). Superfície sem espessura
  (fachada de vidro de face única) não tem o que preencher: a seção ali é uma
  linha, não um anel.
- O recorte exporta modelos e relevo; edifícios OSM, imagem de satélite,
  animações e skins não entram. Peça arrastada de arquivo local (`blob:`) só
  existe na memória do navegador e é avisada como não exportável.

- O `project.sqlite` grava e é lido na abertura, mas o `.arcz` ainda é quem
  carrega a sessão. Os dois convivem nesta fase.
- Undo/redo opera em memória, não sobre o diário persistido.
- A vegetação tem o porte certo por espécie, mas ainda desenha um cone: os 61
  GLB CC0 já estão em `assets/vegetacao` e não são instanciados.
- O telhado de duas águas assenta sobre a caixa envolvente — exato para planta
  retangular, aproximado para contorno irregular do OSM.
- Nenhum componente do manifesto do ARCZ Earth (CesiumJS, Martin, PMTiles,
  osm2streets, COLMAP, Panoramax…) está no banco local nem integrado.
- A decisão entre wgpu nativo e CesiumJS como renderer principal continua
  aberta. Hoje existe um toggle para Cesium que carrega por CDN, o que
  contraria o offline-first e a proibição de dois renderers com estados
  separados.
