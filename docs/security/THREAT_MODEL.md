# ARCZ Threat Model

ARCZ combines CAD/BIM authoring, filesystem assets, plugins, local AI, external datasets and optional network providers. A corrupted project or silently altered world coordinate can be more damaging than an ordinary UI error, so security boundaries are part of architecture.

## Protected assets

Canonical project revisions, georeferencing/units, imported private files, credentials/provider tokens, plugin packages, generated artifacts, provenance records, export integrity and local model/data caches.

## Major threats

- malicious or buggy plugins mutating project truth;
- prompt/tool injection causing destructive AI actions;
- stale concurrent writes overwriting newer scene revisions;
- path traversal or unsafe archive/model import;
- parser bugs in complex CAD/BIM/3D formats;
- unexpected remote network/data exfiltration;
- untrusted shaders/assets exhausting GPU/CPU/memory;
- dependency/update supply-chain compromise;
- source-data/license attribution being stripped;
- coordinates/units/datum silently reinterpreted.

## Baseline controls

1. capability-based plugin registry and explicit grants;
2. process/WASM isolation for high-risk or copyleft/heavy integrations;
3. typed AI tool protocol, dry-run/preview and approval for mutations/external effects;
4. `expected_revision` optimistic concurrency guard;
5. path allowlists and archive extraction limits;
6. parser fuzzing/property tests for hostile formats;
7. network disabled by default; remote providers explicitly enabled;
8. stream/job budgets for world-scale workloads;
9. content hashes/provenance for source and generated artifacts;
10. pinned upstream commits/dependencies and controlled upgrade PRs;
11. canonical project backups/revision log before destructive migrations;
12. visible coordinate frame/unit/source confidence in engineering workflows.

## AI rule

A model response is untrusted input. It cannot grant itself a capability, execute arbitrary shell commands, overwrite a project revision, install executable code or silently send project data to a remote provider.
