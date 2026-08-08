# ARCZ local multimodal agent

The agent turns language, images, video and scene context into **plans and typed tool calls**, not opaque geometry blobs.

## Loop

1. understand intent/scope;
2. read project revision, selection, constraints and references;
3. resolve capabilities from the plugin host;
4. build a dependency-ordered action plan;
5. run analysis/dry-runs;
6. show ghost preview/diff;
7. validate CAD/BIM/scene rules;
8. request approval when policy requires it;
9. execute against `expected_revision`;
10. verify/commit or roll back;
11. attach provenance/artifacts and summarize changes.

## Provider model

Provider-agnostic core. Local LLM/VLM/embeddings/3D models are preferred for the default profile. Remote providers are optional plugins requiring explicit network permission.

## Example: “make this chair” from photos

Segmentation → camera/scale estimation → reconstruction/model → mesh cleanup → UV/PBR → bake → LOD/collider → validation → asset library → scene placement. A text-only fallback is marked approximate rather than claiming an identical scan.

## Security boundary

- no raw shell by default;
- no unrestricted filesystem/network access;
- revision guard on mutations;
- destructive actions previewed/approved;
- model/tool/seed/config provenance where available;
- generated plugin code must enter normal build/test/review gates before execution.
