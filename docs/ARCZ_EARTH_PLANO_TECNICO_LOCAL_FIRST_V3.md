# ARCZ Earth — Plano Técnico Local-First V3

> Documento de evolução consolidado para o ARCZ Earth. Preserva a arquitetura auditada,
> corrige fragilidades já medidas e adiciona o subsistema de geração procedural regional
> de terrenos, lotes, casas, edifícios, vias, vegetação e ambientação, com IA auxiliar,
> execução determinística, streaming por tiles, orçamento explícito e recuperação de falhas.

---

# 0. Objetivo de produto

O ARCZ Earth deve evoluir de um visualizador geográfico com edição e geração local para uma
plataforma geoespacial autoral capaz de:

1. localizar uma área por endereço, lote, bairro, cidade ou estado;
2. compreender o contexto físico e urbano real da região;
3. gerar apenas a área necessária, nunca o território inteiro de uma vez;
4. construir terreno, lotes, vias, casas, prédios, muros, calçadas e vegetação plausíveis;
5. manter coerência regional de tipologia, telhado, materiais, paleta e paisagismo;
6. permitir edição manual sem que uma regeneração destrua o trabalho do usuário;
7. operar em tempo real com LOD, instancing, atlases, impostores e streaming;
8. gerar imagens, filmes, pranchas e exportações 3D;
9. funcionar integralmente offline, sem depender de API, nuvem ou provedor externo; IA e inferência são locais, opcionais e degradáveis;
10. falhar de forma visível, recuperável e sem corromper o projeto.

A geração não será “IA criando uma cidade”. Será um sistema híbrido:

```text
geodados reais
+ regras arquitetônicas e urbanísticas
+ gramáticas procedurais
+ biblioteca modular validada
+ IA para classificação e inferência
+ orçamento de CPU/GPU
+ edição humana preservada
```

---

# 1. Decisões arquitetônicas obrigatórias

## 1.1 Manter a base atual

A implementação parte da topologia já existente:

```text
ES modules puros no navegador
        ↓ HTTP
servidor.py como gateway local
        ↓ subprocessos/jobs
crates Rust como núcleo geométrico e geoespacial
```

Não introduzir framework ou bundler no front nesta etapa. O front atual deve continuar
recarregável diretamente do disco. Novos módulos entram em subpastas e não ampliam `ui.js`.


# 1.3 Princípio local-first absoluto

A palavra **local** neste documento significa:

1. motor geométrico executado no computador do usuário;
2. inferência executada por modelos locais;
3. banco de dados, índices, cache, assets, manifests e histórico locais;
4. geocodificação e busca territorial disponíveis offline;
5. render, difusão, upscale e processamento de imagens locais;
6. nenhum requisito funcional condicionado a API externa;
7. conectores externos são opcionais, removíveis e desligados por padrão;
8. todo dado externo usado uma vez deve ser materializado em pacote local com licença, hash e proveniência.

## 1.4 Modos de rede

```text
offline_strict   → nenhum socket de saída; modo padrão e testado em CI
local_lan       → apenas serviços na máquina ou rede local autorizada
import_assisted → conectores remotos temporários e explicitamente autorizados
```

O estado do projeto registra o modo utilizado, mas um projeto criado em `import_assisted` deve
continuar funcionando em `offline_strict` após a materialização dos dados.

## 1.5 Regra de dependência

Uma capacidade só é considerada pronta quando passa o teste de cabo desconectado. Recursos que
não passam esse teste recebem o rótulo `conector_opcional`, nunca `core`.

## 1.2 Responsabilidade por camada

### Front-end

Responsável por:

- interação;
- seleção de região;
- visualização;
- edição;
- timeline;
- telemetria visível;
- ativação e parametrização de plugins;
- envio de jobs;
- cancelamento;
- aplicação transacional de resultados.

Não deve:

- executar geração pesada;
- acessar rede ou serviços externos diretamente;
- armazenar segredos;
- inferir regras regionais de maneira definitiva;
- manipular caminhos livres do sistema de arquivos.

### Servidor Python

Responsável por:

- gateway HTTP local;
- autenticação local; gestão de conectores externos opcionais, isolados e desligados por padrão;
- cache;
- rate limiting;
- fila local de jobs; conectores de importação remota opcionais e isolados;
- geocodificação local e resolução sobre índices offline;
- coordenação de IA Python;
- emissão de eventos de progresso;
- gestão de processos Rust;
- validação de manifests e arquivos.

Não deve se tornar o motor geométrico principal.

### Rust

Responsável por:

- geometria;
- projeções;
- parsing de OSM/Overture;
- lotes e footprints;
- gramáticas procedurais;
- adaptação ao terreno;
- geração de telhados;
- fachadas paramétricas;
- scatter determinístico;
- LOD;
- mesh merge;
- atlases e manifests de saída;
- validação espacial;
- geração por tile.

## 1.3 IA é auxiliar e substituível

A IA nunca será requisito para abrir, editar ou salvar um projeto.

Ela pode:

- classificar estilo regional;
- estimar atributos ausentes;
- extrair paletas;
- classificar telhados e fachadas;
- segmentar cobertura do solo;
- sugerir presets;
- gerar variações de textura;
- produzir mapas de profundidade, normal ou máscara;
- operar ferramentas do ARCZ por contrato.

Ela não pode:

- modificar `viewer` diretamente;
- gerar geometria e aplicá-la sem validação;
- substituir uma peça bloqueada pelo usuário;
- alterar o projeto fora de uma transação;
- inventar dado real sem marcar confiança e origem;
- escrever segredo no front ou no `projeto.json`.

---

# 2. Correções de fundação antes dos geradores

## 2.1 Autosave com debounce e teto absoluto

Implementar dois relógios:

```text
debounce_quieto = 600 ms
flush_maximo = 5 s
```

Regras:

- alteração inicia ou reinicia o debounce;
- a primeira alteração abre uma janela máxima de 5 segundos;
- ao atingir 5 segundos, salvar mesmo com câmera em movimento;
- gravação atômica: escrever arquivo temporário, `fsync`, renomear;
- manter `projeto.json.bak` da última versão válida;
- registrar `save_revision`, hash e horário;
- câmera pode atualizar estado transitório a 60 Hz, mas persistência deve consolidar o último
  valor sem serializar 60 vezes por segundo.

Critério de aceite:

- girar o globo por 60 segundos;
- matar o processo sem fechar corretamente;
- reabrir;
- recuperar posição com perda máxima de 5 segundos;
- `projeto.json` permanecer parseável.

## 2.2 Estado dividido em persistente, transitório e derivado

O estado único continua como fonte da verdade, mas passa a ter três classes:

```text
persistente  → vai para projeto.json
transitorio  → cursor, hover, FPS, progresso, câmera em interação
 derivado    → bbox, orçamento usado, índices, caches, previews
```

Nenhum dado derivado deve ser salvo como verdade primária quando puder ser recalculado.

## 2.3 Registro central de origens

Criar `app/core/origens.js`:

```js
export const ORIGENS = Object.freeze({
  CAMERA: 'camera',
  CENA: 'cena',
  REGIAO: 'regiao',
  GERADOR: 'gerador',
  IA: 'ia',
  HISTORICO: 'historico',
  SISTEMA: 'sistema'
});
```

Todo módulo consumidor declara explicitamente as origens que processa e as que ignora.
Origem desconhecida em modo desenvolvimento gera aviso, não falha silenciosa.

## 2.4 Guard rail de render

Todo callback chamado pelo Cesium deve passar por wrapper seguro:

```js
safeCallback(nome, fallback, fn)
```

O wrapper:

- captura exceção;
- retorna fallback válido;
- incrementa contador;
- desativa o recurso após limite configurável;
- registra plugin, entidade, frame e stack;
- mostra aviso não bloqueante;
- nunca relança dentro do laço de render.

## 2.5 Health check de inicialização

Cada módulo implementa:

```js
init(ctx) -> { ok, versao, dependencias, avisos }
```

O bootstrap:

1. valida ordem;
2. valida dependências;
3. inicializa;
4. executa smoke test;
5. só então publica o módulo como disponível.

Falha em plugin não deve impedir o globo base de abrir.

---

# 3. Nova organização de código

```text
app/
├── core/
│   ├── contexto.js
│   ├── eventos.js
│   ├── origens.js
│   ├── safe-callback.js
│   ├── transacao.js
│   ├── budget-client.js
│   ├── job-client.js
│   └── schema.js
├── shell/
│   ├── modos/
│   ├── paineis/
│   ├── cards/
│   ├── navbar/
│   └── workspace.js
├── region/
│   ├── region-controller.js
│   ├── region-selector.js
│   ├── region-overlay.js
│   ├── scale-policy.js
│   └── lot-drawing-adapter.js
├── plugins/
│   ├── registry.js
│   ├── loader.js
│   ├── validator.js
│   ├── orchestrator.js
│   ├── lifecycle.js
│   └── builtin/
├── procedural/
│   ├── profile-ui.js
│   ├── generation-ui.js
│   ├── overrides-ui.js
│   └── diagnostics-ui.js
├── cine/
│   ├── timeline.js
│   ├── tracks.js
│   ├── keyframes.js
│   ├── interpolation.js
│   ├── quick-starts.js
│   ├── render-queue.js
│   └── export-camera.js
└── walk/
    ├── panorama-viewer.js
    ├── street-sequence.js
    └── earth-to-street.js
```

Novos crates:

```text
crates/
├── arcz-region       contexto e perfil regional
├── arcz-procedural   gramáticas e composição
├── arcz-roof         telhados robustos
├── arcz-facade       fachadas paramétricas
├── arcz-vegetation   scatter e biomas
├── arcz-tiles        geração, cache e manifests por tile
├── arcz-budget       estimativa e validação de custo
├── arcz-validation   invariantes geométricas
└── arcz-determinism  sementes, hashing e replay
```

Não é obrigatório criar todos de uma vez. A separação deve ocorrer quando o módulo ultrapassar
responsabilidade única ou começar a criar dependência circular.

---

# 4. Região Ativa

## 4.1 Região de trabalho versus região de geração

Nunca confundir:

```text
região de trabalho → o que o usuário selecionou ou está visualizando
região de geração  → tiles dentro do orçamento e da distância ativa
```

Uma cidade inteira pode ser região de trabalho, mas apenas um conjunto pequeno de tiles será
gerado em alta qualidade.

## 4.2 Escalas suportadas

```text
mundo
continente
pais
estado
cidade
bairro
quarteirao
endereco
lote
poligono_manual
```

Cada plugin declara escalas permitidas. O orquestrador não oferece um gerador fora de sua
escala válida.

## 4.3 Resolução espacial por anéis

Ao redor da câmera ou foco:

```text
Anel A — hero       alta fidelidade, editável
Anel B — próximo    procedural completo, LOD1
Anel C — médio      massas + fachada simplificada
Anel D — distante   volumes, impostores, cobertura agregada
Fora                dados-base do globo
```

O tamanho dos anéis depende do perfil de qualidade e do orçamento medido.

## 4.4 Identidade de tile

Todo tile possui chave estável:

```text
provider/version/z/x/y/profile_hash/generator_version/seed
```

A mesma entrada deve gerar o mesmo resultado binário ou geometricamente equivalente.

## 4.5 Estado de tile

```text
MISSING
QUEUED
FETCHING
PREPARING
GENERATING
VALIDATING
READY
APPLYING
ACTIVE
EVICTING
FAILED_RETRYABLE
FAILED_PERMANENT
CANCELLED
```

Toda transição inválida deve ser rejeitada e registrada.

---

# 5. Geo Context Engine

## 5.1 Entrada normalizada

```json
{
  "region_id": "uuid",
  "bbox_wgs84": [0, 0, 0, 0],
  "polygon_wgs84": [],
  "focus": { "lat": 0, "lon": 0 },
  "scale": "bairro",
  "requested_radius_m": 1500,
  "sources": {
    "osm": true,
    "overture": false,
    "dem": true,
    "imagery": true,
    "street": false
  }
}
```

## 5.2 Saída: `RegionContext`

```json
{
  "schema_version": 1,
  "region_id": "uuid",
  "crs_work": "ENU_LOCAL",
  "origin_wgs84": [0, 0, 0],
  "terrain": {
    "min_m": 0,
    "max_m": 0,
    "mean_slope_deg": 0,
    "slope_classes": {},
    "confidence": 1.0
  },
  "urban": {
    "density": "medium",
    "block_pattern": "irregular",
    "road_hierarchy": {},
    "building_height_distribution": {},
    "landuse_distribution": {}
  },
  "environment": {
    "biome": "atlantic_forest_coastal",
    "climate_profile": "humid_subtropical",
    "soil_profile": "unknown"
  },
  "evidence": [],
  "warnings": []
}
```

## 5.3 Evidência e confiança

Toda inferência deve registrar:

```json
{
  "field": "roof.type_distribution",
  "value": { "gable": 0.62, "flat": 0.25, "hip": 0.13 },
  "source": "vision_classifier",
  "source_ref": "dataset-or-image-id",
  "confidence": 0.78,
  "timestamp": "ISO-8601"
}
```

Regra:

- dado explícito supera inferência;
- edição humana supera dado explícito importado;
- inferência de baixa confiança não muda geometria crítica sem confirmação ou fallback.

---

# 6. Regional Style Profile

## 6.1 Perfil não é prompt

O perfil regional é um objeto versionado e validável, não texto livre.

```json
{
  "id": "br.sc.coastal.midrise.v1",
  "version": "1.0.0",
  "architecture": {
    "building_mix": {
      "detached_house": 0.35,
      "townhouse": 0.15,
      "lowrise_multifamily": 0.30,
      "midrise_multifamily": 0.20
    },
    "floor_ranges": {
      "detached_house": [1, 2],
      "midrise_multifamily": [4, 8]
    },
    "setbacks_m": {
      "front": [2.0, 6.0],
      "side": [0.0, 2.5]
    }
  },
  "roofs": {
    "types": { "gable": 0.42, "hip": 0.28, "flat": 0.30 },
    "pitch_deg": [18, 38],
    "eave_m": [0.25, 0.90],
    "materials": {}
  },
  "facades": {
    "palette": [],
    "materials": {},
    "window_ratios": {},
    "balcony_probability": 0.55
  },
  "vegetation": {
    "biome_profile": "atlantic_forest_coastal",
    "street_tree_density": [0.05, 0.25],
    "lot_green_ratio": [0.10, 0.45]
  }
}
```

## 6.2 Camadas de perfil

O perfil final é composição determinística:

```text
perfil_global
→ país
→ estado/bioma
→ cidade
→ bairro
→ lote
→ override do usuário
```

Conflitos são resolvidos por precedência explícita e registrados em relatório.

## 6.3 Biblioteca inicial de perfis

Começar com poucos perfis de alta qualidade:

1. litoral sul brasileiro residencial/misto;
2. metropolitano vertical denso;
3. suburbano horizontal murado;
4. condomínio horizontal contemporâneo;
5. interior brasileiro horizontal;
6. serra úmida e inclinada;
7. industrial/comercial de baixa altura;
8. centro urbano misto com comércio no térreo.

Não criar dezenas de perfis rasos antes de validar os primeiros.

---

# 7. Gerador de lotes e implantação

## 7.1 Fonte de verdade

Ordem de preferência:

1. lote desenhado e bloqueado pelo usuário;
2. lote cadastral importado com licença válida;
3. footprint parcelário confiável;
4. inferência por quadra e frente de rua;
5. lote procedural marcado como estimado.

## 7.2 Algoritmo de parcelamento inferido

Pipeline:

1. construir polígono de quadra a partir das vias;
2. remover áreas não edificáveis;
3. identificar frentes de rua;
4. subdividir por faixas plausíveis;
5. ajustar por esquinas, servidões e declive;
6. validar área, largura mínima, acesso e auto-interseção;
7. marcar confiança.

## 7.3 Área edificável

Calcular:

- recuos;
- taxa de ocupação;
- platôs;
- faixa de acesso;
- garagem provável;
- áreas verdes;
- zonas de alto declive.

A geometria nunca deve ultrapassar o lote, salvo elementos explicitamente permitidos.

---

# 8. Gramática de casas

## 8.1 Estrutura

```text
implantação
→ envelope edificável
→ massa principal
→ volumes secundários
→ pavimentos
→ cobertura
→ aberturas
→ fachada
→ garagem/acesso
→ muros/portões
→ terreno do lote
→ paisagismo
→ detalhes por LOD
```

## 8.2 Tipos mínimos

- térrea retangular;
- térrea em L;
- sobrado compacto;
- sobrado com garagem frontal;
- casa geminada;
- casa em aclive;
- casa em declive com embasamento;
- casa contemporânea de platibanda;
- casa tradicional de telhado aparente.

## 8.3 Regras de plausibilidade

- porta principal acessível;
- garagem compatível com largura do lote;
- janelas associadas a pavimentos válidos;
- ausência de abertura enterrada sem poço de luz;
- telhado sem faces invertidas ou vales impossíveis;
- calhas e cumeeiras coerentes;
- beiral sem atravessar lote vizinho quando proibido;
- escada/rampa dentro de inclinação admissível do preset;
- fachada frontal com hierarquia legível;
- caixa d’água e equipamentos ocultos ou simplificados de forma plausível.

## 8.4 Variação controlada

Toda variação usa semente estável e limites do perfil. Evitar ruído uniforme em tudo.
Variações correlacionadas são preferíveis:

```text
perfil tradicional
→ telhado cerâmico
→ beiral maior
→ esquadria clara
→ paleta quente
```

Não combinar características incompatíveis aleatoriamente.

---

# 9. Gerador robusto de telhados

## 9.1 Tipos

- uma água;
- duas águas;
- quatro águas;
- meia água anexa;
- mansarda simplificada;
- shed industrial;
- platibanda plana;
- composição mista por volumes.

## 9.2 Algoritmo

Para footprints simples:

- regras analíticas especializadas.

Para footprints complexos:

- straight skeleton ou wavefront roof;
- decomposição em polígonos simples;
- união validada;
- fallback para platibanda quando a solução falhar.

## 9.3 Validações

- malha manifold quando aplicável;
- normais consistentes;
- sem triângulos degenerados acima do limiar;
- sem auto-interseção;
- altura de cumeeira limitada;
- drenagem coerente;
- UV orientado por água do telhado;
- espessura e fascia dependentes do LOD.

## 9.4 Política de fallback

Nunca bloquear uma tile inteira por um telhado impossível:

```text
solução regional desejada
→ solução simplificada do mesmo tipo
→ telhado duas águas alinhado ao eixo principal
→ platibanda segura
→ massa sem telhado detalhado + warning
```

---

# 10. Gerador de prédios

## 10.1 Estrutura vertical

```text
embasamento
corpo-tipo
coroamento
cobertura técnica
```

Cada faixa pode ter gramática e material próprios.

## 10.2 Fachada modular

A fachada nasce de uma grade parametrizada:

- vãos por módulo;
- pilares;
- panos opacos;
- varandas;
- guarda-corpos;
- brises;
- shafts;
- variação de canto;
- térreo comercial opcional.

Instanciar módulos repetidos sempre que possível.

## 10.3 Regras de coerência

- alinhamento vertical de shafts e prumadas visuais;
- repetição de pavimento tipo;
- térreo com altura e acesso diferenciados;
- cobertura técnica compatível com a torre;
- varandas sem interpenetração;
- esquadrias dentro da face hospedeira;
- densidade de detalhe proporcional à distância.

## 10.4 Representação por LOD

```text
LOD0 hero      geometria completa visível, varandas, molduras e cobertura
LOD1 próximo   módulos simplificados e materiais completos
LOD2 médio     fachada por atlas, pouca geometria saliente
LOD3 distante  volume + textura/normal agregada
LOD4 horizonte impostor ou massa urbana agregada
```

---

# 11. Materiais e fachadas regionais

## 11.1 Material paramétrico

Cada material declara:

```json
{
  "id": "facade.render.offwhite",
  "category": "facade",
  "base_color_range": [],
  "roughness_range": [0.65, 0.9],
  "normal_strength_range": [0.1, 0.35],
  "weathering": {},
  "texel_density": 256,
  "lod_policy": {}
}
```

## 11.2 Variação sem repetição aparente

Usar:

- atlas;
- variação de UV;
- tint por instância;
- macro noise de baixa frequência;
- decals esparsos;
- máscaras de sujeira orientadas por altura e chuva;
- limite estrito para não transformar realismo em degradação exagerada.

## 11.3 Extração de paleta por IA

A IA devolve paleta estruturada com confiança. O sistema:

- remove outliers;
- agrupa em famílias de material;
- converte para espaço perceptual;
- limita saturação e luminância;
- cria preset editável;
- nunca aplica imagem externa diretamente como textura sem licença e rastreabilidade.

---

# 12. Terreno e adaptação topográfica

## 12.1 Terreno base

- DEM em tiles;
- origem ENU local por região ativa;
- cache por versão e fonte;
- preenchimento de lacunas;
- suavização preservando cristas e drenagem;
- erro vertical registrado.

## 12.2 Adaptação de lote

Para cada implantação:

1. amostrar terreno;
2. calcular plano de melhor ajuste;
3. escolher estratégia: acompanhar, criar platô, corte/aterro ou embasamento;
4. gerar taludes ou muros de arrimo;
5. conectar acesso à rua;
6. evitar descontinuidades visíveis entre tiles.

## 12.3 Costura entre tiles

Tiles vizinhos compartilham borda quantizada e hash de fronteira. A aplicação deve rejeitar
resultado cuja borda não coincide além da tolerância.

---

# 13. Vegetação procedural leve

## 13.1 Biome profile

O bioma define espécies funcionais, não apenas nomes botânicos:

- árvore de rua pequena/média/grande;
- copa densa ou aberta;
- arbusto ornamental;
- arbusto espontâneo;
- touceira;
- capim;
- cobertura de solo;
- massa florestal de encosta.

## 13.2 Scatter determinístico

Entradas:

- máscara de solo;
- distância de edifícios;
- distância de via;
- declive;
- umidade aproximada;
- uso do solo;
- semente do tile;
- perfil regional.

Saída:

- transformações instanciadas;
- espécie/variante;
- LOD inicial;
- cluster id;
- razão da colocação.

## 13.3 Performance

- `EXT_mesh_gpu_instancing` para árvores, postes, arbustos e módulos repetidos;
- atlas compartilhado por família;
- billboard/impostor em distância;
- floresta de fundo como clusters agregados;
- densidade reduzida por screen-space error;
- limite de overdraw em grama;
- grama detalhada apenas em anel hero;
- patches de cobertura em vez de lâminas individuais fora do hero.

## 13.4 Regras de exclusão

Não colocar vegetação:

- dentro de edificações;
- em vias;
- sobre acesso de garagem;
- em água;
- em declive proibido para a espécie;
- encostada a fachada sem regra específica;
- em área manualmente bloqueada.

---

# 14. Vias, calçadas e mobiliário

## 14.1 Hierarquia viária

Classificar:

- arterial;
- coletora;
- local;
- viela;
- pedonal;
- acesso privado.

Cada classe define largura, material, meio-fio, calçada, arborização e mobiliário.

## 14.2 Continuidade

Vias devem ser geradas por segmentos com nós compartilhados. Não gerar cada tile isoladamente
sem costura lógica. Interseções usam gerador específico.

## 14.3 Detalhe por distância

- próximo: guias, sarjetas, faixas, tampas e acessibilidade simplificada;
- médio: seção da via e material;
- distante: malha simples ou própria imagery.

---

# 15. Orquestrador de geração

## 15.1 Pipeline por job

```text
VALIDATE_REQUEST
→ RESOLVE_REGION
→ ACQUIRE_INPUTS
→ BUILD_CONTEXT
→ RESOLVE_STYLE
→ PLAN_TILES
→ ESTIMATE_BUDGET
→ GENERATE
→ VALIDATE_OUTPUT
→ STAGE_RESULT
→ APPLY_TRANSACTION
→ INDEX
→ PERSIST
```

## 15.2 Cancelamento cooperativo

Todo estágio verifica cancelamento. Processos Rust recebem token/job id. Subprocessos devem
ser encerrados com prazo e kill de segurança.

Trocar região:

- cancela jobs não aplicados;
- resultados tardios são descartados por `generation_epoch`;
- assets já compartilhados permanecem no cache;
- nenhuma primitiva órfã fica na cena.

## 15.3 Aplicação transacional

O resultado é primeiro montado em staging:

```text
scene/staging/<job_id>
```

Somente após validação:

1. registrar undo command;
2. adicionar novos recursos;
3. trocar índices;
4. remover versão anterior;
5. confirmar estado;
6. persistir manifest.

Falha antes do commit limpa staging. Falha após início do commit executa rollback.

---

# 16. Budget Engine

## 16.1 Dimensões do orçamento

```text
triângulos visíveis
instâncias
draw calls estimadas
memória de geometria
memória de textura
framebuffer
número de materiais
overdraw de vegetação
tempo CPU de geração
tempo de upload GPU
armazenamento em cache
```

## 16.2 Perfis

```text
LEVE
EQUILIBRADO
ALTO
CINEMATICO
CUSTOM
```

O perfil não é apenas um rótulo. Ele resolve limites numéricos por hardware medido.

## 16.3 Reserva e compromisso

Antes de gerar, o plugin solicita reserva. O orquestrador pode:

- aceitar;
- reduzir qualidade;
- dividir em tiles menores;
- adiar;
- pedir confirmação;
- rejeitar.

Depois da geração, comparar estimativa versus custo real e alimentar telemetria.

## 16.4 Evicção

Prioridade de permanência:

1. seleção atual e objetos bloqueados;
2. anel hero;
3. tiles visíveis;
4. tiles previstos pela velocidade da câmera;
5. cache recente;
6. distante não visível.

---

# 17. Contrato de plugin V2

```js
export default {
  manifest: {
    tipo: 'gerador',
    id: 'arcz.buildings.regional',
    nome: 'Edificações regionais',
    versao: '2.0.0',
    apiVersion: '2',
    escalas: ['lote', 'endereco', 'quarteirao', 'bairro'],
    modos: ['globo', 'walk', 'render'],
    capacidades: ['region.read', 'terrain.read', 'scene.stage', 'assets.read'],
    deterministico: true,
    worker: 'rust',
    custoBase: {
      triangulos: 500000,
      memoriaMB: 180,
      texturasMB: 120,
      drawCalls: 300
    }
  },

  parametros: [],

  async validar(ctx, params) {},
  async estimar(ctx, params) {},
  async preparar(ctx, params, signal) {},
  async gerar(ctx, params, signal, progress) {},
  async validarResultado(ctx, result) {},
  async stage(ctx, result) {},
  async commit(ctx, staged) {},
  async rollback(ctx, staged, reason) {},
  async limpar(ctx) {},
  serializar() {},
  migrar(savedState, fromVersion) {}
};
```

## 17.1 Capacidades, não acesso irrestrito

O `ctx` expõe facades limitadas. Exemplo:

```text
ctx.region.read()
ctx.terrain.sample()
ctx.osm.queryCached()
ctx.assets.resolveById()
ctx.scene.stagePrimitive()
ctx.budget.reserve()
ctx.jobs.progress()
ctx.telemetry.event()
```

Nenhum plugin recebe o `viewer` cru por padrão.

## 17.2 Sandbox lógico

Mesmo rodando no mesmo processo JS, o loader:

- congela o contexto;
- valida manifest;
- controla capacidades;
- mede recursos criados;
- intercepta timers e subscriptions registradas pelo plugin;
- exige descarte no `limpar()`;
- detecta listeners, primitives e handles vazados.

## 17.3 Teste de limpeza

Para aprovar um plugin:

1. medir cena, listeners, timers, maps e memória referenciada;
2. ativar;
3. gerar amostra mínima;
4. limpar;
5. repetir três vezes;
6. comparar contadores;
7. rejeitar crescimento residual acima da tolerância.

---

# 18. Preservação de edição manual

## 18.1 Ownership por entidade

Cada entidade tem:

```json
{
  "owner": "generator:arcz.buildings.regional",
  "source_tile": "...",
  "generation_version": 4,
  "seed": 123,
  "locked": false,
  "overrides": {},
  "provenance": []
}
```

## 18.2 Estados de edição

```text
GENERATED
OVERRIDDEN
LOCKED
DETACHED
DELETED_BY_USER
```

- `OVERRIDDEN`: regeneração mantém parâmetros manuais compatíveis;
- `LOCKED`: entidade não é tocada;
- `DETACHED`: deixa de pertencer ao gerador;
- `DELETED_BY_USER`: tombstone impede reaparecimento até reset explícito.

## 18.3 Regeneração diferencial

Comparar fingerprints de entrada. Regenerar somente:

- tiles alterados;
- entidades afetadas por mudança de perfil;
- níveis de detalhe necessários;
- dependências descendentes.

---

# 19. Determinismo, provenance e reprodutibilidade

## 19.1 Sementes hierárquicas

```text
project_seed
→ region_seed
→ tile_seed
→ parcel_seed
→ building_seed
→ component_seed
```

Gerar seed por hash estável, não por ordem de execução assíncrona.

## 19.2 Manifest de geração

```json
{
  "job_id": "uuid",
  "generator": "arcz.buildings.regional@2.0.0",
  "inputs_hash": "sha256",
  "profile_hash": "sha256",
  "seed": 0,
  "source_versions": {},
  "outputs": [],
  "warnings": [],
  "metrics": {},
  "created_at": "ISO-8601"
}
```

## 19.3 Replay

Um job deve poder ser reexecutado por manifest. Se não produzir resultado equivalente, o
plugin perde a marca `deterministico` e não pode alimentar cache compartilhado.

---

# 20. IA contextual

## 20.0 Execução exclusivamente local

Toda inferência padrão roda localmente. O ARCZ não envia imagem, geometria, endereço, projeto,
prompt, telemetria ou metadado para serviços externos.

Backends suportados devem ser adaptadores locais, por exemplo:

- ONNX Runtime;
- llama.cpp/Ollama local para raciocínio textual e ferramentas;
- PyTorch local para visão e difusão;
- ComfyUI local como grafo opcional de render;
- TensorRT/DirectML/CUDA quando disponíveis;
- CPU fallback para tarefas não interativas.

Cada modelo possui manifesto local com licença, checksum, tamanho, VRAM/RAM estimada,
quantização, dispositivo compatível e fallback. Modelo ausente não causa falha estrutural: o
sistema usa regras procedurais ou solicita instalação/importação local.

Nenhum plugin pode chamar uma API de IA diretamente. Ele solicita inferência ao `Local AI
Broker`, que aplica orçamento, fila, sandbox, timeout, cancelamento, cache e auditoria.


## 20.1 Serviços separados

```text
style-classifier
roof-classifier
facade-palette
landcover-segmentation
height-estimator
depth-estimator
material-synth
render-diffusion
```

Cada serviço tem contrato, versão, modelo, checksum e política de fallback.

## 20.2 Inferência assíncrona e cacheável

Chave de cache:

```text
model_checksum/input_hash/parameters_hash
```

Resultado de IA é artefato derivado, nunca fonte imutável.

## 20.3 Confiança e fallback

Exemplo:

```text
confiança >= 0.80 → aplicar automaticamente em campo não crítico
0.55–0.79         → combinar com perfil regional
< 0.55            → ignorar e usar regra procedural
```

Campos críticos, como limite de lote, nunca dependem só de visão inferida.

## 20.4 Proteção contra estilo incoerente

A IA não produz parâmetros livres. Ela escolhe ou pondera opções válidas do schema regional.
Valores fora do domínio são rejeitados.

---

# 21. Street-level e transição Earth → Street

O módulo de rua deve ser opcional e desacoplado:

```text
Panoramax auto-hospedado/local ou captura própria; instância remota somente como importador opcional
+ visualizador 360
+ mapa sincronizado
+ sequência georreferenciada
+ profundidade/reconstrução opcional
```

A integração deve permitir:

- selecionar ponto no mapa;
- abrir panorama próximo;
- sincronizar heading e posição;
- navegar por sequência;
- retornar ao globo;
- criar transição cinematográfica;
- usar imagens como evidência regional, respeitando licença e privacidade.

Dados do Google Street View não entram no banco próprio do ARCZ.

---

# 22. Cinema e Earth Studio ampliado

## 22.1 Modelo de timeline

```text
timeline
├── camera.position
├── camera.orientation
├── camera.target
├── lens.fov/focalLength
├── lens.aperture
├── focus.distance
├── environment.time
├── environment.weather
├── object.transform
├── object.visibility
├── overlay.opacity
└── generator.parameters
```

## 22.2 Interpolação

- posição geográfica: ENU local por trecho, com conversão segura;
- posição longa: trajetória geodésica;
- orientação: quaternion SLERP;
- valores escalares: linear, bezier, hermite e hold;
- altitude espaço→solo: adaptação logarítmica opcional;
- câmera target: constraint avaliada por frame.

## 22.3 Quick Starts

- zoom-to;
- point-to-point;
- orbit;
- spiral;
- fly-to-and-orbit;
- street-entry;
- reveal-building;
- solar-study;
- drone-pass.

Quick Start cria keyframes editáveis; não é modo especial irreversível.

## 22.4 Render offline

- sequência PNG/EXR quando suportado pelo pipeline;
- checkpoint por frame;
- retomada;
- subframes para motion blur;
- seed e estado congelados;
- preflight de memória;
- relatório de frames falhos;
- nunca sobrescrever frames válidos sem opção explícita.

---

# 23. Render por difusão

## 23.1 Uso seguro

Difusão deve operar sobre passes da cena:

- beauty base;
- depth;
- normals;
- object ids;
- semantic masks;
- material masks;
- sky mask;
- motion/camera metadata quando disponível.

## 23.2 Modos

```text
material enhancement
vegetation enhancement
people/vehicles insertion
weather/time restyle
full photoreal pass
```

## 23.3 Garantias

- arquitetura protegida por condicionamento;
- seed fixa por frame/take;
- máscaras de proteção;
- comparação geométrica pós-processo;
- opção de rejeitar saída cuja borda estrutural se afaste acima do limiar;
- filme marcado experimental até passar teste de estabilidade temporal.

---

# 24. Schemas e migrações

## 24.1 `projeto.json` V2

Adicionar:

```text
schema_version
project_seed
active_region
region_profiles
plugins
procedural_layers
generation_manifests
overrides
tombstones
timeline
render_jobs
source_registry
```

## 24.2 Migração

- migração pura e testável;
- backup automático antes de migrar;
- nunca migrar destrutivamente sem cópia;
- projeto antigo abre com defaults equivalentes ao comportamento atual;
- migração idempotente;
- falha retorna ao arquivo anterior.

---

# 25. API local V2

Rotas sugeridas:

```text
/api/v2/regions/resolve
/api/v2/regions/context
/api/v2/profiles
/api/v2/profiles/infer
/api/v2/tiles/plan
/api/v2/generation/jobs
/api/v2/generation/jobs/{id}
/api/v2/generation/jobs/{id}/cancel
/api/v2/generation/jobs/{id}/events
/api/v2/plugins
/api/v2/plugins/{id}/validate
/api/v2/budget
/api/v2/diagnostics
/api/v2/render/jobs
/api/v2/ai/tools
```

Manter rotas antigas durante período de compatibilidade. Novas rotas devem retornar erro
estruturado:

```json
{
  "error": {
    "code": "BUDGET_EXCEEDED",
    "message": "...",
    "retryable": false,
    "details": {},
    "trace_id": "uuid"
  }
}
```

---

# 26. Operação offline, cache e conectores opcionais

## 26.1 Regra de soberania local

O ARCZ inicia e funciona com a rede indisponível. Nenhuma função essencial pode exigir login,
API key, endpoint público, licença SaaS, telemetria remota ou serviço de terceiro.

- modo padrão: `offline_strict`;
- rede negada por padrão no processo principal e nos workers;
- geocodificação sobre índices locais;
- OSM/Overture/DEM entram por pacotes previamente importados;
- modelos de IA, embeddings e pesos residem no armazenamento local;
- jobs, filas, cache, banco, catálogo e render são locais;
- ausência de internet nunca bloqueia abertura, edição, geração, render ou exportação.

## 26.2 Conectores remotos opcionais

Overpass público, Nominatim, Panoramax remoto, downloads de modelos e outros provedores são
**conectores de importação**, não dependências de execução. Permanecem desabilitados até
ativação explícita do usuário.

Cada conector roda fora do núcleo, com:

- capability explícita de rede;
- allowlist de domínio;
- credenciais no cofre local;
- token bucket e limite por provedor;
- backoff exponencial com jitter;
- `Retry-After`;
- circuit breaker;
- checksum, licença e proveniência;
- importação para pacote local imutável;
- opção de remover o conector sem quebrar projetos.

Depois da importação, o projeto referencia apenas o artefato local por hash.

## 26.3 Cache e fontes em camadas

```text
L1 memória
L2 disco local
L3 pacote offline/importado
L4 conector remoto opcional, somente para importação explícita
```

Cache registra licença, origem, validade e hash.

## 26.4 Corrupção

Todo artefato baixado ou gerado deve ter:

- tamanho;
- checksum;
- magic/header quando aplicável;
- schema/version;
- escrita temporária e rename atômico.

---

# 27. Telemetria local e diagnóstico

Sem enviar dados para fora por padrão.

Painel deve mostrar:

- FPS;
- memória estimada;
- draw calls;
- tiles por estado;
- jobs ativos;
- fila externa;
- cache hit rate;
- tempo por estágio;
- plugins com erro;
- callbacks protegidos acionados;
- recursos vazados;
- origem das entidades;
- orçamento reservado e real.

Exportar pacote diagnóstico sem segredos:

```text
logs
config sanitizada
manifests
versões
hardware
últimos erros
estado de jobs
```

---

# 28. Estratégia de testes

## 28.1 Rust

- unit tests de geometria;
- property-based tests para footprints e telhados;
- fuzzing de parsers OSM/glTF/manifests;
- golden tests de meshes;
- testes de determinismo;
- testes de borda de tile;
- benchmarks por tipo de gerador.

## 28.2 Front-end sem bundler

- lint/syntax check em CI;
- teste de importação de todos os módulos;
- detector de ciclo de import;
- smoke test com navegador real;
- teste da ordem de bootstrap;
- teste de callbacks que lançam;
- teste de montagem única de cards.

## 28.3 Integração

Cenários fixos:

1. lote plano residencial;
2. lote estreito;
3. lote em aclive;
4. lote em declive;
5. quadra irregular;
6. bairro denso;
7. tile sem OSM;
8. DEM ausente;
9. provider 429;
10. cancelamento no meio da geração;
11. crash durante commit;
12. regeneração com objeto bloqueado;
13. abertura de projeto antigo;
14. GPU abaixo do orçamento;
15. render 8K retomável.

## 28.4 Critérios geométricos automáticos

- sem NaN/Inf;
- índice dentro do buffer;
- bbox plausível;
- triangulação válida;
- normais válidas;
- área mínima;
- sem interseção crítica;
- elevação dentro de tolerância;
- acesso ao lote;
- costura de tile;
- orçamento real dentro do limite ou degradação aplicada.

---

# 29. Fases de implementação revisadas

## Fase 0 — Integridade e recuperação

- autosave com teto;
- escrita atômica e backup;
- safe callbacks;
- origem registrada;
- health check;
- diagnóstico mínimo.

**Gate:** projeto não corrompe sob crash e render não morre por callback de plugin.

## Fase 1 — Região Ativa e contratos espaciais

- seleção e autocomplete;
- região de trabalho versus geração;
- desenho de lote;
- escalas;
- ENU local;
- `RegionContext` inicial;
- cache e rate limit.

**Gate:** selecionar cidade não dispara geração de cidade inteira.

## Fase 2 — Tile Orchestrator e Budget Engine

- máquina de estados;
- jobs canceláveis;
- staging/commit/rollback;
- budget;
- evicção;
- manifests;
- determinismo.

**Gate:** plugin de referência gera e remove 100 vezes sem vazamento.

## Fase 3 — Contrato de plugins V2

- capabilities;
- schemas;
- loader;
- validator;
- migração;
- documentação gerada;
- plugin mínimo.

**Gate:** plugin inválido é rejeitado antes de tocar a cena.

## Fase 4 — Geo Context e perfis regionais

- contexto de terreno e urbano;
- evidência/confiança;
- perfis versionados;
- composição de perfil;
- UI de revisão.

**Gate:** toda inferência exibida com origem e confiança.

## Fase 5 — Terreno, lotes e vias

- terreno por tile;
- costura;
- inferência de lotes;
- implantação;
- vias e calçadas básicas;
- adaptação topográfica.

**Gate:** sem fissuras visíveis e sem prédio fora do lote.

## Fase 6 — Casas e telhados

- gramáticas residenciais;
- telhados robustos;
- fachadas;
- muros, garagens e acessos;
- LODs;
- overrides.

**Gate:** corpus de footprints adversariais sem crash; fallback sempre produz saída válida.

## Fase 7 — Prédios e fachadas modulares

- embasamento/corpo/coroamento;
- grade de fachada;
- comércio térreo;
- cobertura técnica;
- instancing;
- atlas.

**Gate:** bairro misto dentro do orçamento equilibrado.

## Fase 8 — Vegetação e biomas

- máscaras;
- scatter determinístico;
- árvores, arbustos, mato e floresta;
- impostores;
- exclusões;
- perfis regionais.

**Gate:** regenerar preserva exatamente as posições para mesma seed.

## Fase 9 — Casca e modos

- Globo;
- Floorplanner;
- Render;
- Walk;
- cards montados uma vez;
- multi-view;
- safe zones.

**Gate:** nenhum `id` duplicado e troca de modo não vaza listeners.

## Fase 10 — Cinema

- timeline;
- tracks;
- quick starts;
- curvas;
- camera target;
- track points;
- render em sequência;
- exportação JSON.

**Gate:** render retomável e trajetória reproduzível.

## Fase 11 — Street-level

- fonte aberta/própria;
- visualizador 360;
- mapa sincronizado;
- sequência;
- transição Earth→Street;
- privacidade e provenance.

**Gate:** nenhum dado sem licença/origem entra no catálogo.

## Fase 12 — IA contextual

- classificadores;
- paleta;
- telhado;
- cobertura do solo;
- cache;
- confiança;
- fallback sem IA.

**Gate:** desligar IA produz projeto funcional e reproduzível.

## Fase 13 — Difusão e 8K

- passes;
- máscaras;
- upscale em tiles;
- validação estrutural;
- imagem estática como entrega estável;
- vídeo experimental.

**Gate:** saída que altera arquitetura além do limite é rejeitada ou sinalizada.

## Fase 14 — Pranchas e documentação técnica

- planta;
- corte;
- elevação;
- estudo solar;
- blueprint;
- templates;
- exportação.

---

# 30. Definition of Done global

Uma fase só está concluída quando:

1. código implementado;
2. schema versionado;
3. migração criada quando necessária;
4. teste unitário e integração;
5. teste de cancelamento;
6. teste de limpeza;
7. telemetria;
8. erro estruturado;
9. documentação;
10. exemplo funcional;
11. orçamento medido;
12. projeto antigo continua abrindo;
13. aplicação abre com plugin desligado;
14. rollback comprovado;
15. sem crescimento de `ui.js`.

---

# 31. Ordem recomendada imediata

A próxima execução deve seguir exatamente:

```text
1. corrigir autosave e persistência atômica
2. criar core/origens, safe-callback e transação
3. formalizar RegionContext e Região Ativa
4. implementar tile state machine
5. implementar Budget Engine mínimo
6. elevar contrato de plugin para V2
7. converter entorno.js em plugin de referência
8. provar cancelamento, limpeza e determinismo
9. só então iniciar terreno/lotes/casas
```

O erro mais caro seria começar por casas, árvores ou IA antes do orquestrador. Isso criaria
módulos visualmente interessantes, porém incompatíveis, difíceis de cancelar, impossíveis de
orçamentar e perigosos para o estado atual.

---

# 32. Resultado esperado

Ao final, o ARCZ deverá permitir que o usuário selecione um endereço ou região e veja surgir
um entorno plausível e regionalmente coerente, com:

- terreno adaptado;
- lotes reais ou explicitamente estimados;
- casas e prédios coerentes;
- telhados robustos;
- fachadas e paletas locais;
- vias, calçadas e muros;
- grama, mato, arbustos e florestas leves;
- LOD e streaming;
- edição manual preservada;
- IA opcional e rastreável;
- cinema e render;
- operação recuperável e auditável.

A qualidade final não dependerá de uma única tecnologia. Ela virá da disciplina do sistema:
contexto real, regras explícitas, geração determinística, módulos validados, orçamento medido,
fallback seguro e edição humana soberana.

# 33. Gates obrigatórios de independência externa

Antes de qualquer release, executar:

1. **Teste sem rede:** bloquear DNS e tráfego de saída; abrir projeto, pesquisar no índice local, gerar terreno, casas, edifícios e vegetação, salvar, reabrir, renderizar e exportar.
2. **Teste sem credenciais:** remover todas as chaves; nenhuma função core pode degradar além de dados ainda não importados.
3. **Teste sem modelo de IA:** remover pesos; geração procedural continua funcional e determinística.
4. **Teste sem servidor público:** substituir endpoints por `127.0.0.1:9`; nenhum job core fica pendurado.
5. **Teste de materialização:** importar dado por conector, desligar o conector e reproduzir o projeto pelo hash local.
6. **Teste de privacidade:** confirmar por instrumentação que nenhum endereço, imagem ou geometria sai da máquina em `offline_strict`.
7. **Teste de remoção:** desinstalar todos os conectores externos; o aplicativo continua inicializando e todos os projetos locais continuam abrindo.

## 33.1 Critério final

O ARCZ não é “offline com cache”. Ele é um sistema local completo que, opcionalmente, pode
importar dados externos. A direção da dependência é sempre:

```text
provedor opcional → pacote local validado → ARCZ
```

Nunca:

```text
ARCZ core → API externa obrigatória
```

