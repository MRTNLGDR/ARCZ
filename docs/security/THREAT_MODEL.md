# ARCZ Threat Model

## Protected assets

Canonical project revisions, user files/assets, local model data, credentials for optional remote services, plugin permissions, provenance/audit history and generated intellectual property.

## Main risks

1. malicious/untrusted plugin mutates or exfiltrates projects;
2. AI tool call performs unintended destructive action;
3. path traversal or arbitrary filesystem access in import/export workers;
4. remote content causes unsafe parser behavior;
5. stale revision overwrites newer edits;
6. unsigned/tampered generated artifacts enter a project;
7. network-enabled provider leaks project/reference data;
8. GPL/other third-party boundary is accidentally merged into incompatible core distribution.

## Controls in the architecture

- capability-based plugin manifests and grants;
- network policy defaults to none;
- local workers isolated behind typed requests;
- expected-revision guards;
- dry-run/approval for mutating AI actions;
- content hashes/provenance;
- immutable upstream materialization;
- allowlisted project/storage roots;
- explicit import validation before canonicalization.

## Required future hardening

- WASI/component sandbox for third-party plugins;
- signed plugin packages and trust store;
- per-plugin filesystem/network scopes;
- secret store integration rather than plaintext `.env` secrets;
- fuzzing for file/import parsers;
- SBOM/dependency/license scanning in CI;
- reproducible release signing;
- optional encrypted project storage.
