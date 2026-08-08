# ADR-0001: Stack do ARCZ — engine Rust (wgpu) + UI Tauri+React

**Data:** 2026-07-30
**Status:** Aceito
**Decisor:** Lucas (LEGRAND) + MiniMax
**Contexto:** Prompt mestre do ARCZ pede um editor 3D completo em Rust. Auditoria
(2026-07-30) mostrou que o `cena.rs` já tem Editor, picking e gizmo, mas tudo
desconectado do renderer. Há também um `server.rs` com preview HTTP já funcional.

## Decisão

O ARCZ vira **dois processos** que se comunicam por HTTP/WebSocket:

1. **Engine (Rust, `C:\Users\lucas\Desktop\ARCZ\`)** — já existe, 4 crates.
   Mantém wgpu + winit + glTF + DEM/imagery. Evolui pra:
   - API HTTP completa no `server.rs` (cenas, objetos, transformações, save/load)
   - WebSocket para push de eventos do viewport (mouse, key) → Rust processa
     picking/gizmo e devolve estado novo + URL do próximo frame JPEG.
   - **Viewport nativo winit** continua (janela desktop) E o servidor HTTP
     atende o cliente Tauri+React que renderiza a UI.

2. **UI (Tauri+React, `C:\Users\lucas\Desktop\FAVELION\monorepo\apps\arcz\`)** —
   novo app dentro do monorepo FAVELION. Reaproveita:
   - `@favelion/design-system` (componentes visuais, tokens)
   - `@favelion/localization` (i18n 23 idiomas já configurado)
   - `@favelion/auth` (se quiser login compartilhado)
   - `@favelion/permissions` (RBAC)
   - Estrutura FSD (`src/entities/`, `src/features/`, `src/widgets/`, `src/shared/`)
     idêntica ao `apps/body` do FAVELION.

3. **Comunicação**:
   - HTTP REST: `GET /estado`, `POST /objetos`, `PATCH /objetos/{id}`,
     `POST /projeto/salvar`, `POST /projeto/carregar`, `GET /render.jpg?w=...&h=...`
   - WebSocket: `/ws` envia eventos do mouse/key do frontend pro Rust;
     Rust devolve mudanças de estado (objeto selecionado, transform aplicado).
   - O Rust processa picking/gizmo/render e devolve o JPEG do frame.
     O frontend só exibe a imagem num `<canvas>` ou `<img>` e envia inputs.

## Por que não outras opções

| Alternativa | Por que não |
|---|---|
| **egui (puro Rust)** | Não reaproveita o design system do FAVELION. Reescreveria a UI do zero. |
| **Next.js puro (sem Tauri)** | O viewport 3D precisa rodar dentro do mesmo processo que a cena (latência). WebGL via canvas no browser funciona, mas fica mais lento que wgpu nativo. Lucas pediu Tauri+React. |
| **Tauri+Next.js (hibrido)** | Tauri embute um webview que renderiza HTML; rodar Next.js DENTRO do Tauri é possível mas adiciona servidor de dev dentro do .exe. Mais simples: Tauri+React puro (SPA) que fala com Rust via comandos Tauri OU HTTP. |
| **Slint** | Mais simples que egui mas comunidade menor; sem reaproveitar FAVELION. |
| **Iced** | Mesmo problema do egui sem reaproveitar FAVELION. |

## Consequências

- **Positivo:** UI rica reaproveitando FAVELION (que já tem 14 packages prontos).
  Performance nativa do wgpu pro viewport. Tauri empacota tudo num `.exe`
  desktop instalável, sem precisar de browser separado.
- **Positivo:** O Rust engine continua sendo o "single source of truth" da cena.
  A UI é burra: exibe o JPEG, envia inputs. Lógica fica toda no Rust onde os
  testes (`cargo test`) já rodam.
- **Positivo:** A Fase 2 (Núcleo do editor) pode ser feita em Rust puro
  (E.1, E.2, E.3, E.4) e testada sem Tauri. Tauri só entra em E.5.
- **Negativo:** Dois processos pra desenvolver e deployar. Mas o Rust engine
  sempre funciona standalone (a CLI atual `arcz` continua rodando sem UI).
- **Negativo:** O Tauri CLI precisa ser baixado (regra do Lucas: primeiro pro
  banco de open sources). O banco será expandido em E.5.
- **Negativo:** Latência do round-trip HTTP pro JPEG. Mitigação: 60fps com JPEG
  q=82 já está bom (o server atual do ARCZ faz isso).

## Pendências

- [ ] Em E.5: baixar Tauri (crate + CLI) pro banco de open sources
  (`99_PENDENTES_LICENCA_LOGIN_E_LAUNCHER/` ou nova pasta `08_TAURI`).
- [ ] Em E.5: criar `apps/arcz` no monorepo FAVELION com Tauri+React.
- [ ] Em E.5: decidir se o Rust engine roda embarcado no Tauri (via
  `tauri::async_runtime::spawn`) ou como subprocesso. Começar como subprocesso
  (mais simples) e embedar depois se precisar.
- [ ] Confirmar com Lucas: o apps/arcz fica no monorepo FAVELION (reaproveita
  infra) ou no ARCZ mesmo (cria monorepo próprio com Next.js)? Default:
  FAVELION, é onde o design system mora.

## Notas técnicas

- Tauri 2.0 (LTS) + React 19 + Vite (template `create-tauri-app`).
- Comunicação Rust↔Tauri: comandos Tauri (`#[tauri::command]`) chamam o engine.
- O engine expõe também HTTP/WebSocket em `127.0.0.1:8099` (a porta que o
  preview atual já usa). O Tauri aponta pra esse endpoint. Em produção,
  o subprocesso Rust sobe automaticamente quando o Tauri abre.
