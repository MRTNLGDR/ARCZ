# ARCZ Validation Strategy

ARCZ must prove behavior at several layers because a green unit test cannot prove that a city streams correctly or that an IFC round-trip preserved semantics.

## Test pyramid

### Contract/unit
Rust/Python/JS unit tests for geodesy, revisions, schemas, plugin permissions, AI action plans, CAD constraints, deterministic generators and parsers.

### Property/fuzz
Geometry invariants, hostile CAD/BIM/3D inputs, archive/path handling, coordinate conversions, scene migrations and plugin manifest parsing.

### Golden/parity
Representative Aedifex scenes and operations become neutral fixtures. Rust ports are compared for node semantics, geometry/topology, material assignments, undo/history behavior and export results within documented tolerances.

### Integration
Address→anchor→terrain/roads/buildings→authoring promotion; IFC worker round-trip; Cesium 3D Tiles publication; Kepler analytics adapter; Blender/Cycles render; local AI tool plan→preview→commit.

### End-to-end
Windows desktop + browser/WebGPU on target hardware, offline mode, large project save/reopen, failure recovery and installer/bootstrap.

### Scale/performance
Parcel, neighborhood, city and region datasets with explicit budgets for frame time, resident cells, geometry/texture/GPU memory, generation throughput, cache size and recovery after worker failure.

## Required CI checks once the complete tree is imported

```bash
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python -m pytest -q
node --test --experimental-default-type=module tests_js/*.mjs
python tools/verify_plugin_catalog.py
python tools/materialize_upstreams.py --dry-run
python tools/verify_handoff.py
```

Blocked environment-dependent gates remain `blocked`; they are never rewritten to pass merely to make a dashboard green.
