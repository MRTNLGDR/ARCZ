# ADR-010 — Aedifex como Building Authoring Kernel do ARCZ Earth

**Status:** aceito para V10; release ainda bloqueado.  
**Upstream:** `TangSY/aedifex` no commit do `UPSTREAM_LOCK.json`.  
**Decisão:** integração global por contratos agora; migração seletiva de compute para Rust depois de paridade.

## Alternativas analisadas

1. transformar Aedifex no core global;
2. integrar apenas por GLB;
3. reescrever tudo em Rust imediatamente;
4. executar editor remoto;
5. manter ARCZ e Aedifex como aplicações independentes;
6. kernel autoral integrado a um World Core ARCZ.

A alternativa 6 foi escolhida.

## Justificativa

- Aedifex já possui scene graph, Floorplanner, nodes, editor, plugins, MCP, IFC e UX especializada;
- ARCZ já possui georreferência, região, terreno, procedural, cinema, Street, render, jobs e provenance;
- substituir um pelo outro criaria regressões;
- GLB não preserva semântica paramétrica;
- big-bang Rust elimina paridade verificável;
- integração por autoridade separada permite evolução reversível.

## Autoridades

### ARCZ

Território, WGS84/ECEF/ENU, Região Ativa, lote, contexto, fontes, terreno/entorno, procedural, budgets/jobs, clima, globo, Street, cinema, prompts, mídia, chat, render e publicação.

### Aedifex

Documento paramétrico, Floorplanner, viewport, todos os node kinds, inspector, selection/snaps, materiais, catálogo, histórico, plugins, IFC e ferramentas MCP de edifício.

### Derivado

GLB em Cesium é somente leitura e sempre vinculado à revisão/hash/anchor. Não recebe edição nem substitui o documento.

## Host

### Compatibilidade V10

O ARCZ atual continua ES Modules sem bundler. O Aedifex é React/Next/R3F. A ponte transitória usa sidecar loopback sandboxed, origem/janela/canal validados e HTTP/SSE local. O modo Floorplanner mantém Cesium visível ao lado do editor.

### Destino

Shell Tauri/React único, pacotes Aedifex internos, bridge in-process e APIs locais somente para workers/MCP. O iframe só é removido após build e lifecycle E2E.

## Chat

Existe um único chat global. Ele importa as ferramentas MCP do Aedifex e as ferramentas ARCZ. Não monta dois históricos. Mutações usam preview, aprovação, revisão esperada e tool result auditável. Ghost preview visual nativo precisa ser ligado após o build upstream.

## Rust

`arcz-cad`, `arcz-bim` e `arcz-aedifex` são destino de compute. Nenhum kind muda de autoridade sem schema parity, round-trip, loss report, golden geometry, undo/redo parity, benchmark e rollback.

<!-- GENERATED_MATRIX:START -->
## Mecanismo de execução da decisão

A decisão é aplicada por uma matriz gerada e fail-closed, não por convenção informal. `UPSTREAM_LOCK.json` fixa o upstream; `CONVERSION_MATRIX.json` classifica toda a superfície obrigatória; `CONVERSION_COVERAGE.json` confronta o checkout materializado. A matriz atual contém 46 node kinds nativos e 21 famílias MCP, com hash `4718a2cb82d5cd83c6eeb72bb1bbd014fac9fad6a8fa27d9c3a9b3a7f9b5aaa4`.

Nenhum componente migra de autoridade apenas porque existe um conversor. A troca Aedifex → Rust exige golden corpus, loss report vazio ou aceito, round-trip, feature flag, rollback e prova de equivalência.
<!-- GENERATED_MATRIX:END -->

## Consequências

- máxima reutilização open source sem blind merge;
- ausência de ferramentas duplicadas;
- escala planetária separada da precisão do edifício;
- upgrades upstream auditáveis;
- bloqueio honesto quando vendors/toolchains faltam;
- mais trabalho inicial em contratos e gates, mas menor risco de perda.
