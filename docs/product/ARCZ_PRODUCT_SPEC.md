# ARCZ Product Specification

## Mission

ARCZ is a universal spatial authoring environment: select a real place or start from an empty world, reconstruct/import its context, lock a project scope, and then design, simulate, document, render and export anything from a chair to a building, district, city or planet.

The product is not merely a globe viewer, CAD program, BIM editor, procedural generator or AI image tool. Its value is the **continuity of one spatial truth** across all of those modes.

## Primary user journeys

### 1. Address → editable 3D project

The user enters an address or clicks a parcel/block/neighborhood/city/state. ARCZ resolves geospatial sources, terrain, roads, parcels, building footprints, vegetation and contextual assets, produces a provenance-backed reconstruction, creates a local ENU authoring frame, and locks the selected scope as a revisioned project.

### 2. Natural language → real CAD/BIM mutation

The user asks: “make a two-story house here”, “move this wall 30 cm”, “design a gamer chair like this photo”, or “create a bridge connecting these roads”. The AI produces a typed action plan, previews changes, invokes specialized plugins and commits validated geometry/semantics into the canonical scene.

### 3. Reference photo/video → asset

The user supplies photos/video of an object or place. ARCZ segments/reconstructs geometry, estimates scale, bakes/creates PBR materials, performs topology/LOD/UV normalization and inserts the result with provenance. The result can be exported to GLB/glTF and, through adapters, other formats.

### 4. Architecture → professional documentation

A building authored in ARCZ must support plan/elevation/section, dimensions, schedules, quantities, IFC exchange, solar/shadow analysis, site/context studies and presentation sheets without duplicating the source model.

### 5. World → interactive simulation

At larger scope ARCZ streams terrain/cities/roads/vegetation and can enable modular simulation layers: traffic, pedestrians, weather, hydrology, rigid bodies, vehicles, lighting/time, utilities and events. Simulations are layers over spatial truth, not destructive edits of source geometry.

## Product surfaces

- **Earth** — globe/map, region selection, sources, context and tiles.
- **Build** — Aedifex-derived architectural editing, CAD/BIM, floorplanner and object placement.
- **World** — procedural generation, biomes, infrastructure and world partitioning.
- **Assets** — import, reconstruction, generation, materials, LOD and library.
- **Simulate** — physics, traffic, crowds, weather, solar, water and time.
- **Render** — real-time WebGPU plus offline photoreal orchestration.
- **Sheets** — plans, sections, elevations, schedules and boards.
- **Agent** — multimodal chat/voice/reference authoring through typed tools.
- **Plugins** — install/enable/disable/update/configure capabilities.
- **Provenance** — source/licensing/transform/version/audit history.

## Scale hierarchy

`Object → Room → Building → Parcel → Block → Neighborhood → City → State → Country → Continent → Planet`.

ARCZ may open a project at any level, but only the active authored scope is canonical/editable. Larger external context is streamed/read-only until explicitly promoted into an ARCZ project revision.

## Core quality bar

A feature is not “done” because a button exists. It must have a real implementation path, deterministic/reproducible behavior where appropriate, error handling, validation, tests, provenance, undo/revision semantics for edits, and an honest blocked/partial status when dependencies are missing.

## Offline/local-first policy

The default profile is local-first. Remote providers are optional plugins requiring explicit permission and provenance. The application must remain usable for authoring with local data/models where installed. No API vendor is allowed to become the canonical storage or scene authority.

## Extensibility targets

ARCZ should eventually be able to host or interoperate with: CAD/BIM solvers, GIS analytics, point-cloud/photogrammetry pipelines, local LLM/VLM/3D models, renderer backends, physics/simulation engines, game engines and web viewers through stable adapters rather than core rewrites.
