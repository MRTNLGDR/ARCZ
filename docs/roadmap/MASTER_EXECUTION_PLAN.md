# ARCZ Master Execution Plan

This roadmap is ordered by dependency, not by visual excitement. The objective is to reach a credible end-to-end product without breaking the working Aedifex/GIS foundations.

## Wave 0 — repository truth and reproducibility

- GitHub repository/branch/CI structure;
- pinned upstream manifest and license notices;
- Windows/Linux bootstrap;
- Rust/Python/JS test runners;
- artifact hashes and validation report;
- feature/status ledger using implemented/partial/contract-ready/blocked.

**Gate:** a clean machine can reproduce the dev environment and run foundational checks.

## Wave 1 — canonical scene + plugin host

- finish Rust scene revision/transaction API;
- complete plugin registry, capability ACL and worker protocol;
- project storage/migrations;
- undo/redo/history/provenance;
- import/export round-trip fixtures.

**Gate:** two plugins can edit the same project without bypassing revision guards.

## Wave 2 — Aedifex parity foundation

- schema/node parity fixtures;
- walls/openings/zones/levels/slabs/ceilings/roofs;
- selection/snapping/transform/history;
- material system;
- AI/MCP tool semantic mapping;
- GLB export parity.

**Gate:** golden architectural scenes round-trip and render within tolerance.

## Wave 3 — address → locked editable project

- geocoder/source adapters;
- parcel/polygon/admin selection;
- GeoAnchor/ECEF/ENU bridge;
- DEM/terrain and OSM transport ingestion;
- building footprints/massing;
- context validation/provenance;
- Cesium publication.

**Gate:** one real address becomes a reproducible locked local project with aligned globe context.

## Wave 4 — specialized world generators

- houses/buildings/facades/roofs;
- roads/lanes/sidewalks/bridges/tunnels;
- vegetation/biomes/agriculture;
- water/coast/rivers;
- poles/signs/lights/utilities;
- procedural PBR and LOD generation.

**Gate:** a block/neighborhood can be regenerated deterministically from the same inputs.

## Wave 5 — assets and multimodal reconstruction

- universal 3D import normalization;
- image/video reconstruction worker;
- object generators (furniture/tableware/textiles/stairs etc.);
- material scan/creation;
- rig/retarget/animation path;
- asset library/search/versioning.

**Gate:** user reference → validated asset → GLB → scene placement works end-to-end.

## Wave 6 — BIM/documentation

- IfcOpenShell worker integration;
- IFC import/export validation;
- quantities/classification/IDS/BCF;
- MEP adapters;
- plan/section/elevation/dimensions/schedules;
- sheet composer.

**Gate:** a representative building exports valid IFC and coordinated documentation.

## Wave 7 — rendering, physics and simulation

- real-time WebGPU quality path;
- Blender/Cycles offline orchestration;
- sun/moon/solar/shadow study;
- Rapier rigid body/vehicle physics;
- traffic/pedestrian simulation;
- water/weather/atmosphere layers;
- cinematic camera/timeline.

**Gate:** reproducible scene can be interactively simulated and rendered offline from the same revision.

## Wave 8 — city/planet scale

- hierarchical world partitioning;
- content-addressed tile cache;
- streaming budgets/LOD/error metrics;
- resumable distributed/local job graph;
- multi-region project management;
- large dataset analytics via Kepler/columnar adapters;
- 3D Tiles publication.

**Gate:** city-scale dataset can stream while a parcel/building remains editable at CAD precision.

## Wave 9 — collaboration and ecosystem

- project diff/merge semantics;
- branchable spatial revisions;
- plugin packaging/signing/permissions;
- collaboration server optionality;
- engine adapters (Godot/Bevy/Unreal/Unity as separate compatibility targets);
- SDK/docs/examples.

**Gate:** third-party plugin can be installed, permissioned, tested and removed without corrupting project truth.
