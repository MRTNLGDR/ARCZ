# Guia da matriz de conversão Aedifex → ARCZ

## Finalidade

Impedir esquecimento, duplicidade e migração silenciosa de capacidade. A matriz é código gerado, validado por schema e verificado por hash.

## Entradas

- `integrations/aedifex/UPSTREAM_LOCK.json`
- `integrations/aedifex/COMMUNITY_SOURCES.json`
- políticas explícitas em `tools/build_aedifex_conversion_matrix.py`

## Saída

- `integrations/aedifex/CONVERSION_MATRIX.json`
- Hash canônico (SHA-256): `4718a2cb82d5cd83c6eeb72bb1bbd014fac9fad6a8fa27d9c3a9b3a7f9b5aaa4`

## Hash

O `matrix_hash` é recalculado sobre JSON canônico; qualquer divergência interrompe o gate.

## Cobertura

```text
packages=7
apps=2
plugins=1
native_node_kinds=46
extension_node_kinds=3
tool_families=21
global_modules=7
community_sources=5
```

## Comandos

```bash
python tools/build_aedifex_conversion_matrix.py
python -m pytest -q tests_python/test_aedifex_inventory_v10.py
python tools/verify_handoff.py --allow-missing-rust
```

## Regra fail-closed

Pacote, kind ou família MCP desconhecida interrompe a geração. Não adicionar fallback genérico. Primeiro auditar o símbolo, definir autoridade, integração, loss policy, testes, blocker e destino; depois atualizar a política do gerador.

## O que a matriz não prova

- checkout integral materializado;
- build Bun/TypeScript do upstream;
- execução dos testes upstream;
- browser E2E com WebGPU/Cesium;
- paridade Aedifex↔Rust;
- render Blender/Cycles;
- funcionamento de modelos locais.

Esses itens permanecem gates separados no relatório de validação e em `TASKS.json`.
