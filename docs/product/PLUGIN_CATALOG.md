# ARCZ Plugin Catalog (generated)

Machine-readable source: `plugins/catalog.json` — 66 plugin families.

| ID | Name | Status | Runtime | Capabilities |
|---|---|---|---|---|
| `core.aedifex` | Aedifex compatibility & Rust parity | `implemented` | `builtin_rust` | `scene.import.aedifex`, `scene.export.aedifex`, `authoring.parity` |
| `geo.region` | Region / parcel selector | `implemented` | `builtin_rust` | `geo.select`, `geo.anchor`, `parcel.lock` |
| `geo.earth` | Earth / globe bridge | `implemented` | `builtin_rust` | `earth.camera`, `earth.tiles`, `geo.ecef`, `geo.enu` |
| `cad.core` | Parametric CAD authority | `implemented` | `builtin_rust` | `cad.read`, `cad.write`, `cad.undo`, `cad.constraints` |
| `bim.core` | BIM quantities & semantics | `implemented` | `builtin_rust` | `bim.quantities`, `bim.cost`, `bim.classification` |
| `terrain` | Terrain generator | `implemented` | `builtin_rust` | `terrain.dem`, `terrain.mesh`, `terrain.material` |
| `roads` | Roads / avenues / lanes | `implemented` | `builtin_rust` | `road.generate`, `road.markings`, `road.sidewalk` |
| `buildings` | Building massing generator | `implemented` | `builtin_rust` | `building.generate`, `building.facade`, `building.roof` |
| `houses` | House generator | `implemented` | `builtin_rust` | `house.generate`, `house.roof`, `house.openings` |
| `vegetation` | Trees / grass / shrubs | `implemented` | `builtin_rust` | `vegetation.scatter`, `vegetation.tree`, `vegetation.grass` |
| `materials.pbr` | PBR material authoring | `partial` | `builtin_rust` | `material.pbr`, `material.project`, `material.bake` |
| `bridges` | Bridges / viaducts | `contract_ready` | `builtin_rust` | `bridge.generate`, `bridge.deck`, `bridge.supports` |
| `water` | Water / coast / rivers / pools | `contract_ready` | `builtin_rust` | `water.surface`, `water.flow`, `water.ocean` |
| `atmosphere` | Sky / clouds / fog | `partial` | `builtin_rust` | `sky.physical`, `cloud.volumetric`, `fog.volumetric` |
| `solar` | Sun / moon / shadows / study | `partial` | `builtin_rust` | `sun.ephemeris`, `moon.ephemeris`, `solar.study`, `shadow.study` |
| `urban.furniture` | Signs / poles / traffic lights | `contract_ready` | `builtin_rust` | `urban.sign`, `urban.lightpole`, `urban.trafficlight` |
| `vehicles` | Vehicles & traffic animation | `contract_ready` | `builtin_rust` | `vehicle.import`, `vehicle.generate`, `traffic.simulate`, `vehicle.animate` |
| `characters` | Characters / skeleton / animation | `contract_ready` | `local_process` | `character.import`, `character.rig`, `character.retarget`, `character.animate`, `asset.glb` |
| `asset.import` | Universal 3D import + GLB | `partial` | `builtin_rust` | `asset.import`, `asset.normalize`, `asset.lod`, `asset.glb` |
| `furniture.table-chair` | Tables / chairs generator | `contract_ready` | `builtin_rust` | `furniture.table`, `furniture.chair`, `furniture.parametric` |
| `furniture.kitchenware` | Plates / glasses / tableware | `contract_ready` | `builtin_rust` | `prop.plate`, `prop.glass`, `prop.tableware` |
| `textiles` | Rugs / curtains / fabrics | `contract_ready` | `builtin_rust` | `textile.rug`, `textile.curtain`, `textile.fabric`, `cloth.simulate` |
| `stairs` | Stairs / ramps / escalators | `contract_ready` | `builtin_rust` | `stair.generate`, `ramp.generate`, `escalator.generate` |
| `mep` | MEP systems | `contract_ready` | `local_process` | `mep.hvac`, `mep.plumbing`, `mep.electrical`, `mep.ports` |
| `ifc` | IFC import/export/validation | `contract_ready` | `local_process` | `ifc.read`, `ifc.write`, `ifc.validate`, `ifc.bcf`, `ifc.ids` |
| `render` | Photoreal renderer orchestration | `partial` | `local_process` | `render.cycles`, `render.passes`, `render.8k`, `render.queue` |
| `physics` | Rigid-body physics | `contract_ready` | `builtin_rust` | `physics.rigid`, `physics.collider`, `physics.vehicle` |
| `sheets` | Architectural sheets / drawings | `partial` | `builtin_rust` | `sheet.layout`, `drawing.plan`, `drawing.section`, `drawing.elevation`, `schedule.table` |
| `photogrammetry` | Photo/video reconstruction | `contract_ready` | `local_process` | `reconstruct.photos`, `reconstruct.video`, `reconstruct.mesh`, `reconstruct.texture` |
| `geo.reconstruction` | Address-to-3D reconstruction | `contract_ready` | `builtin_rust` | `geo.osm`, `geo.dem`, `geo.imagery`, `geo.semantic`, `geo.reconstruct` |
| `ai.agent` | Local multimodal design agent | `partial` | `builtin_rust` | `agent.plan`, `agent.toolcall`, `agent.photo.reference`, `agent.cad.write`, `agent.approval` |
| `analytics.kepler` | Kepler geospatial analytics | `contract_ready` | `web_sidecar` | `analytics.layer`, `analytics.filter`, `analytics.aggregate` |
| `globe.cesium` | CesiumJS globe | `contract_ready` | `web_sidecar` | `globe.render`, `tiles.3d`, `terrain.stream`, `camera.flyto` |
| `core.world` | World partition / layer authority | `partial` | `builtin_rust` | `world.scope`, `world.cells`, `world.layers`, `world.streaming-budget` |
| `core.plugin-host` | Plugin registry / capability permissions | `partial` | `builtin_rust` | `plugin.register`, `plugin.discover`, `plugin.authorize`, `plugin.capability.resolve` |
| `geo.cadastre` | Cadastre / parcel sources | `contract_ready` | `local_process` | `cadastre.import`, `cadastre.parcel`, `cadastre.boundary`, `cadastre.attributes` |
| `geo.imagery` | Imagery / orthophoto sources | `contract_ready` | `local_process` | `imagery.import`, `imagery.orthophoto`, `imagery.reproject`, `imagery.tile` |
| `geo.pointcloud` | Point cloud ingestion | `contract_ready` | `local_process` | `pointcloud.import`, `pointcloud.classify`, `pointcloud.lod`, `pointcloud.mesh` |
| `world.biomes` | Biome / landcover generator | `contract_ready` | `builtin_rust` | `biome.classify`, `biome.generate`, `landcover.generate`, `ecosystem.scatter` |
| `world.geology` | Geology / subsurface layers | `contract_ready` | `builtin_rust` | `geology.layer`, `geology.strata`, `ground.soil`, `ground.excavation` |
| `world.agriculture` | Agriculture / fields | `contract_ready` | `builtin_rust` | `agriculture.field`, `agriculture.crop`, `agriculture.orchard`, `agriculture.season` |
| `infrastructure.rail` | Rail / tram / metro | `contract_ready` | `builtin_rust` | `rail.generate`, `rail.track`, `rail.station`, `rail.catenary` |
| `infrastructure.tunnel` | Tunnels / underground | `contract_ready` | `builtin_rust` | `tunnel.generate`, `tunnel.portal`, `tunnel.section`, `underground.network` |
| `infrastructure.utilities` | Utility networks | `contract_ready` | `local_process` | `utility.power`, `utility.water`, `utility.sewer`, `utility.telecom`, `utility.gas` |
| `transit.public` | Public transit systems | `contract_ready` | `builtin_rust` | `transit.route`, `transit.stop`, `transit.schedule`, `transit.simulate` |
| `hydrology` | Hydrology / drainage / flood | `contract_ready` | `builtin_rust` | `hydrology.watershed`, `hydrology.drainage`, `hydrology.flood`, `hydrology.river` |
| `weather` | Weather / climate layer | `contract_ready` | `local_process` | `weather.state`, `weather.wind`, `weather.rain`, `weather.snow`, `climate.profile` |
| `crowds` | Pedestrians / crowds | `contract_ready` | `builtin_rust` | `crowd.spawn`, `crowd.path`, `crowd.simulate`, `pedestrian.animate` |
| `animals` | Animals / wildlife | `contract_ready` | `builtin_rust` | `animal.spawn`, `animal.rig`, `animal.animate`, `wildlife.simulate` |
| `navigation` | Navigation / pathfinding | `contract_ready` | `builtin_rust` | `nav.mesh`, `nav.path`, `nav.agent`, `nav.accessibility` |
| `lighting` | Architectural / urban lighting | `contract_ready` | `builtin_rust` | `light.fixture`, `light.photometry`, `light.street`, `light.study` |
| `cinema` | Cameras / timeline / shots | `contract_ready` | `builtin_rust` | `camera.shot`, `camera.path`, `timeline.animate`, `cinema.render-plan` |
| `acoustics` | Acoustic analysis | `contract_ready` | `local_process` | `acoustic.room`, `acoustic.ray`, `acoustic.reverb`, `acoustic.noise-map` |
| `energy` | Energy / daylight / performance | `contract_ready` | `local_process` | `energy.model`, `energy.thermal`, `energy.daylight`, `energy.report` |
| `zoning` | Planning / zoning rules | `contract_ready` | `local_process` | `zoning.import`, `zoning.rules`, `zoning.envelope`, `zoning.validate` |
| `collaboration` | Collaboration / presence | `contract_ready` | `local_process` | `collab.presence`, `collab.comment`, `collab.lock`, `collab.sync` |
| `versioning.spatial` | Spatial branching / merge | `contract_ready` | `builtin_rust` | `version.branch`, `version.diff`, `version.merge`, `version.restore` |
| `assets.library` | Asset library / indexing | `contract_ready` | `builtin_rust` | `asset.catalog`, `asset.search`, `asset.version`, `asset.license` |
| `materials.scan` | Material scan / PBR extraction | `contract_ready` | `local_process` | `material.scan`, `material.albedo`, `material.normal`, `material.roughness`, `material.displacement` |
| `reconstruction.neural` | Local neural 3D reconstruction | `contract_ready` | `local_process` | `reconstruct.single-image`, `reconstruct.multiview`, `reconstruct.gaussian`, `reconstruct.neural-mesh` |
| `simulation.events` | World events / scenario system | `contract_ready` | `builtin_rust` | `scenario.create`, `scenario.timeline`, `event.trigger`, `event.replay` |
| `measurements` | Survey / measurement tools | `contract_ready` | `builtin_rust` | `measure.distance`, `measure.area`, `measure.volume`, `measure.slope` |
| `exports.engines` | Game/engine export adapters | `contract_ready` | `local_process` | `export.godot`, `export.bevy`, `export.unreal`, `export.unity` |
| `exports.usd` | USD / interchange adapter | `contract_ready` | `local_process` | `usd.read`, `usd.write`, `usd.stage`, `usd.material` |
| `exports.gis` | GIS export adapters | `contract_ready` | `builtin_rust` | `export.geojson`, `export.gpkg`, `export.tiles`, `export.raster` |
| `reality.xr` | AR / VR / spatial presentation | `contract_ready` | `web_sidecar` | `xr.session`, `xr.anchor`, `xr.teleport`, `xr.review` |
