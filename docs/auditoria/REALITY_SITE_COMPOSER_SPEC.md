# Especificação Técnica do Reality Site Composer — ARCZ / AVA-26

**Data:** 2026-07-30  
**Status:** Especificação Técnica Canônica  
**Autor:** Antigravity  

---

## 1. Esquema do Banco SQLite (`project.sqlite`)

```sql
-- Nós do Scene Graph
CREATE TABLE IF NOT EXISTS scene_nodes (
    id TEXT PRIMARY KEY,
    parent_id TEXT,
    type TEXT NOT NULL,
    name TEXT NOT NULL,
    transform_json TEXT NOT NULL, -- pos, rot, scale, bbox
    georeference_json TEXT,       -- lat, lon, height, heading
    visibility INTEGER NOT NULL DEFAULT 1,
    locked INTEGER NOT NULL DEFAULT 0,
    selectable INTEGER NOT NULL DEFAULT 1,
    layer TEXT NOT NULL DEFAULT 'default',
    material_refs_json TEXT,
    asset_ref TEXT,
    source TEXT NOT NULL,         -- 'observed', 'gis', 'reconstructed', 'inferred'
    confidence REAL NOT NULL DEFAULT 1.0, -- 0.0 a 1.0 (VERDE/AZUL/AMARELO/VERMELHO)
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    metadata_json TEXT,
    FOREIGN KEY(parent_id) REFERENCES scene_nodes(id) ON DELETE SET NULL
);

-- Anotações e Issues 3D
CREATE TABLE IF NOT EXISTS scene_issues (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'open', -- open, in_progress, resolved, closed
    priority TEXT NOT NULL DEFAULT 'medium',
    assignee TEXT,
    target_node_id TEXT,
    world_pos_x REAL NOT NULL,
    world_pos_y REAL NOT NULL,
    world_pos_z REAL NOT NULL,
    camera_state_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(target_node_id) REFERENCES scene_nodes(id) ON DELETE SET NULL
);

-- Panoramas e Reconstruções
CREATE TABLE IF NOT EXISTS reality_panoramas (
    id TEXT PRIMARY KEY,
    source_type TEXT NOT NULL, -- 'client', 'panoramax', 'kartaview', 'mapillary'
    image_path TEXT NOT NULL,
    depth_path TEXT,
    mask_path TEXT,
    lat REAL NOT NULL,
    lon REAL NOT NULL,
    altitude REAL NOT NULL,
    heading REAL NOT NULL,
    pitch REAL NOT NULL,
    roll REAL NOT NULL,
    exif_json TEXT,
    created_at TEXT NOT NULL
);

-- Providência de Licença dos Assets
CREATE TABLE IF NOT EXISTS asset_provenance (
    asset_id TEXT PRIMARY KEY,
    source_name TEXT NOT NULL,
    license_type TEXT NOT NULL, -- 'CC0', 'Apache-2.0', 'BSD-3-Clause', 'MIT', 'ODbL', 'Client-Owned'
    commercial_allowed INTEGER NOT NULL DEFAULT 1,
    attribution_required INTEGER NOT NULL DEFAULT 0,
    license_url TEXT,
    created_at TEXT NOT NULL
);
```

---

## 2. Estrutura do Nó do Scene Graph (JSON Schema)

```json
{
  "id": "node_building_zenite_001",
  "parent_id": "site_root_001",
  "type": "Building",
  "name": "Edifício Zênite",
  "transform": {
    "position": [0.0, 0.0, 0.0],
    "rotation": [0.0, 0.0, 0.0, 1.0],
    "scale": [1.0, 1.0, 1.0]
  },
  "georeference": {
    "latitude": -27.1432,
    "longitude": -48.4901,
    "altitude": 15.4,
    "heading": 145.0
  },
  "visibility": true,
  "locked": false,
  "selectable": true,
  "layer": "Architecture",
  "material_refs": ["mat_glass_facade", "mat_concrete_pbr"],
  "asset_ref": "assets/models/zenite_v1.glb",
  "source": "observed",
  "confidence": 1.0,
  "created_at": "2026-07-30T08:30:00Z",
  "updated_at": "2026-07-30T08:30:00Z",
  "revision": 1,
  "metadata": {
    "floors": 18,
    "architect": "ARCZ Studio"
  }
}
```

---

## 3. Matriz de Cores de Confiança da Reconstrução

| Código Cor | Fonte / Método | Descrição |
|---|---|---|
| **VERDE** | Observado em imagem | Pixels capturados diretamente de fotografia/panorama real |
| **AZUL** | Derivado GIS / OSM | Footprints, extrusões e elevações oficiais do OpenStreetMap/Overture |
| **AMARELO** | Reconstrução Multi-View | Malha/Textura gerada por triangulação SFM/COLMAP/MVS de múltiplas fotos |
| **VERMELHO** | Inferência de IA / Inpainting | Preenchimento generativo de fundos ou áreas ocluídas |

---

## 4. Registro de Workers (Background Microservices)

1. `gis-context-worker`: Download e decodificação de Overture Maps / OSM / DEM GeoTIFF.
2. `street-imagery-worker`: Conexão com STAC Panoramax, KartaView e MapillaryJS.
3. `panorama-worker`: Alinhamento, reprojeção em cubemap e geração de IBL.
4. `reconstruction-worker`: Orquestração de pipelines de reconstrução 3D.
5. `colmap-worker`: Runner seguro de SfM, MVS e triangulação de câmeras via CLI/Python bindings.
6. `metric-depth-worker`: Estimativa de profundidade métrica via MapAnything (Apache 2.0).
7. `segmentation-worker`: Segmentação semântica e mascaramento do edifício existente.
8. `background-recovery-worker`: Reprojeção multi-view e inpainting de fundos ocluídos.
9. `gaussian-splat-worker`: Rasterização CUDA e otimização de Gaussian Splats via `gsplat`.
10. `mesh-worker`: Simplificação, decimação, geração de LOD e conversão GLB via meshopt/KTX2.
11. `material-worker`: Compilação de materiais PBR, ORM (Occlusion/Roughness/Metallic) e sRGB linear.
12. `lighting-worker`: Cálculo de sol georreferenciado, céu físico e Pseudo-HDR / HDRI bracketed.
13. `scene-compositor-worker`: Fusão do modelo novo no entorno reconstruído com oclusão e sombra.
14. `render-worker`: Renderização de saída offscreen de alta resolução (4K / 8K).
15. `project-store-worker`: Gerenciamento transacional do SQLite, autosave, journal e undo/redo.
16. `license-provenance-worker`: Validação de licenças (ODbL, Apache, BSD, MIT) e auditoria de ToS.
