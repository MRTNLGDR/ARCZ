# Shell e painéis globais V10

## Modos

- **Globo:** território, região, ambiente, geração e publicação.
- **Floorplanner:** globo simultâneo + Aedifex autoral.
- **Render:** prompts, referências, câmera, passes, preflight e jobs.
- **Walk:** panoramas locais e Earth→Street.

## Contrato CollapsiblePanelDock

Todo painel global:

- inicia recolhido;
- mantém rail/handle visível;
- abre por hover e focus;
- pin fixa;
- unpin volta ao comportamento temporário;
- resize por pointer e teclado;
- largura clampada entre 240 e 720 px;
- layout persistido em estado versionado;
- Escape recolhe não fixado;
- lazy mount sem duplicação de IDs;
- teardown cancela timers e listeners;
- loading, vazio, erro e bloqueio explícitos.

No Floorplanner, o overlay aplica comportamento equivalente sem editar silenciosamente o upstream. No host Tauri final, o componente será compartilhado.

## Acessibilidade

Hover nunca é o único caminho. Rail, foco, pin, teclado, aria labels e reduced motion são obrigatórios. Painel não deve cobrir permanentemente o viewport nem capturar input quando recolhido.

## Design

Tokens globais controlam superfície, blur, borda, sombra, spacing, tipografia e estados. O globo/3D permanece dominante; painéis são densos, colapsáveis e contextuais.
