# Contrato UI ↔ Engine

Fronteira entre a camada React (`apps/arcz/`, MiniMax) e a camada Rust
(`crates/`, Claude Code). Definido pelo [ADR-0002](../decisions/ADR-0002-viewport-wgpu-nativo-no-tauri.md).

**Regra:** nenhum lado inventa comando novo sem registrar aqui primeiro.

## Princípio

A UI **nunca recebe pixels da cena**. O viewport é uma superfície wgpu nativa
desenhada pelo engine na mesma janela. A UI troca com o engine apenas **estado**:
lista de objetos, seleção, transformação, projeto.

Isso significa que a UI não faz polling de imagem, não usa `<img src="/render">`
e não decodifica JPEG. Ela pergunta "o que tem na cena" e manda "mova o objeto 3".

## Comandos Tauri (`invoke`)

Status: 🔴 não implementado · 🟡 em andamento · 🟢 pronto

### Cena

| Comando | Entrada | Saída | Status |
|---|---|---|---|
| `cena_listar` | — | `Objeto[]` | 🔴 |
| `cena_selecionar` | `{ id: number \| null }` | `Objeto \| null` | 🔴 |
| `cena_selecionado` | — | `Objeto \| null` | 🔴 |
| `cena_adicionar` | `{ arquivo: string, lat?, lon?, pai?: number }` | `Objeto` | 🔴 |
| `cena_remover` | `{ id: number }` | `boolean` | 🔴 |
| `cena_duplicar` | `{ id: number }` | `Objeto` | 🔴 |
| `cena_renomear` | `{ id: number, nome: string }` | `boolean` | 🔴 |
| `cena_visibilidade` | `{ id: number, visivel: boolean }` | `boolean` | 🔴 |
| `cena_isolar` | `{ id: number \| null }` | `boolean` | 🔴 |
| `cena_reparentar` | `{ id: number, pai: number \| null }` | `boolean` | 🔴 |

### Transformação

| Comando | Entrada | Saída | Status |
|---|---|---|---|
| `transform_definir` | `{ id, placement: Placement }` | `Objeto` | 🔴 |
| `transform_modo` | `{ modo: "mover"\|"girar"\|"escalar"\|"camera" }` | — | 🔴 |
| `transform_snap` | `{ metros: number, graus: number }` | — | 🔴 |

### Histórico

| Comando | Entrada | Saída | Status |
|---|---|---|---|
| `historico_desfazer` | — | `boolean` | 🔴 |
| `historico_refazer` | — | `boolean` | 🔴 |
| `historico_tamanho` | — | `{ feitos, refeitos }` | 🔴 |

### Projeto

| Comando | Entrada | Saída | Status |
|---|---|---|---|
| `projeto_salvar` | `{ caminho: string }` | `boolean` | 🔴 |
| `projeto_abrir` | `{ caminho: string }` | `Projeto` | 🔴 |
| `projeto_estado` | — | `Projeto` | 🔴 |
| `projeto_ausentes` | — | `ObjetoSalvo[]` | 🔴 |

### Ambiente

| Comando | Entrada | Saída | Status |
|---|---|---|---|
| `sol_definir` | `{ mes, dia, hora, fuso }` | `{ elevacao, azimute }` | 🔴 |
| `area_recarregar` | `{ lado_m, zoom_imagery }` | `EstadoCena` | 🔴 |

### Biblioteca

| Comando | Entrada | Saída | Status |
|---|---|---|---|
| `biblioteca_listar` | `{ raiz: string, limite?: number }` | `ItemBiblioteca[]` | 🔴 |

## Eventos (engine → UI)

Emitidos pelo Rust, ouvidos com `listen()`:

| Evento | Carga | Quando |
|---|---|---|
| `selecao_mudou` | `Objeto \| null` | Clique no viewport selecionou algo |
| `transform_mudou` | `{ id, placement }` | Gizmo terminou um arrasto |
| `cena_mudou` | — | Objeto adicionado/removido; a UI relista |
| `progresso` | `{ tarefa, atual, total }` | Download de tiles, import de modelo |

## Tipos

Espelham as structs do Rust. A fonte da verdade é o Rust; o TypeScript segue.

```ts
interface Placement {
  lat_deg: number; lon_deg: number;
  heading_deg: number; escala: number;
  offset_leste_m: number; offset_norte_m: number; offset_vertical_m: number;
  assentar_no_terreno: boolean;
}

interface Objeto {
  id: number; nome: string; pai: number | null;
  arquivo: string; visivel: boolean;
  placement: Placement;
  triangulos: number;
  // Caixa envolvente em metros, no quadro ENU local.
  min_enu: [number, number, number];
  max_enu: [number, number, number];
}

interface ItemBiblioteca {
  nome: string; caminho: string; categoria: string;
}
```

## Viewport

O engine desenha numa região da janela. A UI informa a geometria dessa região:

| Comando | Entrada | Descrição |
|---|---|---|
| `viewport_area` | `{ x, y, largura, altura }` | Em pixels físicos, canto superior esquerdo da janela |

A UI chama isso no mount e a cada resize/mudança de layout. **Não** desenhe nada
por cima dessa região no React — a superfície nativa fica acima do webview.

## O que existe hoje no Rust

Estas peças já estão implementadas e testadas; os comandos acima são a fachada
que falta para expô-las:

| Peça | Onde | Testes |
|---|---|---|
| Scene graph com hierarquia pai/filho | `cena.rs` | ✅ |
| Picking por raio contra caixa | `cena.rs::picar` | ✅ |
| Undo/redo por comandos | `cena.rs::Historico` | ✅ |
| Formato `.arcz` versionado, save atômico | `projeto.rs` | ✅ |
| Gizmo (mover/girar/escalar) | `gizmo.rs` | ✅ |
| Sol astronômico NOAA + céu | `arcz-geo::sol`, `sky.wgsl` | ✅ |
| Matriz de modelo na GPU | `arcz-model::matriz_modelo` | ✅ |
| Biblioteca de assets | `cena.rs::varrer_biblioteca` | ✅ |
| Terreno DEM + imagery georreferenciado | `arcz-terrain` | ✅ |
