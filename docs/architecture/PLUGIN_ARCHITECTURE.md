# ARCZ Plugin Architecture

## Goal

Everything specialized should be modular without turning ARCZ into an unsafe collection of scripts. Plugins expose capabilities to the kernel; they do not directly become the kernel.

The contracts start in `crates/arcz-plugin-sdk`, the capability/permission registry in `crates/arcz-plugin-host`, and the human-readable catalog in `plugins/catalog.json`.

## Runtime kinds

- `builtin_rust` — first-party/in-process Rust capability.
- `wasm_component` — sandboxable portable plugin target.
- `local_process` — isolated worker for tools such as Blender/IfcOpenShell/reconstruction.
- `web_sidecar` — browser/WebGPU application such as CesiumJS or Kepler.gl.

## Required plugin properties

Every plugin declares:

- stable ID/version/API version;
- runtime and entrypoint;
- capabilities;
- read/write domains;
- network policy;
- determinism expectations;
- optional GPU requirement;
- provenance metadata.

## Permission model

Registration is not authorization. A plugin can only execute a capability when the active project/policy grants it. Future hosts should additionally gate filesystem paths, network destinations, GPU queues and subprocess execution.

Remote network access is opt-in. A plugin declaring `explicit_remote` must also declare a remote-network capability so the UI can expose the effect clearly.

## Mutation protocol

A scene-changing plugin receives a `ToolRequest` containing project ID and `expected_revision`. The result reports source/result revision, changed node IDs, artifacts and diagnostics. The kernel rejects stale writes.

The intended transaction is:

`plan → dry-run → preview/diff → validate → approve policy → commit revision → publish derivatives`.

## Plugin families

The catalog covers current implemented/partial contracts plus expansion families for world creation: buildings/houses, infrastructure, terrain/geology, vegetation/biomes, water/weather, assets/furniture/textiles, BIM/MEP, traffic/characters/crowds, render/film, physics/simulation, GIS analytics, collaboration, time/versioning and exports.

Status is explicit: `implemented`, `partial`, `contract_ready`, or `blocked`.

## Compatibility philosophy

If an upstream already solves a hard problem well, ARCZ wraps it first. Reimplementation in Rust only replaces an upstream path after parity tests demonstrate that the replacement does not regress the required capability.
