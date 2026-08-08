# ADR-0002: Viewport wgpu nativo no Tauri (revisa o ADR-0001)

**Data:** 2026-07-30
**Status:** Aceito
**Decisor:** Lucas (LEGRAND), coordenacao Claude Code
**Revisa:** ADR-0001 — apenas a parte do transporte do viewport. O resto
(engine Rust + UI Tauri+React + design system FAVELION) **permanece valido**.

## Contexto

O ADR-0001 definiu que o Rust renderiza o quadro e entrega ao frontend como
imagem (`GET /render.jpg`), e que o frontend exibe num `<img>`/`<canvas>` e
devolve eventos de mouse.

Essa arquitetura **ja esta implementada** em `server.rs` e foi medida:

| Operacao | Tempo por quadro | Taxa |
|---|---|---|
| Girar a camera | 45 ms | ~22 fps |
| Mover um objeto | 39 ms | ~25 fps |

Os numeros acima ja incluem duas otimizacoes feitas hoje: matriz de modelo na
GPU (mover deixou de retransformar 936 mil vertices, caiu de 169 ms) e cache de
placement. **O teto restante nao e otimizavel** porque e estrutural: cada quadro
exige renderizar, comprimir em JPEG, transferir e decodificar no webview.

O Lucas relatou exatamente esse sintoma: *"esta lento pra tudo"*, *"nao funciona
igual o Google Earth"*.

## Decisao

O viewport deixa de trafegar imagem. Passa a ser uma **superficie wgpu nativa**
dentro da janela Tauri:

- O Tauri 2 expoe o handle da janela (`raw-window-handle`). O `wgpu` cria a
  `Surface` sobre esse handle, exatamente como ja faz com `winit` hoje em
  `viewport.rs`.
- O React continua desenhando **toda** a interface — Outliner, Inspector,
  biblioteca, barras. O viewport e uma regiao reservada do layout.
- Nenhum pixel de cena passa por HTTP, JSON ou base64.

### O que **nao** muda do ADR-0001

- Engine em Rust, 4 crates, wgpu + glTF + DEM/imagery.
- UI em Tauri + React reaproveitando `@favelion/design-system` e
  `@favelion/localization`.
- Estrutura FSD no `apps/arcz` do monorepo FAVELION.

### O que muda

| Aspecto | ADR-0001 | ADR-0002 |
|---|---|---|
| Pixels do viewport | JPEG por HTTP | Superficie wgpu nativa |
| Taxa alvo | ~22 fps | 60 fps |
| Eventos de mouse | WebSocket → Rust → novo JPEG | Direto na janela nativa |
| Estado da cena | HTTP REST | Comandos Tauri (`invoke`) |
| `server.rs` | Caminho principal | **Preservado** como preview headless e para render em lote |

## Consequencias

**Positivas**
- Elimina o teto de taxa de quadros. Manipular objeto passa a ter resposta
  imediata, que era a queixa central.
- Remove compressao e transferencia por quadro — menos CPU e menos memoria.
- O codigo de render nao muda: `gpu.rs`, `renderer.rs` e os shaders sao os
  mesmos. Troca-se apenas de onde vem a `Surface`.

**Negativas / riscos**
- Compor uma superficie nativa com o webview exige cuidado de z-order e de
  redimensionamento em cada plataforma. **Risco real**, a validar cedo.
- O viewport nao aparece em screenshot do DOM; testar exige captura da janela ou
  o caminho headless.
- `server.rs` passa a ser um segundo caminho de render. Mitigado porque os dois
  usam `Recursos`/`Renderer` — ja e assim hoje entre janela e offscreen, e ha
  teste garantindo que os shaders concordam com o layout dos uniforms.

**Se o risco de composicao se confirmar bloqueante**, o plano B e janela nativa
separada para o viewport (duas janelas Tauri sincronizadas), que sacrifica a
integracao visual mas preserva os 60 fps.

## Verificacao — 2026-07-30

O crate `arcz-tauri` foi criado para medir o risco antes de qualquer UI ser
construida em cima. Resultado da execucao:

```
=== ADR-0002 / teste de composicao ===
  superficie wgpu criada sobre a janela Tauri
  adaptador : NVIDIA GeForce RTX 4090 Laptop GPU
  backend   : Vulkan
  formato   : Bgra8UnormSrgb
  tamanho   : 1200x800
```

**Confirmado:** o `wgpu` aceita o handle da `WebviewWindow` do Tauri e cria a
`Surface` sem adaptacao. A janela sobreviveu 9 s sem erro de validacao. O maior
risco tecnico do ADR — "sera que da para criar a superficie?" — **esta eliminado**.

**Ainda nao verificado:** a ordem de empilhamento entre a superficie e o webview.
Isso exige olhar a janela, e nao e capturavel por automacao (a superficie nativa
nao aparece em screenshot do DOM). O binario desenha um fundo azul-escuro e serve
um HTML vermelho; qual dos dois aparece responde a pergunta:

```
cargo run -p arcz-tauri
```

- **Janela azul-escura** → superficie por cima. Segue o plano principal: a UI
  reserva a regiao do viewport e chama `viewport_area`.
- **Texto vermelho visivel** → superficie por baixo. Aciona o plano B, e o
  layout do `apps/arcz` muda (o MiniMax precisa saber antes de construir).

Enquanto essa confirmacao nao vier, o `server.rs` continua sendo o caminho de
preview em uso.

## Divisao de trabalho

| Camada | Responsavel | Escopo |
|---|---|---|
| Rust / wgpu | **Claude Code** | Viewport nativo, picking, gizmo, render, engine, comandos Tauri |
| TypeScript / React | **MiniMax** | Outliner, Inspector, biblioteca, layout, design system |

Fronteira: `crates/` e do Claude; `apps/arcz/` e do MiniMax. O contrato entre as
duas camadas esta em `docs/integrations/UI_ENGINE_CONTRACT.md` e **so muda por
acordo registrado la**.
