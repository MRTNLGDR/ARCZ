# Plano executável de conversão massiva Aedifex → ARCZ Earth

## Objetivo

Converter e integrar o Aedifex integralmente sem perder funções, sem criar uma segunda cena editável e sem substituir a robustez existente por uma reescrita prematura. ARCZ permanece o domínio geoespacial; Aedifex permanece o kernel autoral até que cada parte do compute tenha paridade Rust comprovada.

## Princípios

1. upstream fixado e imutável;
2. inventário gerado do código, não checklist manual;
3. cobertura fail-closed: item desconhecido bloqueia;
4. uma única autoridade editável por revisão;
5. GLB é derivado, nunca documento;
6. migração incremental, feature-flagged e reversível;
7. nenhuma dependência remota no core;
8. IA local, opcional e auditável;
9. cada onda fecha frontend, backend, storage, segurança, testes e docs;
10. build ausente permanece `BLOCKED`.

<!-- GENERATED_MATRIX:START -->
## Fonte única de cobertura gerada

A conversão não depende de listas manuais paralelas. A fonte executável é:

- `integrations/aedifex/UPSTREAM_LOCK.json` — commit, versões e superfície obrigatória;
- `tools/build_aedifex_conversion_matrix.py` — gerador fail-closed;
- `integrations/aedifex/CONVERSION_MATRIX.json` — matriz canônica;
- `schemas/aedifex-conversion-matrix.schema.json` — contrato validável.

Cobertura atual da matriz:

| Superfície | Quantidade |
|---|---:|
| Pacotes pinados | 7 |
| Apps upstream | 2 |
| Plugins first-party | 1 |
| Node kinds nativos | 46 |
| Node kinds de extensão | 3 |
| Famílias MCP | 21 |
| Módulos globais ARCZ | 7 |
| Fontes comunitárias auditadas | 5 |

Hash canônico da matriz: `4718a2cb82d5cd83c6eeb72bb1bbd014fac9fad6a8fa27d9c3a9b3a7f9b5aaa4`.

O gerador encerra com erro quando surge pacote, node kind ou família MCP sem política. A matriz classifica autoridade, destino, integração, loss policy, feature flag, testes e blockers de cada item. **Ela prova cobertura do lock; não substitui o inventário do checkout, o build upstream, os testes nativos, o E2E ou a paridade Rust.**
<!-- GENERATED_MATRIX:END -->

## Onda 0 — custódia, inventário e reprodutibilidade

### Execução

- materializar `TangSY/aedifex@5319368…` em `opensources/upstream/aedifex`;
- verificar commit e licença;
- calcular SHA-256 de todos os arquivos;
- preservar original read-only;
- regenerar inventário com `arcz_server/aedifex_inventory.py`;
- classificar pacotes, apps, plugins, 46 node kinds, ferramentas MCP, rotas, env vars, URLs e network call sites;
- aplicar `CONVERSION_COVERAGE.json` e rejeitar `UNMAPPED/REVIEW_REQUIRED/BLOCKED`;
- criar fork de trabalho por overlay, nunca editar o original;
- gerar SBOM/notices;
- instalar Bun/dependências a partir de cache/vendor local;
- buildar e executar suíte upstream sem rede.

### Gate

- commit exato;
- inventário integral;
- cobertura sem lacunas;
- build e testes upstream aprovados;
- nenhuma rota/provider remoto ativa em `offline_strict`;
- golden scenes exportadas.

**Estado:** tooling/lock/coverage entregues; materialização/build bloqueados pelo ambiente.

## Onda 1 — autoridade e modelo de dados comum

### Execução

- confirmar `SceneSnapshot` Aedifex como documento autoral;
- manter `FloorplannerStore` revisionado em SQLite WAL;
- exigir `expected_revision` em toda mutação;
- formalizar `ModelingContextPackage`, `GeoAnchor`, `ContextLayer`, `FloorplannerDerivative` e `PrimaryModel`;
- migrar `projeto.json` idempotentemente;
- definir e testar eixo Aedifex ↔ ENU;
- propagar `generation_epoch`;
- invalidar resultados atrasados;
- manter tombstones/overrides/locks;
- adicionar backup/restore e pacote diagnóstico.

### Gate

- conflito visível, sem last-write-wins;
- save/reopen idempotente;
- hash e provenance em toda revisão;
- uma única cópia editável;
- projeto antigo preservado.

**Estado:** implementado e testado em Python/HTTP; corpus histórico real pendente.

## Onda 2 — Floorplanner modelando sobre o globo

### Execução

- manter Cesium montado e interativo no modo Floorplanner;
- host split redimensionável e persistido;
- enquadrar Região Ativa/lote;
- carregar contexto local read-only e hash-verificado no Aedifex;
- importar terreno/limites/vias sem torná-los editáveis por acidente;
- publicar revisão atual automaticamente ou manualmente;
- exportar a cena R3F real por `GLTFExporter`;
- validar GLB binário, hash, revisão e manifest semântico;
- stage/commit/rollback do derivado em Cesium;
- remover corretamente viewer/listeners/processos no lifecycle;
- permitir voltar ao Floorplanner pela revisão paramétrica.

### Gate

- Região → editor → save → GLB → Cesium em três lotes reais;
- norte/altitude/escala corretos;
- 100 ciclos sem leak;
- publicação atrasada não substitui latest;
- gizmo não edita derivado.

**Estado:** código/contratos/testes estáticos entregues; Cesium/Aedifex reais bloqueados.

## Onda 3 — paridade funcional integral do editor

### Cobertura obrigatória

- 46 node kinds do lock;
- Floorplanner 2D, viewport 3D e first-person;
- scene tree, selection, multi-selection, inspector e snaps;
- walls, openings, rooms/zones, levels, slabs, ceilings, roofs;
- stairs, elevators, structure, dimensions e grids;
- terrain sculpt;
- materials/painting;
- catalog/placement/cabinets/items;
- MEP/HVAC/plumbing;
- undo/redo, variants, lifecycle, export e validation;
- IFC;
- plugins e tree plugin;
- AI proposals, planner, templates, room analysis e ghost preview;
- MCP tools e storage/live events admitidos pela cobertura.

### Método

- carregar todos os kinds por registry dinâmico;
- não esconder painel/ferramenta por hardcode ARCZ;
- gerar testes de smoke por kind e tool;
- registrar capabilities e licença por plugin;
- preservar payload desconhecido;
- comparar UI com upstream golden screenshots.

### Gate

- matriz 100% classificada;
- cada node kind cria/edita/salva/reabre/exporta;
- cada tool family possui chamada de contrato;
- plugins carregam e limpam;
- zero botão decorativo.

**Estado:** overlay dinâmico e ledger entregues; build/browser bloqueados.

## Onda 4 — chat único, prompts, idiomas e referências

### Execução

- uma única timeline de conversa;
- catálogo global = ferramentas ARCZ + MCP Aedifex;
- tool calls com request hash, preview hash, approval ID, revision guard e result hash;
- leitura automática opcional;
- mutação/export/destruição com aprovação;
- ligar dry-run MCP ao ghost preview nativo Aedifex;
- confirmar/rejeitar/undo pelo mesmo histórico;
- preservar planner, proposals, templates e room analyzer;
- biblioteca SQLite de prompts/versionamento/tags/idiomas;
- enhancer/translator pelo Local AI Broker;
- mídia real com roles, weights, notes, preview, licença e provenance;
- anexar imagens, vídeo, áudio, PDF, plantas e referências à cena/render/chat;
- nenhuma URL remota persistente.

### Gate

- nenhum segundo `AIChatPanel` montado;
- preview visual e diff correspondem;
- conflito de revisão bloqueia commit;
- modelo ausente falha honestamente;
- versões/import/export de prompts passam tamper guard;
- bytes de referência são relidos e verificados.

**Estado:** chat/tool runs, prompts e mídia implementados; ghost visual nativo bloqueado pelo build upstream.

## Onda 5 — schema parity e conversão Rust por kind

### Ordem

1. site/building/level;
2. wall/fence/openings;
3. zone/slab/ceiling;
4. roof/segments/accessories;
5. stair/elevator;
6. materials/items/cabinets;
7. guides/scans/measurements/grids;
8. HVAC/MEP/plumbing;
9. plugins e payloads desconhecidos.

### Para cada kind

- schema mapping;
- parser/serializer;
- IDs/relações;
- unidades/eixos;
- materials/UV;
- semantic metadata;
- import/export loss report;
- golden JSON/mesh;
- property tests/fuzzing;
- round-trip A→R→A;
- benchmark;
- feature flag;
- rollback.

### Gate

Nenhum kind muda de autoridade antes de `loss=0` para campos suportados e equivalência aceita no corpus.

**Estado:** crates de destino e contratos existem; paridade dos 46 kinds e compilação Rust pendentes.

## Onda 6 — mover compute puro para Rust

### Candidatos

- spatial index;
- intersections/snaps;
- wall joins e footprints;
- polygon Boolean/triangulation;
- room detection e quantities;
- roof solving;
- terrain support;
- collision/validation;
- IFC mapping;
- mesh/LOD/atlas;
- deterministic replay.

### Não mover prematuramente

- React UX;
- plugin UI;
- scene selection/inspector;
- agent proposal UX;
- componentes que dependem do registry sem ABI estável.

### Gate

- output parity;
- undo/redo parity;
- determinismo;
- benchmark melhor ou justificativa funcional;
- fallback para TS por feature flag durante duas versões.

## Onda 7 — host único Tauri/React

### Execução

- criar workspace desktop comum;
- importar pacotes Aedifex pinados como dependências internas;
- remover iframe/sidecar depois do E2E;
- manter API loopback para workers/MCP quando necessário;
- bridge torna-se adapter in-process;
- lifecycle, atalhos, focus, drag/drop e accessibility unificados;
- CSP e filesystem capabilities mínimos;
- instalador e update/rollback local.

### Gate

- nenhum `postMessage` ou segundo processo de UI;
- uma árvore React e um design system;
- crash boundary isolado;
- projeto abre mesmo com plugins opcionais desligados;
- Windows clean install aprovado.

## Onda 8 — render fotorreal, 8K e cinema

### Execução

- Blender/Cycles local;
- câmera/lente/DOF/exposição físicas;
- beauty/depth/normals/object/material/semantic/sky masks;
- EXR/PNG/JPG;
- render em tiles/checkpoint/resume;
- Local AI diffusion/upscale condicionado;
- geometry guard e borda estrutural;
- referências ponderadas;
- LUT/grain/fog/weather;
- cinema da timeline e motion blur por subframes;
- estabilidade temporal antes de declarar vídeo estável.

### Gate

- 8K real com manifest/checksum;
- arquitetura preservada;
- reexecução reproduzível;
- falha/restart retoma frames;
- VRAM/RAM dentro de budget;
- vídeo sem flicker além do limiar aceito.

**Estado:** preflight/worker/contratos entregues; Blender/modelos/hardware bloqueados.

## Onda 9 — documentação, pranchas e release

- plantas, cortes, elevações e schedules;
- IFC round-trip/loss reports;
- estudo solar e pranchas;
- tutorial contextual e ajuda por ferramenta;
- design tokens, atalhos e acessibilidade;
- governança dentro do app;
- SBOM, notices e attribution;
- backup/restore/export;
- Setup.exe/run/stop/uninstall;
- smoke offline/firewall/soak;
- ZIP, SHA-256 e relatório final.

## Matriz de gates transversais

Toda onda exige:

- schema/version/migration;
- unidade, integração e E2E aplicáveis;
- caminho negativo;
- cancelamento;
- rollback;
- cleanup/leak;
- segurança/path/body/network;
- observabilidade;
- documentação sincronizada;
- tarefa/evidência/governança;
- sem mock permanente;
- sem declaração não provada.

## Ordem imediata

1. materializar/buildar upstream pinado;
2. vendor CesiumJS local;
3. executar E2E split Floorplanner/globo;
4. ligar ghost preview nativo ao chat global;
5. testar todos os 46 kinds e tool families;
6. instalar Rust e fechar workspace;
7. iniciar parity converters por dependência;
8. instalar Blender/modelos e gerar render real;
9. migrar para host Tauri/React único;
10. executar release gates no Alienware/Windows.
