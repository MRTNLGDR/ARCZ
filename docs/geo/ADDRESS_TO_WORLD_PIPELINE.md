# Address / selection → locked ARCZ world project

## Inputs

Text address, coordinates, map click, parcel ID, polygon, administrative boundary, imported GIS/CAD/BIM file or empty local scene.

## Pipeline

1. **Resolve selection** into a geographic scope/geometry. Prefer authoritative/local cadastre when available; record fallback sources.
2. **Create GeoAnchor** with WGS84 latitude/longitude/altitude, true north, local ENU axes and metric units.
3. **Lock project scope** so source refreshes cannot silently move authored work.
4. **Plan source coverage** for terrain, imagery, roads, parcels, buildings, landcover, hydrology, trees, urban furniture and transit. Missing data is a gap, not invented exactness.
5. **Reconstruct context** deterministically: DEM→terrain, road graphs→lanes/sidewalks, footprints→massing, landcover→vegetation. Imagery/photogrammetry may refine when available/licensed.
6. **Semantic/PBR enrichment** with provenance for generator/model/version/parameters.
7. **Validate** alignment, topology, intersections, terrain support, scale, normals, textures, duplicate IDs and provenance.
8. **Promote editable scope** into the canonical ARCZ authoring scene. Context remains derived/read-only.
9. **Publish derivatives** such as GLB/glTF, 3D Tiles, sheets and render caches tied to project revision/hash.

## Accuracy classes

- survey/authoritative;
- mapped GIS/cadastre/OSM;
- reconstructed from images/point clouds;
- procedural plausible detail;
- synthetic intentional design.

The UI must expose confidence/provenance. Visual plausibility must never masquerade as surveyed truth.
