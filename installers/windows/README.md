# Windows installer status

`run.bat`, `stop.bat`, `install.ps1` e `uninstall.ps1` são os inicializadores
portáteis auditáveis desta entrega. Um `Setup.exe`/MSI não foi produzido porque
o runtime Aedifex, Cesium e toolchain de empacotamento não estavam
materializados neste ambiente. Não substitua essa ausência por um executável
vazio. O gate permanece em `TASKS.json` e `IMPLEMENTATION_STATUS.json`.
