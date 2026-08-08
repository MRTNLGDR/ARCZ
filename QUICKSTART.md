# ARCZ Quickstart

## 1. Clone the repository

```bash
git clone https://github.com/MRTNLGDR/ARCZ.git
cd ARCZ
git switch agent/arcz-world-foundation
```

This branch is the published V11.1 world-scale foundation. The retained 565-file V10.1/V11.1 application tree is still being imported under issue #1, so do not confuse this branch with the complete desktop/web application yet.

## 2. Prerequisites for the published Rust foundation

- Git
- Rust 1.82+ with Cargo, rustfmt and Clippy
- Python 3.11+ for the upstream materializer (`tomllib`)

## 3. Validate Rust contracts

```bash
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The current workspace exercises the published ARCZ plugin SDK/host, world authority/world-generation DAG and agent contracts.

## 4. Inspect upstream pins without downloading

```bash
python tools/materialize_upstreams.py --dry-run
```

## 5. Materialize immutable upstream snapshots

```bash
python tools/materialize_upstreams.py
```

This clones exact commits listed in `upstreams/manifest.toml` into `upstreams/sources/*`, checks out detached pinned revisions and writes `.arcz-upstream.json` provenance stamps. It refuses to overwrite a dirty upstream unless `--reset` is explicitly requested.

Do **not** develop ARCZ patches directly inside materialized upstreams. Keep ARCZ-owned adapters/ports in ARCZ paths and upgrade upstream commits through reviewed changes to the manifest.

## 6. Read before implementing

Start with `docs/README.md`, especially architecture invariants, capability matrix, address-to-world pipeline, plugin/AI boundaries, roadmap and definition-of-done.

## Complete application bootstrap

The retained V10.1/V11.1 tree contains additional Python server/workers, Windows/Linux scripts, Aedifex/Cesium integration overlays, tests, plugin catalog and application assets. Its complete import and target-hardware validation is explicitly tracked in issue #1. Until that is complete, missing runtime surfaces must remain marked `blocked`/`partial`; do not replace them with mocks and call the application finished.
