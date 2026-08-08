# Upstream integration and conversion strategy

ARCZ does not copy entire external repositories into one namespace and hope they keep working. Each upstream is pinned in `upstreams/manifest.toml`, materialized outside the canonical ARCZ crates, and connected by a narrow adapter.

## Aedifex

Role: authoring reference/oracle for architectural editing and AI tool semantics during migration.

Strategy:

1. preserve pinned upstream source unchanged;
2. inventory schemas, node kinds, geometry systems, tools and user-visible behavior;
3. define ARCZ-neutral fixtures/golden scenes;
4. port authoritative domain/geometry behavior to Rust crate-by-crate;
5. run parity tests against the pinned upstream;
6. switch a capability to the Rust implementation only when parity gates pass;
7. retain a compatibility importer/exporter for Aedifex scene data.

## CesiumJS

Role: proven globe/3D Tiles web presentation and camera/terrain ecosystem.

Cesium is a presentation/streaming sidecar, not the editable scene database. ARCZ publishes georeferenced derivatives into Cesium while authoring stays in the ARCZ scene/world authority.

## Kepler.gl

Role: geospatial visualization/analytics sidecar for filters, layers and large datasets. Analytical results may be written back only through typed ARCZ operations with provenance.

## IfcOpenShell / Bonsai

Role: IFC processing and reference BIM workflows.

IfcOpenShell is consumed through an isolated worker/API boundary. Bonsai is treated as a GPL application/reference integration, not copied into the permissive ARCZ core. This avoids accidental license boundary violations while retaining interoperability.

## General upstream rules

- keep original copyright/license files;
- pin commit hashes;
- never edit materialized upstream directories in place;
- put ARCZ patches/adapters in ARCZ-owned directories;
- record compatibility tests per upstream commit;
- upgrades are explicit PRs with generated diff/inventory reports;
- no feature is claimed equivalent until golden/parity tests pass.
