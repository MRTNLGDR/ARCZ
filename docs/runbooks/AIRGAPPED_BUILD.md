# Runbook de build e validação air-gapped

## 1. Preparar em máquina autorizada

- obter e auditar CesiumJS 1.143;
- produzir wheelhouse Python para a plataforma alvo;
- produzir vendor de crates Rust;
- transferir dados geográficos, assets e modelos como pacotes locais;
- registrar licença, proveniência e SHA-256 de todos os artefatos.

## 2. Isolar

Bloqueie DNS e egress. O modo de runtime deve ser `offline_strict`. Não use
proxy transparente para fazer um teste aparentemente offline.

## 3. Instalar dependências locais

```bash
python tools/vendor_cesium.py --source <Cesium-local> \
  --license-file <LICENSE-local> --version 1.143.0
python -m pip install --no-index --find-links vendor/python/wheelhouse \
  -r requirements-dev.txt
cp .cargo/config.offline.example.toml .cargo/config.toml
```

## 4. Verificar

```bash
python tools/verify_handoff.py
python tools/offline_acceptance.py
```

Não use `--allow-missing-rust` numa release. Essa opção existe só para produzir
um relatório honesto quando a ferramenta está ausente.

## 5. Provar o worker real

```bash
python tools/build_generation_worker.py
python tools/smoke_generation.py \
  --package examples/source-package/minimal-package \
  --generator houses
```

O smoke test exige o binário real, gera arquivos reais e valida manifest/GLB.
Ele se recusa a substituir worker ausente por processo Python ou resultado
pré-gravado.

## 6. Provar o navegador

- iniciar `python servidor.py`;
- confirmar carregamento exclusivamente de `127.0.0.1`/mesma origem;
- abrir Natural Earth local;
- selecionar índice geográfico local;
- ativar DEM apenas quando o pacote existir;
- executar geração, cancelamento, rollback, limpeza e reabertura;
- inspecionar sockets do processo e do navegador.

## 7. Critério de release

Só liberar quando `docs/audit/VALIDATION_REPORT.md` estiver `PASSED`, sem gates
`FAILED` ou `BLOCKED`, e o teste de cabo desconectado estiver documentado no
hardware alvo.
