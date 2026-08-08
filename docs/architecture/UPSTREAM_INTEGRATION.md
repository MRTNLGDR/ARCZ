# Upstream integration and conversion strategy

ARCZ does not copy entire external repositories into one namespace and hope they keep working. Each upstream is pinned, kept immutable, and connected by a narrow adapter/parity boundary.

## Aedifex

Architectural authoring/AI reference during Rust migration. Preserve the pinned source, inventory schemas/node kinds/tools/behavior, build neutral golden fixtures, port capability-by-capability to Rust, compare semantics/topology/exports, and switch authority only after parity gates pass.

## CesiumJS

Globe/terrain/3D Tiles web presentation. Cesium is a streaming/view sidecar, never the editable project database. ARCZ publishes georeferenced derivatives into it.

## Kepler.gl

Large geospatial visualization/analytics sidecar. Analytical results return to ARCZ only through typed operations with provenance.

## IfcOpenShell / Bonsai

IfcOpenShell is used through an isolated IFC engine/worker boundary. Bonsai is treated as a GPL application/reference integration rather than silently absorbing GPL code into the permissive ARCZ core.

## Upgrade rules

- pin commit hashes;
- preserve licenses/notices;
- never edit materialized upstream source in place;
- put ARCZ adapters/patches in ARCZ-owned paths;
- upgrade through explicit PRs and compatibility reports;
- do not claim equivalence without golden/parity evidence.
