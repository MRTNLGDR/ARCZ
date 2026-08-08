# Round-trip Região → Floorplanner → Globo V10

## 1. Seleção territorial

A Região Ativa pode ser estado, cidade, bairro, quarteirão, endereço, lote ou polígono manual. Região de trabalho e região de geração são distintas. O lote desenhado/bloqueado pelo usuário tem precedência sobre inferências.

## 2. ModelingContextPackage

O backend valida e hash-eia:

- origem WGS84/ENU;
- norte verdadeiro;
- `axis_policy=AEDIFEX_X_EAST_Y_UP_Z_SOUTH`;
- offset vertical;
- lote/envelope/restrições;
- terreno/vias/context layers;
- perfis regionais;
- pacotes-fonte, licença e provenance;
- referências;
- `generation_epoch`.

Nenhum lote cadastral é inventado pelo geocoder.

## 3. Workspace simultâneo

No modo Floorplanner:

- o elemento DOM do Cesium é preservado e movido para o painel do globo;
- o globo continua navegável;
- o sidecar/editor ocupa o painel autoral;
- split ratio e visibilidade persistem;
- o usuário pode focar a Região Ativa;
- resize chama o ajuste do Cesium sem recriar o viewer;
- mobile alterna superfícies sem duplicar cenas.

## 4. Context layers

Terreno, vias, entorno e referências espaciais entram como objetos read-only:

- caminho local;
- SHA-256 obrigatório;
- coordinate space explícito;
- transform calculada pelo bridge;
- `arczExportExclude=true`;
- raycast desabilitado quando não editável;
- não entram no GLB do edifício.

## 5. Revisões

`FloorplannerStore` usa SQLite WAL. Save exige `expected_revision`; conflito retorna `FLOORPLANNER_VERSION_CONFLICT`. Eventos SSE informam commits remotos sem sobrescrever estado não salvo.

## 6. Exportação

`ArczSceneExportBridge` acessa a cena R3F real, clona objetos publicáveis, exclui helpers/luzes/câmeras/context layers e usa `GLTFExporter` binário. Cena sem mesh falha.

O gateway valida:

1. body limit;
2. projeto/revisão/scene hash;
3. magic `glTF`;
4. versão 2;
5. comprimento e chunks;
6. JSON chunk;
7. semantic manifest;
8. SHA-256;
9. atomic write;
10. deduplicação por conteúdo.

## 7. Publicação

Cada revisão salva pode agendar publicação automática. Também existe publicação manual e flush ao sair do modo. O host usa request ID e canal por sessão; respostas atrasadas são verificadas.

`FloorplannerDerivative` registra:

- projeto;
- revisão;
- scene hash;
- export ID/hash/path;
- `GeoAnchor`;
- generation epoch;
- provenance e created_at.

`cena.js` posiciona o GLB por frame ENU→ECEF. O derivado não recebe gizmo.

## 8. Volta à edição

Abrir o Floorplanner carrega a revisão paramétrica. Nunca se tenta reconstruir o documento a partir do GLB.

## 9. Falhas e rollback

- export falho preserva o derivado anterior;
- commit de revisão não é revertido por falha de publicação;
- publicação atrasada não vira latest;
- contexto corrompido é recusado;
- unmount remove listeners/timers/iframe, mas não destrói o Cesium global;
- troca de região cancela jobs não aplicados.

## 10. Gates ainda pendentes

- checkout/build Aedifex real;
- CesiumJS local;
- E2E em lotes plano/aclive/declive;
- north/altitude/scale visual;
- GLB do navegador real;
- 100 ciclos sem leak;
- stress de revisão concorrente;
- smoke no Windows alvo.
