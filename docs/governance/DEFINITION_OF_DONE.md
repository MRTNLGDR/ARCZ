# Definition of Done

ARCZ uses four implementation states.

## Implemented

Source exists, is wired to the real runtime path, has relevant tests, handles failures, persists/returns real data, and is exercised by at least one integration path.

## Partial

A real implementation exists, but one or more production gates are missing: dependency materialization, performance, parity, full error handling, target-platform test, UI integration, or E2E evidence.

## Contract-ready

The manifest/API/data contract is designed and validated, but the capability runtime is not yet complete. This is useful architecture, not a claim of working functionality.

## Blocked

A known dependency or environment prevents verification/implementation. Blocked items stay visible and never become green through mocks.

## Mandatory evidence for “done” features

- source path and owner crate/plugin;
- automated tests;
- validation of malformed input;
- no silent network dependency in offline profile;
- revision/undo behavior for edits;
- provenance for imported/generated assets;
- license boundary documented when third-party code/data is involved;
- performance budget for world/render-heavy features;
- user-visible error instead of fake completion.
