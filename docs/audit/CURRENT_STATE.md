# Auditoria do Estado Atual do ARCZ (CURRENT_STATE.md)

> **ARQUIVO HISTÓRICO NÃO REVALIDADO NA V10.** Preserve como evidência da
> auditoria anterior, mas não use seus números/status como estado atual. A fonte
> autoritativa é `docs/audit/VALIDATION_REPORT.md`.


**Data:** 2026-07-30  
**Auditores:** Antigravity + LEGRAND  
**Status do Projeto:** Workspace Rust Funcional (300+ testes passing, 0 erros)

---

## 1. Arquitetura Identificada

1. **Raiz do Projeto**: `c:\Users\lucas\Desktop\ARCZ\`
2. **Workspace Cargo**: 7 crates ativas em `crates/`:
   - `arcz-app`: Binário desktop com CLI, servidor de preview HTTP/WebSocket (`server.rs`) e pipeline offscreen PNG.
   - `arcz-biblioteca`: Módulo de gerenciamento de assets CC0 e planta arquitetônica.
   - `arcz-geo`: Geodesia (WGS84, ECEF, ENU), cálculo solar NOAA e transformações.
   - `arcz-model`: Leitor de glTF/GLB com texturas PBR, hierarquia de nós e matrizes de modelo na GPU.
   - `arcz-osm`: Leitor Overpass/OSM, triangulação e gerador procedural de cidade (ruas, edifícios, recuos).
   - `arcz-tauri`: Casca Tauri 2 com integração de superfície nativa `wgpu` (ADR-0002).
   - `arcz-terrain`: Terreno DEM/GeoTIFF Terrarium, streaming de imagery e mazaico georreferenciado.
3. **Renderer de Referência Único**: `wgpu` nativo (GPU RTX 4090 / Vulkan). Sem duplicação de renderers locais.
4. **Scene Graph & Persistência**:
   - `cena.rs`: Gerencia o Scene Graph com hierarquia pai/filho, picking por raio, atalhos de gizmo (mover/girar/escalar), undo/redo (`Historico`) e lixeira lógica.
   - `projeto.rs`: Salvamento/abertura do formato `.arcz` com versionamento e integridade atômica.
   - SQLite (`project.sqlite`): Arquitetura relacional configurada conforme `ADR-0004` e `REALITY_SITE_COMPOSER_SPEC.md`.

---

## 2. Diagnóstico de Testes e Compilação

- **Comando**: `cargo test --workspace`
- **Resultado**: 300+ testes unitários e de integração aprovados com **0 falhas**.
- **Preview HTTP ao vivo**: Ativo e escutando na porta `http://127.0.0.1:8099`.
- **Render PNG Offscreen**: Validado gerando `preview/scene.png` em 1600x900 com iluminação PBR.

---

## 3. Primeiro Teste Vertical Validado (Seção 28)

O ciclo completo de usabilidade e persistência foi auditado e aprovado:
1. Criar projeto no ARCZ (`--novo` / `projeto_salvar`).
2. Instanciar modelos GLB / assets da biblioteca (`arcz-model`, `arcz-biblioteca`).
3. Selecionar objetos via picking por raio (`cena.rs::picar`).
4. Exibir seleção no Outliner e Inspector (`UI_ENGINE_CONTRACT.md`).
5. Aplicar transformações via Gizmo (mover, girar, escalar, duplicação Alt).
6. Desfazer / Refazer operações via stack de histōrico (`cena.rs::Historico`).
7. Salvar projeto em `.arcz` (`project.sqlite`).
8. Reabrir e verificar integridade relacional e espacial.
9. Mover objetos excluídos para a lixeira lógica (`--lixeira`).
10. Restaurar da lixeira.
