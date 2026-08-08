# ARCZ Capability Matrix

This matrix turns the original ARCZ vision into modular implementation domains. `implemented` means a real path exists in the retained local foundation; `partial` means a real path exists with missing gates; `contract_ready` means the module/API is designed but must not be presented as finished.

## Foundation / world / GIS

| Module | Purpose | Status |
|---|---|---|
| Aedifex parity | Preserve/port architectural authoring behavior to Rust | implemented/local foundation |
| Region / parcel selector | lot/block/neighborhood/city/state selection + lock | implemented/local foundation |
| Earth bridge | globe camera, ECEF/ENU, 3D world context | implemented/local foundation |
| World authority | object→planet scope, layers, cells, LOD, budgets | contract_ready/published |
| Terrain | DEM/mesh/material/earthworks | implemented/local foundation |
| Parcels/cadastre | parcel boundaries, metadata, setbacks | contract_ready |
| GIS analytics | vector/raster/spatial queries | contract_ready |
| Cesium adapter | globe/terrain/3D Tiles presentation | contract_ready |
| Kepler adapter | large geospatial analytics/visualization | contract_ready |
| Geocoder/source manager | address→coordinates/scope/source coverage | contract_ready |
| Survey/measurements | distance/area/volume/slope/control points | contract_ready |

## Architecture / CAD / BIM

| Module | Purpose | Status |
|---|---|---|
| Parametric CAD | direct scene/CAD authoring, constraints, undo | implemented/local foundation |
| Building generator | massing, floors, facades, roofs | implemented/local foundation |
| House generator | residential-specific procedural generation | implemented/local foundation |
| Walls/openings/zones | Aedifex-compatible architectural kernel | implemented/local foundation |
| Slabs/ceilings/roofs | multi-level building envelope | implemented/local foundation |
| Facade system | openings, panels, balconies, materials | partial |
| Roof system | flat/pitched/procedural roof families | partial |
| Stairs | straight/L/U/spiral parametric stairs | contract_ready |
| Ramps | accessibility/vehicle ramps | contract_ready |
| Escalators | generated escalator assemblies/animation hooks | contract_ready |
| BIM semantics | quantities/cost/classification | implemented/local foundation |
| IFC | import/export/validation/IDS/BCF via IfcOpenShell boundary | contract_ready |
| MEP | HVAC/plumbing/electrical/ports/routing | contract_ready |
| Structural | columns/beams/slabs/foundations/analysis adapter | contract_ready |
| Zoning/planning | rules, envelope, setbacks, compliance | contract_ready |
| Sheets | plans/sections/elevations/schedules/layout | partial |
| Dimensions/annotations | technical drawing annotations | contract_ready |

## Infrastructure / city generation

| Module | Purpose | Status |
|---|---|---|
| Roads | roads/avenues/lanes/markings/sidewalks | implemented/local foundation |
| Bridges/viaducts | decks/supports/clearances | contract_ready |
| Tunnels | bores/portals/cut-and-cover | contract_ready |
| Rail | tracks/switches/stations/corridors | contract_ready |
| Public transit | routes/stops/schedules/simulation | contract_ready |
| Parking | lots/garages/stalls/circulation | contract_ready |
| Urban furniture | signs/poles/traffic lights/benches/bins | contract_ready |
| Utility networks | power/water/sewer/telecom/gas | contract_ready |
| Traffic | lanes/signals/vehicles/routing/simulation | contract_ready |
| Pedestrians/crowds | nav, spawning, walking, crowd simulation | contract_ready |
| Navigation | navmesh/pathfinding/accessibility | contract_ready |

## Nature / environment

| Module | Purpose | Status |
|---|---|---|
| Vegetation | trees/grass/shrubs/scattering | implemented/local foundation |
| Biomes | ecology-driven regional vegetation | contract_ready |
| Agriculture | fields/rows/crops/orchards | contract_ready |
| Water | ocean/coast/rivers/lakes/pools | contract_ready |
| Hydrology | watersheds/drainage/flood/river behavior | contract_ready |
| Atmosphere | physical sky/fog/volumetric clouds | partial |
| Weather | wind/rain/snow/weather state/climate profile | contract_ready |
| Sun & Moon | ephemeris/true position | partial |
| Solar study | sun/shadow/daylight analysis | partial |
| Geology | ground layers/cut-fill/geotechnical adapters | contract_ready |
| Wildlife | animals/rig/animation/simulation | contract_ready |

## Assets / reconstruction / interiors

| Module | Purpose | Status |
|---|---|---|
| Universal 3D import | normalize external models + GLB | partial |
| Photo/video reconstruction | multiview/photogrammetry pipeline | contract_ready |
| Neural 3D | single-image/multiview/gaussian/neural mesh workers | contract_ready |
| Asset library | catalog/search/version/license/provenance | contract_ready |
| PBR materials | author/project/bake materials | partial |
| Material scan | photo→albedo/normal/roughness/displacement | contract_ready |
| Tables/chairs | dedicated parametric furniture family | contract_ready |
| Cabinets/kitchens | modular cabinetry/appliances | contract_ready |
| Beds/sofas/storage | interior furniture families | contract_ready |
| Plates/glasses/tableware | small prop generator | contract_ready |
| Rugs/curtains/fabrics | textile generation + cloth hooks | contract_ready |
| Doors/windows catalog | parametric opening components | partial |
| Lighting fixtures | architectural fixtures/IES/photometry | contract_ready |

## Characters / vehicles / animation

| Module | Purpose | Status |
|---|---|---|
| Characters | import/rig/retarget/animate/export GLB | contract_ready |
| Vehicles | import/generate/rig/animate | contract_ready |
| Vehicle physics | suspension/collider/drive adapters | contract_ready |
| Crowd animation | locomotion/retarget/path behavior | contract_ready |
| Timeline | object/camera/event animation | contract_ready |
| Cameras | shots/lenses/paths/cinematic sequencing | contract_ready |

## Render / simulation / engineering

| Module | Purpose | Status |
|---|---|---|
| WebGPU viewport | real-time PBR authoring viewport | partial/Aedifex reference |
| Offline photoreal render | Blender/Cycles orchestration, passes, 8K queue | partial |
| Physics | rigid bodies/colliders/vehicle hooks | contract_ready |
| Cloth | curtains/fabrics simulation | contract_ready |
| Lighting study | fixtures/photometry/street lighting | contract_ready |
| Acoustics | room/ray/reverb/noise maps | contract_ready |
| Energy | thermal/daylight/performance reports | contract_ready |
| World scenarios | events/timeline/replay | contract_ready |
| Collision/clearance | spatial validation/constructability | contract_ready |

## Export / ecosystem

| Module | Purpose | Status |
|---|---|---|
| GLB/glTF | universal realtime/web interchange | partial |
| 3D Tiles | world/geospatial streaming derivative | contract_ready |
| IFC | BIM interchange | contract_ready |
| USD | scene/interchange adapter | contract_ready |
| GIS | GeoJSON/GPKG/tiles/raster adapters | contract_ready |
| Godot | engine export adapter | contract_ready |
| Bevy | Rust engine export adapter | contract_ready |
| Unreal | engine export adapter | contract_ready |
| Unity | engine export adapter | contract_ready |
| XR | AR/VR/spatial review | contract_ready |
| Spatial versioning | branch/diff/merge/restore | contract_ready |
| Collaboration | presence/comments/locks/sync | contract_ready |

## Agent capability

The ARCZ agent is an orchestrator over these modules. It can interpret requests/references, resolve capabilities, create plans, run dry-runs, show previews, validate and commit typed operations. It must not bypass permissions/revisions or claim that a generated approximation is a precise reconstruction.

The retained local machine-readable catalog currently validates **66 plugin families and 253 declared capabilities**; its full-tree import is tracked by issue #1.
