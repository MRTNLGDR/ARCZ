# Política de forks e contribuições Aedifex

Nenhum fork entra por popularidade ou por possuir mais arquivos. A admissão usa:

1. base/merge-base e distância do commit fixado;
2. licença e autoria dos arquivos novos;
3. diff por domínio;
4. dependências e chamadas de rede;
5. testes e evidências;
6. segurança de assets/modelos;
7. compatibilidade de schema/plugin;
8. custo de manutenção e rollback.

Classificações possíveis:

- **REJECTED_BLIND_MERGE:** nunca mesclar inteiro;
- **CANDIDATE_PATCH:** revisar arquivo/commit isolado;
- **REFERENCE_ONLY:** usar como pesquisa, não código;
- **ADMITTED_PLUGIN:** plugin vendorizado e testado;
- **UPSTREAM_ACCEPTED:** mudança já incorporada ao commit selecionado.

A auditoria atual está em `resources/aedifex/community-audit.json`. O fork com
cenas/themes pode fornecer ideias/fixtures depois da licença dos assets; forks
muito divergentes permanecem referência. Nenhum foi copiado para o core V10; qualquer admissão continua condicionada a licença, inventário, patch isolado e testes de paridade.
