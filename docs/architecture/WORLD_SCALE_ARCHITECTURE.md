# World-scale architecture

## Why a dedicated world authority exists

A building editor can keep every object in memory. A city or planet cannot. ARCZ therefore separates the **canonical authored scene** from **world context**, which is partitioned, streamed and cached by cell/layer/LOD.

`crates/arcz-world` defines the first Rust contracts for this boundary.

## Coordinate strategy

- WGS84 is the source geographic reference.
- ECEF is suitable for planet-scale placement and globe integration.
- Local ENU is the authoring frame for stable CAD/BIM precision around a selected project anchor.
- Exporters are responsible for preserving the transform between authored local coordinates and global placement.

No generator may silently reinterpret units, north, elevation datum or coordinate system.

## World cells

World context is addressed by hierarchical cells. The exact spatial index may evolve (quadtree/S2/H3/3D Tiles compatible indexing), so V11 exposes a neutral `WorldCellId { level, x, y }` contract rather than coupling the kernel to one vendor.

Each cell may contain multiple independent layers:

- terrain/DEM;
- imagery/material references;
- parcels/cadastre;
- roads/rail/transit;
- buildings;
- hydrology/coast;
- vegetation/biomes;
- utilities;
- atmosphere/weather;
- simulations;
- analytics;
- ARCZ-authored overrides.

## Mutability rule

External source layers are read-only. Procedural reconstructions are derived read-only artifacts. The ARCZ authoring layer is the only canonical editable layer. A user may promote/convert a derived feature into authoring space, at which point its provenance and source transform are preserved.

## LOD strategy

A planet view should load coarse terrain/building massing. A street/building view should progressively request higher detail. Object authoring can use full CAD/BIM precision locally while distant context remains proxy geometry.

Recommended hierarchy:

- LOD 0–5: planetary/continental context;
- LOD 6–10: country/state/city massing;
- LOD 11–15: neighborhoods/blocks/roads;
- LOD 16–20: parcels/building envelopes;
- authored local detail: scene-native, not forced into geographic tile quantization while editing.

The exact thresholds are configuration, not hard-coded truth.

## Streaming budget

`StreamBudget` limits resident cells, geometry bytes, texture bytes, GPU bytes and concurrent jobs. Runtime policy may adapt those values to device capability. Eviction is cache policy only; it must not discard canonical project revisions.

## Derived world products

World context can be exported/published as:

- GLB/glTF assets;
- 3D Tiles tilesets;
- raster/vector/terrain caches;
- semantic JSON/GeoJSON/Parquet adapters;
- render proxies;
- simulation layers.

A derivative always records the source project revision and hashes so stale derivatives can be invalidated.

## “Make the world” mode

For a very large selected area ARCZ runs a job graph rather than one monolithic generation:

1. partition scope;
2. resolve source coverage per cell;
3. generate/ingest terrain;
4. build transport network;
5. reconstruct buildings;
6. vegetation/landcover/hydrology;
7. urban furniture/utilities;
8. materials and LODs;
9. semantic validation;
10. tiles/content-addressed publication.

Failures are cell/layer-specific and resumable. A failed tree generator must not invalidate valid terrain or roads.
