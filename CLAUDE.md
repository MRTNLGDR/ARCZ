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

# Instruções ARCZ para Claude e outros agentes

Este arquivo não substitui `AGENTS.md`. Leia `AGENTS.md` integralmente.

A entrega aceita é código real, executável e testado. Não use componentes fictícios, dados inventados, endpoints simulados, arquivos vazios, funções que retornam sucesso fixo ou UI desconectada. Quando uma dependência local não estiver instalada, retorne o erro estruturado previsto e documente o comando de instalação/compilação.
