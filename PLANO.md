# ARCZ — Plano de Arquitetura

> **ATUALIZAÇÃO V10:** este documento é histórico. A arquitetura autoritativa atual, incluindo a integração integral Aedifex, está em `docs/integration/AEDIFEX_CAPABILITY_LEDGER_V10.md` e `docs/integration/MASS_CONVERSION_EXECUTION_PLAN.md`. Em caso de conflito, esses documentos e `IMPLEMENTATION_STATUS.json` prevalecem.

**Objetivo:** app que puxa dados de satélite/GIS/topografia de uma região real, recebe um modelo 3D
em escala real do usuário e o encaixa georreferenciado, gera proceduralmente o entorno urbano
(casas, prédios, telhados, fachadas) texturizado a partir de ortofotos e fotos de drone/rua, permite
mobiliar/decorar/vegetar com biblioteca de assets, posicionar câmeras com parâmetros ópticos reais,
renderizar stills 8K e animações start→end, e depois aprimorar via IA por prompt preservando a
geometria.

**Veredito curto:** é factível, mas **não é "um app"** — são 7 subsistemas independentes. O único
jeito de isso existir é fatia vertical: cada fatia tem que rodar ponta a ponta antes da próxima
começar. O plano abaixo é ordenado por risco técnico, não por facilidade.

---

## 1. Decisão de stack

**Rust é a linguagem-hospedeira. C++ entra por FFI só onde não existe equivalente maduro em Rust.**

Motivo: as libs que realmente carregam o peso (GDAL, OpenUSD, Cycles, COLMAP, CGAL) são C++ e não
serão reescritas. Mas o *app* — pipeline paralelo, streaming de tiles, job queue, UI, grafo de cena,
gerador procedural — é exatamente onde Rust ganha (sem UB em código concorrente pesado, `rayon`,
tooling, e você já tem base Rust nos outros projetos).

Escrever tudo em C++ puro só evitaria as camadas de FFI, ao custo de produtividade e de todo um
sistema de build. Não compensa.

| Camada | Linguagem | Por quê |
|---|---|---|
| App core, procgen, cena, jobs, streaming | **Rust** | segurança em paralelismo pesado, `rayon`, cargo |
| Viewport realtime | **Rust** (wgpu / Bevy) | mesmo processo, sem serialização |
| GIS I/O, geodésia | C++ via FFI (`gdal`, `proj` crates já existem) | GDAL não tem substituto |
| Cena / interchange | OpenUSD (C++, FFI) | padrão de fato, exporta pra Blender/UE/Omniverse |
| Render final | Cycles standalone (C++, **Apache-2.0**) | path tracer com câmera física + OptiX |
| Fotogrametria | COLMAP + OpenMVS (C++, processos externos) | CLI, não precisa linkar |
| IA / difusão | **ComfyUI headless via HTTP** | reimplementar difusão em Rust é desperdício puro |

> ⚠️ **Cycles é Apache-2.0 (independente do Blender, que é GPL).** Embutir Cycles standalone
> **não** contamina o ARCZ. Linkar `bpy`/Blender **contamina**. Não misture.

### UI
Duas opções reais:
- **egui + wgpu** (tudo em Rust, um binário, viewport nativo integrado) — recomendado.
- **Tauri + React** com viewport wgpu embutido — só se você quiser reaproveitar UI do
  ZENITE/AVANGARD-VISUAL. Custa uma ponte extra.

---

## 2. O problema que mata 90% dos projetos desse tipo: precisão geodésica

Coordenada UTM tem ~7 dígitos significativos (ex: `E 333.421,58  N 7.394.882,12`). `f32` da GPU tem
~7 dígitos **no total**. Resultado: se você jogar coordenada de mundo real direto no shader, a
geometria **treme e se rasga** (vertex jitter) — e você só descobre depois de já ter construído meio
app em cima.

**Solução obrigatória, desde o commit 1:**
1. Todo dado geoespacial vive em **`f64`**, em **ECEF** (Earth-Centered Earth-Fixed) ou UTM/`f64`.
2. A cena de renderização usa **origem local flutuante** (ENU — East/North/Up ancorado num ponto de
   referência do projeto). Rebase da origem quando a câmera se afasta > ~10 km.
3. Na GPU, `relative-to-eye`: subtrai a posição da câmera em `f64` na CPU, manda `f32`.
4. Cada tile de terreno carrega sua própria origem local.

**Atalho legítimo:** [Cesium Native](https://github.com/CesiumGS/cesium-native) (C++, Apache-2.0) já
resolve isso, mais 3D Tiles + streaming + LOD. É a decisão de maior alavancagem do projeto inteiro.
A alternativa é [osgEarth](https://github.com/gwaldron/osgearth) (C++), que entrega terreno global +
imagery + DEM + feature-to-geometry quase pronto, mas te amarra ao OpenSceneGraph (arquitetura
antiga, difícil de casar com wgpu).

**Recomendação:** Cesium Native para geodésia/tiles + Bevy/wgpu para renderizar. Não force osgEarth.

---

## 3. Módulos (crates do workspace)

```
arcz/
├── crates/
│   ├── arcz-geo        # bbox lat/lon, CRS, reprojeção (PROJ), download de fontes
│   ├── arcz-terrain    # DEM → heightfield → mesh, quadtree LOD, drape de ortofoto
│   ├── arcz-align      # import glTF/FBX/IFC do usuário, georreferenciar, escala real, snap no solo
│   ├── arcz-procgen    # footprint → massa → telhado (straight skeleton) → fachada (split grammar)
│   ├── arcz-material   # grafo MaterialX, paleta extraída de foto, texture bombing
│   ├── arcz-photogram  # drone → COLMAP/OpenMVS/3DGS → mesh/splat georreferenciado
│   ├── arcz-scene      # grafo USD, instancing, biblioteca de assets, scatter de vegetação
│   ├── arcz-camera     # câmera física (sensor/focal/f-stop/ISO/shutter/shift), spline start→end
│   ├── arcz-render     # bridge Cycles: export cena, AOVs, ACES, tiles 8K, denoise
│   ├── arcz-ai         # cliente ComfyUI: img2img + ControlNet a partir dos AOVs, upscale
│   └── arcz-app        # UI egui, viewport wgpu, job queue, projeto/save
```

Cada crate é testável isolado. `arcz-app` só orquestra.

---

## 4. Dados — fontes reais, gratuitas, e as pegadinhas de licença

### Elevação (topografia)
| Fonte | Resolução | Cobertura | Licença |
|---|---|---|---|
| **Copernicus DEM GLO-30** | 30 m | global | livre, atribuição |
| SRTM v3 | 30 m | ±60° lat | domínio público |
| ALOS AW3D30 | 30 m | global | livre |
| **USGS 3DEP** (LiDAR) | 1 m / point cloud | EUA | domínio público |
| **TOPODATA / INPE** | 30 m | Brasil | livre |
| **LiDAR municipal** (GeoSampa-SP, Rio, Curitiba) | **0,5–1 m** | cidade | livre |

Acesso prático: **[OpenTopography API](https://opentopography.org/)** entrega GLO-30/SRTM/3DEP por
bbox numa chamada só. É por aí que a Fatia 0 deve começar.

### Imagery (ortofoto/satélite)
| Fonte | Resolução | Nota |
|---|---|---|
| **Sentinel-2** | 10 m | livre, Copernicus Data Space, revisita 5 dias |
| Landsat 8/9 | 30 m | livre |
| **CBERS-4A (INPE)** | **2 m** | **gratuito, Brasil** — muito subutilizado |
| **Ortofotos municipais** (GeoSampa) | **10 cm** | livre, é o que salva a textura de telhado |
| Mapbox / Esri / Bing | 0,3–1 m | pago / ToS restritivo |

### Footprints de edifício + altura
| Fonte | Nota |
|---|---|
| **Overture Maps Foundation** | global, com altura, **licença permissiva** — ✅ melhor escolha |
| OpenStreetMap | rico (`building:levels`, `roof:shape`, `building:material`) mas **ODbL — share-alike no derivado** ⚠️ |
| Microsoft Global ML Building Footprints | global, ODbL |
| Google Open Buildings | LatAm/África/Ásia, CC-BY |
| CityGML LoD2 (3D BAG - NL, DE) | prédios 3D prontos, livre — só Europa |

> ⚠️ **ODbL é a maior armadilha comercial do projeto.** Se você gerar a cidade a partir de OSM e
> vender o render, há argumento de que o *database derivado* deve ser aberto. Overture existe
> exatamente pra fugir disso. **Padrão do ARCZ deve ser Overture; OSM só como opt-in com aviso.**

> ⚠️ **Google Photorealistic 3D Tiles**: qualidade absurda, mas o ToS proíbe armazenar/derivar
> geometria. Serve pra *visualizar*, não pra ser a base do seu produto.

### Fotos do lugar (referência de fachada)
- **[Mapillary](https://www.mapillary.com/)** — CC-BY-SA, API livre, imagens de rua. ✅ opção limpa.
- Google Street View — ToS proíbe armazenar/derivar textura. ❌
- **Drone do próprio usuário** — o caminho sem risco nenhum, e o de melhor qualidade.

### Céu / iluminação
- HDRIs CC0 do [Poly Haven](https://polyhaven.com/hdris), **ou** céu procedural Hosek-Wilkie com
  posição solar calculada por lat/lon/data/hora → habilita **estudo de insolação** (item comercial
  forte no imobiliário).

---

## 5. Geração procedural de edifícios

Pipeline por footprint:

```
polígono 2D + altura  →  simplificação/retificação (ângulos retos)
   →  extrusão de massa (LOD1)
   →  STRAIGHT SKELETON → telhado (duas águas, quatro águas, mansarda) (LOD2)
   →  SPLIT GRAMMAR na fachada: andares → módulos → esquadrias (LOD3)
   →  atribuição de material por paleta extraída de ortofoto (telhado) + foto de rua (fachada)
```

**Straight skeleton** é o algoritmo certo pra telhado a partir de footprint arbitrário.
⚠️ A implementação canônica é do **CGAL, que é GPL** (dual-license comercial paga). Para uso
comercial: implementar Aichholzer–Aurenhammer (é ~600 linhas, robustez é o difícil) ou usar um crate
Rust MIT. **Decida isso cedo — trocar depois é caro.**

**Split grammar** é a técnica do CityEngine (proprietário). Referências open pra estudar:
[osm2world](https://osm2world.org/) (Java), [Blosm](https://github.com/vvoovv/blosm) (Blender, GPL —
estudar, não copiar), `random3dcity`.

**Onde a IA entra de verdade aqui (e é o pulo do gato):** rodar segmentação de fachada
(SegFormer/DeepLab treinado em CMP Facade / ECP Dataset) sobre a foto de rua/drone → extrai nº de
andares, grade de janelas, cor dominante, material → **isso vira parâmetro da grammar**. Assim o
prédio procedural *se parece* com o prédio real, sem fotogrametria. Muito mais barato e robusto que
reconstruir mesh, e é literalmente o que você descreveu por "texturiza procedural com base em
referências do mapa e fotos".

---

## 6. Texturização

- **MaterialX** (Apache-2.0, C++) como formato de grafo de material — funciona em Cycles, USD, UE,
  Blender. Não invente formato próprio.
- Biblioteca base CC0: [ambientCG](https://ambientcg.com/), Poly Haven.
- **Extração de paleta**: k-means na ortofoto (recorte do footprint) → cor de telhado;
  k-means na foto de rua → cor de fachada e de esquadria.
- **Anti-tiling**: histogram-preserving blending (Heitz & Neyret) ou texture bombing. Sem isso, 200
  casas com a mesma textura viram um padrão xadrez visível de longe — erro clássico.
- UV: cada módulo da grammar gera sua própria UV no momento da criação. **Nunca** faça unwrap global
  de uma cidade.

---

## 7. Fotogrametria de drone

Pipeline como processos externos (não linkar):
```
fotos + EXIF GPS  →  COLMAP (SfM: poses de câmera)
   →  OpenMVS (denso + mesh + textura)      # caminho "mesh"
   →  ou 3D Gaussian Splatting / OpenSplat  # caminho "splat"
   →  georreferenciar por GPS/GCP → alinhar ao terreno (ICP / Umeyama)
```

**Sugestão que muda o produto:** use **mesh só para o prédio-alvo** e **Gaussian Splatting para o
entorno**. O entorno fica fotorreal sem você modelar nada. O problema é que splat não renderiza em
path tracer — a solução é composite: renderiza o splat como *plate* de fundo, path-traceia o prédio,
e combina usando o **depth AOV** para oclusão correta. Isso é exatamente o que o
**ZENITE-VISTAS-3D** precisa.

---

## 8. Câmeras reais

Modelo de câmera física completo, não "FOV":

| Parâmetro | Por quê |
|---|---|
| Sensor (mm, W×H) | full-frame 36×24, APS-C, MFT |
| Distância focal (mm) | 14/24/35/50/85 |
| f-stop | profundidade de campo real |
| Shutter (s) | motion blur |
| ISO | com grain opcional |
| **Lens shift / tilt-shift** | **essencial em arquitetura** — mantém verticais paralelas sem distorcer |
| Distorção + vignette + CA | realismo; pode ser desligado |

**Animação start→end:** keyframes com spline Catmull-Rom para posição + SLERP para rotação, com
*ease* configurável e opção de "câmera em trilho" (dolly) ou órbita. Timeline com preview em tempo
real no viewport wgpu antes de mandar pro Cycles.

**Color pipeline:** OpenColorIO com ACES ou AgX. Sem isso, render 8K sai com céu estourado e sombras
plásticas — e nenhum pós-processamento salva.

---

## 9. Render 8K

- **Cycles standalone** com OptiX (sua RTX no m18 R2 dá conta).
- 8K = 7680×4320 = 33 Mpx. O gargalo é **VRAM**, não tempo. Estratégias:
  1. Render em **tiles** com scene resident + composite.
  2. Render 4K nativo + **upscale 2× por IA** (mais rápido, e o passo de IA já está no pipeline).
  3. Out-of-core / instancing agressivo na vegetação (`PointInstancer` do USD).
- **Denoise:** OptiX Denoiser ou Intel OIDN, com albedo+normal AOVs (muito melhor que denoise cego).
- **AOVs obrigatórios** (são o insumo da etapa de IA): `depth`, `normal`, `albedo`, `position`,
  `cryptomatte` (object/material/asset).

Alternativa a Cycles: **LuxCoreRender** (Apache-2.0, C++) — mais fiel fisicamente, comunidade menor.
Cycles ganha por OptiX + ecossistema de materiais.

---

## 10. Aprimoramento por IA (prompt)

O erro comum é jogar o render num img2img e perder a arquitetura. A forma correta:

```
render 8K (ou 4K) + AOVs
  → ControlNet Depth  (do depth AOV, não estimado!)  ─┐
  → ControlNet Normal (do normal AOV)                 ├→ img2img, denoise 0.20–0.35
  → ControlNet Canny/Lineart (arestas duras)         ─┘
  → máscaras por cryptomatte → prompt regional
       ("céu: entardecer nublado", "vegetação: mata atlântica", "asfalto: molhado")
  → upscale em tiles (Ultimate SD Upscale / SUPIR) até 8K
```

Usar AOVs reais em vez de mapas estimados é a diferença entre "IA respeitou o prédio" e "IA inventou
uma janela a mais". Denoise acima de ~0.4 começa a mudar geometria.

**Integração:** ComfyUI headless local, chamado por HTTP a partir do `arcz-ai` com um *workflow JSON*
versionado por preset. Você já tem experiência de ComfyUI/Civitai no ZENITE — reaproveita direto.
Alternativa 100% nativa se quiser binário único: `stable-diffusion.cpp` (GGML, C++) — mas perde
ControlNet avançado e a agilidade de trocar modelo.

---

## 11. Sobre forkear o "Adifex"

O projeto é **[TangSY/aedifex](https://github.com/TangSY/aedifex)** — editor arquitetônico 3D open
source em **TypeScript / Three.js / WebGPU / React**, com paredes, portas, janelas, catálogo de
móveis e assistente de IA em linguagem natural.

**Não serve como base do ARCZ**, por dois motivos:
1. Stack incompatível com o que você pediu (é web/TS, você quer C++/Rust).
2. Escopo é **planta baixa indoor** — não tem GIS, nem terreno, nem procedural urbano, nem path
   tracing. Ou seja, ele cobre a parte *menor* do seu problema.

**Onde ele vale:** como referência de UX para o módulo de interiores (mobiliar/decorar) e para o
catálogo de móveis. Se você quiser um editor de interiores web rápido paralelo ao ARCZ, aí sim vale
forkear ele isolado.

---

## 12. Roadmap — fatias verticais

Cada fatia **roda ponta a ponta** antes da próxima começar. Nada de "faz o módulo X inteiro".

| # | Fatia | Entrega verificável | Risco que elimina |
|---|---|---|---|
| **0** | **Terreno georreferenciado** | digita bbox lat/lon → baixa Copernicus DEM + Sentinel-2 → malha com ortofoto drapeada → câmera orbita sem jitter | **precisão geodésica** (o maior) |
| 1 | Import do modelo do usuário | carrega glTF/FBX/IFC, posiciona por lat/lon + rotação, escala real verificada com régua, snap no terreno | pipeline de asset + unidades |
| 2 | Cidade procedural | footprints Overture → LOD1 (massa) → LOD2 (telhado por straight skeleton) → quarteirão inteiro em volta do modelo | procgen + licença |
| 3 | Materiais + vegetação | MaterialX + paleta da ortofoto + scatter de árvores instanciadas + fachada por split grammar | "parece real?" |
| 4 | Câmera + render | câmera física, timeline start→end, export pro Cycles, still 8K + sequência com denoise e ACES | VRAM/tempo de render |
| 5 | IA | ComfyUI + ControlNet dos AOVs + upscale, preset por prompt | preservação de geometria |
| 6 | Fotogrametria/splat | COLMAP→OpenMVS/3DGS, alinhamento, composite splat+pathtrace | qualidade do entorno |

**Estimativa honesta:** cada fatia é 2–6 semanas de trabalho focado. Fatia 0 é a mais curta e a mais
importante. O conjunto, feito por uma pessoa sem cortar escopo, é **12–24 meses**. Com corte de
escopo (ex: só LOD2 sem split grammar, sem fotogrametria) cai pra ~6 meses.

---

## 13. Riscos que eu não vou esconder

1. **ODbL do OpenStreetMap** — pode contaminar o produto comercial. Mitigação: Overture por padrão.
2. **CGAL é GPL** — o straight skeleton é a peça mais tentadora e a mais perigosa. Decida no início.
3. **Google 3D Tiles / Street View** — proibido derivar. Não construa nada em cima.
4. **Precisão geodésica** — se errar, o retrabalho é o app inteiro. Por isso é a Fatia 0.
5. **VRAM em 8K** — provável necessidade de render em tiles ou 4K+upscale.
6. **Fachada procedural convincente** é o item mais difícil de "parecer real" — é onde a maioria dos
   geradores de cidade fracassa visualmente.
7. **Escopo** — o maior risco não é técnico. É querer as 7 fatias ao mesmo tempo.

---

## 14. Sugestões que você não pediu (e que eu acho que mudam o jogo)

1. **Unificar com o ZENITE-VISTAS-3D.** ARCZ é o *engine*; ZENITE é o *produto* (vistas de
   apartamento no terreno real). Você não tem dois projetos — tem um engine e uma aplicação dele.
   Isso corta trabalho duplicado imediatamente.
2. **Modo "vista de janela"**: dado andar + apartamento no modelo importado, a câmera se posiciona
   sozinha na janela com lente de arquitetura. Isso *é* o produto imobiliário — automatizar essa
   câmera vale mais que qualquer feature de modelagem.
3. **Sun study / insolação**: data+hora+lat/lon → sombra real. Vende sozinho para incorporadora, e o
   custo marginal é quase zero (você já vai ter a posição solar pro céu procedural).
4. **Splat pro entorno, mesh pro alvo** (seção 7) — atalho brutal pra fotorrealismo.
5. **Exportar USD sempre.** Mesmo que você renderize em Cycles, gravar a cena em USD te dá saída
   grátis pra Blender, Unreal e Omniverse — e é seu seguro caso o `arcz-render` demore.
6. **Cache local agressivo de tiles** (SQLite + blob store) — você já tem esse padrão implementado
   no ZENITE (`storage/blob_store.rs`, SHA-256, escrita atômica). Reaproveita.
7. **Não construa UI antes da Fatia 2.** Terminal + arquivo de config JSON é suficiente pra validar
   as fatias 0 e 1, e economiza semanas.

---

## 15. Próxima decisão que depende de você

1. **Confirmar Rust host + FFI C++** (minha recomendação) ou C++ puro.
2. **Uso comercial?** Define se pode usar CGAL/OSM ou se precisa de alternativa permissiva.
3. **Autorizar a Fatia 0** — é a única que precisa começar agora, e ela é curta.

Assim que você confirmar, eu abro o workspace Cargo e implemento a Fatia 0 inteira
(bbox → DEM → Sentinel-2 → mesh → viewport sem jitter), com testes.
