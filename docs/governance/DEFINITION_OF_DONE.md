# Definition of Done

ARCZ does not treat architecture diagrams, manifests, UI buttons or placeholder interfaces as finished features.

## Status vocabulary

- `implemented` — real runtime path is wired and exercised with relevant validation/tests.
- `partial` — real implementation exists, but one or more production gates are still missing.
- `contract_ready` — API/schema/plugin contract exists, but the complete runtime is not yet implemented.
- `blocked` — a concrete dependency, environment or external requirement prevents verification. The blocker remains visible.

## Mandatory evidence for `implemented`

1. source owner/module is identified;
2. happy-path and malformed-input behavior is tested;
3. errors are surfaced instead of silently falling back to fake results;
4. offline mode performs no undeclared network access;
5. scene mutations use revision/history/undo semantics where applicable;
6. generated/imported data records provenance and coordinate/unit assumptions;
7. third-party license/data/model terms are recorded;
8. performance/memory budget is defined for scale-sensitive features;
9. export/import round trips are validated when interchange is part of the feature;
10. user-visible state reflects actual runtime state.

## AI-specific gate

AI may propose plans and typed tool calls. It is not allowed to bypass project revision checks, plugin permissions, validation or approval rules simply because a model returned a result.

## World-scale gate

A generator is not considered world-capable until it is resumable/cell-addressable, deterministic or provenance-traceable, LOD-aware, memory-budgeted and safe to recompute without corrupting canonical authoring revisions.
