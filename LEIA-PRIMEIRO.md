# Leia primeiro — ARCZ Earth + Aedifex Global V10.1

Esta árvore é uma integração cumulativa, não um clone visual nem uma coleção de placeholders.

## Autoridades

- **ARCZ:** mundo, coordenadas, região/lote, contexto, procedural, jobs, cinema, Street, prompts, mídia, chat, render e publicação.
- **Aedifex:** documento paramétrico e todas as ferramentas de autoria do edifício.
- **SQLite:** revisões, eventos, chats, tool runs, prompts e catálogos.
- **GLB:** derivado readonly; nunca documento editável.
- **Rust CAD/BIM:** destino do compute após paridade, não autoridade atual.

## Estado da hospedagem

A ponte transitória usa sidecar Next/Bun local e iframe sandboxed porque o frontend ARCZ atual é ES Modules sem bundler. No modo Floorplanner, Cesium não é destruído: permanece visível e navegável ao lado do editor. O destino é um único host Tauri/React.

É proibido:

- iframe remoto;
- `postMessage` sem origem, janela e canal verificados;
- duas cenas editáveis;
- GLB como save;
- IA externa obrigatória;
- esconder ferramenta Aedifex sem registro na matriz;
- converter kind para Rust sem loss/golden/parity;
- declarar build/runtime/teste não executado.

## Implementado

- Região/lote → contexto/anchor;
- projeto/revisão/conflito/SSE;
- split Globo + Floorplanner;
- contexto read-only e export GLB real em fonte;
- publicação de derivado;
- chat único com ferramentas e aprovação;
- prompts versionados/multilíngues;
- referências reais;
- IFC transacional em fonte;
- preflight/render local;
- painéis colapsados/hover/pin;
- globo cinematográfico;
- inventário e coverage fail-closed.

## Não declarado como concluído

- checkout/build integral do Aedifex;
- CesiumJS local e E2E browser;
- ghost preview visual global;
- cargo fmt/check/test;
- paridade Rust dos 46 kinds;
- Blender/Cycles e render real;
- modelos locais;
- execução limpa dos inicializadores em PowerShell/Windows;
- validação Docker Compose;
- Setup.exe/MSI/AppImage/DEB e soak Windows.

## Ordem de leitura

1. `AGENTS.md`
2. `docs/integration/AEDIFEX_CAPABILITY_LEDGER_V10.md`
3. `docs/integration/AEDIFEX_DECISION_RECORD.md`
4. `integrations/aedifex/CONVERSION_MATRIX.json`
5. `docs/integration/CONVERSION_MATRIX_GUIDE.md`
6. `docs/integration/USER_REQUIREMENT_TRACEABILITY_V10.md`
7. `docs/integration/MASS_CONVERSION_EXECUTION_PLAN.md`
8. `IMPLEMENTATION_STATUS.json`
9. `TASKS.json`
10. `docs/audit/VALIDATION_REPORT.md`
