# ARCZ Earth - Auditoria Unificada de Produto, UI, Engine e Documentacao

**Data da auditoria:** 2026-08-04  
**Raiz auditada:** `C:\Users\lucas\Desktop\ARCZ`  
**Status deste documento:** CANONICO  
**Escopo:** documentos, crates Rust, interfaces HTML/JavaScript, servidor HTTP local, contrato de comandos e ponte Tauri/wgpu.  
**Regra desta auditoria:** nenhuma alteracao de codigo e nenhuma afirmacao de teste atual sem execucao. Os numeros de testes encontrados em documentos antigos sao tratados apenas como evidencia historica.

---

## 1. Veredito executivo

O ARCZ tem um nucleo Rust tecnicamente relevante e varios componentes reais: geodesia WGS84/ENU, terreno DEM, imagery, importacao glTF/GLB, cidade procedural OSM, render wgpu, cena editavel, picking, gizmo, workspace, autosave e SQLite/WAL.

O produto desktop integrado, porem, **nao esta concluido**. A interface atual em `crates/arcz-app/src/ui` e um prototipo visual extenso, com pequena injecao de dados reais e muitas acoes simuladas. O crate `arcz-tauri` e uma prova de composicao de superficie, nao o aplicativo completo. A ponte UI -> renderer registra ou ecoa varias operacoes sem alterar a cena real.

### Diagnostico direto

| Area | Estado real | Veredito |
|---|---|---|
| Engine geoespacial | Implementado em crates dedicadas | FORTE |
| Terreno, imagery e OSM procedural | Implementados no nucleo, com cache e rotas de preview | FORTE/PARCIAL |
| Importacao e placement 3D | Implementados no nucleo e no preview | FORTE/PARCIAL |
| Renderer wgpu nativo/offscreen | Funcional como engine e preview | FORTE |
| Scene Graph | Existe, mas ha duas implementacoes divergentes | BLOQUEADOR ARQUITETURAL |
| Persistencia | `.arcz` e SQLite coexistem com responsabilidade dividida | BLOQUEADOR ARQUITETURAL |
| UI Earth atual | Visualmente ampla; majoritariamente mock/simulacao | PROTOTIPO |
| Contrato UI/backend | 69 comandos contratuais; 12 marcados implementados, 57 pendentes | INCOMPLETO |
| Tauri/wgpu | Surface azul e comandos de prova; sem viewport ARCZ integrado | EXPERIMENTO |
| CAD/BIM/Floorplanner | Canvas local e estruturas preliminares; sem engine CAD/BIM final | PROTOTIPO |
| Reality/Street/Reconstruction | Endpoints e workers leves; sem pipeline real completo | PROTOTIPO |
| Takes/Render Queue/Pranchas | UI simulada; sem fila persistente integrada | PROTOTIPO |
| IA | Chat e progresso simulados no navegador | NAO IMPLEMENTADO |
| Seguranca do servidor local | Ha escrita arbitraria potencial e mutacoes por GET | CRITICO |
| Documentacao | Contraditoria, duplicada e desatualizada | PRECISA CONSOLIDACAO |

### Conclusao

O ARCZ nao deve ser tratado como "produto quase pronto". Deve ser tratado como:

1. Um engine Rust real e valioso.
2. Um preview tecnico funcional.
3. Uma UI de referencia ampla, mas nao conectada end-to-end.
4. Um conjunto de contratos e crates de futuro ainda em formato de prototipo.

O principal trabalho nao e adicionar mais telas. E fechar uma unica arquitetura autoritativa e conectar, com operacoes reais, as telas essenciais: projetos, cena, transformacao, biblioteca, terreno e render.

---

## 2. Fonte da verdade apos esta auditoria

### Deve ser considerado canônico

| Assunto | Fonte canônica atual |
|---|---|
| Workspace Rust | `Cargo.toml` da raiz |
| Engine e aplicacao atual | `crates/arcz-app/src` |
| Contrato HTTP de preview | `crates/arcz-app/src/server.rs` |
| Contrato declarativo de comandos | `crates/arcz-app/src/comandos.rs` |
| UI de referencia servida pelo Rust | `crates/arcz-app/src/ui` |
| Preview tecnico operacional | `crates/arcz-app/src/preview.html` |
| Experimento Tauri/wgpu | `crates/arcz-tauri/src` |
| Decisao de viewport nativo | `docs/decisions/ADR-0002-viewport-wgpu-nativo-no-tauri.md` |
| Arquitetura alvo de Reality | `docs/decisions/ADR-0004-reality-site-composer-architecture.md` |
| Estado geral e prioridades | Este documento |

### Nao deve ser tratado como estado atual

- Contagens de crates, testes, comandos ou status escritas em auditorias de 2026-07-30.
- Referencias ao React/FAVELION como se a UI estivesse dentro deste repositorio e integrada ao binario atual.
- Matrizes que marcam telas como `implemented` apenas porque o HTML existe.
- Status `OK`, `Instalado`, `Ativo`, `Concluido`, `Renderizando` ou numeros de hardware exibidos pela UI sem leitura do backend.
- Mensagens de toast da UI que dizem que uma acao foi registrada ou validada quando nenhum comando real foi executado.

---

## 3. Inventario real do workspace

O `Cargo.toml` atual declara **11 crates**, nao 4, 7 ou 8:

| Crate | Papel | Estado auditado |
|---|---|---|
| `arcz-app` | Aplicacao, renderer, viewport, servidor, projeto, workspace, DB, workers e UI embutida | PRINCIPAL, mas concentrado demais |
| `arcz-biblioteca` | Catalogo, assets parametrizados, PolyHaven | REAL/PARCIAL |
| `arcz-earth` | Schemas e defaults de Earth/regional packages/takes | PROTOTIPO CONTRATUAL |
| `arcz-geo` | WGS84, ECEF, ENU, bbox, tiles e sol | REAL |
| `arcz-jobs` | Estruturas de fila e estado de jobs em memoria | PROTOTIPO |
| `arcz-model` | glTF/GLB, materiais, KMZ e placement | REAL |
| `arcz-osm` | Overpass, entidades, triangulacao e geracao procedural | REAL |
| `arcz-provenance` | Tipos basicos de licenca e proveniencia | PROTOTIPO |
| `arcz-scene` | Segundo Scene Graph e CommandBus | DUPLICADO/NAO AUTORITATIVO |
| `arcz-tauri` | Prova de surface wgpu sobre WebviewWindow | EXPERIMENTO |
| `arcz-terrain` | Cache, DEM, mosaico, malha e quantized-mesh | REAL |

### Problema estrutural

`arcz-app` contem a implementacao realmente usada de cena, persistencia, renderer, API e workers, enquanto `arcz-scene`, `arcz-jobs`, `arcz-provenance` e parte de `arcz-earth` repetem conceitos em crates isoladas e pouco conectadas. O projeto tem mais de uma representacao para cena, jobs, origem de dados e estado Earth.

**Decisao recomendada:** nao criar mais crates de contrato ate consolidar as existentes. Extrair do `arcz-app` apenas quando o crate extraido passar a ser usado pelo executavel principal.

---

## 4. Superficies de interface existentes

Hoje existem cinco superficies distintas:

| Interface | Local | Uso real | Estado |
|---|---|---|---|
| Preview tecnico | `crates/arcz-app/src/preview.html` | Manipula camera, placement, render, picking e rotas reais | MANTER COMO DEBUG/OPERACAO |
| ARCZ Earth UI Reference | `crates/arcz-app/src/ui` | Shell amplo servido em `/earth`; poucos dados reais | PROTOTIPO DE PRODUTO |
| Cesium Sandcastle interno | `pagina_cesium()` em `server.rs` | Demo separada com CDN e dados de exemplo | REMOVER OU ISOLAR COMO LAB |
| Tauri | `crates/arcz-tauri` | Teste de surface azul e comandos de prova | LABORATORIO |
| UI JS da raiz | `app/` | Nao e servida pelo caminho principal auditado | LEGADO/ORFA |

### Recomendacao de simplificacao

Manter apenas dois caminhos oficiais:

1. **Produto desktop:** Tauri + uma unica UI + viewport wgpu real.
2. **Ferramenta de diagnostico/headless:** `preview.html` + rotas HTTP locais.

O Sandcastle Cesium, a pasta `app/` e outras shells nao devem continuar evoluindo como produtos paralelos.

---

## 5. Auditoria da UI atual por tela

### Legenda

- **REAL:** executa uma operacao real do backend e apresenta o resultado.
- **PARCIAL:** recebe algum dado real, mas a maioria da tela ou acao ainda e mock.
- **SIMULADO:** muda apenas estado JavaScript, usa timer, random ou toast.
- **NAO CONECTADO:** existe backend relacionado, mas a tela nao o chama.
- **QUEBRAVEL:** ha caminho claro para erro de runtime ou dado enganoso.

| Tela | Estado | O que e real | O que falta ou esta enganoso |
|---|---|---|---|
| Dashboard | PARCIAL/QUEBRAVEL | Lista de projetos pode vir de `project.list` | Clima, regiao, GPU, memoria, cache, usuario e acoes rapidas sao fixos; se nao houver projeto real, `state.project` pode ficar indefinido |
| Globo 3D | SIMULADO/NAO CONECTADO | Engine tem terreno, imagery, CZML e rotas Cesium separadas | A tela mostra mapa CSS, layers e selecao ficticios; nao embute renderer nem Cesium real |
| Mapas Offline | PARCIAL | `package.list` conta arquivos do cache | Nao existe package manager real, checksum por pacote, update atomico ou rollback conectados |
| Regioes | SIMULADO | Nenhum fluxo regional completo da tela | Cards e cobertura vem de `data.js`; `region.list` devolve essencialmente o mesmo resumo de cache de `package.list` |
| Projetos | PARCIAL | Catalogo do workspace pode substituir o mock | Criar, importar, favoritos, compartilhados, arquivar e abrir fluxo completo nao chamam comandos reais |
| Projeto ativo | SIMULADO/NAO CONECTADO | Ha engine de cena e persistencia fora da tela | KPIs, scene graph, versoes, publicar e relatorios sao ilustrativos |
| Biblioteca 3D | NAO CONECTADO | Backend tem `asset.search`, `asset.tag` e rota `/biblioteca` | Tela usa `D.assets`; adicionar item apenas mostra feedback visual |
| ARCZ Designer | SIMULADO | Canvas desenha segmentos de parede em memoria | Nao persiste, nao gera SceneNode real, nao calcula area/custo, portas, janelas, lajes, escadas ou IFC/DXF reais |
| Inserir modelo 1-4 | SIMULADO | Seletor de arquivo HTML existe | Importacao, validacao, placement, composicao, persistencia e exportacao sao texto/toast; `model.import` esta pendente |
| Captura e cena | SIMULADO/NAO CONECTADO | Ha endpoints experimentais de ingest | A tela nao orquestra arquivos, hashes, EXIF, frames, poses nem privacidade |
| Reconstrucao 3D | SIMULADO/NAO CONECTADO | Worker leve cria referencias de asset | Nao executa COLMAP, MapAnything, MVS, splat, mesh ou cancelamento real |
| Street 360 | SIMULADO/NAO CONECTADO | Existe worker e endpoint demo Panoramax | Usa panorama/demo e nao implementa busca, pose, profundidade, mascara, oclusao ou composicao final |
| Take & Render | SIMULADO | Adiciona item em array JS | Progresso e gerado por timer aleatorio; nao ha journal, worker, retomada ou artefato de saida |
| Render Archviz Pro | SIMULADO/PARCIAL | Renderer wgpu produz PNG/JPEG no preview | Presets, fila, denoise, sequencia e saida Pro nao estao conectados |
| Pranchas | SIMULADO | Nenhuma geracao documental de producao | Exportar PDF/DXF/IFC apenas gera toast |
| Timeline/Revisoes | SIMULADO | Workspace tem snapshots e lixeira em outras rotas/CLI | Tela nao le journal nem restaura revisao real |
| IA local e nuvem | SIMULADO | Nenhum runtime de inferencia usado | Resposta e criada por `setTimeout`; fila e estado local; nao ha modelos, tool dispatcher ou politica real |
| Configuracoes | SIMULADO | Algumas politicas existem no Rust | Caminhos, GPU, rede, atualizacao, modelos e licencas sao valores fixos e nao persistem |

### Falha de honestidade da interface

O handler global de acoes termina em um fallback que informa que a acao foi registrada e que o contrato de backend esta documentado. Isso faz botoes sem implementacao parecerem funcionais.

**Regra obrigatoria para a proxima UI:** toda acao precisa estar em um destes estados visiveis:

- Disponivel e executada com resposta real.
- Indisponivel, com motivo e dependencia.
- Experimental, com escopo limitado explicito.

Nunca exibir sucesso para uma acao que apenas alterou estado local ou mostrou um toast.

---

## 6. Contrato de comandos

O comentario de cabecalho em `comandos.rs` menciona 78 comandos, mas o codigo e os testes declaram **69 comandos contratuais**.

### Contagem real

- Contrato: 69 comandos.
- Implementados declarados dentro do contrato: 12.
- Pendentes: 57.
- Meta-comando adicional: `capability.list`.
- A lista `IMPLEMENTADOS` tem 13 itens porque inclui o meta-comando.

### Cobertura nominal

`12 / 69 = 17,4%` do contrato.

Essa cobertura e apenas nominal. Cinco comandos marcados como implementados ainda sao superficiais ou nao mutam estado.

### Implementados com resposta de dados util

| Comando | Estado real |
|---|---|
| `workspace.status` | Le estado da cena atual |
| `project.list` | Lista workspace padrao |
| `project.get` | Abre um projeto pelo slug |
| `scene.list` | Lista objetos do Editor |
| `asset.search` | Pesquisa catalogo da biblioteca |
| `asset.tag` | Retorna metadados de um item |
| `terrain.sample` | Amostra altitude no terreno |

### Marcados implementados, mas semanticamente parciais

| Comando | Problema |
|---|---|
| `camera.set` | Valida e devolve lat/lon, mas nao altera camera persistente |
| `layer.toggle` | Valida nome/booleano, mas nao altera renderer ou scene graph |
| `earth.open` | Retorna camera estatica e `offline: true`; nao abre globo |
| `package.list` | Conta todos os arquivos de um cache, nao gerencia pacotes regionais |
| `region.list` | Reutiliza a mesma resposta de `package.list`; nao lista regioes reais |

### Comandos pendentes por bloco

| Bloco | Comandos principais pendentes |
|---|---|
| Pacotes | `package.install`, `package.update`, `package.remove` |
| Regioes | `region.inspect`, `region.build` |
| Projetos | `project.create`, `project.import`, `project.export`, `project.update`, `project.commit`, `revision.restore` |
| Cena e picking | `pick.world` |
| Assets | `asset.import`, `asset.download` |
| CAD/Designer | `cad.command`, `cad.snap`, `cad.generate_mesh`, `room.compute`, `cost.compute`, `drawing.generate`, `sheet.compose`, `pdf.export`, `dxf.export`, `ifc.export` |
| Modelos | `model.import`, `model.validate`, `model.optimize`, `placement.capture`, `placement.snap` |
| Composicao | `panorama.align`, `depth.estimate`, `occlusion.build`, `light.estimate`, `composition.validate`, `compose.project` |
| Captura/Reality | `capture.ingest`, `frame.extract`, `pose.solve`, `privacy.mask`, `reconstruction.start`, `reconstruction.cancel`, `splat.export`, `mesh.export`, `panorama.search`, `panorama.load` |
| Render | `render.preview`, `render.panorama`, `take.create`, `take.update`, `take.render`, `queue.add`, `render.configure`, `render.submit`, `render.cancel`, `denoise.run` |
| Timeline | `timeline.seek`, `czml.import` |

### Problema de medicao

A UI calcula a mensagem de capacidades usando o tamanho de `implementados` dividido pelo total do contrato. Como `capability.list` entra na lista de implementados mas nao entra no contrato, pode aparecer `13/69`, apesar de apenas 12 comandos contratuais estarem marcados implementados.

**Correcao recomendada:** retornar separadamente `meta`, `implemented_operational`, `implemented_partial` e `pending`. O frontend deve desabilitar comandos pendentes de verdade.

---

## 7. Auditoria por bloco de modulo

## Bloco A - Geodesia e mundo

### Concluido

- WGS84, ECEF e ENU em `f64`.
- BBox geoespacial e tiles.
- Calculo solar.
- Uso consistente do frame local no nucleo.

### Incompleto

- Nao ha uma unica API de mundo exposta ao produto desktop.
- O crate `arcz-earth` monta uma cena default com coordenadas e caminhos fixos.
- `renderizar_take` devolve bandas predefinidas; nao executa geracao procedural nem render.

### Prioridade

**P1:** usar `arcz-geo` e `arcz-terrain` como base autoritativa e reduzir `arcz-earth` a schemas/servicos reais, removendo defaults que parecem dados de producao.

## Bloco B - Terreno, imagery, globo e pacotes offline

### Concluido

- Cache local de tiles.
- DEM/mosaico/malha.
- Quantized-mesh e imagery servidos pelo Rust.
- Rotas de terreno e imagem para visualizacao Cesium.

### Incompleto

- Package builder regional real.
- Manifesto, checksum e licenca por pacote.
- Instalacao, update, rollback e remocao.
- UI de globo ligada a um renderer real.

### Problema

`package.list` e `region.list` contam arquivos em um cache global e os apresentam como um pacote da regiao atual. Isso nao e um sistema de pacotes.

### Prioridade

**P1:** implementar manifesto regional e operacoes atomicas antes de exibir update/rollback na UI.

## Bloco C - Modelos, importacao e placement

### Concluido

- Loader glTF/GLB e suporte de materiais.
- Placement georreferenciado.
- Otimizacao/copia leve para entrega do modelo ao globo.
- Transformacao de matriz na GPU.
- Preview com camera, gizmo, snap e corte.

### Incompleto

- Fluxo de importacao do produto.
- Validacao de arquivo e unidade exposta pelo contrato.
- Persistencia transacional do asset importado.
- Acoes `model.import`, `model.validate`, `model.optimize` e `placement.*`.

### Prioridade

**P1:** transformar o preview funcional em comandos de produto, sem duplicar logica na UI.

## Bloco D - Scene Graph, Inspector, Outliner e Gizmo

### Concluido

- `arcz-app::cena` contem Editor, objetos, SceneNode, hierarquia, Outliner, Inspector, picking, command bus e operacoes de cena.
- O servidor expoe `/outliner`, `/inspector`, `/picar`, `/undo` e `/redo`.

### Bloqueador

Existe um segundo Scene Graph em `arcz-scene`, com tipos e IDs diferentes. Ele nao e a fonte usada pelo executavel principal.

Problemas do `arcz-scene`:

- Timestamp fixo em 2026-07-30.
- Undo empilha a operacao inversa no redo, e `redo()` reaplica essa inversa; a semantica nao restaura corretamente a operacao original.
- Tipos e estrutura divergem de `arcz-app::cena`.
- Nao esta ligado ao renderer, DB ou UI principal.

### Prioridade

**P0:** escolher uma unica implementacao. Recomendacao: manter temporariamente `arcz-app::cena` como autoritativa e migrar para `arcz-scene` somente com testes de compatibilidade e uso real no executavel.

## Bloco E - Projeto, Workspace, SQLite e historico

### Concluido

- Workspace com listagem e abertura.
- Formato `.arcz` versionado.
- SQLite/WAL com cena e journal.
- Autosave e retomada de placement no servidor.

### Bloqueador

Ha duas fontes de verdade:

- O comentario do codigo chama `project.sqlite` de autoritativo.
- A sessao ainda depende do `.arcz` para abrir parte do projeto.
- O SQLite e lido para placement, enquanto outras informacoes continuam no `.arcz`.
- Falhas de autosave sao apenas logadas; a UI mostra autosave ativo de forma fixa.

### Risco

Um projeto pode ter cena, journal e metadados divergentes entre `.arcz` e SQLite sem que a UI informe qual foi usado.

### Prioridade

**P0:** definir um formato autoritativo. Recomendacao: diretorio de projeto + `project.sqlite` como verdade; `.arcz` deve ser pacote de import/export, nao segundo banco ativo.

## Bloco F - Biblioteca e assets

### Concluido

- Catalogo Rust com ambiente, papel, licenca e dimensao.
- Busca e tags por comando.
- Rota adicional `/biblioteca` baseada em manifesto.
- Adicao de item na cena pelo preview.

### Incompleto

- UI Earth nao chama `asset.search`/`asset.tag`.
- Ha duas APIs de biblioteca com formatos diferentes.
- Download/import e verificacao de asset nao estao no contrato operacional.
- A UI afirma SHA-256 verificado para itens mockados.

### Prioridade

**P1:** uma unica API de asset, com ID de conteudo, licenca, arquivo local, checksum e estado de download.

## Bloco G - Renderer, camera, takes e composicao

### Concluido

- Renderer wgpu nativo e offscreen.
- PNG, JPEG e composicao transparente.
- Camera orbital, corte e luz solar.
- Preview tecnico responde a mudancas reais.

### Incompleto

- Fila persistente de render.
- Takes persistidos e keyframes reais na UI.
- Cancelamento, retomada, artefatos e logs.
- Denoise e render panoramico.
- Integracao com o Tauri.

### Problema

`arcz-jobs` e apenas uma fila em memoria, com heartbeat fixo e sem worker, persistencia ou ownership de artefato. A UI usa uma segunda fila em JavaScript com progresso aleatorio.

### Prioridade

**P1/P2:** criar um unico job store persistente depois de fechar Scene Graph e DB.

## Bloco H - Tauri e viewport nativo

### Concluido

- A surface wgpu pode ser criada sobre `WebviewWindow`.
- Resize da janela reconfigura a surface.
- Comandos Tauri de prova existem.

### Nao concluido

- O renderer ARCZ nao desenha nessa surface; apenas uma cor solida.
- `viewport_area` apenas registra e retorna `true`.
- A surface ocupa/configura a janela, nao uma regiao funcional do layout.
- O z-order final entre webview e surface ainda depende de verificacao visual registrada no ADR.
- A UI React citada nos documentos nao esta dentro deste workspace auditado.

### Falhas da ponte

- `SetCamera` emite evento, mas nao atualiza `estado.camera`; o relatorio pode continuar devolvendo a camera default.
- `TransformNode` apenas grava log.
- `ResizeViewport` guarda bounds, mas nao reconfigura a surface para o retangulo.
- `SetSnapping` guarda apenas o booleano; o passo informado nao e usado.
- Nao ha Scene Graph ou Renderer real dentro do estado Tauri.

### Prioridade

**P0:** integrar um frame real, selecao real e transformacao real antes de evoluir mais a UI de produto.

## Bloco I - CAD, Designer, BIM e documentos

### Concluido

- Canvas de desenho de paredes como demonstracao.
- Worker CAD leve e estruturas de planta/mobiliario no `arcz-app`.

### Nao concluido

- Modelo parametrico autoritativo.
- Constraints, openings, rooms e pavimentos reais.
- Geracao de malha a partir dos comandos.
- Calculo confiavel de area/custo.
- IFC, DXF, PDF e pranchas.
- Undo/redo persistente do Designer.

### Prioridade

**P2:** nao integrar OCCT/IfcOpenShell antes de existir um fluxo de projeto/cena persistente e uma API de comandos consolidada.

## Bloco J - Captura, Street 360 e reconstrucao

### Concluido

- ADR e especificacao de arquitetura.
- Tipos de SceneNode e alguns workers/endpoints experimentais.
- Rota de composicao PNG transparente.

### Nao concluido

- Ingest real de foto/video/drone/LiDAR.
- Extracao de frames e EXIF.
- Busca real Panoramax/KartaView/Mapillary.
- Pose, profundidade, segmentacao, privacidade, oclusao e luz.
- COLMAP/MVS/splat/mesh.
- Proveniencia de artefatos e licencas end-to-end.

### Problemas concretos

- `/streetview/ingest` fabrica item demo com URL e data fixas.
- Payload JSON invalido em varios endpoints cai em dados default, em vez de retornar erro.
- `reconstruct/ingest` registra asset/referencia; isso nao equivale a reconstrucao.

### Prioridade

**P2:** primeiro implementar ingest auditavel + jobs persistentes. Depois conectar provedores e pipelines externos.

## Bloco K - IA

### Estado

Nao implementado como produto.

- Chat do frontend responde por timer.
- Nao ha runtime local integrado.
- Nao ha roteamento por custo/privacidade real.
- Nao ha schema de tool calling ligado aos 69 comandos.
- Nao ha aprovacao humana transacional antes de alterar cena.

### Prioridade

**P3:** integrar IA somente depois que os comandos base forem operacionais e idempotentes. A IA nao deve chamar toasts; deve propor comandos validaveis, revisar diffs e commitar via journal.

## Bloco L - Proveniencia e licencas

### Concluido

- Enums basicos e verificacao comercial simples.
- Politica de custo gratuito no nucleo.
- Matriz documental de fontes.

### Incompleto

- `arcz-provenance` nao esta conectado a assets, pacotes, renders ou exports.
- Versao e nivel de confianca sao defaults fixos.
- Relatorio da UI e estatico.
- Nao existe bloqueio end-to-end de exportacao com licenca incompatível.

### Prioridade

**P1/P2:** toda ingestao deve criar registro de proveniencia; todo export deve agregar atribuicoes e bloqueios.

---

## 8. Achados criticos de codigo e arquitetura

## P0 - Corrigir antes de distribuir o servidor local

### 8.1 Escrita de arquivo controlada pela query

A rota `POST /upload?nome=...` usa o nome recebido para montar `cache/<nome>` sem a mesma validacao de traversal aplicada a outras rotas. Isso permite tentar caminhos com `..`, separadores ou caminho absoluto. O corpo tambem e convertido para `String`, o que corrompe binarios e nao tem limite de tamanho.

**Risco:** escrita fora do cache, corrupcao de arquivo, consumo excessivo de memoria e falso suporte a upload 3D binario.

**Correcao exigida:** aceitar apenas nome de arquivo normalizado, bloquear componentes de caminho, limitar tamanho, usar bytes, validar magic/extensao e gravar atomicamente em diretorio controlado.

### 8.2 Mutacoes por GET e ausencia de protecao local

Rotas como `/salvar`, `/entorno`, `/entorno/limpar`, `/area`, `/adicionar` e `/agua` alteram estado usando GET. O servidor nao tem token de sessao, validacao de origem ou protecao CSRF.

**Risco:** outra pagina/processo local pode disparar operacoes no ARCZ. Bind em `127.0.0.1` reduz exposicao de rede, mas nao protege contra navegadores e processos na mesma maquina.

**Correcao exigida:** mutacoes por POST/PATCH/DELETE, token efemero de sessao, `Origin` permitido, content type validado e respostas HTTP corretas.

### 8.3 Dupla fonte de verdade da cena e do projeto

`arcz-app::cena` e `arcz-scene` divergem. `.arcz` e SQLite tambem dividem autoridade.

**Risco:** undo/redo, autosave, reabertura e UI exibirem estados diferentes.

**Correcao exigida:** uma cena autoritativa e um store autoritativo.

### 8.4 UI informa sucesso sem operacao real

Toasts, filas, IA, exports e varios botoes comunicam sucesso sem backend.

**Risco:** perda de confianca e perda de trabalho, especialmente em salvar/exportar/renderizar.

**Correcao exigida:** desabilitar acoes pendentes e exigir comprovante de backend/artefato antes de mostrar sucesso.

## P1 - Corrigir na fundacao do produto

### 8.5 Comandos marcados implementados sem efeito

`camera.set` e `layer.toggle` retornam `ok` sem mutacao real. `earth.open`, `package.list` e `region.list` sao respostas superficiais.

**Correcao:** reclassificar como `partial` ate executar e persistir a operacao.

### 8.6 Servidor single-thread e chamadas sincronas na carga

O servidor processa uma requisicao por vez e o bootstrap usa XHR sincrono para cinco comandos.

**Risco:** congelamento da UI durante disco/rede/GPU e head-of-line blocking.

**Correcao:** bootstrap assincrono com shell de carregamento; separar render/GPU serial de leitura de estado e jobs.

### 8.7 Erros de dominio devolvidos como 404

Qualquer falha do `match` vira 404, inclusive payload invalido, erro interno ou recurso indisponivel.

**Correcao:** 400 para entrada invalida, 404 para recurso ausente, 409 para conflito, 422 para validacao, 500 para falha interna e JSON de erro uniforme.

### 8.8 Defaults silenciosos em ingestao

Payload invalido em endpoints GIS, CAD, reconstruct e outros pode executar um request default.

**Risco:** a UI acredita que processou o arquivo enviado, mas o backend cria um objeto demo.

**Correcao:** payload invalido deve falhar. Dados demo so em fixture/teste explicito.

### 8.9 Cesium de laboratorio viola a promessa offline

`pagina_cesium()` carrega Cesium CSS/JS por CDN e oferece Cesium World Terrain. Em paralelo existe rota de vendor local.

**Correcao:** remover a pagina de laboratorio do produto ou usar somente bundle vendorizado e provedores permitidos.

### 8.10 Projeto vazio pode quebrar a Dashboard

O bootstrap substitui `D.projects` pela lista real. Se estiver vazia, `state.project = D.projects[0]` fica indefinido e partes da Dashboard acessam `state.project.name` e `state.project.local`.

**Correcao:** estado vazio de projeto como fluxo de primeira classe.

## P2 - Reduzir divida antes das features avancadas

### 8.11 Duplicacao de APIs

- `asset.search` vs `/biblioteca`.
- Comandos `/cmd/*` vs rotas especificas.
- Scene Graph no `arcz-app` vs `arcz-scene`.
- Jobs Rust vs fila JS.
- Preview nativo vs Earth UI vs Cesium demo vs Tauri lab.

**Correcao:** definir contratos de dominio e adaptadores de transporte, sem duplicar regra.

### 8.12 Dados fixos com aparencia operacional

GPU, VRAM, RAM, cache, clima, usuario, versao, baseline, modelo de IA, custos, cobertura regional e status sao fixos em UI ou crates de prototipo.

**Correcao:** dados reais ou estado `indisponivel`; nunca numeros ficticios em tela de diagnostico.

---

## 9. Bloqueadores de produto

| ID | Bloqueador | Impacto | Decisao necessaria |
|---|---|---|---|
| B-01 | Renderer principal e composicao da janela | Impede UI desktop integrada | Confirmar wgpu nativo como editor principal e Cesium apenas como visualizador/exportador, ou o inverso |
| B-02 | Z-order/recorte da surface Tauri | Pode impedir viewport dentro do layout | Fechar verificacao visual e implementar composicao real |
| B-03 | Scene Graph duplicado | Impede API, undo/redo e persistencia coerentes | Escolher implementacao autoritativa |
| B-04 | `.arcz` vs SQLite | Risco de divergencia e perda de estado | Definir store autoritativo e papel do pacote `.arcz` |
| B-05 | UI de produto | Ha shell estatica no repo e React/FAVELION citado fora dele | Escolher um unico repositorio e runtime de UI |
| B-06 | Contrato de comandos 82,6% pendente | A maioria das telas nao pode funcionar | Priorizar comando por jornada vertical, nao por tela |
| B-07 | Seguranca da API localhost | Impede distribuicao segura | Corrigir upload, metodos, token, limites e erros |
| B-08 | Jobs/reality/AI sem runtime | Simulacoes nao geram artefatos | Implementar store e workers reais antes da UI avancada |

---

## 10. Ordem recomendada de execucao

### P0 - Fundacao obrigatoria

1. Consolidar Scene Graph.
2. Consolidar persistencia em SQLite e definir `.arcz` como pacote.
3. Corrigir seguranca do servidor local.
4. Remover falso sucesso da UI e desabilitar acoes pendentes.
5. Fechar o Tauri com um frame real, bounds reais e input real.

### P1 - Primeira entrega vertical de produto

1. Abrir/listar/criar projeto real.
2. Listar cena real em Outliner.
3. Selecionar no viewport e sincronizar Inspector.
4. Mover/girar/escalar com journal.
5. Undo/redo persistente.
6. Salvar, fechar e reabrir sem divergencia.
7. Importar GLB real e inserir asset real.
8. Renderizar still real com artefato e proveniencia.

### P2 - Mundo e producao

1. Pacotes regionais com manifesto e rollback.
2. Globo real conectado.
3. Fila persistente de jobs.
4. Takes, render panoramico e documentos.
5. Ingest auditavel de captura e Street 360.

### P3 - Expansao

1. Reconstrucao COLMAP/MVS/splat.
2. Designer CAD/BIM.
3. IA local/tool dispatcher.
4. Nuvem opcional e colaboracao.

### Regra de priorizacao

Nao construir mais uma tela enquanto a jornada vertical P1 nao estiver fechada. O aceite minimo deve ser:

> criar/abrir projeto -> importar modelo -> selecionar -> transformar -> desfazer/refazer -> salvar -> fechar -> reabrir -> renderizar -> localizar o artefato.

---

## 11. Matriz de risco

| Risco | Probabilidade | Impacto | Nivel | Mitigacao |
|---|---:|---:|---:|---|
| Divergencia SQLite/`.arcz` | Alta | Critico | P0 | Store unico + migracao |
| Escrita insegura em upload | Media | Critico | P0 | Path seguro, bytes, limite e atomicidade |
| UI indicar sucesso falso | Alta | Alto | P0 | Capability real + acoes desabilitadas |
| Surface Tauri nao compor com webview | Media | Alto | P0 | Spike visual concluido com renderer real e plano B |
| Scene Graph duplicado | Alta | Alto | P0 | Uma crate autoritativa |
| Servidor travar por operacao longa | Alta | Alto | P1 | Jobs e separacao de executor GPU |
| Pacote offline ser apenas cache informal | Alta | Medio | P1 | Manifesto regional e lifecycle |
| Licencas nao acompanharem export | Media | Alto | P1 | Proveniencia por asset/artefato |
| Reality usar dados demo como reais | Alta | Alto | P1 | Falhar payload invalido e marcar fixture |
| IA alterar cena sem transacao | Media | Alto | P2 | Proposta, diff, aprovacao e journal |
| Documentos voltarem a divergir | Alta | Medio | P1 | Um CURRENT_STATE canonico gerado de dados reais |

---

## 12. Higiene documental: remover ou arquivar

Os arquivos abaixo nao devem continuar como documentacao corrente. Eles contem duplicacao, contagens antigas ou status incorreto.

### Arquivar como historico

| Arquivo | Motivo |
|---|---|
| `docs/auditoria/ARCZ-2026-07-30.md` | Auditoria inicial superada por varias implementacoes posteriores |
| `docs/decisions/ADR-0001-arcz-stack-engine-rust-ui-tauri-react.md` | Transporte foi revisado pelo ADR-0002 e referencia arquitetura externa nao confirmada neste repo |
| `docs/audit/TEST_BASELINE.md` | Snapshot historico; nao prova estado atual |
| `docs/audit/EXECUTION_ORDER.json` | Roadmap antigo e incompatível com os bloqueadores atuais |

### Remover da documentacao corrente ou substituir por ponteiro para este arquivo

| Arquivo | Motivo |
|---|---|
| `docs/audit/CURRENT_STATE.md` | Desatualizado e contraditorio |
| `docs/auditoria/CURRENT_STATE.md` | Duplicata quase literal |
| `docs/audit/REPOSITORY_BASELINE.md` | Diz 8 crates e UI React integrada; o workspace atual tem 11 crates e a UI em uso e outra |
| `docs/audit/UI_TO_BACKEND_MAP.csv` | Marca telas como implementadas quando sao mock/simulacao |
| `docs/audit/CAPABILITY_MATRIX.json` | Commit, contagens e evidencia de UI estao antigos |
| `docs/audit/GAP_ANALYSIS.json` | Subestima gaps e trata componentes de prototipo como entregues |

### Manter, mas atualizar status

| Arquivo | Acao |
|---|---|
| `docs/decisions/ADR-0002-viewport-wgpu-nativo-no-tauri.md` | Manter decisao; status de execucao deve ser PARCIAL |
| `docs/decisions/ADR-0004-reality-site-composer-architecture.md` | Manter como arquitetura alvo, nao estado atual |
| `docs/integrations/UI_ENGINE_CONTRACT.md` | Atualizar para o contrato real de 69 comandos e marcar Tauri commands de prova |
| `docs/auditoria/REALITY_SITE_COMPOSER_SPEC.md` | Manter como especificacao futura; separar tabelas implementadas das propostas |
| `docs/open-source/*` | Manter, com revisao periodica de versao, licenca e disponibilidade |

### Candidatos de codigo/UI a arquivamento futuro

Nenhum codigo foi removido nesta auditoria. Os seguintes itens devem ser avaliados depois de escolher a UI oficial:

- Pasta `app/` da raiz, se continuar sem entrypoint oficial.
- `pagina_cesium()` embutida no `server.rs`, substituindo-a por viewer vendorizado ou lab isolado.
- `arcz-scene`, se nao for escolhido como destino da migracao.
- `arcz-jobs`, se a fila real for implementada diretamente no store principal sem reutiliza-lo.
- Duplicacao entre `/biblioteca` e `asset.search`.

---

## 13. Documentacao unificada recomendada

Depois da limpeza, a arvore documental corrente deve ser curta:

```text
docs/
  auditoria/
    ARCZ_AUDITORIA_UNIFICADA_2026-08-04.md   # estado corrente e prioridades
    REALITY_SITE_COMPOSER_SPEC.md             # especificacao alvo
  decisions/
    ADR-0002-viewport-wgpu-nativo-no-tauri.md
    ADR-0004-reality-site-composer-architecture.md
  integrations/
    UI_ENGINE_CONTRACT.md
  open-source/
    OPEN_SOURCE_CATALOG.md
    LICENSE_MATRIX.md
    INTEGRATION_STATUS.md
    BIBLIOTECA_MOBILIARIO.md
  archive/
    2026-07-30/
      ... documentos superados ...
```

### Regra de manutencao

Todo documento de estado deve conter:

- Data.
- Commit ou identificador da versao auditada.
- Evidencia de execucao, se houver.
- Diferenca explicita entre `implementado`, `integrado`, `testado` e `simulado`.
- Link para o contrato ou codigo fonte.
- Responsavel e proxima acao.

---

## 14. Criterios de aceite para considerar cada bloco concluido

| Bloco | Criterio de aceite |
|---|---|
| Tauri/Viewport | Cena ARCZ real a 60 fps alvo, resize por bounds, picking e transformacao no mesmo estado |
| Projetos | CRUD, autosave, crash recovery e reabertura sem divergencia |
| Scene Graph | Uma implementacao, IDs estaveis, hierarquia, Inspector, Outliner e journal |
| Assets | Import/download local, hash, licenca, preview e insercao persistida |
| Globo | Terreno/imagery/modelo reais no renderer escolhido, offline apos cache |
| Pacotes | Manifesto, tamanho, checksum, licencas, install/update/remove/rollback |
| Render | Job persistente, progresso real, cancelamento, retomada e artefato |
| Reality | Ingest real, hash, pose, proveniencia, outputs e falha explicita |
| Designer | Comandos parametricos persistidos, malha derivada, medidas calculadas e export real |
| IA | Modelo real, ferramentas tipadas, aprovacao, diff e journal transacional |
| Documentacao | Estado gerado/atualizado sem duplicatas ou indicadores ficticios |

---

## 15. Evidencias auditadas

### Codigo principal

- `Cargo.toml`
- `crates/arcz-app/src/main.rs`
- `crates/arcz-app/src/server.rs`
- `crates/arcz-app/src/comandos.rs`
- `crates/arcz-app/src/ui/index.html`
- `crates/arcz-app/src/ui/app.js`
- `crates/arcz-tauri/src/main.rs`
- `crates/arcz-tauri/src/renderer_bridge.rs`
- `crates/arcz-tauri/src/superficie.rs`
- `crates/arcz-earth/src/lib.rs`
- `crates/arcz-scene/src/lib.rs`
- `crates/arcz-jobs/src/lib.rs`
- `crates/arcz-provenance/src/lib.rs`

### Documentacao cruzada

- `docs/audit/*`
- `docs/auditoria/*`
- `docs/decisions/ADR-0001*`
- `docs/decisions/ADR-0002*`
- `docs/decisions/ADR-0004*`
- `docs/integrations/UI_ENGINE_CONTRACT.md`
- `docs/open-source/*`
- `docs/agent/ARCZ_EXECUTION_STATE.json`

### Limite desta auditoria

Nao foram executados build, testes, clippy, servidor, interface ou GPU nesta tarefa. Portanto:

- Nao se confirma que os testes historicamente registrados continuam verdes.
- Nao se confirma o comportamento visual atual da surface Tauri.
- Nao se confirma o estado do repositorio React/FAVELION externo.
- Nao se confirma disponibilidade atual de provedores de rede.

Essas validacoes devem ser uma tarefa separada e reproduzivel, depois da decisao de arquitetura P0.

---

## 16. Estado final da auditoria

### Concluido nesta tarefa

- Cruzamento de UI, telas, contrato de comandos, servidor, Tauri e documentos.
- Identificacao de modulos reais, parciais, simulados e duplicados.
- Priorizacao P0/P1/P2/P3.
- Registro de riscos e bloqueadores.
- Plano de higiene documental.
- Definicao deste documento como fonte canônica de estado.

### Nao realizado por restricao de escopo

- Nenhuma alteracao de codigo.
- Nenhuma exclusao de arquivo historico.
- Nenhuma execucao de testes ou validacao visual.
- Nenhuma decisao irreversivel sobre renderer, UI, Scene Graph ou store.

### Proxima decisao recomendada

Fechar em uma unica decisao tecnica os quatro itens abaixo:

1. `arcz-app::cena` ou `arcz-scene` como Scene Graph autoritativo.
2. SQLite ou `.arcz` como store autoritativo.
3. wgpu nativo ou Cesium como renderer principal do editor.
4. UI embutida atual ou React/Tauri externo como produto oficial.

Sem essas quatro escolhas, qualquer nova tela aumenta a divergencia em vez de aproximar o ARCZ de um produto funcional.
