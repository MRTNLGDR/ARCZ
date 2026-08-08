# Auditoria técnica do Aedifex para integração ARCZ

## Snapshot auditado

O lock registra o commit e os pacotes `core`, `viewer`, `editor`, `nodes`,
`mcp` e `plugin-trees`. O código auditado é modular, usa schemas Zod, Zustand,
React Three Fiber/Three, registry de kinds, plugins e SQLite local no MCP.

## Capacidades encontradas

- scene graph e patches revisionáveis;
- editor 2D/3D e superfícies headless para host customizado;
- níveis, zonas, paredes, portas, janelas, slabs, ceilings, roofs, stairs;
- elevadores, colunas, fences, cabinets, items e MEP;
- terreno, guides/scans, measurements e construction dimensions;
- materiais, material painting, catálogo e placements;
- first-person, thumbnails e exportação de planta vetorial/PDF;
- plugin discovery/registry e plugin de árvores;
- MCP headless com operações reais, storage SQLite WAL, version check e SSE;
- chat de agente que propõe/aplica mutações de cena.

## Riscos encontrados

- eixos Aedifex são X/Z no chão e Y vertical; ARCZ usa ENU;
- câmera/planta podem adicionar rotação visual que não pode virar transformação
  de modelo por inferência;
- o editor original possui uma rota de IA compatível com provedor externo;
- o frontend ARCZ atual não consegue importar diretamente o workspace React;
- forks comunitários divergem muito e não são substitutos automáticos;
- GLB perde semântica paramétrica e não pode ser a fonte de edição;
- plugins podem trazer rede, assets ou licenças incompatíveis.

## Mitigações implementadas

- `GeoAnchor` explícito e ponte testada;
- Local AI Broker substitui a rota de provider;
- sidecar loopback fixado, build manifest e health check;
- revision store ARCZ com conflito otimista;
- export GLB real, validado e readonly;
- overlay em vez de editar upstream;
- catálogo de ferramentas global com capacidades;
- política de admissão de forks/plugins por commit/licença/hash/testes.

## Comunidade

Os forks pesquisados foram classificados em
`resources/aedifex/community-audit.json`. Nenhum é mesclado cegamente. Mudanças
úteis entram por cherry-pick ou reimplementação isolada depois de auditoria de
licença, diff, tests e compatibilidade com o commit fixado.
