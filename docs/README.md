# ARCZ Documentation Index

Read ARCZ in this order when implementing or reviewing the system.

1. [`../README.md`](../README.md) — product overview and repository map.
2. [`../ARCHITECTURE.md`](../ARCHITECTURE.md) — non-negotiable architectural invariants.
3. [`product/ARCZ_PRODUCT_SPEC.md`](product/ARCZ_PRODUCT_SPEC.md) — product mission, user journeys and quality bar.
4. [`architecture/WORLD_SCALE_ARCHITECTURE.md`](architecture/WORLD_SCALE_ARCHITECTURE.md) — coordinates, cells, layers, LOD and world job graph.
5. [`geo/ADDRESS_TO_WORLD_PIPELINE.md`](geo/ADDRESS_TO_WORLD_PIPELINE.md) — address/parcel/region to locked editable project.
6. [`architecture/PLUGIN_ARCHITECTURE.md`](architecture/PLUGIN_ARCHITECTURE.md) — modular runtime and capability/permission model.
7. [`ai/ARCZ_AGENT.md`](ai/ARCZ_AGENT.md) — safe multimodal AI tool-use loop.
8. [`architecture/UPSTREAM_INTEGRATION.md`](architecture/UPSTREAM_INTEGRATION.md) — Aedifex/Cesium/Kepler/IfcOpenShell/Bonsai parity strategy.
9. [`roadmap/MASTER_EXECUTION_PLAN.md`](roadmap/MASTER_EXECUTION_PLAN.md) — dependency-ordered implementation waves and gates.
10. [`governance/DEFINITION_OF_DONE.md`](governance/DEFINITION_OF_DONE.md) — implemented/partial/contract-ready/blocked claim policy.
11. [`testing/VALIDATION_STRATEGY.md`](testing/VALIDATION_STRATEGY.md) — unit→parity→E2E→scale validation.
12. [`security/THREAT_MODEL.md`](security/THREAT_MODEL.md) — trust boundaries and controls.
13. [`licenses/LICENSE_BOUNDARIES.md`](licenses/LICENSE_BOUNDARIES.md) — software/data/model license boundaries.
14. [`../IMPLEMENTATION_STATUS_V11.json`](../IMPLEMENTATION_STATUS_V11.json) — machine-readable current status.
15. [`../upstreams/manifest.toml`](../upstreams/manifest.toml) — exact pinned upstream revisions.

## Engineering rule

Documentation is part of the product contract. When implementation changes canonical scene semantics, world coordinates, plugin permissions, upstream boundaries, AI mutation policy or status claims, update the corresponding document in the same PR.
