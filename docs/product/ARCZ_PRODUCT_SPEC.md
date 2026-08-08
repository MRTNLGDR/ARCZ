# ARCZ Product Specification

## Mission

ARCZ is a universal spatial authoring environment: select a real place or start from an empty world, reconstruct/import its context, lock a project scope, and then design, simulate, document, render and export anything from a chair to a building, district, city or planet.

Its value is the continuity of **one spatial truth** across globe/GIS, procedural generation, CAD/BIM, assets, AI, simulation and rendering.

## Primary journeys

### Address → editable 3D
Resolve an address/coordinate/parcel/polygon, establish WGS84 + local ENU, ingest available terrain/roads/parcels/buildings/vegetation, reconstruct context with provenance, lock the scope and promote only the selected authoring layer to canonical editable state.

### Natural language/photo → CAD/BIM mutation
Requests such as “make a two-story house here”, “move this wall 30 cm”, “create a gamer chair like this photo” or “connect these roads with a bridge” become typed action plans. The agent resolves specialized plugins, dry-runs, previews, validates and commits against an expected project revision.

### Reference → reusable asset
Photos/video can route through segmentation, scale/camera estimation, reconstruction, cleanup, UV/PBR, LOD/collider, validation, asset library and GLB placement. Approximate procedural fallbacks must never be labeled as exact scans.

### Architecture → professional output
The same building model should support plans, sections, elevations, dimensions, schedules, quantities, IFC exchange, solar/shadow analysis, site studies and boards without creating a second source model.

### World → simulation
At larger scale ARCZ streams world cells/layers and can enable modular traffic, pedestrian, vehicle, weather, hydrology, physics, lighting, utilities and event simulations as overlays over spatial truth.

## Product surfaces

Earth · Build · World · Assets · Simulate · Render · Sheets · Agent · Plugins · Provenance.

## Scale hierarchy

`Object → Room → Building → Parcel → Block → Neighborhood → City → State → Country → Continent → Planet`.

Large context stays streamed/read-only until promoted into an ARCZ project revision.

## Quality bar

A feature is not done because a button or interface exists. It needs a real runtime path, error handling, validation, tests, provenance, undo/revision semantics where relevant, and honest `implemented` / `partial` / `contract_ready` / `blocked` status.

## Offline/local-first

Local-first is the default. Remote providers are optional capability-scoped plugins and may not become the canonical scene/database. Network use must be explicit and attributable.
