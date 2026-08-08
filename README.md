# ARCZ

**Repository:** `MRTNLGDR/ARCZ`  
**Product:** ARCZ  
**Direction:** Rust-first · local-first · Web + Desktop · object-to-planet spatial authoring.

ARCZ is being built as a universal spatial authoring system that joins **real-world geospatial context + procedural reconstruction + CAD/BIM + AI tool-use + assets + simulation + photoreal rendering** without creating separate incompatible copies of the same project.

The immediate foundation comes from the supplied `ARCZ_EARTH_AEDIFEX_GLOBAL_V10_1` work and is evolved here rather than discarded. Aedifex remains the architectural authoring/parity reference while ARCZ progressively moves authoritative domain behavior into Rust.

## What ARCZ is meant to do

A user can start with an address, coordinate, parcel, polygon or empty world; select anything from a lot to a city/state/planet; lock that scope; reconstruct terrain, roads, buildings, vegetation and context; then design or alter the result using direct CAD/BIM tools or a multimodal local AI agent.

The same canonical revision should be usable for:

- floorplanning and architectural authoring;
- BIM/IFC and quantities;
- road/bridge/infrastructure design;
- procedural city/world generation;
- object reconstruction from photos/video;
- characters, vehicles and animation;
- physics, solar, traffic, weather and other simulations;
- real-time WebGPU viewing and offline photoreal rendering;
- architectural sheets and presentation;
- GLB/glTF, IFC, 3D Tiles and adapter-based engine/GIS export.

## Architecture at a glance

```text
Earth / Map / Address / Parcel / Polygon
                   │
                   ▼
      ARCZ World Authority (Rust)
 WGS84/ECEF/ENU · cells · layers · LOD
 streaming budgets · provenance · scope lock
                   │
                   ▼
       ARCZ Canonical Scene (Rust)
 scene · revisions · CAD · BIM · history
                   │
         ┌─────────┴─────────┐
         ▼                   ▼
 Plugin Host/SDK          ARCZ Agent
 capabilities/ACL      text/photo/video
         │              typed tool plans
         └─────────┬─────────┘
                   ▼
 Specialized plugins/workers/sidecars
 terrain · roads · houses · buildings · PBR
 IFC · reconstruction · render · simulations
 CesiumJS · Kepler.gl · Blender · IfcOpenShell
                   │
                   ▼
 GLB/glTF · IFC · 3D Tiles · sheets · renders
```

The canonical authoring scene is never replaced by Cesium, Kepler, Blender or an AI model. Those systems consume/produce validated derivatives through adapters.

## Foundation currently present

- Rust workspace with world/geo/scene/CAD/BIM/procedural/application crates;
- `arcz-world` object→planet scope/layer/stream-budget contracts;
- `arcz-plugin-sdk` typed manifests/tool requests/results;
- `arcz-plugin-host` capability registry and permission gate;
- `arcz-agent` provider-agnostic action plans with revision/risk checks;
- Aedifex compatibility/parity layer;
- Earth/region/terrain/tiles/roof/facade/vegetation/procedural foundations;
- Python application/server/workers from the V10.1 foundation;
- Cesium/Aedifex integration overlays and conversion ledgers;
- 66 plugin families / 253 declared capabilities with explicit status;
- pinned upstream manifest for Aedifex, CesiumJS, Kepler.gl and IfcOpenShell/Bonsai;
- offline-first bootstrap, validation and test tooling.

`contract_ready` is not the same as production-ready. See `docs/governance/DEFINITION_OF_DONE.md` and `IMPLEMENTATION_STATUS_V11.json` for the claim policy.

## Upstream strategy

ARCZ intentionally does **not** blind-merge large open-source projects.

- **Aedifex** — architectural editor/AI authoring reference; port to Rust behind parity fixtures.
- **CesiumJS** — globe/3D Tiles presentation sidecar.
- **Kepler.gl** — large geospatial analytics/visualization sidecar.
- **IfcOpenShell** — IFC engine boundary.
- **Bonsai** — GPL application/reference integration kept outside the permissive core boundary.

Pinned revisions are in `upstreams/manifest.toml`. Materialize them with `tools/materialize_upstreams.py` on a machine with GitHub access.

## Repository map

```text
crates/          Rust kernel/domain/application crates
app/             retained V10.1 web/application assets
arcz_server/     local service layer
workers/         isolated local workers
plugins/         capability catalog
integrations/    Aedifex and external integration layers
upstreams/       immutable upstream manifest/materialization target
docs/            architecture, audit, plans and engineering governance
tools/           verification/materialization/build utilities
tests*/          Python/JS and other checks
```

## Documentation — read in this order

1. `LEIA-PRIMEIRO.md`
2. `ARCHITECTURE.md`
3. `docs/product/ARCZ_PRODUCT_SPEC.md`
4. `docs/architecture/ARCZ_V11_MASTER_PLAN.md`
5. `docs/architecture/WORLD_SCALE_ARCHITECTURE.md`
6. `docs/geo/ADDRESS_TO_WORLD_PIPELINE.md`
7. `docs/architecture/PLUGIN_ARCHITECTURE.md`
8. `docs/ai/ARCZ_AGENT.md`
9. `docs/roadmap/MASTER_EXECUTION_PLAN.md`
10. `docs/governance/DEFINITION_OF_DONE.md`
11. `docs/testing/VALIDATION_STRATEGY.md`
12. `docs/licenses/LICENSE_BOUNDARIES.md`

`docs/README.md` is the full documentation index.

## Running the retained application foundation

Read `QUICKSTART.md`. Windows entry points retained from V10.1:

```text
install.ps1
run.bat
stop.bat
uninstall.ps1
```

Before interactive startup, `run.bat` calls `tools/runtime_preflight.py --profile interactive`. The preflight should fail closed when heavyweight pinned runtimes required by a requested mode are not present rather than silently presenting a fake implementation.

## Foundation validation

```bash
python -m pytest -q
node --test --experimental-default-type=module tests_js/*.mjs
python tools/verify_plugin_catalog.py
python tools/materialize_upstreams.py --dry-run
python tools/verify_handoff.py
```

Rust compilation is an explicit gate and must be executed on a development machine with the configured Rust toolchain; lack of a toolchain is recorded as `BLOCKED`, not converted into success.

## Build order

The dependency-ordered implementation plan is in `docs/roadmap/MASTER_EXECUTION_PLAN.md`:

`repository truth → scene/plugin host → Aedifex parity → address-to-project → world generators → asset reconstruction → BIM/docs → render/simulation → city/planet streaming → collaboration/ecosystem`.

## License

ARCZ-owned Rust workspace code declares `MIT OR Apache-2.0` unless a file says otherwise. Third-party code, datasets, imagery, model weights and assets retain their own terms. See `LICENSE`, `THIRD_PARTY_NOTICES.md` and `docs/licenses/LICENSE_BOUNDARIES.md`.

## Compatibility vocabulary retained for V10 validation

The retained foundation is still identified in historical validation as **Global V10**. Its building-authoring reference boundary remains the **Aedifex Building Authoring Kernel** while capabilities migrate behind ARCZ Rust parity tests.
