# TEST_BASELINE.md — Test Baseline Report

> **ARQUIVO HISTÓRICO NÃO REVALIDADO NA V10.** Preserve como evidência da
> auditoria anterior, mas não use seus números/status como estado atual. A fonte
> autoritativa é `docs/audit/VALIDATION_REPORT.md`.


**Execution Date**: 2026-07-30
**Environment**: Windows 11 x64, Rust 1.85+, wgpu 27 (Vulkan / DX12)

---

## 1. Summary

| Test Suite / Crate | Passed | Failed | Ignored | Duration | Status |
|---|---|---|---|---|---|
| `arcz-app` | 189 | 0 | 0 | 0.04s | ✅ PASS |
| `arcz-biblioteca` | 0 | 0 | 0 | 0.00s | ✅ PASS |
| `arcz-earth` | 25 | 0 | 0 | 0.00s | ✅ PASS |
| `arcz-geo` | 14 | 0 | 0 | 0.00s | ✅ PASS |
| `arcz-model` | 13 | 0 | 0 | 0.00s | ✅ PASS |
| `arcz-osm` | 76 | 0 | 0 | 0.09s | ✅ PASS |
| `arcz-tauri` | 3 | 0 | 0 | 0.00s | ✅ PASS |
| `arcz-terrain` | 39 | 0 | 0 | 0.02s | ✅ PASS |
| **Total Workspace** | **359** | **0** | **0** | **0.15s** | ✅ **100% PASS** |

---

## 2. Desktop UI Build Baseline (`apps/arcz`)

- **Toolchain**: Node.js v22+, Vite 5.4.21, React 19, TypeScript 5.5+
- **Typecheck (`tsc -p tsconfig.json --noEmit`)**: 0 errors
- **Production Build (`vite build`)**:
  - `dist/index.html`: 0.39 kB
  - `dist/assets/index-RvX5i29W.css`: 18.27 kB
  - `dist/assets/index-IvgF4Ixl.js`: 217.15 kB (50 modules transformed in 1.69s)
  - **Status**: ✅ PASS

---

## 3. Discrepancy Note on Suite Count

The spec context mentioned an expected test suites count of 16. In the actual codebase, cargo executes 8 workspace crate test suites + 1 doc-test suite + 1 React UI build suite = 10 active test pipelines. All 359 unit and integration test cases across these pipelines run with 100% pass rate.
