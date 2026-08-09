# ARCZ completion gates

`100% done` is a verifiable state, not a label. The repository may only claim it when all source gates and all required runtime gates for the declared release profile are green.

## Gate A — repository integrity

- complete source tree is on GitHub;
- no secrets, generated caches or user data are committed;
- third-party software/data/model provenance is explicit;
- immutable upstreams are pinned by commit and materialized outside ARCZ history.

## Gate B — source correctness

- Python compile + pytest: green;
- JavaScript tests + syntax: green;
- TypeScript Aedifex overlay syntax: green;
- schemas/manifests/plugin catalog: green;
- Rust 1.97.1 workspace: `fmt`, `check`, `test`, `clippy -D warnings` green.

## Gate C — canonical authoring

- one editable scene authority;
- revision guard, undo/redo, persistence and recovery are exercised;
- CAD/BIM mutation is deterministic and validated;
- AI tools cannot bypass capability, approval or revision checks.

## Gate D — geographic world pipeline

- address/coordinates/selection resolves to an explicit WGS84 anchor;
- parcel/region lock is reproducible and records source provenance;
- terrain, transport, buildings, hydrology, vegetation and enrichment run as resumable per-cell jobs;
- LOD/stream budgets prevent planet/city scale from becoming one resident scene;
- generated geometry never claims survey accuracy without survey-grade sources.

## Gate E — upstream parity and sidecars

- pinned Aedifex is materialized and its parity ledger is green;
- CesiumJS local vendor/globe/terrain/3D Tiles path is green;
- Kepler.gl analytics adapter is validated;
- IfcOpenShell IFC worker is validated;
- Bonsai GPL boundary remains isolated and documented.

## Gate F — creation plugins

A plugin family is production-ready only when its declared capabilities have executable tests or a deterministic validation fixture. This includes architecture, houses, buildings, roads, bridges, tunnels, terrain, vegetation, hydrology, atmosphere/weather, solar, urban furniture, vehicles/traffic, characters/rig/animation, import/export, furniture/props, fabrics, stairs/escalators, BIM/MEP, PBR/materials, physics, render and sheets.

## Gate G — local AI and reconstruction

- text/image/video/reference inputs pass through the local AI broker;
- model weights are hash/license/version verified;
- action plans are typed and revision guarded;
- reconstruction outputs carry source/model/tool provenance;
- missing models fail closed instead of returning fake geometry.

## Gate H — render and target runtimes

- Web/WebGPU build and smoke test green;
- Tauri desktop build and smoke test green on supported OS targets;
- Blender/Cycles path green when selected;
- offline-strict boot green with no hidden remote dependency;
- exports reopen/validate in their intended consumers.

## Release profiles

- `source-verified`: Gates A-B.
- `authoring-verified`: Gates A-C.
- `world-verified`: Gates A-E.
- `full-local`: Gates A-H on target hardware with required optional engines/models installed.

The UI and documentation must display the actual profile reached. `partial`, `contract_ready`, `blocked`, `optional_missing`, or `not_run` are never synonyms for `done`.
