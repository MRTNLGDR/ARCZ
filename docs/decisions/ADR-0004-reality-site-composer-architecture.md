# ADR-0004: Arquitetura Reality Site Composer (GIS + Reconstrução Visual + Panoramas 360 + PBR + Scene Graph SQLite)

**Data:** 2026-07-30  
**Status:** Aceito  
**Decisor:** Lucas (LEGRAND) + Antigravity  

## Contexto

Para o ecossistema ARCZ/AVA-26, a inserção de empreendimentos imobiliários no contexto urbano exige uma cadeia integrada de ferramentas sem depender de scraping proibido ou licenças restritivas. Nenhum software open-source isolado entrega do endereço ao render PBR. O ARCZ necessita de uma arquitetura modular unindo GIS global, reconstrução por fotografia, composição 360 com iluminação coerente e persistência relacional do Scene Graph em SQLite.

## Decisão

Implementar o módulo **Reality Site Composer** no ARCZ estruturado em 8 camadas fundamentais:

### 1. Reconstrução Automática do Entorno
- **Dados geográficos**: OpenStreetMap + Overture Maps (footprints, altura, `BuildingPart`).
- **Geração 3D**: OSM2World para exportação de terreno, vias, vegetação e construções em GLB/glTF PBR com LOD.
- **Topografia**: DEM / GeoTIFF.
- **Visualização**: CesiumJS (globo e 3D Tiles) + Babylon.js / wgpu nativo (editor local). Oclusão/hiding de prédios existentes via API do Cesium.

### 2. Reconstrução Realista por Fotografia e Vídeo
- **Preview por IA**: MapAnything (checkpoint Apache 2.0 comercial) ou VGGT Commercial.
- **Reconstrução geométrica**: COLMAP (BSD, SfM, MVS, nuvens de pontos, mapas de profundidade e alinhamento GPS).
- **Entorno fotográfico**: Nerfstudio + `gsplat` (Apache 2.0, rasterização CUDA).
- **Processamento de Drones**: OpenDroneMap (ODM) isolado em worker secundário (análise AGPL).
- **Exclusão explícita**: DUSt3R não é utilizado como dependência padrão por restrições CC-BY-NC-SA.

### 3. Botão "Street 360" & Fontes de Panoramas
- **Fluxo**: Seleção de rua -> Busca de panoramas próximos -> Pose/orientação -> Câmera & profundidade -> Máscara de segmentação -> Remoção do edifício existente -> Posicionamento georreferenciado do novo projeto -> Iluminação/céu/reflexos -> Render 360 final.
- **Fontes permitidas**:
  1. Captura própria do cliente
  2. Panoramax (API STAC open-source)
  3. KartaView
  4. Mapillary (MapillaryJS MIT)
  5. Google Street View **exclusivamente como visualizador oficial isolado**, sem scraping nem extração de 3D/HDRI.

### 4. Desocultação e Inpainting de Fundos
- **Reconstrução de ocultações**: Uso de múltiplos panoramas da mesma via + fotos de ângulos laterais + ortofotos aéreas + GIS OSM/Overture.
- **Tagging de Confiança**:
  - `VERDE`: Observado diretamente em imagem real.
  - `AZUL`: Derivado de dados GIS/OSM.
  - `AMARELO`: Reconstruído por múltiplas vistas.
  - `VERMELHO`: Inferido por IA (inpainting).

### 5. Reflexos, Vidro e Iluminação Coerente (PBR)
- **Nível 1 (Environment Map)**: Panorama como Cubemap IBL.
- **Nível 2 (Pseudo-HDR)**: Estimativa solar (azimute, elevação, horário/lat/lon) + extensão de alcance dinâmico.
- **Nível 3 (HDRI Real)**: Captura bracketed (-4 EV a +4 EV) em `.exr` / `.hdr`.

### 6. Scene Graph Persistente e SQLite
- **Estrutura de arquivo `.arcz`**:
  ```text
  MeuProjeto.arcz/
  ├── project.sqlite
  ├── project.json
  ├── scene/
  │   ├── graph.json
  │   ├── cameras.json
  │   └── environments.json
  ├── gis/
  ├── reality/
  ├── assets/
  ├── renders/
  ├── documents/
  └── cache/
  ```
- **Esquema de Nós**: Suporte a 26 tipos de entidades (`Building`, `Wall`, `Furniture`, `Light`, `Panorama`, `GaussianSplat`, `Issue`, etc.) com persistência transacional SQLite.
- **Gizmo Universal**: Atalhos W, E, R, Q, restrição X/Y/Z, snap Ctrl, precisão Shift e duplicação Alt.

### 7. Gestão Integrada do Projeto
- Issues anotas diretamente em coordenadas 3D da cena (`target_node`, `world_position`, `camera`).
- Painéis dedicados para Dashboard, Tree, Properties, Tasks, Issues, Approvals, Versions, Render Queue, Asset Browser e License Provenance.

### 8. Worker Architecture
- Controlador em Rust/Tauri se comunicando por WebSocket/gRPC com 16 workers especializados: `gis-context-worker`, `street-imagery-worker`, `panorama-worker`, `reconstruction-worker`, `colmap-worker`, `metric-depth-worker`, `segmentation-worker`, `background-recovery-worker`, `gaussian-splat-worker`, `mesh-worker`, `material-worker`, `lighting-worker`, `scene-compositor-worker`, `render-worker`, `project-store-worker`, `license-provenance-worker`.

## Consequências
- Proteção jurídica contra uso indevido de APIs proprietárias (Google Street View isolado em iframe/viewer oficial).
- Fidelidade métrica e fotográfica combinando GIS com reconstrução Neural/Splatting.
- Persistência robusta livre de perdas através de WAL SQLite e undo/redo transacional.
