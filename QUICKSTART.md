# QUICKSTART — ARCZ Earth + Aedifex Global V10

## Estado desta entrega

O código, contratos, testes e integração V10 estão empacotados. A auditoria
final deste ambiente está em `BLOCKED`, não em `READY`, porque os bytes locais
do Cesium/Aedifex, Rust, Blender e modelos não estavam disponíveis. Consulte
`docs/audit/VALIDATION_REPORT.md` antes de executar.

## Windows

1. Extraia o ZIP em um caminho curto, por exemplo `D:\ARCZ-Earth`.
2. Materialize as dependências locais auditadas:

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1 `
  -AedifexSource "D:\opensources\aedifex" `
  -CesiumSource "D:\deps\Cesium" `
  -CesiumLicense "D:\deps\Cesium-LICENSE.md"
```

3. Execute `run.bat`.
4. Para encerrar, execute `stop.bat`.
5. O relatório de preflight fica em `logs/runtime-preflight.json`.

O checkout Aedifex precisa estar exatamente no commit registrado em
`integrations/aedifex/UPSTREAM_LOCK.json`. O instalador recusa outro commit.

## Linux

```bash
./install.sh \
  --aedifex-source /opt/opensources/aedifex \
  --cesium-source /opt/deps/Cesium \
  --cesium-license /opt/deps/Cesium-LICENSE.md
./run.sh
./stop.sh
```

## Validação antes de abrir

```bash
python tools/runtime_preflight.py --profile interactive
python tools/verify_handoff.py --allow-missing-rust
```

Para validar também geração Rust, Blender/Cycles, prompt enhancer, tradução,
difusão e upscale:

```bash
python tools/runtime_preflight.py --profile full
```

## Modo import-assisted opcional

O core continua `offline_strict`. Downloads só podem ocorrer durante uma
importação explicitamente autorizada. No Windows:

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1 -ImportAssisted -CloneAedifex `
  -CesiumSource "D:\deps\Cesium" -CesiumLicense "D:\deps\Cesium-LICENSE.md"
```

Após materializar e validar os artefatos, volte a `offline_strict`.
