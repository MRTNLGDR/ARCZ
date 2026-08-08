# Aedifex dentro do ARCZ Earth V10

Aedifex é o **Building Authoring Kernel**. ARCZ é o **World Core**. A cena editável é um `SceneSnapshot` revisionado; o globo recebe apenas GLB readonly com `GeoAnchor`.

## Host de compatibilidade

Enquanto ARCZ permanece ES Modules sem bundler e Aedifex permanece React/Next/R3F:

- sidecar local em loopback;
- iframe sandboxed;
- canal criptográfico por sessão;
- origem e `contentWindow` validados;
- HTTP/SSE local;
- Cesium permanece visível no split Floorplanner;
- nenhuma rede externa ou chave de provider.

Destino: host Tauri/React único.

## Materialização

```bash
python tools/vendor_aedifex.py --source <checkout-local>
python tools/build_aedifex_sidecar.py
python tools/verify_aedifex_integration.py
```

O checkout deve corresponder a `UPSTREAM_LOCK.json`. O original é preservado; overlays entram no fork controlado. O inventário e a cobertura são obrigatórios.

## Fluxo

```text
Região/lote → ModelingContext + GeoAnchor
→ projeto/revisão Aedifex
→ edição sobre contexto read-only
→ GLB real
→ validação/hash/store atômico
→ derivado readonly no Cesium
```

Leia `docs/integration/AEDIFEX_CAPABILITY_LEDGER_V10.md`.
