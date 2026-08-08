# Ledger integral de capacidades — Aedifex no ARCZ Earth V10

Este documento é a fonte humana de cobertura da conversão. A fonte executável é formada por:

- `integrations/aedifex/UPSTREAM_LOCK.json` — commit, pacotes, kinds e famílias obrigatórias;
- `integrations/aedifex/CONVERSION_COVERAGE.json` — política fail-closed aplicada ao inventário real;
- `arcz_server/aedifex_inventory.py` — enumeração de pacotes, apps, plugins, nodes, ferramentas MCP, rotas, variáveis, URLs e chamadas de rede;
- `integrations/aedifex/FEATURE_MATRIX_CONVERSION.json` — autoridade e estado de cada capacidade;
- `IMPLEMENTATION_STATUS.json` e `TASKS.json` — execução, bloqueios e caminho crítico.

Nenhuma lista manual permite declarar paridade. Quando o checkout pinado for materializado, o inventário do código deve ser regenerado; qualquer item não classificado fica bloqueado.

## 1. Snapshot auditado

| Campo | Valor |
|---|---|
| Repositório selecionado | `TangSY/aedifex` |
| Commit imutável | `5319368bae16500ca5267f6f8d68b36c9586d5bb` |
| Licença do código | MIT |
| Plugin API | v2 |
| Runtime upstream | Bun, TypeScript, React, Next, Three/R3F, Zustand/Zundo, Zod |
| Estado neste pacote | lock, auditoria, overlays, contratos e testes entregues; checkout/build integral ainda não materializado |

A ausência do checkout não é mascarada. `AedifexRegistry.status()` retorna bloqueadores e o Floorplanner não monta um editor falso.

<!-- GENERATED_MATRIX:START -->
## 1.1 Matriz canônica de conversão

`CONVERSION_MATRIX.json` é gerada do lock imutável e cobre exatamente 7 pacotes, 2 apps, 1 plugin first-party, 46 node kinds nativos, 3 kinds de extensão e 21 famílias MCP. Seu hash canônico atual é `4718a2cb82d5cd83c6eeb72bb1bbd014fac9fad6a8fa27d9c3a9b3a7f9b5aaa4`.

Cada entrada declara autoridade, caminho de integração, destino Rust futuro, política de perda, feature flag, testes e blockers. Qualquer símbolo obrigatório novo sem classificação faz `tools/build_aedifex_conversion_matrix.py` falhar. O inventário real do checkout pinado continua obrigatório antes de aprovar a conversão integral.
<!-- GENERATED_MATRIX:END -->

## 2. Decisão de arquitetura

### 2.1 Autoridade global do ARCZ

- WGS84, ECEF e ENU;
- Região Ativa, estado, cidade, bairro, endereço, lote e polígono manual;
- fontes geográficas locais, terreno, vias, entorno e perfis regionais;
- geração procedural, tiles, budget, jobs, provenance e cache;
- clima, Sol, atmosfera, globo, Street, cinema e render;
- biblioteca global de prompts, mídias e chat;
- publicação de derivados no globo.

### 2.2 Autoridade de autoria do Aedifex

- `SceneSnapshot` paramétrico revisionado;
- scene graph de site, building, levels e elementos;
- Floorplanner 2D e viewport 3D;
- seleção, snaps, inspector, materiais, histórico e plugins;
- ferramentas MCP/agent que leem e alteram o edifício;
- IFC para scene graph;
- exportações derivadas.

### 2.3 Regra de cópia única

Existe uma única cena editável por revisão. O GLB publicado em Cesium é:

- derivado;
- somente leitura;
- content-addressed;
- vinculado a `project_id`, `revision`, `scene_hash`, `GeoAnchor` e `generation_epoch`;
- incapaz de substituir o documento paramétrico.

A conversão para Rust não muda essa autoridade antes de passar por schema parity, loss report, golden geometry, undo/redo parity e migração reversível.

## 3. Pacotes obrigatórios

| Pacote | Papel no sistema final | Estado V10 |
|---|---|---|
| `@aedifex/core` | schemas, store, histórico, scene graph, geometria e serviços | preservação integral definida; build bloqueado |
| `@aedifex/viewer` | viewport Three/R3F/WebGPU, câmera, seleção e render de nodes | preservação integral definida; build/browser bloqueado |
| `@aedifex/editor` | Floorplanner, inspector, catálogo, ferramentas, AI UX e exportações | host ARCZ implementado; build/browser bloqueado |
| `@aedifex/nodes` | bundle nativo de todos os node kinds | carregamento por Plugin API v2 implementado; build bloqueado |
| `@aedifex/mcp` | SceneBridge headless, storage, tools e operações transacionais | ponte global implementada em fonte; runtime build bloqueado |
| `@aedifex/ifc-converter` | conversão IFC local para scene graph | painel/transação implementados em fonte; WASM/build bloqueados |
| `@aedifex/plugin-trees` | árvores/grama/flores procedurais e exemplo de plugin externo | admissão local definida; build/assets bloqueados |

Apps de demonstração upstream são preservados como referência, não montados como uma segunda aplicação concorrente.

## 4. Cobertura integral dos 46 node kinds

Todos permanecem no registry Aedifex até existir conversor Rust com paridade comprovada.

### Estrutura e hierarquia

`site`, `building`, `level`, `spawn`, `structural-grid`.

### Envoltória e arquitetura

`wall`, `fence`, `slab`, `ceiling`, `door`, `window`, `column`, `zone`.

### Coberturas e acessórios

`roof`, `roof-segment`, `box-vent`, `ridge-vent`, `turbine-vent`, `cupola`, `eyebrow-vent`, `chimney`, `solar-panel`, `skylight`, `dormer`, `gutter`, `downspout`.

### Circulação vertical

`stair`, `stair-segment`, `elevator`.

### Mobiliário e catálogo

`item`, `shelf`, `cabinet`, `cabinet-module`.

### Documentação, medição e contexto

`guide`, `scan`, `measurement`, `construction-dimension`.

### HVAC, linhas e hidráulica

`duct-segment`, `duct-fitting`, `duct-terminal`, `hvac-equipment`, `lineset`, `liquid-line`, `pipe-segment`, `pipe-fitting`, `pipe-trap`.

### Gate por kind

Cada kind só migra compute para Rust quando possuir:

1. schema de entrada e saída explícito;
2. preservação de IDs, parent/children, metadata e plugin payloads;
3. importação Aedifex → `CadDocument`;
4. exportação `CadDocument` → Aedifex;
5. loss report sem descarte silencioso;
6. corpus de cenas golden;
7. equivalência geométrica e semântica;
8. teste de undo/redo, clone, delete e serialization;
9. benchmark e orçamento;
10. feature flag e rollback.

## 5. Ferramentas MCP e agente

O catálogo global deve importar as ferramentas reais do MCP, sem reescrever uma lista manual. As famílias obrigatórias são:

- consulta de cena, node, descrição, busca e resumo por nível;
- medição, quantidades e verificação;
- construção, rooms e templates;
- patches atômicos;
- criação/duplicação de levels;
- criação/atualização de walls;
- placement e atualização de items;
- openings, cut-outs, doors e windows;
- zones;
- delete;
- undo/redo;
- export JSON/GLB;
- validação e colisões;
- scene lifecycle e persistência;
- variantes;
- photo-to-scene;
- ferramentas de visão quando o backend local estiver instalado.

### Política de execução

- leitura pode executar automaticamente;
- exportação, mutação e destruição exigem preview e aprovação explícita;
- `project_id` e `expected_revision` são controlados pelo host, não pelo modelo;
- preview e resultado possuem hash e audit trail;
- conflito de revisão bloqueia commit;
- nenhuma resposta textual pode afirmar execução sem tool result real.

### Preview visual

A V10 possui dry-run/diff real no chat global. O preview translúcido nativo do Aedifex (`applyGhostPreview`, confirmação e rejeição) foi identificado e deve ser ligado ao adaptador após o checkout pinado compilar. Ele permanece gate aberto; não é declarado equivalente apenas porque o diff existe.

## 6. Floorplanner modelando sobre a região

O modo Floorplanner preserva Cesium no mesmo workspace:

- globo permanece visível, real e navegável;
- editor Aedifex ocupa o painel autoral;
- split é redimensionável e persistido;
- botão enquadra a Região Ativa/lote;
- contexto territorial entra como camadas read-only e hash-verificadas;
- terreno, vias, entorno e limites não entram na cena editável por cópia acidental;
- norte, origem e offset pertencem ao `GeoAnchor`;
- cada revisão validada pode ser publicada automaticamente como GLB readonly;
- fechar o Floorplanner publica a revisão pendente antes de desmontar, quando habilitado.

### Política de eixos

```text
ARCZ ENU:   X=east, Y=north, Z=up
Aedifex:    X=east, Y=up,    Z=south
norte:      -Z no documento Aedifex
```

A transformação jamais é inferida a partir da câmera ou de uma rotação visual da planta.

## 7. UI global sem redundância

### Modos

- Globo;
- Floorplanner;
- Render;
- Walk.

### Painéis

Todos os painéis globais usam o mesmo contrato:

- recolhidos por padrão;
- rail sempre acessível;
- hover e focus abrem temporariamente;
- pin fixa;
- resize por ponteiro e teclado;
- largura/estado persistidos;
- Escape recolhe painel não fixado;
- conteúdo montado uma vez;
- teardown remove listeners/timers.

### Chat único

Há uma única superfície global de conversa e histórico. Ela reúne:

- ferramentas territoriais ARCZ;
- ferramentas MCP Aedifex;
- prompt/media/render/cinema/governança;
- contexto da revisão ativa;
- aprovações e tool runs.

A UI nativa `AIChatPanel` do upstream é preservada como fonte de recursos, mas não é montada em paralelo para evitar dois históricos e duas autoridades. Planejamento, templates, room analysis, proposals e Ghost preview devem ser expostos pelo adaptador global, não por um segundo chat.

## 8. Biblioteca de prompts e idiomas

Implementado em SQLite local:

- templates positivos e negativos;
- categorias/finalidades;
- variáveis obrigatórias;
- tags;
- idioma arbitrário BCP-47;
- histórico de versões;
- duplicação de built-ins;
- archive;
- import/export com hash;
- compilação determinística;
- enhancer local;
- tradução local;
- modelo/checksum/cache/provenance.

Modelo ausente retorna erro estruturado. Nenhum texto estático é apresentado como inferência.

## 9. Mídias de referência

O store aceita bytes reais e preserva:

- SHA-256 e tamanho;
- MIME e dimensões quando aplicável;
- nome original Unicode como metadado;
- papéis: architecture, geometry, facade, material, style, lighting, camera, landscape, people, vehicle, plan e outros admitidos pelo schema;
- peso e notas;
- licença e redistribuição;
- provenance;
- preview real de imagem, vídeo, áudio e PDF quando suportado;
- endpoint binário local;
- verificação de corrupção antes do uso.

URL remota não vira dependência persistente; o conteúdo precisa ser materializado e registrado.

## 10. Render fotorreal e imagem 8K

O workspace fotorreal congela:

- projeto/revisão/hash;
- GLB real publicado;
- câmera, lente, abertura, foco e exposição;
- resolução/formato/passes;
- prompt compilado e negative prompt;
- referências por hash e peso;
- engine/quality/samples;
- seed;
- modelo local;
- geometry guard;
- budget e destino.

### Pipeline estável

```text
Aedifex revision
→ GLB real
→ Blender/Cycles local
→ beauty + depth + normals + IDs + masks
→ validação de estrutura
→ difusão local opcional
→ upscale em tiles com overlap
→ validação pós-processo
→ manifest e checksums
```

A ausência de Blender, worker, modelo ou GLB final bloqueia o job. Nenhuma imagem vazia ou “sucesso” sintético é criada.

Filme por difusão permanece experimental até passar estabilidade temporal, geometry guard e rejeição automática de flicker/deriva.

## 11. Globo cinematográfico

A apresentação opera sobre o Cesium real:

- universo/estrelas;
- atmosfera e ground atmosphere;
- Sol, Lua, fog, brilho/saturação/hue controlados;
- nuvens do subsistema de ambiente;
- trajetória espaço → órbita → sítio;
- cancelamento e botão pular;
- `prefers-reduced-motion`;
- restauração dos controles;
- navegação normal depois da abertura.

`Camera.flyTo()` é convertido para Promise por callbacks `complete/cancel`; não existe `await` falso. Destino inválido não move a câmera.

## 12. IFC, importações e saídas

### IFC

- conversão local por `@aedifex/ifc-converter` e `web-ifc`;
- WASM copiado localmente no build;
- importação transacional;
- rollback para a cena anterior em erro;
- save com revisão esperada;
- sem serviço remoto.

### Saídas preservadas/planejadas

- scene JSON revisionado;
- GLB e JSON;
- IFC import e futuro round-trip com loss report;
- floorplan vetorial/PDF do upstream;
- PNG/EXR/JPG;
- passes técnicos;
- plantas, cortes, elevações, schedules e pranchas;
- câmera/timeline JSON;
- pacote de diagnóstico.

## 13. Comunidade e outros repositórios

- forks são comparados por commit/diff, licença e ativos;
- nenhum fork substitui automaticamente o upstream pinado;
- código útil entra por patch isolado ou plugin;
- assets exigem licença e provenance separadas;
- plugins precisam passar serialization, cleanup, determinism, network e rollback;
- forks obsoletos/divergentes permanecem referência.

O fork `pablogventura/aedifex` contém uma alteração de cenas/tema de sítio que pode ser estudada, mas nenhum asset é admitido antes da auditoria de licença. Demais forks observados não são mesclados cegamente.

## 14. Estado objetivo da V10

### Implementado e testado neste pacote

- contratos de autoridade e coordenadas;
- store de projetos/revisões/eventos;
- bridge de contexto;
- host split Globo + Floorplanner;
- publicação automática/manual de revisão;
- export/ingest GLB seguro em fonte;
- prompt library versionada;
- mídia binária/content-addressed;
- chat único com tool runs, preview, aprovação/rejeição e revision guard;
- overlay Aedifex, IFC e catálogo dinâmico em fonte;
- preflight fotorreal honesto;
- painéis collapsed/hover/pin;
- abertura cinematográfica corrigida;
- inventário/coverage fail-closed;
- testes Python/JavaScript e schemas.

### Bloqueios que impedem “conversão integral validada”

- checkout completo do commit pinado ausente;
- Bun/dependências/build upstream não executados;
- bundle CesiumJS local ausente;
- cargo/rustc ausentes;
- Blender/Cycles ausente;
- modelos locais ausentes;
- E2E browser e visual não executados;
- ghost preview nativo global ainda não ligado/validado;
- IFC/GLB round-trip real sem corpus;
- conversores Rust por 46 kinds ainda não têm paridade;
- instalador/soak Windows final pendentes.

Até esses gates passarem, o status correto é implementação de integração com bloqueios reais, não release integral.
