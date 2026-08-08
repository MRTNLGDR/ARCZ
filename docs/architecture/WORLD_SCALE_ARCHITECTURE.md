# World-scale architecture

ARCZ separates the canonical authored scene from world context because a building can fit in memory while a city or planet cannot.

## Coordinate strategy

- WGS84 is the geographic reference.
- ECEF is suitable for planet-scale placement/globe integration.
- local ENU is the stable metric authoring frame around the project anchor.
- exporters preserve the local↔global transform.

No generator may silently reinterpret units, north, elevation datum or coordinate system.

## World partition

World context is split into hierarchical cells and independent layers: terrain, imagery, parcels, buildings, roads, rail/transit, hydrology, vegetation/biomes, utilities, atmosphere/weather, simulations and analytics.

The first V11 contract uses a neutral `WorldCellId { level, x, y }`; the storage/index implementation may later map to quadtree/S2/H3/3D-Tiles-compatible schemes without coupling the kernel to one vendor.

## Mutability

External source layers are read-only. Procedural reconstruction is a derived read-only artifact. The ARCZ authoring layer is canonical/editable. Promotion from derived context to authoring preserves provenance and transform.

## LOD and streaming

Planet/state/city views load coarse data; neighborhood/parcel views progressively request detail; CAD/BIM authoring remains full precision locally. `StreamBudget` caps resident cells, geometry, textures, GPU memory and concurrent jobs. Cache eviction never discards canonical revisions.

## “Make the world” job graph

1. partition selected scope;
2. resolve source coverage per cell;
3. terrain/DEM;
4. transport network;
5. buildings;
6. vegetation/landcover/hydrology;
7. urban furniture/utilities;
8. PBR/material/LOD enrichment;
9. semantic/geometric validation;
10. content-addressed publication to runtime tiles.

Jobs are resumable by cell/layer. Failure of one generator cannot invalidate already verified layers.
