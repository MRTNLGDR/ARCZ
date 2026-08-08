# ARCZ–Aedifex Bridge Protocol v2

## Autoridade

- ARCZ: WGS84/ECEF/ENU, Região Ativa, lote, terreno, fontes e render.
- Aedifex: scene graph editável do edifício.
- GLB: derivado readonly identificado por projeto/revisão/hash.

## Coordenadas

- ARCZ ENU: `[east, north, up]`, metros.
- Aedifex: X/Z no chão, Y para cima.
- Política: `X=east`, `Y=up`, `Z=south`; norte = `-Z`.
- `north_rotation_deg` e `vertical_offset_m` pertencem ao `GeoAnchor`.
- Nunca derive transformação da câmera ou de orientação visual da planta.

## Ciclo

```text
POST context
→ create/open project
→ load revision
→ edit
→ save(expected_revision)
→ export request ID
→ actual R3F scene GLB
→ binary upload headers
→ GLB validation + SHA-256 + atomic store
→ derivative event
→ stage/load readonly model in globe
```

## Concorrência

`expected_revision` é obrigatório. Conflito retorna erro estruturado; não existe
last-write-wins. Export atrasado de revisão anterior não substitui o latest.

## Sidecar transitório

A V10 aceita somente iframe sandboxed do sidecar loopback cujo build foi
verificado. A bridge valida `event.origin`, `event.source`, project ID, request
ID e revision. Mensagens desconhecidas são ignoradas/registradas. O destino é
uma integração in-process Tauri/React sem alterar os contratos.

## Segurança

- sem `allow-same-origin`/capabilities além do necessário sem análise;
- sem navegação externa;
- body limits;
- GLB validado antes do disco/cena;
- paths nunca vêm do cliente;
- modelo no globo não recebe gizmo;
- Local AI Broker é a única entrada de IA.
