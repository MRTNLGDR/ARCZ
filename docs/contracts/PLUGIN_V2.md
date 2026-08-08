# Contrato Plugin V2

## Manifesto

Validado por `schemas/plugin-manifest-v2.schema.json`. Campos centrais:

- identidade/versionamento/API;
- escalas e modos;
- capabilities mínimas;
- worker real;
- determinismo;
- custo-base.

## Ciclo de vida

```text
validar
→ estimar
→ preparar
→ gerar
→ validarResultado
→ stage
→ commit
```

Em qualquer falha após `stage`, execute `rollback`. `limpar` deve remover listeners, timers, subscriptions, primitives e handles registrados.

## Contexto com capabilities

O plugin não recebe `viewer` cru. Ele recebe facades congeladas conforme capabilities:

- `region.read`
- `terrain.read`
- `osm.read`
- `inputs.resolve`
- `assets.read`
- `scene.stage`
- `budget.reserve`
- `job.progress`
- `telemetry.write`
- `ai.local`

Acesso não concedido lança erro. Não troque isso por `undefined` silencioso.

## Teste de vazamento

Ativar, gerar, limpar e repetir pelo menos três vezes. Compare primitivos, listeners, timers, maps e referências. Crescimento residual acima da tolerância reprova o plugin.
