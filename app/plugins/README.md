# Plugins V2

Plugins nunca recebem `viewer` cru. Use o `ctx` de capabilities. Um gerador deve
validar/estimar/preparar/gerar/validar resultado/stage/commit/rollback/limpar.
`limpar()` é parte do contrato e precisa passar leak test.
