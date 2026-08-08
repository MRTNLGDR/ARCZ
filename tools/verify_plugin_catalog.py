#!/usr/bin/env python3
import json, re, sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
data=json.loads((ROOT/'plugins/catalog.json').read_text(encoding='utf-8'))
errors=[]; ids=set(); caps=set()
allowed_status={'implemented','partial','contract_ready','blocked'}
allowed_runtime={'builtin_rust','wasm_component','local_process','web_sidecar'}
for p in data.get('plugins',[]):
    pid=p.get('id','')
    if not re.fullmatch(r'[a-z0-9][a-z0-9._-]*',pid): errors.append(f'invalid id {pid!r}')
    if pid in ids: errors.append(f'duplicate id {pid}')
    ids.add(pid)
    if p.get('status') not in allowed_status: errors.append(f'{pid}: invalid status')
    if p.get('runtime') not in allowed_runtime: errors.append(f'{pid}: invalid runtime')
    if p.get('api_version') != 1: errors.append(f'{pid}: api_version must be 1')
    if not p.get('entrypoint'): errors.append(f'{pid}: missing entrypoint')
    local=set()
    for cap in p.get('capabilities',[]):
        if not cap or cap in local: errors.append(f'{pid}: duplicate/empty capability {cap!r}')
        local.add(cap); caps.add(cap)
for required in ['cad.write','geo.reconstruct','building.generate','road.generate','ifc.write','render.8k','agent.cad.write','asset.glb']:
    if required not in caps: errors.append(f'missing required capability {required}')
print(json.dumps({'plugins':len(ids),'capabilities':len(caps),'errors':errors},indent=2,ensure_ascii=False))
sys.exit(1 if errors else 0)
