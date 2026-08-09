<!-- REGRA-RUN-BAT :: nao remover :: mantida por Claude e GPT via D:\AGENT_SYNC -->
# REGRA OBRIGATORIA — INICIALIZADOR UNICO (`RUN.bat`)

**Vale para qualquer agente (Claude, GPT/Codex, Gemini, Copilot) e para o usuario.**

Este projeto tem **um unico** ponto de entrada executavel: **`RUN.bat`, na raiz**.
Um clique nele faz, sempre nesta ordem:

1. **GIT**     — `git fetch` + `git pull --ff-only` (traz o que os agentes corrigiram)
2. **INSTALL** — instala/atualiza todas as dependencias
3. **RUN**     — sobe o servidor na porta reservada do projeto
4. **ABRIR**   — abre o navegador

Opcoes: `RUN.bat /sem-navegador` · `/sem-pull` · `/reinstalar` · `/parar`

## O que voce NAO pode fazer

- **Nao crie outro `.bat`, `.cmd`, `.ps1` ou `.sh` de inicializacao.** Nao existe
  "primeiro rode o instalador, depois o outro script". E um arquivo so.
- **Nao mande o usuario rodar** `install.ps1`, `run.ps1`, `start.bat`, `LIGAR.bat`,
  `INSTALL_CINENODE.bat`, `PERZON_MAIN.bat`, `stop.bat` ou qualquer inicializador
  antigo. A resposta a "como rodo isso?" e sempre: **clique no `RUN.bat`**.
  Os antigos continuam em disco so porque estao versionados no git.
- **Nao duplique logica dentro do `.bat`.** Ele tem 3 linhas uteis de proposito.

## Onde voce corrige e evolui

Todo o comportamento vive em **um motor compartilhado**:

    D:\AGENT_SYNC\motor-run.ps1

Precisa de passo novo, dependencia nova, porta diferente, verificacao a mais?
**Edite esse arquivo** e registre no seu log (`CLAUDE-LOG.md` ou `GPT-LOG.md`).
O catalogo de projetos (raiz, porta, tem git ou nao) esta no topo dele.
Evoluir o motor e obrigatorio; recriar bats e proibido.

## Regras tecnicas que ja custaram bug real

- Todo `.ps1` **tem que ser salvo em UTF-8 COM BOM**. Os `.bat` chamam o Windows
  PowerShell 5.1, que le arquivo sem BOM como Windows-1252 e quebra o parser no
  primeiro acento. Isso ja derrubou o instalador do CineNode e o proprio motor.
- Em script chamado por `.bat`, use `$ErrorActionPreference = "Continue"` e
  verifique `$LASTEXITCODE`. Com `"Stop"`, a saida normal de `git`/`npm`/`docker`
  em stderr vira erro terminante.
- Para subir Vite em segundo plano, chame `node node_modules\vite\bin\vite.js`
  direto. `npm run dev` falha com `Cannot find module npm-cli.js`.
- O passo GIT **nunca** usa `git reset --hard`. Alteracao local vai para
  `git stash` antes do pull e volta depois.

## Coordenacao entre os agentes

`D:\AGENT_SYNC\` — `PROTOCOLO.md`, `SERVIDORES.md` (mapa de portas), `LOCKS.md`,
`CLAUDE-LOG.md`, `GPT-LOG.md`. Leia o log do outro antes de comecar; escreva no
seu ao terminar; nunca edite o log do outro.

<!-- fim REGRA-RUN-BAT -->

# Contrato obrigatório para agentes — ARCZ Earth + Aedifex V10.1

## Leitura obrigatória

1. `LEIA-PRIMEIRO.md`
2. `docs/integration/AEDIFEX_CAPABILITY_LEDGER_V10.md`
3. `integrations/aedifex/UPSTREAM_LOCK.json`
4. `integrations/aedifex/CONVERSION_COVERAGE.json`
5. `integrations/aedifex/CONVERSION_MATRIX.json`
6. `docs/integration/CONVERSION_MATRIX_GUIDE.md`
7. `docs/integration/USER_REQUIREMENT_TRACEABILITY_V10.md`
8. `integrations/aedifex/AUTHOR_REPOSITORY_AUDIT.json`
9. `integrations/aedifex/COMPATIBILITY_MATRIX.json`
10. `docs/integration/FLOORPLANNER_GLOBE_ROUNDTRIP.md`
11. `docs/implementation/NO_MOCK_POLICY.md`
12. `IMPLEMENTATION_STATUS.json`
13. `TASKS.json`
14. `docs/audit/VALIDATION_REPORT.md`

## Invariantes

1. ARCZ é a autoridade WGS84/ECEF/ENU; Aedifex é a autoridade da cena editável.
2. Há uma única cópia editável. GLB é readonly e revisionado.
3. O upstream pinado é imutável; mudanças entram por overlay.
4. O inventário do checkout é obrigatório e coverage é fail-closed.
5. Sidecar só escuta loopback; origem, janela, canal, projeto, revisão e request ID são validados.
6. O iframe é transitório; não reescreva Aedifex em ES Modules puros.
7. Não migre compute para Rust sem schema/loss/golden/parity/rollback.
8. IA passa pelo Local AI Broker; pesos ausentes geram erro.
9. Existe um chat global; não monte dois históricos.
10. Mutação/export/destruição exigem preview/aprovação/revision guard.
11. Mídia exige bytes/hash/MIME/licença/provenance.
12. Render exige preflight e inputs reais.
13. Painéis começam recolhidos e permanecem acessíveis por foco/teclado/pin.
14. `app/ui.js` não cresce.
15. Projeto novo não carrega empreendimento fixo.
16. Callback Cesium nunca relança no render loop.
17. Nenhum provider remoto é core.
18. Nenhuma capacidade é `DONE` com gate associado `BLOCKED`.
19. Inicializador deve executar preflight e falhar com ação concreta; nunca abrir UI quebrada.
20. Setup/Docker só recebem status aprovado após execução limpa no runtime correspondente.

## Definition of Done de uma onda

- requisito e matriz atualizados;
- schema e migração;
- frontend/backend/storage/worker integrados;
- sucesso, erro, conflito, cancelamento e recuperação;
- cleanup/leak;
- segurança/offline;
- testes reais;
- evidence e hashes;
- docs/governança;
- rollback;
- sem mock permanente.

## Comandos

```bash
python tools/runtime_preflight.py --profile interactive
python -m pytest -q
node --test --experimental-default-type=module tests_js/*.mjs
node tools/check_typescript_syntax.mjs integrations/aedifex/overlay
python tools/job_cancel_stress.py --iterations 100
python tools/verify_aedifex_integration.py
python tools/verify_handoff.py
python tools/sync_governance_v10.py
```
