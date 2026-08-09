# COORDENAÇÃO ENTRE AGENTES — LEIA ANTES DE ALTERAR ESTE PROJETO

## Como rodar este projeto

Clique em **`RUN.bat`**, na raiz desta pasta. É o único arquivo necessário:
ele puxa do git, instala, atualiza e abre no navegador, nessa ordem.

    RUN.bat                  puxa, instala, sobe e abre
    RUN.bat /sem-navegador   sobe sem abrir o navegador
    RUN.bat /sem-pull        nao atualiza pelo git
    RUN.bat /reinstalar      refaz node_modules / .venv
    RUN.bat /parar           derruba o servidor deste projeto

Os `.bat` antigos continuam em disco por estarem no git, mas nao sao mais o
caminho de uso. Nao crie novos `.bat`: o motor compartilhado fica em
`D:\AGENT_SYNC\motor-run.ps1` e e la que se corrige comportamento.

## Registro compartilhado entre Claude e GPT

    D:\AGENT_SYNC\

- `PROTOCOLO.md`  — regras de escrita e rotina obrigatória
- `SERVIDORES.md` — porta e inicializador de cada um dos 6 projetos
- `LOCKS.md`      — quem está mexendo em quê agora
- `CLAUDE-LOG.md` — o que o Claude fez, com evidência
- `GPT-LOG.md`    — o que o GPT fez, com evidência

Antes de mexer: leia `LOCKS.md` e o log do outro agente.
Depois de mexer: escreva no seu próprio log e libere o lock.
Não edite o log do outro agente.