# Chat global, prompts, referências e render fotorreal V10

## 1. Chat único

A V10 não monta um chat ARCZ e outro Aedifex ao mesmo tempo. `GlobalChatPanel` é a superfície única e recebe:

- Região Ativa e lote;
- projeto/revisão/scene hash;
- tools ARCZ;
- tools MCP Aedifex;
- prompts e referências;
- render/cinema/Street/governança;
- histórico e tool runs persistidos.

A UI nativa Aedifex continua preservada no upstream como fonte de agent loop, planner, proposals, templates, room analyzer e ghost preview. Esses recursos são adaptados ao chat global, não montados como segundo histórico.

## 2. Segurança de ferramentas

- ferramentas read-only podem autoexecutar;
- mutate/export/destructive exigem dry-run e aprovação;
- `project_id` e `expected_revision` vêm do host;
- preview, request, result e error são hash-eados/persistidos;
- approval ID é único;
- revision mismatch impede commit;
- reject/cancel são estados explícitos;
- modelo nunca recebe acesso direto ao viewer ou filesystem.

A V10 possui diff real. O ghost visual translúcido nativo do Aedifex permanece bloqueado até o workspace upstream compilar e o adapter de `ValidatedOperation` passar testes.

## 3. Prompt library

SQLite local, com:

- slug/título/categoria/finalidade;
- idioma BCP-47;
- positive e negative template;
- variables e validação;
- tags;
- built-ins imutáveis;
- duplicate/archive;
- versões e hashes;
- import/export bundle com tamper guard;
- compile determinístico;
- enhance e translate via Local AI Broker;
- cache por model checksum/input/params.

Modelo ausente retorna `MODEL_NOT_INSTALLED`; não há enhancer de texto fixo disfarçado.

Os manifestos locais usam tarefas exatas e validadas: `chat.global`,
`prompt.enhance`, `prompt.translate`, `render-diffusion` e `upscale`. Um
manifesto com nome diferente não é aceito como substituto silencioso.

## 4. Mídias

Uploads guardam bytes reais por hash. Metadados incluem roles, weight, notes, licença e provenance. O painel oferece preview real quando o navegador suporta o MIME. Corrupção bloqueia uso.

Papéis podem orientar:

- arquitetura/geometria;
- fachada/material;
- estilo/iluminação;
- câmera/composição;
- paisagismo;
- pessoas/veículos;
- planta/documento;
- referência geral.

## 5. Render fotorreal

Preflight exige revisão e inputs atuais. Para alta/ultra exige GLB real do viewport. O job congela a cena e referências.

### Pipeline

```text
SceneSnapshot → GLB → Blender/Cycles → passes → geometry guard
→ difusão local opcional → upscale em tiles → checksum/manifest
```

### Passes

Beauty, depth, normals, object IDs, semantic masks, material masks e sky mask. EXR é suportado pelo contrato quando o worker/viewer local estiver instalado.

### Qualidade

- câmera/lente/apertura/foco;
- 4K/8K/custom;
- samples e denoise;
- tile overlap;
- seed;
- positive/negative prompt;
- referências ponderadas;
- proteção estrutural;
- checkpoint/resume;
- relatório de falhas.

Nenhum job é marcado pronto sem Blender/worker/modelo/GLB necessários.
