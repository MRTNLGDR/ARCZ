# Estado dos módulos V10

A fonte machine-readable é `IMPLEMENTATION_STATUS.json`.

| Módulo | Estado honesto |
|---|---|
| Persistência, migração, API, jobs, packages e geocoder | verificados em Python |
| Shell, contracts, transações e painéis | verificados em JavaScript; browser real pendente |
| ModelingContext/GeoAnchor | contratos testados |
| Revisões Floorplanner/SSE/conflito | verificados em Python/HTTP |
| GLB round-trip | ingestão e contratos verificados; R3F→Cesium E2E bloqueado |
| Aedifex upstream/build | bloqueado: bytes/Bun/dependências ausentes |
| Rust CAD/BIM/Aedifex | implementado sem compilação |
| Prompts/mídias/chat | stores e UI testados; inferência bloqueada por modelos |
| Blender photoreal | worker/preflight prontos; executável ausente |
| Cesium/globo cinematográfico | código pronto; vendor/smoke visual bloqueados |
| Street/cinema/pranchas | parciais com gates explícitos |
| Inicializadores/preflight | contratos + Linux syntax aprovados; PowerShell/Docker e clean install bloqueados |

`CONTRACT_READY`, `PARTIAL_VERIFIED` e `IMPLEMENTED_UNVERIFIED` não equivalem a
release funcional completa.
