# Validation Strategy

## Test layers

### Unit

Pure geometry, geodesy, plugin contracts, parsers, revision logic and deterministic generators.

### Schema/contract

JSON schemas, plugin catalog, manifests, tool requests/results and migrations.

### Golden geometry

Known inputs produce canonical meshes/scene graphs/IFC structures within tolerances. Hashes are used only where representation is intended to be byte-stable.

### Aedifex parity

Pinned Aedifex scenes/actions are replayed and compared against ARCZ ports for node semantics, dimensions, topology and exports.

### GIS alignment

Known anchors/parcels/roads/terrain are checked against reference coordinates and tolerances.

### E2E

Address → lock → reconstruct → author → export → Cesium/render, plus reference image → asset → placement → GLB.

### Performance

World-cell streaming, generation queues, GPU memory, render frame time, city-scale dataset loading and 8K offline jobs.

## CI matrix target

- Linux: Rust/Python/JS unit + schema + non-GPU integration;
- Windows: desktop/bootstrap/path/worker integration;
- Web: WASM/WebGPU build + browser smoke tests;
- GPU/nightly: render/reconstruction/large-scene tests on dedicated runner.

## Failure policy

Missing optional heavyweight dependencies may produce a clearly identified skipped/blocked test. Core correctness tests may not be silently skipped.
