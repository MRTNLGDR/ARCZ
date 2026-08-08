# Alertas técnicos que a próxima IA não pode ignorar

## 1. Não crie uma segunda aplicação

O front continua ES Modules puros. Não introduza React/Vite/Webpack no caminho
principal apenas para implementar um painel. Isso quebraria recarga direta,
ordem de bootstrap e integração com o código auditado.

## 2. `ui.js` é legado sensível

A V5 alterou `ui.js` somente para remover egress/segredos herdados. Novas
features entram em `app/shell`, `app/procedural`, `app/cine`, `app/walk` ou
plugin. Cards com o mesmo `id` não podem coexistir.

## 3. Callback Cesium nunca relança

Qualquer função chamada em `CallbackProperty`, evento de render ou provider usa
`safeCallback`/tratamento equivalente. Exceção não capturada pode matar o render
até reload.

## 4. Estado e origem

- registre toda origem em `app/core/origens.js`;
- observadores aceitam/ignoram origens explicitamente;
- dados transitórios de câmera/FPS não viram verdade persistente;
- segredo não entra em `projeto.json`;
- schema muda somente com migração idempotente e backup.

## 5. Inputs reais

Gerador recebe `generator-input-package` montado pelo servidor. Não leia arquivo
arbitrário do front e não consulte provider dentro do worker. Dado ausente é
`GENERATOR_INPUT_MISSING`; dado estimado precisa estar autorizado e marcado.

## 6. Coordenadas

Não misture WGS84, Web Mercator e ENU. Todo pacote declara CRS/origem. Grid DEM
com origem divergente é recusado por `GRID_REPROJECTION_REQUIRED` até existir
resampler explícito. Nunca “corrija” deslocamento somando offset visual.

## 7. Seeds

Seed deriva de hash estável da hierarquia projeto→região→tile→parcela→edifício.
Nunca derive da ordem de conclusão assíncrona, horário ou `Math.random()` no
motor determinístico.

## 8. Tiles e bordas

Bordas compartilhadas têm quantização/hash. Resultado com costura fora da
tolerância não é aplicado. Interseção viária não deve ser gerada isoladamente
em cada tile.

## 9. Orçamento

Plugin reserva antes de gerar e informa custo real depois. Quando exceder:
reduza LOD/densidade, divida tile, adie ou rejeite. Não remova validação nem
oculte custo para “fazer caber”.

## 10. Cancelamento

Cheque cancelamento em cada estágio. Subprocesso recebe job id/token e tem kill
de segurança. Resultado tardio com epoch antigo é descartado antes de staging.

## 11. Transação

```text
generate → validate files/schema/geometry → stage → commit → index → persist
```

Falha antes do commit remove staging. Falha durante commit executa rollback
LIFO. `limpar()` precisa remover primitiva, listener, timer, map entry e handle.

## 12. Rede

- boot: Natural Earth local + elipsoide;
- `offline_strict`: mesma origem/loopback somente;
- `local_lan`: mesma origem, loopback ou IP privado literal;
- `import_assisted`: autorização explícita, temporária e auditada;
- provider não pode ser requisito para reabrir projeto;
- CDN nunca é solução para vendor ausente.

## 13. DEM

`/dem` serve somente arquivo local. O cliente lança `DEM_LOCAL_MISSING` se não
encontrar tile nem pai real. Não reintroduza array de zeros silencioso.

## 14. IA

Plugin não chama modelo direto. Usa broker com manifesto/checksum, orçamento,
timeout, cancelamento e cache. Saída precisa respeitar schema; texto livre não
vira parâmetro geométrico. Modelo ausente é erro, não resposta estática.

## 15. Rust

A checagem de delimitadores do handoff não é compilação. Antes de editar
algoritmo, rode `cargo fmt/check/test`. Não marque crate como verificado porque o
arquivo parseia visualmente.

## 16. Prova

Atualize status somente depois do gate. Relatório `BLOCKED` continua bloqueado,
mesmo com zero `FAILED`.
