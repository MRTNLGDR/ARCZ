# Rastreabilidade do pedido — ARCZ Earth + Aedifex Global V10.1

| Requisito | Decisão/implementação | Evidência | Estado objetivo |
|---|---|---|---|
| Selecionar estado/cidade/bairro/endereço/lote/polígono | Região Ativa ARCZ gera `ModelingContextPackage` e `GeoAnchor` | `app/region`, `arcz_server/floorplanner_store.py`, schemas | VERIFICADO por contratos; dados reais dependem de pacotes locais |
| Modelar diretamente sobre a região do globo | split Cesium + Floorplanner, contexto territorial readonly e publicação por revisão | `app/floorplanner`, overlay `arcz-floorplanner` | fonte/testes aprovados; browser real BLOQUEADO |
| Escolher entre substituir core ou integrar globalmente | Aedifex = Building Authoring Kernel; ARCZ = World Core | `AEDIFEX_DECISION_RECORD.md` | DECIDIDO e aplicado |
| Preservar tudo do Aedifex sem redundância | 46 kinds nativos + 3 extensões + 21 famílias MCP em matriz fail-closed | `CONVERSION_MATRIX.json` | cobertura do lock VERIFICADA; build upstream BLOQUEADO |
| Incorporar contribuições first-party/comunidade | nodes, MCP, trees e IFC mapeados; forks auditados sem blind merge | `COMMUNITY_SOURCES.json`, `AUTHOR_REPOSITORY_AUDIT.json` | auditado; patches externos não admitidos sem licença/diff |
| Um único chat global | histórico ARCZ único; catálogo MCP dinâmico; nenhum segundo `AIChatPanel` | `app/chat`, `chat_workspace.py`, overlay | backend/JS VERIFICADOS; runtime MCP upstream BLOQUEADO |
| Tudo do agente Aedifex | leitura real; mutação em cópia; diff; aprovação; revision guard; rollback | `arcz-aedifex-tools`, tool-run APIs | fonte/testes aprovados; ghost/planner/room analysis E2E BLOQUEADO |
| Biblioteca de prompts | versões, slug, compile, bundles com hash, import/export/conflitos | `prompt_library.py`, `app/prompts` | VERIFICADO |
| Enhancer, tradução e idiomas | Local AI Broker, manifests e erro explícito sem pesos | `ai_broker.py`, model schemas | contrato VERIFICADO; modelos BLOQUEADOS |
| Mídias de referência | image/video/audio/PDF/HDR/BIM/CAD/geodados/nuvem de pontos por hash/licença | `reference_media.py`, `app/media` | VERIFICADO; previews dependem do host final |
| Imagens ultra photoreal/8K | GLB Aedifex real → Blender/Cycles → passes → difusão/upscale local opcional | `photoreal.py`, worker Blender | source/preflight VERIFICADOS; render real BLOQUEADO |
| Não alterar arquitetura no enhancement | depth/normal/object/material masks + geometry guard | schemas/workers/render | contrato pronto; modelos/runtime BLOQUEADOS |
| Painéis collapsed/hover/pin | tabs ARIA, teclado, touch, foco, pin, resize, persistência | `collapsible-panel-dock.js` | 41 testes JS; visual/axe BLOQUEADO |
| Globo inicial cinematográfico e navegável | atmosfera, Sol, nuvens locais, intro cancelável, `flyTo` por callback e restore | `cinematic-globe.js`, `ambiente.js` | unit/sintaxe VERIFICADOS; Cesium real BLOQUEADO |
| Tudo local, providers opcionais | `offline_strict`, loopback, pacotes materializados, broker local | network policy, registries, verifier | gates estáticos aprovados; firewall Windows final BLOQUEADO |
| Sem mocks/simulações | ausência de runtime gera erro/blocker; high/ultra não usa reconstrução parcial | `NO_MOCK_POLICY.md`, verifier | gate `no_mock_no_stub_policy` APROVADO |
| Entrega para outra IA | AGENTS, ledger, matriz, tasks, status, reports, checksums | raiz do pacote | ENTREGUE; blockers preservados |

A palavra “integral” só poderá mudar para `APROVADO` depois de materializar o checkout pinado, executar build/testes upstream, E2E WebGPU/Cesium, todos os kinds/tools, IFC, Blender, modelos, Rust e aceite Windows.
