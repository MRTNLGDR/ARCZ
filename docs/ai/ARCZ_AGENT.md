# ARCZ local multimodal agent

## Purpose

The ARCZ agent turns language, images, video and existing scene context into **plans and typed tool calls**, not opaque geometry blobs. A user should be able to say “make a modern house on this lot”, upload a chair photo, ask for a road/bridge, or request BIM edits while retaining revision safety and inspectability.

## Provider model

The core is provider-agnostic. Local LLM/VLM/embeddings/3D models are preferred for the default profile; remote OpenAI-compatible or other providers may be optional plugins with explicit network permission.

## Agent loop

1. understand intent and scope;
2. collect project revision, selection, constraints and reference assets;
3. resolve capabilities from the plugin host;
4. produce a dependency-ordered `ActionPlan`;
5. run read-only analysis and dry-runs;
6. show ghost/preview/diff for user-visible geometry changes;
7. validate code/CAD/BIM rules;
8. request approval when policy requires it;
9. execute against `expected_revision`;
10. verify result and either commit or roll back;
11. summarize what changed and attach provenance/artifacts.

## Tool categories

- scene query/edit/selection;
- CAD constraints and geometry;
- BIM semantics/IFC;
- terrain/road/building/vegetation generators;
- asset reconstruction/import/materials;
- solar/physics/simulation;
- GIS/analytics;
- rendering/sheets/export;
- plugin discovery/configuration.

## Reference-driven object generation

For “make this chair” from photos, ARCZ should route through segmentation → camera/scale estimation → multi-view/reconstruction model if available → mesh cleanup → UV/material creation → PBR bake → LOD/collider → validation → asset library → placement. A text-only procedural fallback must be clearly labeled as approximate rather than claiming an identical scan.

## Self-improvement boundary

The agent may generate plugin source, tests or manifests, but generated code enters the repository like any other code: sandbox/build/test/review gates first. The running model cannot silently rewrite its own security/permission kernel.

## Safety/integrity

- no raw shell by default;
- no unrestricted filesystem access;
- project/revision guard on mutations;
- network disabled unless explicitly granted;
- destructive actions previewed/approved;
- generated content records model/tool/seed/config where available;
- failure never becomes a fake success state.
