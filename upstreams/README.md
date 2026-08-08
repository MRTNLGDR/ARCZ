# ARCZ upstreams

`manifest.toml` pins the exact source revisions used as compatibility oracles.
The original checkouts live under `upstreams/sources/` and are **immutable**.
ARCZ changes go into Rust crates, adapters, overlays or isolated plugin workers.

Materialize on a machine with GitHub access:

```bash
python tools/materialize_upstreams.py
python tools/materialize_upstreams.py --only aedifex
```

This is intentionally not a blind "copy everything into one dependency graph".
MIT/Apache components can be embedded after notices; LGPL uses a documented
link/process boundary; Bonsai's GPL subtree remains isolated unless ARCZ itself is
distributed under compatible GPL terms for that combined component.
