# Próxima execução obrigatória

1. Leia `docs/integration/MASS_CONVERSION_EXECUTION_PLAN.md`.
2. Execute `ARCZ-AED-001`: materialize/build o Aedifex pinado sem alterar o
   upstream preservado.
3. Execute `ARCZ-031`: instale CesiumJS local.
4. Feche Rust com `cargo fmt/check/test`.
5. Execute `ARCZ-AED-002` e `003`: round-trip e lifecycle reais.
6. Instale Blender e modelos locais; feche render/prompts/tradução.
7. Só depois inicie a conversão por kind e o host Tauri único.

Não substitua os vendors ausentes por CDN, o Blender por arquivo vazio, o modelo
por resposta estática, nem o E2E por teste textual.
