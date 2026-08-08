# License boundaries

ARCZ core workspace declares `MIT OR Apache-2.0`. Third-party components retain their own licenses and notices.

## Pinned upstream policy

See `upstreams/manifest.toml` and `THIRD_PARTY_NOTICES.md` for exact repository/revision tracking.

- Aedifex: permissive MIT upstream; retain notices when code is copied/ported.
- CesiumJS: Apache-2.0 upstream; preserve notice/license obligations.
- Kepler.gl: MIT upstream; preserve notice/license obligations.
- IfcOpenShell: LGPL family boundary; use through a defined engine/worker boundary and comply with redistribution requirements.
- Bonsai: GPL application/subtree boundary; treat as external/reference integration rather than silently absorbing GPL code into the permissive ARCZ core.

## Data/model licensing

Code license does not grant rights to map imagery, scans, textures, trained model weights, datasets or generated third-party assets. Every data/model adapter must record its own license/source/provenance. “Available on the internet” is not a license.
