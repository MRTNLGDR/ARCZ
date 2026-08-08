# Crates Rust vendorizados

O código Rust usa crates do ecossistema, mas o runtime não depende de serviços
externos. Para um build air-gapped, execute `cargo vendor` em máquina autorizada,
audite licenças/hashes, transfira o resultado para `vendor/rust/vendor/` e copie
`.cargo/config.offline.example.toml` para `.cargo/config.toml`.

Este handoff não contém esse vendor porque o arquivo de origem não o incluiu e
`cargo` não estava disponível no ambiente de geração. O verificador não declara
o Rust aprovado sem `cargo check/test` reais.
