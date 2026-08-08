# ARCZ documentation index

This directory is the executable design record for ARCZ. Statements marked **implemented** refer to source present in this repository; **partial** means a real implementation exists but lacks production gates; **contract-ready** means interfaces/manifests exist but the runtime implementation is not complete; **roadmap** is design only.

## Start here

- `../LEIA-PRIMEIRO.md` — operating rules and current truth.
- `../ARCHITECTURE.md` — system map and invariants.
- `product/ARCZ_PRODUCT_SPEC.md` — what the product must become.
- `architecture/ARCZ_V11_MASTER_PLAN.md` — V11 foundation decisions.
- `roadmap/MASTER_EXECUTION_PLAN.md` — implementation waves and gates.

## Architecture

- `architecture/WORLD_SCALE_ARCHITECTURE.md` — object-to-planet data model, LOD and streaming.
- `architecture/UPSTREAM_INTEGRATION.md` — Aedifex/Cesium/Kepler/IfcOpenShell/Bonsai boundaries.
- `architecture/PLUGIN_ARCHITECTURE.md` — modularity, permissions and ABI strategy.

## Authoring and AI

- `geo/ADDRESS_TO_WORLD_PIPELINE.md` — address/selection to locked editable reconstruction.
- `ai/ARCZ_AGENT.md` — local multimodal design agent and tool safety.

## Engineering governance

- `governance/DEFINITION_OF_DONE.md` — what counts as implemented.
- `security/THREAT_MODEL.md` — local-first security boundaries.
- `testing/VALIDATION_STRATEGY.md` — unit/parity/golden/E2E requirements.
- `licenses/LICENSE_BOUNDARIES.md` — third-party licensing rules.

## Existing V10/V11 evidence

The existing `audit/`, `integration/`, `adr/`, `architecture/` and status files remain authoritative historical evidence. V11 does not erase V10; it layers a cleaner Rust/world/plugin architecture over it.
