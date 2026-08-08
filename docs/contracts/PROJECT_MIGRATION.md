# Contrato de migração de projeto — V1 → V2

Implementação autoritativa: `arcz_server/project_migrations.py`.

## Regra única

Não crie defaults ou migrações paralelas em `servidor.py`, plugins ou workers.
O servidor, a gravação e os testes devem chamar `migrate_project()` ou
`migrate_project_file()`.

## Garantias implementadas

- função pura: a entrada não é modificada;
- idempotência: aplicar V2 sobre V2 não altera o documento;
- seed estável para projeto legado sem `project_seed`;
- conversão de `posicao.lugar/cena/camera` para o formato plano;
- conversão dos dicionários legados de `takes`, `pecas` e `lugares` em listas;
- remoção de `ambiente.token_mapbox`;
- normalização segura para `offline_strict` quando o modo é inválido;
- recusa explícita de schema mais novo que o runtime;
- validação por `project-v2.schema.json`;
- escrita atômica, `fsync`, `replace` e backup `.bak` antes da publicação;
- arquivo original intacto quando a migração ou validação falha.

## Proibições

- não descartar campos desconhecidos: o schema permite extensões;
- não gerar dados territoriais, manifests ou resultados de IA durante migração;
- não criar timestamp dentro da função pura;
- não substituir projeto de versão futura por defaults V2;
- não remover o backup antes de concluir um ciclo real de abertura/salvamento;
- não declarar compatibilidade total sem executar o corpus de projetos reais.

## Gate de implementação

```bash
python -m pytest -q tests_python/test_local_first_v5.py \
  -k project_migration
```

Além dos testes unitários, a release deve executar um corpus somente-leitura de
projetos reais antigos:

1. copiar cada projeto para diretório temporário;
2. migrar;
3. validar schema;
4. abrir no navegador com Cesium local;
5. comparar posição, câmera, ambiente, takes, peças e lugares;
6. salvar/reabrir;
7. migrar novamente e provar equivalência canônica;
8. nunca alterar o exemplar original do corpus.

A ausência desse corpus não invalida a implementação, mas mantém aberto o gate
de compatibilidade histórica da release.
