# Cobertura funcional Aedifex → ARCZ Earth V10

Legenda:

- **PRESERVADO** — permanece integral no kernel Aedifex pinado;
- **INTEGRADO EM FONTE** — bridge/overlay/backend implementado, mas build real ainda precisa ser executado;
- **VERIFICADO** — teste automatizado foi realmente executado;
- **BLOQUEADO** — pré-condição objetiva ausente;
- **RUST PARITY** — destino de compute, não autoridade atual.

<!-- GENERATED_MATRIX:START -->
## Matriz canônica

A garantia fail-closed é `integrations/aedifex/CONVERSION_MATRIX.json`, gerada por `tools/build_aedifex_conversion_matrix.py` a partir do lock. Ela cobre 46 kinds nativos, 3 extensões e 21 famílias MCP, além dos pacotes, apps, módulos globais e fontes comunitárias. Hash atual: `4718a2cb82d5cd83c6eeb72bb1bbd014fac9fad6a8fa27d9c3a9b3a7f9b5aaa4`.

`CONVERSION_COVERAGE.json` continua responsável por confrontar o inventário do checkout materializado. Nenhuma tabela editorial pode aprovar um item ausente dessas duas fontes.
<!-- GENERATED_MATRIX:END -->

## Autoria arquitetônica

| Domínio | Autoridade | Estado V10 |
|---|---|---|
| Site, building e levels | Aedifex | PRESERVADO; revisão ARCZ VERIFICADA |
| Walls, fences, slabs e ceilings | Aedifex | PRESERVADO; overlay dinâmico EM FONTE |
| Doors, windows e openings | Aedifex | PRESERVADO |
| Zones/rooms/quantities | Aedifex | PRESERVADO |
| Roofs, segments e acessórios | Aedifex | PRESERVADO; RUST PARITY futura |
| Stairs e elevators | Aedifex | PRESERVADO |
| Cabinets, shelves, items e catálogo | Aedifex | PRESERVADO |
| Columns, grids, guides, scans e dimensions | Aedifex | PRESERVADO |
| HVAC, ducts, linesets e plumbing | Aedifex | PRESERVADO |
| Terrain sculpt do site | Aedifex + ARCZ context | ponte read-only/hash EM FONTE |
| Materials e painting | Aedifex | PRESERVADO; provenance ARCZ |
| Selection, inspector, snaps e undo/redo | Aedifex | PRESERVADO |
| Plugins/trees | Aedifex Plugin API v2 | admissão/duplicate guard EM FONTE |
| IFC | Aedifex converter | import transacional EM FONTE; build/WASM BLOQUEADO |

## Território e mundo

| Domínio | Autoridade | Estado V10 |
|---|---|---|
| WGS84/ECEF/ENU | ARCZ | VERIFICADO por contrato/testes |
| Região/lote | ARCZ | VERIFICADO |
| Pacotes DEM/OSM/Overture | ARCZ | local-first; importadores reais ainda pendentes |
| Entorno procedural regional | ARCZ | contratos/crates; Rust build BLOQUEADO |
| Globo e navegação | ARCZ/Cesium | código implementado; vendor/browser BLOQUEADOS |
| Atmosfera, Sol, Lua, nuvens e fog | ARCZ | apresentação implementada; visual BLOQUEADO |
| Street-level | ARCZ | contratos locais; catálogo/WebGL BLOQUEADOS |
| Cinema/timeline | ARCZ | matemática/testes VERIFICADOS; captura real BLOQUEADA |

## Floorplanner sobre o globo

- Cesium permanece visível e navegável;
- split resize/persistência;
- foco na região;
- contexto read-only;
- revisão única;
- publicação automática/manual;
- GLB derivado readonly;
- north/height/axis por `GeoAnchor`.

Código implementado e testes JS aprovados. E2E real depende de vendor Cesium e build Aedifex.

## IA, chat, prompts e mídia

| Domínio | Estado V10 |
|---|---|
| Chat global único | implementado/persistido/testado |
| Catálogo de tools ARCZ + Aedifex | implementado em fonte; Aedifex runtime bloqueado |
| Dry-run/diff/approval/reject/revision guard | implementado/testado |
| Ghost preview translúcido Aedifex | identificado; integração visual BLOQUEADA |
| Agent planner/templates/proposals/room analyzer | preservados upstream; exposição global pendente do build |
| Local AI Broker | implementado/testado sem modelos |
| Prompt library/version/history/import/export | implementado/testado |
| Enhance/translate multilíngue | contrato local; pesos BLOQUEADOS |
| Referências image/video/audio/PDF | store/metadata/preview implementados/testados |

## Saídas

| Saída | Estado V10 |
|---|---|
| Scene revision Aedifex | VERIFICADO Python/HTTP |
| GLB binário validado | VERIFICADO no gateway com fixture real mínima; viewport real BLOQUEADO |
| Derivado georreferenciado | código/teste estático; Cesium real BLOQUEADO |
| Floorplan vector/PDF | PRESERVADO upstream; build BLOQUEADO |
| IFC import | integração em fonte; WASM/build BLOQUEADOS |
| Blender/Cycles | worker/preflight em fonte; executável BLOQUEADO |
| Difusão/upscale 8K | contratos/broker; modelos BLOQUEADOS |
| Filme fotorreal | experimental até gate temporal |
| Plantas/cortes/elevações/pranchas | base existente; integração completa pendente |

## Garantia de não esquecimento

`aedifex_inventory.py` enumera o checkout materializado. `CONVERSION_COVERAGE.json` bloqueia qualquer pacote, app, plugin, kind, tool, route, env, URL ou network call não classificado. A paridade não pode ser aprovada apenas por esta tabela.
