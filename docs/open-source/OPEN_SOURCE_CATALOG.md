# Catálogo open source auditado — V10

A fonte machine-readable é `opensources/manifest.json`. **Status de integração
não é inferido pelo nome da pasta.** Um componente só está instalado quando os
bytes, licença, commit/versão, hashes, build e testes correspondentes existem.

| Componente | Licença | Uso no ARCZ | Estado V10 |
|---|---|---|---|
| Aedifex | MIT | Building Authoring Kernel | lock/overlay prontos; upstream/build ausentes |
| CesiumJS 1.143 | Apache-2.0 | globo, câmera, 3D Tiles | vendor ausente; CDN proibida |
| Blender | GPL-3.0+ | worker externo Cycles | scripts prontos; executável ausente |
| OpenStreetMap data | ODbL-1.0 | vias/footprints/contexto | somente por pacotes locais licenciados |
| Overture data | licença por tema/release | contexto complementar | importador/pacote deve registrar licença |
| Panoramax | licença da instância/imagem | Street-level | local/auto-hospedado ou importação explícita |
| Poly Haven assets | CC0 | materiais/HDRI/assets | apenas itens materializados e verificados |
| Kenney assets | CC0 | vegetação/props | apenas itens presentes em manifest local |

## Regras

- GPL/AGPL usados como processos separados quando aplicável.
- dados ODbL mantêm attribution e provenance.
- pesos/modelos têm licença própria e nunca herdam automaticamente a licença do
  código.
- conteúdo `NC`, licença desconhecida ou assets sem origem ficam bloqueados.
- forks Aedifex passam por `COMMUNITY_SOURCES.json`; nenhum blind merge.
- providers online são conectores de importação, nunca dependência core.
