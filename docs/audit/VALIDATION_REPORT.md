# ARCZ Earth + Aedifex Global V10 — relatório de validação

Gerado em: `2026-08-08T22:35:17.127168Z`

**Resultado geral:** `BLOCKED`

> `BLOCKED` não significa aprovado. Significa que a verificação não pôde ser executada neste ambiente.

| Verificação | Estado | Duração |
|---|---:|---:|
| `python_compileall` | **PASSED** | 137 ms |
| `python_pytest` | **PASSED** | 10456 ms |
| `job_cancel_race_stress` | **PASSED** | 8237 ms |
| `javascript_tests` | **PASSED** | 151 ms |
| `typescript_overlay_syntax` | **PASSED** | 571 ms |
| `javascript_syntax` | **PASSED** | 1538 ms |
| `aedifex_conversion_matrix_generation` | **PASSED** | 566 ms |
| `json_parse` | **PASSED** | 4 ms |
| `json_schema_self_check` | **PASSED** | 880 ms |
| `resource_schema_validation` | **PASSED** | 3 ms |
| `cargo_workspace_structure` | **PASSED** | 3 ms |
| `rust_delimiter_sanity_not_compile` | **PASSED** | 35 ms |
| `rustfmt_check` | **BLOCKED** | 0 ms |
| `cargo_check_workspace` | **BLOCKED** | 0 ms |
| `cargo_test_workspace` | **BLOCKED** | 0 ms |
| `no_mock_no_stub_policy` | **PASSED** | 78 ms |
| `core_contains_no_hardcoded_remote_url` | **PASSED** | 12 ms |
| `browser_local_first_boot_contract` | **PASSED** | 0 ms |
| `aedifex_v10_source_contracts` | **PASSED** | 1 ms |
| `documentation_v10_contract` | **PASSED** | 0 ms |
| `release_entrypoint_contract` | **PASSED** | 0 ms |
| `linux_launcher_syntax` | **PASSED** | 2 ms |
| `windows_launcher_runtime` | **BLOCKED** | 0 ms |
| `docker_compose_runtime` | **BLOCKED** | 0 ms |
| `cesium_local_vendor` | **BLOCKED** | 0 ms |
| `aedifex_vendor_and_build` | **BLOCKED** | 39 ms |
| `photoreal_worker_contract` | **PASSED** | 0 ms |
| `blender_photoreal_runtime` | **BLOCKED** | 0 ms |
| `local_ai_render_models` | **BLOCKED** | 3 ms |
| `runtime_artifact_cleanup` | **PASSED** | 0 ms |
| `release_tree_has_no_runtime_artifacts` | **PASSED** | 7 ms |

## Detalhes

### python_compileall — PASSED

```json
{}
```

### python_pytest — PASSED

```json
{
  "command": [
    "/opt/pyvenv/bin/python",
    "-m",
    "pytest",
    "-q"
  ],
  "returncode": 0,
  "stdout": "......................................................s................. [ 59%]\n..............................ss..................                       [100%]\n119 passed, 3 skipped in 8.19s\n",
  "stderr": ""
}
```

### job_cancel_race_stress — PASSED

```json
{
  "command": [
    "/opt/pyvenv/bin/python",
    "tools/job_cancel_stress.py",
    "--iterations",
    "100"
  ],
  "returncode": 0,
  "stdout": "{\"schema_version\": 1, \"iterations\": 100, \"passed\": 100, \"failures\": [], \"ok\": true}\n",
  "stderr": ""
}
```

### javascript_tests — PASSED

```json
{
  "command": [
    "node",
    "--test",
    "--experimental-default-type=module",
    "tests_js/aedifex-v10-contracts.test.mjs",
    "tests_js/core.test.mjs",
    "tests_js/fusion-v6.test.mjs",
    "tests_js/v10-integration.test.mjs"
  ],
  "returncode": 0,
  "stdout": "TAP version 13\n# Subtest: camadas de contexto são locais, imutáveis, deduplicadas e determinísticas\nok 1 - camadas de contexto são locais, imutáveis, deduplicadas e determinísticas\n  ---\n  duration_ms: 11.560006\n  type: 'test'\n  ...\n# Subtest: camada de contexto recusa provider remoto e hash ausente\nok 2 - camada de contexto recusa provider remoto e hash ausente\n  ---\n  duration_ms: 0.49914\n  type: 'test'\n  ...\n# Subtest: canal postMessage usa entropia criptográfica e o sidecar o exige\nok 3 - canal postMessage usa entropia criptográfica e o sidecar o exige\n  ---\n  duration_ms: 1.029937\n  type: 'test'\n  ...\n# Subtest: estado persistente V10 inclui camadas de contexto sem segredo ou URL externa\nok 4 - estado persistente V10 inclui camadas de contexto sem segredo ou URL externa\n  ---\n  duration_ms: 0.239846\n  type: 'test'\n  ...\n# Subtest: overlay possui um único fluxo IFC transacional e copia WASM local\nok 5 - overlay possui um único fluxo IFC transacional e copia WASM local\n  ---\n  duration_ms: 5.795\n  type: 'test'\n  ...\n# Subtest: contexto Aedifex 3D é readonly, hash-verificado e excluído do export\nok 6 - contexto Aedifex 3D é readonly, hash-verificado e excluído do export\n  ---\n  duration_ms: 1.352265\n  type: 'test'\n  ...\n# Subtest: host e sidecar exigem o mesmo canal em ambas as direções\nok 7 - host e sidecar exigem o mesmo canal em ambas as direções\n  ---\n  duration_ms: 1.581103\n  type: 'test'\n  ...\n# Subtest: transação confirma em ordem e descarta rollbacks\nok 8 - transação confirma em ordem e descarta rollbacks\n  ---\n  duration_ms: 1.620202\n  type: 'test'\n  ...\n# Subtest: falha durante commit executa rollback LIFO\nok 9 - falha durante commit executa rollback LIFO\n  ---\n  duration_ms: 0.653238\n  type: 'test'\n  ...\n# Subtest: safeCallback nunca relança no laço e desativa após limiar\nok 10 - safeCallback nunca relança no laço e desativa após limiar\n  ---\n  duration_ms: 1.19406\n  type: 'test'\n  ...\n# Subtest: ResourceTracker limpa recursos uma única vez\nok 11 - ResourceTracker limpa recursos uma única vez\n  ---\n  duration_ms: 0.295498\n  type: 'test'\n  ...\n# Subtest: contexto de plugin bloqueia capabilities não concedidas\nok 12 - contexto de plugin bloqueia capabilities não concedidas\n  ---\n  duration_ms: 0.557356\n  type: 'test'\n  ...\n# Subtest: interpolação geográfica, quaternion e altitude permanecem finitas\nok 13 - interpolação geográfica, quaternion e altitude permanecem finitas\n  ---\n  duration_ms: 0.383519\n  type: 'test'\n  ...\n# Subtest: track usa keyframes ordenados e hold não interpola\nok 14 - track usa keyframes ordenados e hold não interpola\n  ---\n  duration_ms: 0.417078\n  type: 'test'\n  ...\n# Subtest: política de escala impede geração planetária e limita bairro\nok 15 - política de escala impede geração planetária e limita bairro\n  ---\n  duration_ms: 0.263561\n  type: 'test'\n  ...\n# Subtest: PluginRegistry rejeita contratos incompletos e filtra por modo\nok 16 - PluginRegistry rejeita contratos incompletos e filtra por modo\n  ---\n  duration_ms: 0.852273\n  type: 'test'\n  ...\n# Subtest: política de rede do navegador bloqueia egress em offline_strict\nok 17 - política de rede do navegador bloqueia egress em offline_strict\n  ---\n  duration_ms: 0.778484\n  type: 'test'\n  ...\n# Subtest: local_lan aceita IP privado literal e recusa DNS público\nok 18 - local_lan aceita IP privado literal e recusa DNS público\n  ---\n  duration_ms: 0.621591\n  type: 'test'\n  ...\n# Subtest: estado inicial é local-first e não persiste segredo de provider\nok 19 - estado inicial é local-first e não persiste segredo de provider\n  ---\n  duration_ms: 0.219926\n  type: 'test'\n  ...\n# Subtest: ModelingContextRequest usa recorte desenhado como autoridade do lote\nok 20 - ModelingContextRequest usa recorte desenhado como autoridade do lote\n  ---\n  duration_ms: 2.083669\n  type: 'test'\n  ...\n# Subtest: ModelingContextRequest recusa abertura sem Região Ativa\nok 21 - ModelingContextRequest recusa abertura sem Região Ativa\n  ---\n  duration_ms: 0.335217\n  type: 'test'\n  ...\n# Subtest: pedido fotorreal referencia revisão real e deduplica mídias\nok 22 - pedido fotorreal referencia revisão real e deduplica mídias\n  ---\n  duration_ms: 0.538809\n  type: 'test'\n  ...\n# Subtest: PhotorealClient usa somente rotas locais e expõe cancelamento\nok 23 - PhotorealClient usa somente rotas locais e expõe cancelamento\n  ---\n  duration_ms: 0.247156\n  type: 'test'\n  ...\n# Subtest: FusionSharedState valida hashes e propaga prompt para render\nok 24 - FusionSharedState valida hashes e propaga prompt para render\n  ---\n  duration_ms: 0.296189\n  type: 'test'\n  ...\n# Subtest: LocalApiClient rejeita egress e preserva erro estruturado\nok 25 - LocalApiClient rejeita egress e preserva erro estruturado\n  ---\n  duration_ms: 14.23392\n  type: 'test'\n  ...\n# Subtest: StreetSequence navega e localiza frame sem provider\nok 26 - StreetSequence navega e localiza frame sem provider\n  ---\n  duration_ms: 0.464178\n  type: 'test'\n  ...\n# Subtest: estado inicial V6 contém shell, Floorplanner e abertura cinematográfica válidos\nok 27 - estado inicial V6 contém shell, Floorplanner e abertura cinematográfica válidos\n  ---\n  duration_ms: 0.260406\n  type: 'test'\n  ...\n# Subtest: rota nativa Aedifex usa broker ARCZ local e não cliente OpenAI\nok 28 - rota nativa Aedifex usa broker ARCZ local e não cliente OpenAI\n  ---\n  duration_ms: 6.012357\n  type: 'test'\n  ...\n# Subtest: bootstrap Aedifex registra plugins locais sem descoberta remota\nok 29 - bootstrap Aedifex registra plugins locais sem descoberta remota\n  ---\n  duration_ms: 1.759078\n  type: 'test'\n  ...\n# Subtest: Floorplanner usa um único agente global com ferramentas Aedifex, sem duplicar editor ou histórico\nok 30 - Floorplanner usa um único agente global com ferramentas Aedifex, sem duplicar editor ou histórico\n  ---\n  duration_ms: 1.752538\n  type: 'test'\n  ...\n# Subtest: dock usa navegação circular, hover apenas em ponteiro fino e largura segura\nok 31 - dock usa navegação circular, hover apenas em ponteiro fino e largura segura\n  ---\n  duration_ms: 1.063437\n  type: 'test'\n  ...\n# Subtest: baseline cinematográfico e nuvens locais não dependem de provider\nok 32 - baseline cinematográfico e nuvens locais não dependem de provider\n  ---\n  duration_ms: 0.68131\n  type: 'test'\n  ...\n# Subtest: estado da abertura funciona em runtimes sem CustomEvent\nok 33 - estado da abertura funciona em runtimes sem CustomEvent\n  ---\n  duration_ms: 0.648281\n  type: 'test'\n  ...\n# Subtest: layout de autoria mantém globo visível e limita dimensões persistíveis\nok 34 - layout de autoria mantém globo visível e limita dimensões persistíveis\n  ---\n  duration_ms: 0.776802\n  type: 'test'\n  ...\n# Subtest: lote desenhado domina bbox e resumo da Região Ativa\nok 35 - lote desenhado domina bbox e resumo da Região Ativa\n  ---\n  duration_ms: 0.327886\n  type: 'test'\n  ...\n# Subtest: biblioteca de prompts normaliza slug/tags e exige saída textual real\nok 36 - biblioteca de prompts normaliza slug/tags e exige saída textual real\n  ---\n  duration_ms: 0.731624\n  type: 'test'\n  ...\n# Subtest: mídias preservam papéis válidos e escolhem preview sem fingir suporte\nok 37 - mídias preservam papéis válidos e escolhem preview sem fingir suporte\n  ---\n  duration_ms: 0.233336\n  type: 'test'\n  ...\n# Subtest: request fotorreal valida câmera, passes e saída 8K sem provider remoto\nok 38 - request fotorreal valida câmera, passes e saída 8K sem provider remoto\n  ---\n  duration_ms: 0.528754\n  type: 'test'\n  ...\n# Subtest: configuração cinematográfica é limitada e destino inválido não move câmera\nok 39 - configuração cinematográfica é limitada e destino inválido não move câmera\n  ---\n  duration_ms: 0.335888\n  type: 'test'\n  ...\n# Subtest: flyToCamera só resolve quando callback real do Cesium termina\nok 40 - flyToCamera só resolve quando callback real do Cesium termina\n  ---\n  duration_ms: 0.628291\n  type: 'test'\n  ...\n# Subtest: host Floorplanner conserva Cesium, publicação por revisão e autoridade única\nok 41 - host Floorplanner conserva Cesium, publicação por revisão e autoridade única\n  ---\n  duration_ms: 7.29253\n  type: 'test'\n  ...\n1..41\n# tests 41\n# suites 0\n# pass 41\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n# duration_ms 125.032344\n",
  "stderr": ""
}
```

### typescript_overlay_syntax — PASSED

```json
{
  "command": [
    "node",
    "tools/check_typescript_syntax.mjs",
    "integrations/aedifex/overlay"
  ],
  "returncode": 0,
  "stdout": "{\n  \"files\": 19,\n  \"failures\": []\n}\n",
  "stderr": ""
}
```

### javascript_syntax — PASSED

```json
{
  "files": 101,
  "failures": []
}
```

### aedifex_conversion_matrix_generation — PASSED

```json
{
  "command": [
    "/opt/pyvenv/bin/python",
    "tools/build_aedifex_conversion_matrix.py"
  ],
  "returncode": 0,
  "stdout": "{\"path\": \"/mnt/data/arkz/integrations/aedifex/CONVERSION_MATRIX.json\", \"hash\": \"4718a2cb82d5cd83c6eeb72bb1bbd014fac9fad6a8fa27d9c3a9b3a7f9b5aaa4\", \"counts\": {\"packages\": 7, \"apps\": 2, \"plugins\": 1, \"native_node_kinds\": 46, \"extension_node_kinds\": 3, \"tool_families\": 21, \"global_modules\": 7, \"community_sources\": 5}}\n",
  "stderr": ""
}
```

### json_parse — PASSED

```json
{
  "files": 80,
  "failures": []
}
```

### json_schema_self_check — PASSED

```json
{
  "schemas": 38,
  "failures": []
}
```

### resource_schema_validation — PASSED

```json
{
  "validated": 9,
  "failures": []
}
```

### cargo_workspace_structure — PASSED

```json
{
  "members": 29,
  "failures": []
}
```

### rust_delimiter_sanity_not_compile — PASSED

```json
{
  "files": 23,
  "failures": [],
  "warning": "Não substitui cargo check"
}
```

### rustfmt_check — BLOCKED

```json
{
  "reason": "cargo/rustfmt ausente neste ambiente",
  "command": [
    "cargo",
    "fmt",
    "--all",
    "--",
    "--check"
  ]
}
```

### cargo_check_workspace — BLOCKED

```json
{
  "reason": "cargo/rustc ausente neste ambiente",
  "command": [
    "cargo",
    "check",
    "--workspace",
    "--all-targets"
  ]
}
```

### cargo_test_workspace — BLOCKED

```json
{
  "reason": "cargo/rustc ausente neste ambiente",
  "command": [
    "cargo",
    "test",
    "--workspace",
    "--all-targets"
  ]
}
```

### no_mock_no_stub_policy — PASSED

```json
{
  "failures": []
}
```

### core_contains_no_hardcoded_remote_url — PASSED

```json
{
  "findings": []
}
```

### browser_local_first_boot_contract — PASSED

```json
{
  "failures": [],
  "warning": "Prova estática; executar smoke browser/firewall no hardware alvo"
}
```

### aedifex_v10_source_contracts — PASSED

```json
{
  "failures": [],
  "warning": "Gate estático; não substitui build Aedifex, Cesium real ou E2E browser"
}
```

### documentation_v10_contract — PASSED

```json
{
  "failures": []
}
```

### release_entrypoint_contract — PASSED

```json
{
  "files": 25,
  "failures": [],
  "warning": "Gate estático; instalação limpa permanece um gate de runtime"
}
```

### linux_launcher_syntax — PASSED

```json
{
  "command": [
    "bash",
    "-n",
    "scripts/linux/common.sh",
    "scripts/linux/install.sh",
    "scripts/linux/run.sh",
    "scripts/linux/stop.sh",
    "scripts/linux/uninstall.sh",
    "install.sh",
    "run.sh",
    "stop.sh",
    "uninstall.sh"
  ],
  "returncode": 0,
  "stdout": "",
  "stderr": ""
}
```

### windows_launcher_runtime — BLOCKED

```json
{
  "reason": "PowerShell não está disponível neste ambiente; scripts Windows não foram executados",
  "command": "powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1 ..."
}
```

### docker_compose_runtime — BLOCKED

```json
{
  "reason": "Docker/Compose ausente; configuração universal não foi validada neste ambiente",
  "command": "docker compose config && docker compose up --build"
}
```

### cesium_local_vendor — BLOCKED

```json
{
  "reason": "vendor CesiumJS foi excluído do arquivo de origem e precisa ser instalado localmente",
  "missing": [
    "vendor/cesium/Cesium/Cesium.js",
    "vendor/cesium/Cesium/Widgets/widgets.css",
    "vendor/cesium/Cesium/Assets/Textures/NaturalEarthII/tilemapresource.xml",
    "vendor/cesium/LICENSE.md",
    "vendor/cesium/manifest.json"
  ],
  "command": "python tools/vendor_cesium.py --source <local> --license-file <local> --version 1.143.0"
}
```

### aedifex_vendor_and_build — BLOCKED

```json
{
  "reason": "upstream/fork/build Aedifex pinados ainda não foram materializados e compilados",
  "blockers": [
    {
      "code": "AEDIFEX_UPSTREAM_MISSING",
      "files": [
        "LICENSE",
        "package.json",
        "bun.lock",
        "apps/editor/package.json",
        "apps/ifc-converter/package.json",
        "packages/core/package.json",
        "packages/viewer/package.json",
        "packages/editor/package.json",
        "packages/mcp/package.json",
        "packages/nodes/package.json",
        "packages/plugin-trees/package.json",
        "packages/ifc-converter/package.json"
      ]
    },
    {
      "code": "AEDIFEX_FORK_MISSING",
      "files": [
        "LICENSE",
        "package.json",
        "bun.lock",
        "apps/editor/package.json",
        "apps/ifc-converter/package.json",
        "packages/core/package.json",
        "packages/viewer/package.json",
        "packages/editor/package.json",
        "packages/mcp/package.json",
        "packages/nodes/package.json",
        "packages/plugin-trees/package.json",
        "packages/ifc-converter/package.json"
      ]
    },
    {
      "code": "AEDIFEX_PACKAGE_MATRIX_INVALID",
      "scope": "upstream",
      "packages": [
        {
          "name": "@aedifex/core",
          "path": "packages/core/package.json",
          "expected_version": "0.10.0",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        },
        {
          "name": "@aedifex/viewer",
          "path": "packages/viewer/package.json",
          "expected_version": "0.10.0",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        },
        {
          "name": "@aedifex/editor",
          "path": "packages/editor/package.json",
          "expected_version": "0.9.3",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        },
        {
          "name": "@aedifex/mcp",
          "path": "packages/mcp/package.json",
          "expected_version": "0.3.3",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        },
        {
          "name": "@aedifex/nodes",
          "path": "packages/nodes/package.json",
          "expected_version": "0.2.0",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        },
        {
          "name": "@aedifex/plugin-trees",
          "path": "packages/plugin-trees/package.json",
          "expected_version": "0.1.1",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        },
        {
          "name": "@aedifex/ifc-converter",
          "path": "packages/ifc-converter/package.json",
          "expected_version": "0.1.3",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        }
      ]
    },
    {
      "code": "AEDIFEX_PACKAGE_MATRIX_INVALID",
      "scope": "fork",
      "packages": [
        {
          "name": "@aedifex/core",
          "path": "packages/core/package.json",
          "expected_version": "0.10.0",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        },
        {
          "name": "@aedifex/viewer",
          "path": "packages/viewer/package.json",
          "expected_version": "0.10.0",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        },
        {
          "name": "@aedifex/editor",
          "path": "packages/editor/package.json",
          "expected_version": "0.9.3",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        },
        {
          "name": "@aedifex/mcp",
          "path": "packages/mcp/package.json",
          "expected_version": "0.3.3",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        },
        {
          "name": "@aedifex/nodes",
          "path": "packages/nodes/package.json",
          "expected_version": "0.2.0",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        },
        {
          "name": "@aedifex/plugin-trees",
          "path": "packages/plugin-trees/package.json",
          "expected_version": "0.1.1",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        },
        {
          "name": "@aedifex/ifc-converter",
          "path": "packages/ifc-converter/package.json",
          "expected_version": "0.1.3",
          "exists": false,
          "valid": false,
          "error": "PACKAGE_MANIFEST_MISSING"
        }
      ]
    },
    {
      "code": "AEDIFEX_LICENSE_UNVERIFIED"
    },
    {
      "code": "AEDIFEX_COMMIT_UNVERIFIED"
    },
    {
      "code": "AEDIFEX_FORK_COMMIT_UNVERIFIED"
    },
    {
      "code": "AEDIFEX_UPSTREAM_INVENTORY_MISSING"
    },
    {
      "code": "AEDIFEX_BRIDGE_BUILD_MISSING"
    }
  ],
  "command": "python tools/vendor_aedifex.py --source <checkout-local> && python tools/build_aedifex_sidecar.py"
}
```

### photoreal_worker_contract — PASSED

```json
{
  "files": [
    "resources/workers/render.photoreal.worker.json",
    "workers/blender/launch_blender.py",
    "workers/blender/render_floor_scene.py"
  ]
}
```

### blender_photoreal_runtime — BLOCKED

```json
{
  "reason": "Blender/Cycles local não instalado; o worker não pode produzir imagens reais",
  "command": "instale Blender local e defina ARCZ_BLENDER=<caminho>"
}
```

### local_ai_render_models — BLOCKED

```json
{
  "reason": "modelos locais necessários não foram materializados/verificados",
  "missing_tasks": [
    "chat.global",
    "prompt.enhance",
    "prompt.translate",
    "render-diffusion",
    "upscale"
  ],
  "manifests": []
}
```

### runtime_artifact_cleanup — PASSED

```json
{
  "removed": [
    "__pycache__",
    "arcz_server/__pycache__",
    "tests_python/__pycache__",
    "tools/__pycache__",
    "tools/aedifex/__pycache__",
    "workers/blender/__pycache__",
    ".pytest_cache",
    "jobs/jobs.sqlite3",
    "jobs/budget.sqlite3",
    "data/registry.sqlite3",
    "data/media/registry.sqlite3",
    "data/floorplanner/floorplanner.sqlite3",
    "data/indexes/geocoder.sqlite3",
    "data/chat/chat.sqlite3",
    "data/prompts/prompts.sqlite3",
    "scene/staging/.gitkeep",
    "data/floorplanner/exports/.gitkeep",
    "data/media/content/.gitkeep",
    "logs/.gitkeep"
  ]
}
```

### release_tree_has_no_runtime_artifacts — PASSED

```json
{
  "forbidden": []
}
```
