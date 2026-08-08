# Protocolo de implementação para outra IA

## 1. Missão

Continuar o ARCZ Earth sem reescrever sua base e sem produzir demonstrações desconectadas. Cada alteração deve terminar em código executável, testes, documentação, telemetria, erro estruturado e rollback.

## 2. Fonte da verdade

- arquitetura auditada: `docs/ARCZ_ARQUITETURA_E_PLANO.md`;
- plano consolidado: `docs/ARCZ_EARTH_PLANO_TECNICO_LOCAL_FIRST_V3.md`;
- estado executável atual: código + `IMPLEMENTATION_STATUS.json`;
- trabalho futuro: `TASKS.json`;
- prova de validação: `docs/audit/VALIDATION_REPORT.md`.

Quando documentos antigos conflitarem com código/testes atuais, não adivinhe: registre a divergência e atualize o documento autoritativo com evidência.

## 3. Fluxo obrigatório de uma tarefa

```text
READ
→ REPRODUCE
→ DEFINE CONTRACT
→ IMPLEMENT
→ VALIDATE INPUT
→ EXECUTE
→ VALIDATE OUTPUT
→ TEST FAILURE/CANCEL/CLEANUP
→ DOCUMENT
→ VERIFY WHOLE TREE
→ UPDATE STATUS
```

### READ

Leia o módulo existente, consumidores, schemas, testes e invariantes. Não edite `ui.js` para encaixar funcionalidade nova.

### REPRODUCE

Antes de corrigir bug, crie teste que falhe pelo motivo real. Não use teste que apenas compara um valor inventado.

### DEFINE CONTRACT

Defina:

- entrada versionada;
- saída versionada;
- erros possíveis;
- cancelamento;
- orçamento;
- ownership/provenance;
- cleanup/rollback;
- comportamento sem dado/modelo/asset.

### IMPLEMENT

- JS: ES Modules puros, sem import circular e sem bundler.
- Python: gateway, validação, jobs, persistência e coordenação; não gerar geometria pesada.
- Rust: geometria, tiles, gramáticas, LOD, validação e exportação.
- Arquivos: escrita temporária + `fsync` + rename/replace atômico.

### VALIDATE OUTPUT

Nenhum resultado toca a cena antes de:

- checksums conferirem;
- arquivos existirem e não estarem vazios;
- schema passar;
- geometria não conter NaN/Inf;
- índices/atributos serem válidos;
- orçamento ser aceito ou degradado explicitamente;
- `generation_epoch` continuar atual.

### TEST

Todo módulo novo exige ao menos:

1. happy path com entrada real mínima;
2. entrada inválida;
3. dependência ausente;
4. cancelamento;
5. rollback/cleanup;
6. determinismo quando declarado;
7. cabo desconectado para capacidade core.

## 4. Ordem segura

Não avance para visual avançado enquanto o gate anterior estiver bloqueado:

1. instalar e validar o vendor Cesium local;
2. compilar e testar workspace Rust com dependências vendorizadas;
3. provar worker Rust ponta a ponta com pacote local mínimo;
4. provar aplicação e rollback no Cesium real;
5. validar a migração V1→V2 já implementada sobre o corpus real;
6. completar importadores locais OSM/DEM/geocoder;
7. completar geradores geométricos avançados;
8. integrar shell/modes;
9. integrar cinema/render;
10. integrar street-level;
11. instalar e testar modelos locais;
12. difusão/8K;
13. pranchas técnicas finais.

## 5. Convenção de status

- `VERIFIED`: implementado e gate passou neste ambiente.
- `IMPLEMENTED_UNVERIFIED`: código existe, mas falta ferramenta/hardware para validar.
- `CONTRACT_READY`: contrato real existe; implementação operacional depende de asset/modelo/adaptador local.
- `PARTIAL`: caminho principal existe, mas o gate completo ainda não passa.
- `NOT_IMPLEMENTED`: não existe implementação; nunca apresentar como concluído.
- `BLOCKED`: falta pré-condição objetiva.

## 6. Proibições para o agente

- não instalar dependência remota em runtime;
- não mudar `offline_strict` para liberar desenvolvimento;
- não usar Google/Mapbox/serviços de IA como core;
- não copiar dados proprietários;
- não remover checks para “fazer passar”;
- não desativar testes herdados;
- não alterar o schema sem migração;
- não transformar warning crítico em log silencioso;
- não afirmar “pronto” quando cargo/browser/hardware não foram executados.
