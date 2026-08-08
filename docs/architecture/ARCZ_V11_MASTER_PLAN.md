# ARCZ V11 — Master Architecture

## Product definition

**Repository:** `MRTNLGDR/ARCZ`  
**Program:** `ARCZ`  
**Primary model:** one authoritative, revisioned, georeferenced CAD/BIM scene.  
**Targets:** Windows/macOS/Linux desktop through Tauri 2 and browser through Rust/WASM + WebGPU-compatible sidecars.

ARCZ is not a pile of forks. It is a Rust authority layer that can consume proven upstream systems without letting any upstream become an irreversible architectural dependency.

## The invariant

The editable project is `ARCZ Scene + GeoAnchor + provenance`. Cesium 3D Tiles, GLB, IFC, drawings, renders and analysis layers are derived artifacts. A provider, renderer, LLM, Blender, Aedifex, Kepler or Bonsai process never becomes the source of truth.

## Upstream strategy

| Upstream | Role | Boundary | Port policy |
|---|---|---|---|
| Aedifex | authoring UX + conformance oracle | local web sidecar initially | port schema, geometry, snaps, tools and transactions into Rust/WASM behind parity tests |
| CesiumJS | globe + 3D Tiles | web sidecar | keep upstream intact; ARCZ owns geo model and tiles pipeline |
| Kepler.gl | GIS analytics | web sidecar | analytics only; never owns editable geometry |
| IfcOpenShell | IFC geometry/semantics | local process/dynamic boundary | use for IFC fidelity; Rust facade owns contracts |
| Bonsai | BIM workflow reference | optional Blender worker, GPL boundary | do not link blindly into permissive core |

## Runtime layers

1. **arcz-cad** — canonical parametric nodes, constraints and revisions.
2. **arcz-geo/arcz-region/arcz-earth** — WGS84/ECEF/ENU, active region, parcel lock, globe bridge.
3. **arcz-procedural** — deterministic roads, parcels, massing, roofs, facade and terrain generators.
4. **arcz-bim** — quantities, classifications, schedules and IFC facade.
5. **arcz-plugin-sdk** — stable capability contracts shared by built-in Rust, WASM, process and web plugins.
6. **arcz-agent** — provider-agnostic plan; AI proposes, tools validate, revision guard commits.
7. **arcz-tauri** — desktop shell; web target consumes the same JSON/WASM contracts.
8. **derived workers** — Blender/Cycles, reconstruction models, texture baking, upscalers.

## Address → locked 3D project

`search address → resolve polygon → select lot/block/neighborhood/city/state → acquire licensed source package → GeoAnchor → terrain → road graph → footprints → semantic classification → procedural reconstruction → confidence map → manual/AI corrections → lock project revision`.

Exactness is evidence-based. OSM/Overture/DEM provide geometry/context; imagery/panoramas/user photos refine facade/vegetation/material hypotheses. Unknown geometry is marked uncertain instead of fabricated as “exact”.

## AI authoring

The agent receives text, images, video, drawings or references. A request such as “make this gaming chair” becomes:

`reference ingest → image/geometry analysis → parametric intent → generator/reconstruction tool → PBR material tool → scale validation → collision/placement → ghost diff → user/automatic policy approval → revision commit → GLB/IFC derived output`.

No LLM gets raw scene mutation privileges. All writes pass through typed plugin tools with project ID, expected revision, dry-run and approval policy.

## Plugin families

The catalog in `plugins/catalog.json` defines the first 33 families: Aedifex, region, earth, CAD, BIM, terrain, roads, buildings, houses, vegetation, PBR, bridges, water, atmosphere, solar, urban furniture, vehicles, characters, universal import, furniture generators, tableware, textiles, stairs/escalators, MEP, IFC, render, physics, sheets, photogrammetry, geo reconstruction, AI agent, Kepler analytics and Cesium globe.

## Rendering

- interactive viewport: WebGPU path, local PBR, instancing, LOD/HLOD;
- global view: CesiumJS/3D Tiles initially;
- offline photoreal: Blender/Cycles worker with deterministic scene export and pass manifests;
- later native viewport: `wgpu`/Bevy adapter can coexist without changing the project model;
- physics: Rapier adapter;
- exact CAD/B-Rep: keep `arcz-cad` high-level parametric authority and add a kernel adapter (Rust Truck where sufficient; OCCT boundary for STEP/B-Rep cases requiring mature operations).

## Aedifex Rust conversion waves

**Wave 0 — oracle:** materialize exact upstream, build it unchanged, run its tests and snapshot every node/tool/export.  
**Wave 1 — schema/state:** Rust types, serde compatibility, revision/undo, spatial index.  
**Wave 2 — geometry:** wall/opening/zone/slab/ceiling/roof/item generation with golden geometry tests.  
**Wave 3 — tools:** snaps, selection, collision, transforms, placement, ghost preview and transactions.  
**Wave 4 — AI/MCP:** tool semantics move to `arcz-agent`/plugin SDK; upstream AI becomes replaceable.  
**Wave 5 — viewer:** Rust/WASM WebGPU renderer reaches visual parity; legacy R3F remains fallback until accepted.  
**Wave 6 — UI:** optional Rust UI migration only after functional parity; do not rewrite stable UI merely for language purity.

## Definition of “not broken”

An upstream feature is only marked converted when: fixture imported; operation executed in both implementations; canonical scene diff accepted; geometry bounds/topology compared; export round-trip passes; undo/redo passes; performance budget passes; and visual golden/E2E passes where applicable.

## Immediate gates

1. materialize four pinned upstreams;
2. install Rust and run `cargo fmt/check/test --workspace`;
3. build Aedifex at the pinned commit unchanged;
4. build local Cesium and Kepler bundles;
5. run real IFC fixture through IfcOpenShell boundary;
6. execute lot→procedural→Aedifex→GLB→Cesium round-trip;
7. only then start claiming full conversion parity.


## V11.1 world-scale additions

- `arcz-world`: scope hierarchy through planet scale, world layers, cells and stream budgets.
- `arcz-plugin-host`: capability-indexed registry and permission gate.
- plugin catalog expanded to 66 families / 253 capabilities covering world, infrastructure, simulation, assets and export.
- full product/world/AI/plugin/upstream/security/testing documentation indexed from `docs/README.md`.

These additions are architecture/foundation. They do not convert contract-ready plugins into production implementations.
