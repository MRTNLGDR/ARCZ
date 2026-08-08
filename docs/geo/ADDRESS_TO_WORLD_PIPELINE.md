# Address / selection → locked ARCZ world project

## Inputs

A project may begin from text address, coordinates, map click, parcel ID, polygon, administrative boundary, imported GIS/CAD/BIM file or an empty local scene.

## Pipeline

### 1. Resolve selection

Normalize the request into a geographic scope and geometry. Prefer authoritative/local cadastral data when available; otherwise record the fallback source explicitly.

### 2. Create GeoAnchor

Choose WGS84 latitude/longitude/altitude and true north. Establish local ENU axes and metric units. This transform is immutable for a revision unless the user performs an explicit re-anchor operation.

### 3. Lock project scope

Persist selected parcel/polygon/administrative geometry and source identifiers. A lock prevents later source refreshes from silently moving the authored project.

### 4. Source coverage plan

For each world layer decide what source is available: terrain, imagery, roads, parcels, buildings, landcover, hydrology, trees, urban furniture, transit and optional metadata. Missing data becomes an explicit gap, not fabricated “exactness”.

### 5. Reconstruct context

Generate a deterministic first-pass context from source facts. Building footprints become massing; road graphs become lanes/sidewalks; DEM becomes terrain; landcover guides vegetation. Reference imagery/photogrammetry can refine geometry/textures when licensing/data quality permits.

### 6. Semantic/PBR enrichment

Assign semantic classes, materials and procedural details. Generated assets retain the source/generator/model version and parameters.

### 7. Validation

Check geographic alignment, topology, intersections, terrain support, scale, normals, missing textures, invalid geometry, duplicate IDs and provenance.

### 8. Promote editable scope

Context remains derived/read-only. The selected project/layers are promoted into the ARCZ canonical authoring scene, with source links retained.

### 9. Publish derivatives

Generate view/runtime derivatives: GLB/glTF, 3D Tiles, thumbnails, sheets or render caches. Derivatives reference the project revision/hash and can be rebuilt.

## Accuracy policy

ARCZ must distinguish:

- **survey/authoritative** — verified source geometry;
- **mapped** — OSM/cadastre/GIS facts;
- **reconstructed** — inferred from images/point clouds;
- **procedural** — plausible generated detail;
- **synthetic** — intentionally invented design.

The UI should make this confidence/provenance visible. “Looks exact” is never allowed to masquerade as surveyed truth.
