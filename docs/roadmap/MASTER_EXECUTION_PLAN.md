# ARCZ Master Execution Plan

The roadmap is dependency-ordered so ARCZ can become huge without becoming a pile of broken integrations.

## Wave 0 — repository truth
Reproducible bootstrap, pinned upstreams, license notices, CI/test runners, hashes and honest status ledger.

**Gate:** clean development machine can reproduce foundational checks.

## Wave 1 — canonical scene + plugin host
Rust scene revision/transaction API, plugin registry/capability ACL, storage/migrations, undo/history/provenance.

**Gate:** multiple plugins can mutate one project only through revision guards.

## Wave 2 — Aedifex parity
Walls/openings/zones/levels/slabs/ceilings/roofs, selection/snapping/history/materials, AI/MCP tool mapping and GLB round-trip golden fixtures.

**Gate:** representative Aedifex architectural scenes round-trip within defined tolerance.

## Wave 3 — address → locked project
Geocoder/source adapters, parcel/admin selection, WGS84/ECEF/ENU, DEM, OSM transport, building footprints/context and Cesium publication.

**Gate:** a real address becomes a reproducible aligned editable project.

## Wave 4 — world generators
Houses/buildings/facades/roofs; roads/bridges/tunnels/rail; vegetation/biomes/agriculture; hydrology; urban furniture/utilities; PBR/LOD.

**Gate:** block/neighborhood regeneration is deterministic from the same inputs.

## Wave 5 — assets/reconstruction
Universal 3D normalization, photo/video reconstruction, furniture/tableware/textiles/stairs, material capture, characters/rigging and asset library.

**Gate:** reference → validated asset → GLB → scene placement.

## Wave 6 — BIM/documentation
IfcOpenShell worker, IFC validation/IDS/BCF, quantities, MEP, plans/sections/elevations/dimensions/schedules/sheets.

**Gate:** representative building exports valid coordinated IFC/documentation.

## Wave 7 — rendering/simulation
WebGPU real-time, Blender/Cycles offline, sun/moon/solar, Rapier physics, traffic/crowds/vehicles, water/weather/atmosphere and cinema timeline.

**Gate:** same scene revision drives simulation and offline render.

## Wave 8 — city/planet
Hierarchical world partition, content-addressed tile cache, LOD/error metrics, resumable job graph, large geospatial analytics and 3D Tiles publication.

**Gate:** city-scale context streams while parcel/building remains editable at CAD precision.

## Wave 9 — ecosystem
Spatial diff/merge, collaboration, signed/permissioned plugin packages, SDK and engine adapters (Godot/Bevy/Unreal/Unity as separate targets).

**Gate:** third-party plugin can be installed/tested/removed without corrupting project truth.
