# Ferramentas de implementação e verificação

| Ferramenta | Função | Acessa rede? |
|---|---|---|
| `tools/verify_handoff.py` | executa gates e gera relatório | não |
| `tools/vendor_cesium.py` | instala Cesium local validado/atômico | não |
| `tools/build_generation_worker.py` | compila worker Rust real | cargo pode exigir vendor local |
| `tools/smoke_generation.py` | job real pacote→worker→GLB/manifest | não |
| `tools/create_source_package.py` | materializa pacote com licença/hash | não |
| `tools/offline_acceptance.py` | roteiro/gates de aceite offline | não |
| `tools/job_cancel_stress.py` | prova 100× que cancelamento não volta a RUNNING | não |
| `tools/clean_release.py` | remove caches/runtime antes do ZIP | não |
| `tools/build_handoff_manifest.py` | gera manifesto/SHA256SUMS do handoff | não |

## Códigos de saída do verificador

- `0`: todos os gates passaram ou apenas Rust estava bloqueado e foi usada a
  opção explícita `--allow-missing-rust`;
- `1`: existe gate `FAILED`;
- `2`: existe gate `BLOCKED` não autorizado, incluindo vendor Cesium ausente.

A opção permissiva nunca muda o status no relatório; ela só permite automação
de análise parcial quando **exclusivamente** os gates Rust estão bloqueados.
