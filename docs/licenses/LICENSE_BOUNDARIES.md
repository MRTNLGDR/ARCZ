# ARCZ License Boundaries

This is an engineering boundary document, not legal advice.

## ARCZ-owned core

ARCZ-owned Rust workspace code is intended to use `MIT OR Apache-2.0` unless a file/package explicitly says otherwise. Keep copyright and license notices with distributed source/binaries as required.

## Pinned upstreams

| Upstream | Declared boundary | ARCZ use |
|---|---|---|
| Aedifex | MIT | Authoring/parity reference; adapter/port behind tests |
| CesiumJS | Apache-2.0 | Web globe/terrain/3D Tiles sidecar |
| Kepler.gl | MIT | Geospatial analytics/visualization sidecar |
| IfcOpenShell | LGPL-3.0-or-later | Isolated IFC engine/worker boundary |
| Bonsai (within IfcOpenShell repo) | GPL-3.0-or-later | External application/reference integration, not silently copied into permissive ARCZ core |

Pinned revisions live in `upstreams/manifest.toml`. Preserve upstream license/notice files when materializing or redistributing source.

## Data is separate from software

Open-source software licensing does not grant rights to every map tile, imagery source, cadastral dataset, photogrammetry capture, 3D asset, HDRI, font, texture, AI model weight or training dataset used with ARCZ. Every data/model adapter must expose source, terms and attribution/provenance requirements.

## AI-generated and reconstructed assets

Record the source references, model/tool/version, important parameters, resulting artifact hash and any upstream asset/data terms. Do not label procedural approximations as surveyed or source-identical reconstruction.

## Distribution rule

When license obligations conflict with a desired monolithic distribution, keep the component as a sidecar/process/plugin with its original license instead of obscuring or removing the obligation.
