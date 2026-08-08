# Matriz plano → código — ARCZ Earth + Aedifex Global V10

Leia com `IMPLEMENTATION_STATUS.json`; arquivo existente não significa runtime
aprovado.

## Fundação local-first

| Requisito | Código | Gate |
|---|---|---|
| autosave/atomicidade/migração | `estado.js`, `atomic_io.py`, `project_migrations.py` | Python aprovado; corpus real pendente |
| rede negada por padrão | `network_policy.py`, `network-mode.js`, guards de ambiente | testes/static aprovados; firewall alvo pendente |
| jobs/cancel/recovery | `jobs.py`, SSE/client | testes + stress; worker real pendente |
| source packages/hash/licença | `source_registry.py`, schemas | testes aprovados |
| Região Ativa/ENU/perfis | `region_service.py`, `geo_model_bridge.py`, `app/region` | contratos aprovados; dados reais dependem de pacotes |
| tiles/budget/plugins | services + crates + ES modules | Python/JS; Rust/Cesium gates abertos |

## Aedifex e Floorplanner

| Requisito | Código | Gate |
|---|---|---|
| pin/upstream imutável | `UPSTREAM_LOCK.json`, vendor tool | upstream ausente |
| overlay ARCZ | `integrations/aedifex/overlay` | TS syntax; build Bun ausente |
| contexto da região/lote | `modeling-context.js`, `GeoAnchor` schemas | testes aprovados |
| projetos/revisões/SSE | `floorplanner_store.py`, client | Python/HTTP aprovados |
| IA local | overlay route → Local AI Broker | contrato aprovado; modelos ausentes |
| chat Aedifex + global | combined panels + tool catalog | testes JS/Python; inferência real ausente |
| GLB real | `arcz-scene-export-bridge.tsx` | fonte/static; R3F real bloqueado |
| ingestão GLB | upload binary + validation/content store | Python/HTTP aprovado |
| publicação no globo | `floorplanner-host.js`, `cena.js` | matemática/static; Cesium real bloqueado |
| CAD/BIM Rust | `arcz-cad`, `arcz-bim`, `arcz-aedifex` | fonte; cargo ausente |

## UX global

| Requisito | Código | Gate |
|---|---|---|
| Globo/Floorplanner/Render/Walk | `fusion-shell.js` | JS; smoke visual bloqueado |
| collapsed/hover/pin | panel dock + Aedifex behavior | JS/TS; acessibilidade visual pendente |
| globo cinematográfico | `cinematic-globe.js`, `ambiente.js` | syntax/contracts; Cesium visual pendente |
| Street | panorama catalog/viewer/sequence | contratos; mídia/WebGL reais pendentes |
| cinema | timeline/tracks/quick starts | matemática JS; frame renderer pendente |

## Conteúdo e render

| Requisito | Código | Gate |
|---|---|---|
| mídia reference | content-addressed store/panel | Python aprovado |
| prompt library | SQLite/client/panel | Python/JS; models para enhance/translate ausentes |
| preflight/jobs fotorreais | `photoreal.py`, workspace | testes aprovados |
| Blender/Cycles | worker scripts/manifest | executável ausente |
| diffusion/upscale 8K | broker/contracts/pass plan | modelos/worker ausentes |
| pranchas | SVG/floorplan upstream | parcial; build/vistas/PDF final pendentes |

## Release

A release permanece bloqueada enquanto Aedifex, Cesium, Rust, Blender e modelos
locais não forem materializados e os E2E/soak/offline gates não passarem.

## Instalação e distribuição

| Requisito | Código | Gate |
|---|---|---|
| preflight fail-closed | `tools/runtime_preflight.py` | Python aprovado |
| Windows portátil | `install.ps1`, `run.bat`, `stop.bat`, `uninstall.ps1` | PowerShell/clean machine bloqueados |
| Linux portátil | `install.sh`, `run.sh`, `stop.sh`, `uninstall.sh` | sintaxe Bash aprovada; E2E pendente |
| Docker alternativo | `Dockerfile`, `docker-compose.yml` | Docker ausente; runtime bloqueado |
| modelos instaláveis | `ai-model-manifest.schema.json` | nomes de tarefas validados; pesos ausentes |
