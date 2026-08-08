# REPOSITORY_BASELINE.md — ARCZ Earth Repository Baseline

> **ARQUIVO HISTÓRICO NÃO REVALIDADO NA V10.** Preserve como evidência da
> auditoria anterior, mas não use seus números/status como estado atual. A fonte
> autoritativa é `docs/audit/VALIDATION_REPORT.md`.


**Audit Date**: 2026-07-30
**Auditor**: ARCZ Earth Lead Engineer (Antigravity)
**Repository Path**: `C:\Users\lucas\Desktop\ARCZ`

---

## 1. Git Repository State

| Property | Value |
|---|---|
| **Branch** | `main` |
| **Commit Hash** | `2c6f821` |
| **Commit Message** | `Liga o snapping e o project.sqlite ao nucleo (T130/T140)` |
| **Working Tree** | Clean (no untracked or modified files) |
| **Remote** | `* main` (local authoritative git repository) |

---

## 2. Workspace Crates Inventory

The Rust workspace consists of 8 crates configured in `Cargo.toml`:

```
crates/
├── arcz-app          # Core application host, viewport, server, renderer & scene graph
├── arcz-biblioteca   # CC0 Archviz & interior asset library manifest & loader (52 PolyHaven + 14 procedural)
├── arcz-earth        # ARCZ Earth master spec baseline, scene schema & regional package validator
├── arcz-geo          # Geodesy math, WGS84, ECEF, ENU, slippy tiles & NOAA astronomical sun positioning
├── arcz-model        # glTF/GLB 3D model parser, hierarchy builder & placement calculation
├── arcz-osm          # OpenStreetMap PBF parser, Overpass query engine, road network & procedural building generator
├── arcz-tauri        # Tauri 2 desktop app wrapper & wgpu native surface bridge
└── arcz-terrain      # SRTM DEM terrarium decoder, imagery caching (SHA-256) & GPU mesh generator
```

---

## 3. UI App Inventory

Two UI implementations exist in the repository ecosystem:
1. **Reference Single-Page UI**: `crates/arcz-app/src/ui/` (`index.html`, `app.js`, `styles.css`, `data.js`) and `crates/arcz-app/src/preview.html` — Zero-dependency standalone HTML/CSS/JS interface.
2. **Production Desktop UI (React 19)**: `FAVELION/monorepo/apps/arcz/` — Vite 5 + React 19 + Tailwind CSS + Zustand store shell designed for Tauri 2 embedding.

---

## 4. Key Architectural Contracts & ADRs

- **ADR-0002**: Native wgpu Viewport Surface within Tauri window. The Rust wgpu surface is rendered directly onto the viewport region.
- **UI_ENGINE_CONTRACT.md**: Explicit separation between Rust core engine (`crates/`) and React UI shell (`apps/arcz/`).
- **Cost Policy**: Strictly Zero-Cost Default — No paid cloud calls, mandatory offline-first fallback.
