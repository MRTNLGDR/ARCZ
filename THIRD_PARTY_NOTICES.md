# Third-party notices

## Aedifex

- Projeto: `TangSY/aedifex`
- Commit fixado: `5319368bae16500ca5267f6f8d68b36c9586d5bb`
- Licença declarada: MIT
- Uso: Building Authoring Kernel, Floorplanner, viewer, nodes, MCP e plugins.
- Modificações: overlays em `integrations/aedifex/overlay`; o original não é
  distribuído nesta entrega porque não pôde ser materializado no ambiente.
- License copy: `integrations/aedifex/LICENSE.upstream`.

## CesiumJS

- Versão esperada: 1.143.0
- Licença: Apache-2.0
- Uso: globo e câmera.
- Estado: bytes locais ausentes; nenhuma CDN substitutiva foi inserida.

## Blender

- Licença: GPL-3.0-or-later
- Uso: executável separado/worker local para Cycles e passes.
- Estado: não distribuído e não instalado; o ARCZ apenas contém scripts de
  automação para uma instalação local do usuário.

## Dados e assets

Cada pacote territorial, panorama, mídia, textura e modelo deve manter licença,
atribuição, provenance e SHA-256 no respectivo manifesto. Este arquivo não
concede licença sobre dados do usuário nem sobre material não distribuído.


## Kepler.gl

- Projeto: `keplergl/kepler.gl`
- Commit fixado: `621bb236fb33f0f6de9dbd730c2e18edac40b764`
- Licença: MIT
- Uso previsto: análise geoespacial como web sidecar; não é autoridade da cena.

## IfcOpenShell / Bonsai

- Projeto: `IfcOpenShell/IfcOpenShell`
- Commit fixado: `7ed8584edc6609654cea608d699348c9cca7ce5d`
- IfcOpenShell: LGPL-3.0-or-later.
- Bonsai: GPL-3.0-or-later.
- Uso previsto: IfcOpenShell em boundary de interoperabilidade IFC; Bonsai somente como referência/worker Blender isolado salvo decisão explícita de distribuição GPL compatível.
