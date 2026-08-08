# Roadmap ARCZ Earth + Aedifex Global V10

`TASKS.json` é a fonte executável.

## Entregue nesta sessão

- matriz canônica gerada e fail-closed para 7 pacotes, 46 kinds nativos, 3 extensões e 21 famílias MCP;
- hash `4718a2cb82d5cd83c6eeb72bb1bbd014fac9fad6a8fa27d9c3a9b3a7f9b5aaa4` verificado automaticamente;
- chat MCP com leitura real e mutação por preview/aprovação/revision guard em fonte;
- prompt bundles import/export com hash;
- mídia ampliada e validada por formato;
- render high/ultra bloqueia sem GLB Aedifex real;
- dock acessível e globo cinematográfico protegidos por testes.

## P0 — fechar runtime real

1. materializar o commit pinado e executar inventário/coverage;
2. buildar Aedifex offline com Bun;
3. vendor CesiumJS 1.143 local;
4. executar Região → Floorplanner → GLB → Cesium;
5. ligar ghost preview nativo ao chat global;
6. smoke de todos os 46 node kinds e famílias MCP;
7. 100 ciclos de lifecycle sem leak;
8. instalar Rust e fechar fmt/check/test;
9. instalar Blender/Cycles e modelos locais;
10. renderizar imagem 8K real e executar aceite offline Windows;
11. executar install/run/stop/restart/uninstall em máquina Windows limpa;
12. validar `docker compose config/up/build` com vendors materializados.

## P1 — conversão massiva sem perda

1. schema parity site/building/level;
2. walls/openings;
3. rooms/slabs/ceilings;
4. roofs/accessories;
5. stairs/elevators;
6. materials/items/cabinets;
7. documentation/grids;
8. MEP/plumbing;
9. plugins/payloads desconhecidos;
10. golden round-trip e loss report por kind.

## P2 — core e host final

- mover compute puro para Rust por feature flags;
- unificar em Tauri/React;
- remover sidecar após E2E;
- IFC round-trip;
- plantas/cortes/elevações/schedules;
- render temporal estável;
- instalador, assinatura, update e rollback.
