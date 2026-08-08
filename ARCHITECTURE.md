# ARCZ Architecture

ARCZ is a local-first, Rust-first world/architecture authoring system that can operate at object, parcel, neighborhood, city, state, country, continent, or planetary scale. The product combines geospatial context, procedural reconstruction, CAD/BIM authoring, physically based rendering, simulation, AI tool-use, and export to web/game/engineering formats.

## Non-negotiable invariants

1. **One canonical editable scene.** Cesium, Kepler, renderers, exports and analytics consume derivatives; they do not become competing scene authorities.
2. **Georeferencing is explicit.** Every project has a WGS84 anchor and a local ENU modeling frame. Large-scale data may remain ECEF/tiled while authored geometry stays numerically stable locally.
3. **Upstreams remain upstreams.** Aedifex, CesiumJS, Kepler.gl and IfcOpenShell/Bonsai are pinned and integrated through adapters/parity tests; they are not blindly merged into one license/codebase.
4. **Plugins never own project truth.** A plugin proposes/returns operations or artifacts; the kernel validates revision, permission and provenance before committing changes.
5. **AI never gets raw authority.** Models plan and call typed tools. Destructive or irreversible mutations require dry-run/preview and approval policy.
6. **World scale is streamed.** Planet-scale projects are partitioned into cells/layers/LODs. Only the active working set is resident.
7. **Every generated fact has provenance.** Source data, model/tool version, parameters, timestamps/hashes and transformations are recorded.

## Runtime topology

```text
                     ┌──────────────────────────────────────────┐
                     │              ARCZ UI                     │
                     │ Web (WASM/WebGPU) + Desktop (Tauri)      │
                     └──────────────────┬───────────────────────┘
                                        │ typed commands/events
                     ┌──────────────────▼───────────────────────┐
                     │           ARCZ Authoring Kernel          │
                     │ scene · CAD · BIM · revision · history   │
                     │ geo anchor · provenance · validation      │
                     └───────┬───────────────┬──────────────────┘
                             │               │
                  ┌──────────▼──────┐  ┌────▼──────────────────┐
                  │ Plugin Host/SDK │  │ ARCZ World Authority  │
                  │ capability ACL  │  │ cells/layers/LOD      │
                  └──────┬──────────┘  │ streaming/budgets     │
                         │             └──────┬────────────────┘
        ┌────────────────┼────────────────────┼────────────────────────┐
        │                │                    │                        │
   procedural       AI workers          geo sources              render/sim
 roads/buildings   local VLM/LLM       OSM/DEM/imagery          Blender/Rapier
 vegetation/PBR    reconstruction      Cesium/3D Tiles          traffic/weather
        │                │                    │                        │
        └────────────────┴──────────┬─────────┴────────────────────────┘
                                   │ validated artifacts
                            GLB · glTF · IFC · 3D Tiles
                            USD* · images · sheets · data
```

`*` USD is a roadmap adapter, not a production claim in this foundation.

## Core crates

- `arcz-scene`: canonical scene/revision model.
- `arcz-cad`: parametric CAD operations and constraints.
- `arcz-bim`: BIM semantics, quantities and classifications.
- `arcz-geo`: geodesy and coordinate primitives.
- `arcz-region`: selection/locking of geographic scope.
- `arcz-world`: planet-to-object layer, cell, LOD and budget contracts.
- `arcz-earth`: globe bridge and Earth presentation.
- `arcz-tiles`: tiled derivatives and streaming artifacts.
- `arcz-plugin-sdk`: stable plugin/tool contracts.
- `arcz-plugin-host`: registry, capability discovery and permission gate.
- `arcz-agent`: provider-agnostic AI action plans with revision/risk guards.
- `arcz-procedural`: deterministic procedural generation building blocks.
- `arcz-terrain`, `arcz-roof`, `arcz-facade`, `arcz-vegetation`: specialized generators.
- `arcz-aedifex`: compatibility/parity boundary during Rust migration.
- `arcz-validation`, `arcz-determinism`, `arcz-provenance`, `arcz-budget`, `arcz-jobs`: operational integrity.
- `arcz-app` + `arcz-tauri`: application/service and desktop shell.

## Read next

- `docs/product/ARCZ_PRODUCT_SPEC.md`
- `docs/architecture/ARCZ_V11_MASTER_PLAN.md`
- `docs/architecture/WORLD_SCALE_ARCHITECTURE.md`
- `docs/architecture/UPSTREAM_INTEGRATION.md`
- `docs/architecture/PLUGIN_ARCHITECTURE.md`
- `docs/geo/ADDRESS_TO_WORLD_PIPELINE.md`
- `docs/ai/ARCZ_AGENT.md`
- `docs/roadmap/MASTER_EXECUTION_PLAN.md`
