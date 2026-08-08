# ARCZ Plugin Architecture

## Goal

Everything specialized should be modular without turning ARCZ into an unsafe collection of scripts. Plugins expose capabilities to the kernel; they do not directly become the kernel or own project truth.

## Runtime kinds

- `builtin_rust` — first-party/in-process Rust.
- `wasm_component` — portable sandboxable plugin target.
- `local_process` — isolated worker for Blender, IfcOpenShell, reconstruction and heavyweight tools.
- `web_sidecar` — browser/WebGPU system such as CesiumJS or Kepler.gl.

## Required manifest data

Stable ID/version/API version, runtime/entrypoint, capabilities, read/write domains, network policy, determinism expectation, optional GPU requirement and provenance metadata.

## Permission model

Registration is not authorization. The active project/policy grants capabilities. Future hosts additionally gate filesystem paths, network destinations, subprocesses and GPU resources. Remote network is opt-in and visible.

## Mutation protocol

A scene-changing plugin receives a typed request with project ID and `expected_revision`. It returns source/result revision, changed node IDs, artifacts and diagnostics. Stale writes are rejected.

`plan → dry-run → preview/diff → validate → approval policy → commit revision → publish derivatives`.

## Families

ARCZ organizes specialized capabilities into independent families: terrain/geology, parcels, roads/rail/bridges/tunnels, houses/buildings/facades/roofs, vegetation/biomes/agriculture, water/weather, assets/furniture/textiles/materials, BIM/MEP/IFC, traffic/vehicles/characters/crowds, lighting/acoustics/energy, rendering/cinema, physics/events, GIS analytics, engine/GIS exports and collaboration/versioning.

If an upstream already solves a hard problem, ARCZ wraps it first and only replaces it after parity tests.
