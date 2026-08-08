# GitHub bootstrap

Target repository: `MRTNLGDR/ARCZ`.

The repository exists. The source tree should be published through reviewed branches/PRs so the first public history includes architecture, license boundaries, validation evidence and an honest implementation ledger.

Recommended local setup after cloning:

```bash
git remote -v
python tools/materialize_upstreams.py --dry-run
python tools/verify_plugin_catalog.py
python -m pytest -q
node --test --experimental-default-type=module tests_js/*.mjs
```

Do not commit materialized third-party upstream build outputs, caches, secrets, downloaded model weights or generated render caches unless their directory has an explicit versioning policy.
