# Runbook — build e verificação

## Pré-requisitos

- Python 3.12+;
- Node 20+;
- Rust 1.82+ e Cargo;
- navegador Chromium compatível com CesiumJS local;
- dependências Python já instaladas no ambiente local.

## Sequência

```bash
python tools/clean_release.py
python -m pytest -q
node --test --experimental-default-type=module tests_js/core.test.mjs
python tools/build_generation_worker.py
python tools/verify_handoff.py
```

Qualquer `FAILED` impede merge/release. Qualquer `BLOCKED` impede declarar validação completa.

## Smoke test do worker

1. Crie um pacote local mínimo pelo exemplo em `examples/source-package/minimal`.
2. Importe-o.
3. Resolva a Região Ativa.
4. Chame `/generation/inputs/resolve`.
5. Crie um job.
6. Espere `COMPLETED`.
7. Verifique `manifest.json`, SHA-256 e GLB não vazio.
8. Aplique por staging e remova/regenere três vezes.

## Windows

O worker final é `target/release/arcz-generation-cli.exe`. O rename atômico usa `MoveFileExW` com replace e write-through.
